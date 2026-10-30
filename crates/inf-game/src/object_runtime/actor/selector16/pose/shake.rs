//! Process camera jitter tail 00410966..00410B38; shared RNG order is X/Z/Y.

use inf_formats::native_basis::NativeBasis;

use super::{CameraPoseError, CameraPoseTickRequest};
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::actor::selector16::Selector16State;

pub(super) fn advance(
    state: &Selector16State,
    position: &mut [u16; 3],
    basis: &mut NativeBasis,
    request: &mut CameraPoseTickRequest<'_>,
) -> Result<(), CameraPoseError> {
    let globals = &mut *request.globals;
    let delta = request.microseconds;
    if request.shake_enabled == 0 {
        globals.shake_remaining_micros = 5_000_000;
        globals.shake_pause_micros = 0;
        return Ok(());
    }
    if globals.shake_remaining_micros != 0 {
        if (delta as i32) < (globals.shake_remaining_micros as i32) {
            globals.shake_remaining_micros = globals.shake_remaining_micros.wrapping_sub(delta);
        } else {
            globals.shake_remaining_micros = 0;
            globals.shake_pause_micros =
                (name_entry_rng_next(request.rng).wrapping_add(0x7a12)).wrapping_mul(16);
        }
    } else {
        if (delta as i32) < (globals.shake_pause_micros as i32) {
            globals.shake_pause_micros = globals.shake_pause_micros.wrapping_sub(delta);
        } else {
            globals.shake_pause_micros = 0;
            globals.shake_remaining_micros =
                (name_entry_rng_next(request.rng).wrapping_add(0x1312d)).wrapping_mul(64);
        }
        if (globals.shake_step_micros as i32) < (delta as i32) {
            if state.mode_188 == 2 {
                for axis in [0, 2, 1] {
                    let amount =
                        1024_u16.wrapping_sub((name_entry_rng_next(request.rng) as u16) >> 5);
                    globals.rotation_jitter[axis] =
                        globals.rotation_jitter[axis].wrapping_add(amount);
                }
            } else {
                for axis in [0, 2, 1] {
                    let amount =
                        32_u16.wrapping_sub((name_entry_rng_next(request.rng) as u16) >> 10);
                    position[axis] = position[axis].wrapping_add(amount);
                }
            }
            globals.shake_step_micros = 50_000;
        } else {
            globals.shake_step_micros = globals.shake_step_micros.wrapping_sub(delta);
        }
    }
    if state.mode_188 == 2 {
        for word in &mut globals.rotation_jitter {
            *word = ((i32::from(*word as i16) * 15) / 16) as u16;
        }
        request
            .host
            .rotate_local_vector(basis, globals.rotation_jitter)?;
    }
    Ok(())
}
