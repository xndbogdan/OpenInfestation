//! Camera smoothing block 0040F170/190/1C0 and its CRT initializer tables.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use super::CameraParameterBlock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraParameterState {
    /// +00/+02/+04, signed WORD velocities in world-coordinate units.
    pub current_velocity: Option<[u16; 3]>,
    /// +06/+08/+0A, wrapping world-coordinate WORDs.
    pub current_position: Option<[u16; 3]>,
    /// +0C/+0E/+10 and +12/+14/+16, written together by 0040F190.
    pub desired_velocity: Option<[u16; 3]>,
    pub desired_position: Option<[u16; 3]>,
}

impl CameraParameterState {
    pub const UNWRITTEN: Self = Self {
        current_velocity: None,
        current_position: None,
        desired_velocity: None,
        desired_position: None,
    };

    /// 0040F190 copies both vectors without changing the current state.
    pub fn set_target(&mut self, position: [u16; 3], velocity: [u16; 3]) {
        self.desired_position = Some(position);
        self.desired_velocity = Some(velocity);
    }

    /// 0040F1C0. Only the low return byte is meaningful: true means a snap,
    /// including a far-threshold snap, and coordinates/velocity are WORDs.
    /// `raw_floor` supplies the signed H12 byte shifted by 005372F8, not the
    /// interpolated terrain/water height used by the later camera clamp.
    pub fn advance(
        &mut self,
        block: CameraParameterBlock,
        request: CameraSlewRequest,
        mut raw_floor: impl FnMut([u16; 2]) -> Result<i16, CameraSlewError>,
    ) -> Result<bool, CameraSlewError> {
        let desired_position = self
            .desired_position
            .ok_or(CameraSlewError::UnwrittenTarget)?;
        let desired_velocity = self
            .desired_velocity
            .ok_or(CameraSlewError::UnwrittenTarget)?;
        if request.snap != 0 && request.terrain_bypass != 0 {
            self.current_position = Some(desired_position);
            self.current_velocity = Some(desired_velocity);
            return Ok(true);
        }
        let mut position = self
            .current_position
            .ok_or(CameraSlewError::UnwrittenCurrent)?;
        let mut velocity = self
            .current_velocity
            .ok_or(CameraSlewError::UnwrittenCurrent)?;
        let table = smoothing_table(block)?;
        let delta = (request.microseconds >> 5) as i32;
        let mut differences = std::array::from_fn::<_, 3, _>(|axis| {
            position[axis].wrapping_sub(desired_position[axis]) as i16
        });
        if request.terrain_bypass != 0 {
            for difference in &mut differences {
                let value = i32::from(*difference);
                if value > 0 {
                    if value < table.deadzone {
                        *difference = 0;
                    } else if value > table.far_threshold {
                        self.current_position = Some(desired_position);
                        self.current_velocity = Some(desired_velocity);
                        return Ok(true);
                    } else {
                        *difference = difference.wrapping_sub(table.deadzone as i16);
                    }
                } else if value > table.deadzone.wrapping_neg() {
                    *difference = 0;
                } else if value < table.far_threshold.wrapping_neg() {
                    self.current_position = Some(desired_position);
                    self.current_velocity = Some(desired_velocity);
                    return Ok(true);
                } else {
                    *difference = difference.wrapping_add(table.deadzone as i16);
                }
            }
        }
        let sum = squared_length(differences);
        let mut distance = 0;
        if sum != 0 {
            distance = isqrt_48efc0(sum) as i16 as i32;
            // 0040F345 multiplies the first square root by table +08,
            // whose CRT initializer stored the square of its third argument.
            let mut speed = isqrt_48efc0(table.speed_squared.wrapping_mul(distance)) as i16 as i32;
            if request.terrain_bypass == 0 && speed > 2000 {
                speed = 2000;
            }
            if distance < (speed.wrapping_mul(delta) >> 15) as i16 as i32 {
                for axis in 0..3 {
                    position[axis] = position[axis].wrapping_sub(differences[axis] as u16);
                }
                differences = [0; 3];
            } else {
                for difference in &mut differences {
                    let normalized =
                        divide(i32::from(*difference).wrapping_shl(14), distance)? as i16 as i32;
                    *difference = (normalized.wrapping_mul(speed).wrapping_neg() >> 14) as i16;
                }
            }
        }
        if distance > 1024 {
            differences[1] = differences[1].wrapping_add(((distance - 1024) / 100) as i16);
        }
        let changes = std::array::from_fn::<_, 3, _>(|axis| {
            desired_velocity[axis]
                .wrapping_sub(velocity[axis])
                .wrapping_add(differences[axis] as u16) as i16
        });
        let sum = squared_length(changes);
        if sum != 0 {
            let magnitude = isqrt_48efc0(sum) as i16 as i32;
            let maximum = (table.maximum_acceleration.wrapping_mul(delta) >> 15) as i16 as i32;
            if maximum < magnitude {
                for axis in 0..3 {
                    let normalized =
                        divide(i32::from(changes[axis]).wrapping_shl(14), magnitude)? as i16 as i32;
                    velocity[axis] = velocity[axis]
                        .wrapping_add((normalized.wrapping_mul(maximum) >> 14) as u16);
                }
            } else {
                for axis in 0..3 {
                    velocity[axis] = velocity[axis].wrapping_add(changes[axis] as u16);
                }
            }
        }
        for axis in 0..3 {
            let product = i32::from(velocity[axis] as i16).wrapping_mul(delta);
            let movement = product.wrapping_add((product >> 31) & 0x7fff) >> 15;
            position[axis] = position[axis].wrapping_add(movement as u16);
        }
        // Native 0040F492..505 has already written velocity/position before
        // acquiring H12. A required terrain host failure retains that prefix.
        self.current_position = Some(position);
        self.current_velocity = Some(velocity);
        if request.terrain_bypass == 0 {
            let floor = i32::from(raw_floor([position[0], position[2]])?);
            let old_y = i32::from(position[1] as i16);
            if old_y < floor + 512 {
                position[1] = ((floor - old_y + 512) / 2 + old_y) as u16;
            }
        }
        self.current_position = Some(position);
        self.current_velocity = Some(velocity);
        Ok(false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraSlewRequest {
    pub microseconds: u32,
    pub snap: u8,
    /// Native camera +185: nonzero enables deadzone/far snap and skips the
    /// raw terrain midpoint lift; it is not the later eye terrain clamp.
    pub terrain_bypass: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CameraSlewError {
    #[error("camera target parameter words have not been written")]
    UnwrittenTarget,
    #[error("camera current parameter words have not been initialized by a snap")]
    UnwrittenCurrent,
    #[error("camera smoothing table 0x{0:08X} is unowned")]
    UnownedTable(u32),
    #[error("camera smoothing table selector {0} is outside the native range")]
    InvalidMode(u32),
    #[error("camera smoothing would fault in native signed division")]
    DivisionFault,
    #[error("camera raw terrain query is unowned: {0}")]
    RawTerrain(String),
}

#[derive(Clone, Copy)]
struct SmoothingTable {
    maximum_acceleration: i32,
    far_threshold: i32,
    speed_squared: i32,
    deadzone: i32,
}

fn smoothing_table(block: CameraParameterBlock) -> Result<SmoothingTable, CameraSlewError> {
    let address = match block.word_18 {
        0 => block.table_1c,
        1 => block.table_20,
        mode => return Err(CameraSlewError::InvalidMode(mode)),
    };
    // CRT initializers 0040F000, 0040F030, 0040F060, 0040F090, 0040F0C0,
    // 0040F0F0, 0040F120 and 0040F150 (through the jumps 0040EFF0..0040F140
    // the CRT table 0052E00C..0052E028 lists) call 0040EFC0(acceleration,
    // far threshold, speed coefficient, deadzone).
    let (maximum_acceleration, far_threshold, coefficient, deadzone) = match address {
        0x0054_6960 => (5000, 3000, 40_i32, 32),
        0x0054_6970 => (12000, 3000, 80, 32),
        0x0054_6980 => (12000, 3000, 80, 48),
        0x0054_6990 => (3500, 4000, 20, 48),
        0x0054_69a8 => (6000, 3000, 80, 32),
        0x0054_69b8 => (12000, 3000, 120, 32),
        0x0054_69c8 => (8000, 3500, 600, 48),
        0x0054_69d8 => (4000, 3500, 40, 48),
        _ => return Err(CameraSlewError::UnownedTable(address)),
    };
    Ok(SmoothingTable {
        maximum_acceleration,
        far_threshold,
        speed_squared: coefficient * coefficient,
        deadzone,
    })
}

fn squared_length(words: [i16; 3]) -> i32 {
    let [x, y, z] = words.map(i32::from);
    z.wrapping_mul(z)
        .wrapping_add(y.wrapping_mul(y))
        .wrapping_add(x.wrapping_mul(x))
}

fn divide(numerator: i32, denominator: i32) -> Result<i32, CameraSlewError> {
    numerator
        .checked_div(denominator)
        .ok_or(CameraSlewError::DivisionFault)
}

#[cfg(test)]
#[path = "parameters_tests.rs"]
mod tests;
