//! Opcode1A: 004A1D50/004A2F00 fog-gated sprite quads.
//!
//! Near/Far share lookup, packed size/angle, signed extent saturation,
//! Q31 corner expansion and the corner clip with1B. Both clip each corner
//! against the published viewport0055F234/38 (004A1F7E/004A1FC4). The Far
//! source-fogFF gate stays distinct from1B. Both ground packets reserve
//! three blocks; Far appends context+54 color and repeated source fog.
//!
//! Labeled `mode-from-scene-state`: retained scene mode supplies invocation+5C. Outer
//! 0049FEF0 copies drawing-context+70 at4A00AB..00B5. World00546EF8 is
//! image-zero; frontend00547558 is loader-zero and43B581..B587 copies it
//! into each local context. These establish initial/copy custody, not a
//! universal mode-setter policy. Keep the channel explicit and replaceable.
//! Required renderer+04 material sorting is owned; portable opaque texture
//! allocation ordering retains its separately documented approximation.

use inf_formats::ModelInstructionKind;

use super::{
    clip_or_skip,
    effect::{expanded_sprite_corners, screen_clip},
    textured_sort_depth, vertex_depth, vertex_fog, vertex_xy, PrimitiveCallback, PrimitiveCommand,
    PrimitiveError, PrimitiveHost, PrimitiveOutcome, PrimitiveState, PrimitiveTables,
};
use crate::{
    model_context::ModelDispatch,
    model_execution::ModelPrimitiveRequest,
    model_values::ModelValues,
    render_resources::{RenderResourceData, RenderResourceError},
};

pub fn prepare_ground<'a, H: PrimitiveHost<'a>>(
    request: &ModelPrimitiveRequest<'_>,
    state: PrimitiveState,
    tables: &PrimitiveTables,
    values: &mut ModelValues,
    host: &mut H,
) -> Result<PrimitiveOutcome, PrimitiveError<H::Error>> {
    let ModelInstructionKind::Opaque(command) = &request.instruction.kind else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    let [dest, slot, size_word, angle_word] = command.arguments.as_slice() else {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    };
    if request.instruction.raw_command as u8 != 0x1a {
        return Err(PrimitiveError::Record(request.instruction.span.start));
    }
    let record = request.instruction.span.start;
    // Native lookup precedes even the vertex reference (00405540
    // first): the H3 sprite bank resolves the slot; a null entry
    // faults retail.
    let texture = host
        .texture(i32::from(*slot as i16))
        .map_err(PrimitiveError::Host)?;
    let index =
        usize::try_from(i32::from(*dest as i16)).map_err(|_| PrimitiveError::Record(record))?;
    // Clip-40 gate; unprojected dynamic slots skip the face.
    let Some(clip) = clip_or_skip(request, index)? else {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    };
    let far = request.dispatch == ModelDispatch::Far;
    if clip & 0x40 != 0 || (far && vertex_fog(request, index)? == 255) {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    }
    let size = values
        .evaluate_packed(*size_word, host)
        .map_err(PrimitiveError::Value)?;
    let angle = values
        .evaluate_packed(*angle_word, host)
        .map_err(PrimitiveError::Value)?;
    let depth = vertex_depth(request, index)?;
    if (depth as i32) < 64 {
        // The native branch reads uninitialized stack extent words. Preserve
        // preceding lookup/callbacks and reject this inconsistent slot state.
        return Err(PrimitiveError::Unsupported {
            opcode: 0x1a,
            reason: "uninitialized extent after a depth below 64 without clip 0x40",
        });
    }
    let texture = texture.ok_or(PrimitiveError::Unsupported {
        opcode: 0x1a,
        reason: "a non-null sprite at the dimensions dereference",
    })?;
    let RenderResourceData::Sprite(entry) = texture.data else {
        return Err(RenderResourceError::Registration {
            request: texture.request,
        }
        .into());
    };
    let variant = entry
        .variants
        .first()
        .ok_or(RenderResourceError::SpriteDimensions {
            slot: texture.request.slot(),
        })?;
    if variant.width.max(variant.height) == 0 {
        return Err(PrimitiveError::Unsupported {
            opcode: 0x1a,
            reason: "nonzero sprite dimensions at native integer division",
        });
    }
    let corners = expanded_sprite_corners(
        vertex_xy(request, index)?,
        depth as i32,
        [size, angle],
        [variant.width, variant.height],
        state.projection.focal,
        &tables.sine_table,
    );
    // The same 0055F234/38 viewport codes as1B: unsigned below the bound is
    // inside (2/0x10), otherwise negative (1/8) or beyond (4/0x20).
    let clip = corners.iter().fold(0, |combined, xy| {
        combined | screen_clip(*xy, state.projection.viewport)
    });
    if tables.clip_acceptance[usize::from(clip)] == 0 {
        return Ok(PrimitiveOutcome {
            commands: 0,
            rejected: 1,
        });
    }
    let key = textured_sort_depth(depth, state.mode, Some(texture), host)?;
    let mut output = PrimitiveCommand::new(
        if far {
            PrimitiveCallback::TexturedQuadFar
        } else {
            PrimitiveCallback::TexturedQuadNear
        },
        key,
        3,
    );
    for (corner, xy) in corners.into_iter().enumerate() {
        output.write_u32(corner * 8, xy);
        output.write_u32(corner * 8 + 4, depth);
    }
    output.write_u32(0x20, state.mode);
    output.write_texture(0x24, Some(texture));
    if far {
        output.write_u32(0x28, state.far_color);
        let fog = vertex_fog(request, index)?;
        for offset in 0x2c..0x30 {
            output.write_byte(offset, fog);
        }
    }
    host.submit(output).map_err(PrimitiveError::Host)?;
    Ok(PrimitiveOutcome {
        commands: 1,
        rejected: 0,
    })
}

#[cfg(test)]
#[path = "ground_tests.rs"]
mod tests;
