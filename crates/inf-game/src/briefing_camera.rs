//! Briefing screen's WORD-vector movement in 0040C770, independent of the
//! world's selector-16 camera. Kind 8 in 0040CE40 changes the target X/Z;
//! constructor 0040A420 clears both vectors, while Detail retains them.

use inf_formats::{
    native_basis::{isqrt_48efc0, NativeBasis},
    BriefingCamera,
};

use crate::{
    menu_basis::MenuBasisMath,
    object_runtime::{ObjectArena, ObjectError, ObjectList},
    terrain_runtime::{TerrainHeightScale, TerrainRuntime},
};

/// Sources read by C770 and kind 8. Lookup uses current G+10 membership,
/// not the separate G+48 actor list or the live-world camera's eye.
#[derive(Clone, Copy)]
pub struct BriefingCameraInputs<'a> {
    pub world: BriefingCameraWorld<'a>,
    pub math: &'a MenuBasisMath,
}

#[derive(Clone, Copy)]
pub struct BriefingCameraWorld<'a> {
    pub arena: &'a ObjectArena,
    pub terrain: &'a TerrainRuntime,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BriefingCameraState {
    /// Screen +160/+162/+164, wrapping world WORDs.
    current: [u16; 3],
    /// Screen +166/+16A. Kind 8 does not write the intervening target Y.
    target_xz: [u16; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct BriefingCameraTick {
    pub record: BriefingCamera,
    pub heading: i32,
    pub delta_micros: i32,
    pub map_scale_word: u16,
    pub terrain_camera_disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingCameraMotion {
    /// C770 takes the special decrement-to-0x10 map-scale branch.
    pub retreat_map_scale: bool,
    pub terrain_camera_disabled: bool,
    pub restart_pulse: bool,
    pub distance: u32,
}

impl BriefingCameraState {
    pub fn current_words(&self) -> [u16; 3] {
        self.current
    }

    pub fn target_xz_words(&self) -> [u16; 2] {
        self.target_xz
    }

    /// 0040D0CC..0040D125: reference zero and a missing match retain the
    /// preceding target. Comparison sign-extends +2A but uses a DWORD ID.
    pub fn activate(&mut self, reference: u32, arena: &ObjectArena) -> Result<(), ObjectError> {
        if let Some(target) = find_target(reference, arena)? {
            self.target_xz = target;
        }
        Ok(())
    }

    /// Port option, not retail: [`Self::activate`], except that a
    /// reference made by [`stand_off_reference`] targets a point that far
    /// back from its actor against `heading`, so the eye frames the actor
    /// from in front instead of sitting inside it. Other references are
    /// retail's.
    pub fn activate_standing_off(
        &mut self,
        reference: u32,
        heading: i32,
        math: &MenuBasisMath,
        arena: &ObjectArena,
    ) -> Result<(), ObjectError> {
        if reference & STAND_OFF_FLAG == 0 {
            return self.activate(reference, arena);
        }
        let distance = f64::from((reference >> 16) & 0x3fff) * 100.0;
        if let Some([x, z]) = find_target(reference & 0xffff, arena)? {
            let mut basis = NativeBasis::INITIAL;
            math.rotate(&mut basis, heading);
            let [forward_x, forward_z] =
                [7, 9].map(|index| f64::from(f32::from_bits(basis.words[index])));
            self.target_xz = [
                x.wrapping_sub((forward_x * distance) as i64 as u16),
                z.wrapping_sub((forward_z * distance) as i64 as u16),
            ];
        }
        Ok(())
    }

    pub fn tick(
        &mut self,
        request: BriefingCameraTick,
        inputs: BriefingCameraInputs<'_>,
    ) -> BriefingCameraMotion {
        // 0040C7E3..0040C85C copies the shared identity basis, rotates X/Z,
        // then __ftol truncates its stored forward components times 1200.0
        // (00520390). These offsets sample stored cells, not interpolation.
        let mut basis = NativeBasis::INITIAL;
        inputs.math.rotate(&mut basis, request.heading);
        let offset = [7, 9].map(|index| {
            (f64::from(f32::from_bits(basis.words[index])) * 1200.0).trunc() as i64 as u16
        });
        let [x, y, z] = self.current;
        let ground = stored_height(inputs.world.terrain, [x, z]).max(stored_height(
            inputs.world.terrain,
            [x.wrapping_add(offset[0]), z.wrapping_add(offset[1])],
        ));
        let dx = self.target_xz[0].wrapping_sub(x) as i16;
        let dz = self.target_xz[1].wrapping_sub(z) as i16;
        let dy = y.wrapping_sub(ground as u16).wrapping_sub(400) as i16;
        let difference = [dx, dy, dz].map(i32::from);
        let square = difference.into_iter().fold(0i32, |sum, value| {
            sum.wrapping_add(value.wrapping_mul(value))
        });
        let distance = isqrt_48efc0(square);
        let retreat_map_scale = request.record.track_byte == 0 && distance > 2000;
        let mut terrain_camera_disabled = request.terrain_camera_disabled;
        let mut restart_pulse = false;
        if request.record.terrain_byte == 0 {
            if distance > 1000 {
                terrain_camera_disabled = true;
            } else if terrain_camera_disabled {
                terrain_camera_disabled = false;
                restart_pulse = true;
            }
        } else {
            terrain_camera_disabled = false;
        }
        // Speed is an unsigned WORD; differences wrap and sign-extend.
        let limit = (u32::from(request.record.speed_word) * 4).min(distance) as i32;
        let scaled = [dx, dz].map(|value| {
            if distance == 0 {
                value
            } else {
                (i32::from(value).wrapping_mul(limit) / distance as i32) as i16
            }
        });
        let steps = std::array::from_fn::<_, 2, _>(|index| {
            let step = scaled_step(scaled[index], request.delta_micros);
            let original = [dx, dz][index];
            if original != 0 && step == 0 {
                if original > 0 {
                    1
                } else {
                    -1
                }
            } else {
                step
            }
        });
        if !retreat_map_scale || request.map_scale_word < 0x2000 {
            self.current[0] = x.wrapping_add(steps[0] as u16);
            self.current[2] = z.wrapping_add(steps[1] as u16);
        }
        // Y uses the uncapped error and has no forced minimum step.
        if !(-100..=100).contains(&dy) {
            self.current[1] = y.wrapping_sub(scaled_step(dy, request.delta_micros) as u16);
        }
        BriefingCameraMotion {
            retreat_map_scale,
            terrain_camera_disabled,
            restart_pulse,
            distance,
        }
    }
}

/// Port option, not retail: the reference bit asking
/// [`BriefingCameraState::activate_standing_off`] to stand back from its
/// actor. Retail's references are placement numbers plus one.
pub const STAND_OFF_FLAG: u32 = 0x4000_0000;

/// A camera reference to `actor` (a placement number plus one, as retail's)
/// whose eye stands `distance` world units back from it, in hundreds.
pub const fn stand_off_reference(actor: u16, distance: u32) -> u32 {
    STAND_OFF_FLAG | ((distance / 100) & 0x3fff) << 16 | actor as u32
}

/// The X/Z of the Flag-0 object whose placement is `reference` minus one;
/// None for reference zero or no match.
fn find_target(reference: u32, arena: &ObjectArena) -> Result<Option<[u16; 2]>, ObjectError> {
    if reference == 0 {
        return Ok(None);
    }
    let wanted = reference.wrapping_sub(1) as i32;
    for node in arena.list(ObjectList::Flag0Primary) {
        let object = arena.object(node.object)?;
        if i32::from(object.base.placement_word as i16) == wanted {
            let [x, _, z] = object
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(node.object))?;
            return Ok(Some([x, z]));
        }
    }
    Ok(None)
}

/// C770 reads the aligned signed height byte and signed low WORD plane,
/// retaining the higher of them. Its forward sample applies the same rule.
fn stored_height(terrain: &TerrainRuntime, xz: [u16; 2]) -> i16 {
    let shift = match terrain.sampling().height_scale {
        TerrainHeightScale::Units16 => 4,
        TerrainHeightScale::Units32 => 5,
    };
    let cell = (i32::from(terrain.cell_at(xz).height_i8()) << shift) as i16;
    let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
    cell.max(plane)
}

/// 0040CA57/0040CB00 IMUL, SHL/RCL: signed product shifted by 31, low WORD.
fn scaled_step(error: i16, delta_micros: i32) -> i16 {
    ((i64::from(error) * i64::from(delta_micros.wrapping_shl(12))) >> 31) as i16
}

#[cfg(test)]
#[path = "briefing_camera_tests.rs"]
mod tests;
