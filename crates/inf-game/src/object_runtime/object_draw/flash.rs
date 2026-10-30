//! Complete actor flash 0047DEA0/0047DFA0. Saved values belong to actor
//! +B8/+BC/+C0, not a draw-local rollback. Only the reached H7 host owns
//! signed-index/resource custody and native display conversion. Context+6C
//! changes here do not publish a persistent renderer FarColor command.

use super::{
    unowned, Flag0ObjectDrawError, Flag0ObjectDrawHost, ObjectDrawCallbackBoundary,
    ObjectDrawContext, ObjectDrawServiceError,
};
use crate::{
    frontend_scene::{SceneCommand, SceneFade},
    object_runtime::{actor::native_trunc_low32, ObjectArena, ObjectId},
};

#[cfg(test)]
mod tests;

pub(super) fn enter<T, H: Flag0ObjectDrawHost>(
    arena: &mut ObjectArena,
    actor: ObjectId,
    context: &mut ObjectDrawContext,
    intensity: i32,
    color: i32,
    host: &mut H,
) -> Result<(), Flag0ObjectDrawError<T, H::Error>> {
    let position = arena.draw_position(actor)?;
    let delta: [i32; 3] = std::array::from_fn(|axis| {
        i32::from(position[axis].wrapping_sub(context.view.eye[axis] as u16) as i16)
    });
    let words = context.view.basis.words;
    let coefficient = |axis: usize| f64::from(f32::from_bits(words[7 + axis]));
    //47DEC4..DEEF: wrapped WORD deltas, Z product then Y, then X;
    //CRT truncating QWORD conversion supplies the low DWORD.
    let depth = native_trunc_low32(
        (f64::from(delta[2]) * coefficient(2) + f64::from(delta[1]) * coefficient(1))
            + f64::from(delta[0]) * coefficient(0),
    );
    {
        let state = arena.actor_mut(actor)?;
        state.flash_near_bc = Some(context.near); //47DEF9
        state.flash_far_c0 = Some(context.far); //47DF02
        state.flash_far_color_b8 = Some(context.far_color); //47DF0D
    }
    let threshold = context.far.wrapping_mul(3).wrapping_sub(context.near) / 4;
    if depth >= threshold {
        return Ok(()); //Saved fields remain written; no extent/H7 publication.
    }

    let diameter = i32::from(arena.object(actor)?.base.word_28) * 2;
    let far_color = match host.material_color(color) {
        Ok(value) => value,
        Err(ObjectDrawServiceError::UnownedCallback(reason)) => {
            return Err(unowned(actor, reason));
        }
        Err(ObjectDrawServiceError::Backend(error)) => {
            return Err(Flag0ObjectDrawError::Host(error));
        }
    };
    //47DF69..DF78 sign extend BOTH the supplied intensity LOW WORD and the
    //LOW WORD after wrapped DWORD multiplication/shift. Neither is a clamp.
    let correction = (i32::from(intensity as i16)
        .wrapping_mul(diameter)
        .wrapping_shl(1)
        >> 15) as i16 as i32;
    context.far_color = far_color; //47DF7B, caller+6C only.
    context.far = diameter.wrapping_add(depth); //47DF87
    context.near = diameter.wrapping_sub(correction).wrapping_add(depth); //47DF8A
    host.scene(SceneCommand::PublishFade(SceneFade::new([
        context.near,
        context.far,
    ])))
    .map_err(Flag0ObjectDrawError::Host)
}

pub(super) fn restore<T, H: Flag0ObjectDrawHost>(
    arena: &ObjectArena,
    actor: ObjectId,
    context: &mut ObjectDrawContext,
    host: &mut H,
) -> Result<(), Flag0ObjectDrawError<T, H::Error>> {
    let required = |offset| -> Flag0ObjectDrawError<T, H::Error> {
        unowned(
            actor,
            ObjectDrawCallbackBoundary::ActorFlashField { offset },
        )
    };
    //47DFA4..DFBC: each read precedes its individual caller store. A later
    //required-field stop preserves earlier stores, with no rollback/finally.
    context.far_color = arena
        .actor(actor)?
        .flash_far_color_b8
        .ok_or_else(|| required(0xb8))?;
    context.near = arena
        .actor(actor)?
        .flash_near_bc
        .ok_or_else(|| required(0xbc))?;
    context.far = arena
        .actor(actor)?
        .flash_far_c0
        .ok_or_else(|| required(0xc0))?;
    host.scene(SceneCommand::PublishFade(SceneFade::new([
        context.near,
        context.far,
    ])))
    .map_err(Flag0ObjectDrawError::Host)
}
