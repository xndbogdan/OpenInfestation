//! 00410BF0 desired-eye line clipping and 0041063A..71D floor clamp.

use inf_formats::native_basis::isqrt_48efc0;

use super::{CameraHeightRequest, CameraPoseError, CameraPoseHost};
use crate::object_runtime::actor::selector16::{CameraSlewError, Selector16State};

pub(super) fn clip_eye_to_terrain(
    eye: &mut [u16; 3],
    focus: [u16; 3],
    clearance: i32,
    host: &mut dyn CameraPoseHost,
) -> Result<(), CameraPoseError> {
    let differences =
        std::array::from_fn::<_, 3, _>(|axis| focus[axis].wrapping_sub(eye[axis]) as i16 as i32);
    let [dx, original_dy, dz] = differences;
    let dx_squared = dx.wrapping_mul(dx);
    let dz_squared = dz.wrapping_mul(dz);
    let length = isqrt_48efc0(
        dx_squared
            .wrapping_add(original_dy.wrapping_mul(original_dy))
            .wrapping_add(dz_squared),
    ) as i16 as i32;
    if length <= 128 {
        return Ok(());
    }
    let mut dy = original_dy;
    let mut distance = 128_i32;
    let x_step = dx.wrapping_mul(128);
    let z_step = dz.wrapping_mul(128);
    let mut x_offset = x_step;
    let mut z_offset = z_step;
    loop {
        let x = (focus[0] as i16 as i32).wrapping_sub(divide(x_offset, length)?);
        let z = (focus[2] as i16 as i32).wrapping_sub(divide(z_offset, length)?);
        let floor = i32::from(host.query_height(CameraHeightRequest::InterpolatedTerrain {
            xz: [x as u16, z as u16],
        })?)
        .wrapping_add(clearance);
        let focus_y = focus[1] as i16 as i32;
        let line_y =
            focus_y.wrapping_sub(divide((dy as i16 as i32).wrapping_mul(distance), length)?);
        if line_y < floor {
            dy = divide(focus_y.wrapping_sub(floor).wrapping_mul(length), distance)?;
        }
        x_offset = x_offset.wrapping_add(x_step);
        z_offset = z_offset.wrapping_add(z_step);
        distance = distance.wrapping_add(128);
        if distance >= length {
            break;
        }
    }
    if dy as i16 != original_dy as i16 {
        let dy = dy as i16 as i32;
        let adjusted_length = isqrt_48efc0(
            dx_squared
                .wrapping_add(dy.wrapping_mul(dy))
                .wrapping_add(dz_squared),
        ) as i16 as i32;
        for (axis, difference) in [dx, dy, dz].into_iter().enumerate() {
            eye[axis] = focus[axis]
                .wrapping_sub(divide(difference.wrapping_mul(length), adjusted_length)? as u16);
        }
    }
    Ok(())
}

pub(super) fn clamp_eye(
    state: &mut Selector16State,
    host: &mut dyn CameraPoseHost,
) -> Result<(), CameraPoseError> {
    // Both operands are signed desired Y words (native +14C/+170), and
    // the subtraction retains a full DWORD instead of wrapping to a WORD.
    let desired_eye = state
        .eye_138
        .desired_position
        .ok_or(CameraSlewError::UnwrittenTarget)?;
    let desired_look = state
        .look_15c
        .desired_position
        .ok_or(CameraSlewError::UnwrittenTarget)?;
    let gap = ((desired_eye[1] as i16 as i32 - desired_look[1] as i16 as i32).abs() + 32).min(80);
    let xz = [desired_eye[0], desired_eye[2]];
    let terrain_height = host.query_height(CameraHeightRequest::InterpolatedTerrain { xz })?;
    let water_height =
        host.query_height(CameraHeightRequest::WaterSurface { xz, terrain_height })?;
    let floor = i32::from(terrain_height.max(water_height)) + gap;
    let position = state
        .eye_138
        .current_position
        .as_mut()
        .ok_or(CameraSlewError::UnwrittenCurrent)?;
    if (position[1] as i16 as i32) < floor {
        position[1] = floor as u16;
        let velocity = state
            .eye_138
            .current_velocity
            .as_mut()
            .ok_or(CameraSlewError::UnwrittenCurrent)?;
        let y = velocity[1] as i16 as i32;
        if y < 0 {
            velocity[1] = -(y / 4) as u16;
        }
    }
    Ok(())
}

fn divide(numerator: i32, denominator: i32) -> Result<i32, CameraPoseError> {
    numerator
        .checked_div(denominator)
        .ok_or(CameraPoseError::DivisionFault)
}
