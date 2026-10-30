//! Bounded root-identity policy for native 004A0CE0 view-plane branches.

use serde::Serialize;

use crate::{ByteSpan, ModelEntry, ModelInstruction, ModelInstructionKind};

/// One diagnostic submission. Explicit inspection state never changes the
/// intrinsic entry or the unconditioned asset census.
#[derive(Clone, Copy, Debug)]
pub struct ModelDiagnosticRequest<'a> {
    pub entry: &'a ModelEntry,
    pub view: ModelDiagnosticView<'a>,
}

impl<'a> ModelDiagnosticRequest<'a> {
    pub const fn unconditioned(entry: &'a ModelEntry) -> Self {
        Self {
            entry,
            view: ModelDiagnosticView::Unconditioned,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ModelDiagnosticView<'a> {
    Unconditioned,
    /// A diagnostic eye in the root model's coordinate system, with the root
    /// explicitly placed at zero under an identity basis. This is not a retail
    /// camera observation. `source` must be the OVL bytes owning `entry` spans.
    ExplicitEye {
        source: &'a [u8],
        eye_root: [i32; 3],
        /// Low BYTE[0055F250]; native shifts use its low five bits.
        coordinate_shift: u8,
    },
}

/// Literal source and arithmetic custody for one resolved 0B decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelDiagnosticViewPlaneDecision {
    pub instruction_span: ByteSpan,
    pub vector_source_span: ByteSpan,
    pub vertex_slot: i16,
    pub vector_index: i16,
    pub raw_vector: [i16; 3],
    pub effective_vector: [i32; 3],
    pub world_point: [i32; 3],
    pub eye_root: [i32; 3],
    pub coordinate_shift: u8,
    pub dot_product: i32,
    pub jump_taken: bool,
}

pub(super) fn supports_root_identity(entry: &ModelEntry) -> bool {
    // These commands can change current/saved transforms or enter another
    // invocation. The static diagnostic's approximate nested rotation math
    // is not authority for a native branch decision. Reject the entire entry
    // for this first bounded policy, including conditionally skipped changes.
    entry.program.instructions.iter().all(|instruction| {
        !matches!(
            instruction.raw_command as u8,
            0x02..=0x05 | 0x09 | 0x0e..=0x10 | 0x15 | 0x1c | 0xee | 0xef
        )
    })
}

pub(super) fn evaluate(
    entry: &ModelEntry,
    instruction: &ModelInstruction,
    view: ModelDiagnosticView<'_>,
    world_slots: &[Option<[i32; 3]>],
) -> Option<ModelDiagnosticViewPlaneDecision> {
    let ModelDiagnosticView::ExplicitEye {
        source,
        eye_root,
        coordinate_shift,
    } = view
    else {
        return None;
    };
    let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
        return None;
    };
    let [_, vertex_slot, vector_index] = command.arguments.as_slice() else {
        return None;
    };
    let vertex_slot = *vertex_slot as i16;
    let vector_index = *vector_index as i16;
    let world_point = *world_slots
        .get(usize::try_from(vertex_slot).ok()?)?
        .as_ref()?;

    // 004A0188 publishes the raw normal-pool pointer. Unlike lighting,
    // 004A0CFC..004A0D0F uses signed index*6, with no /2 or -6 correction.
    // Retail zone signs therefore read their own command bytes. Permit that
    // literal in-entry access, while containing accesses outside this entry.
    let start = entry
        .normals_span
        .start
        .checked_add_signed(isize::from(vector_index) * 6)?;
    let end = start.checked_add(6)?;
    if start < entry.content_span.start || end > entry.content_span.end {
        return None;
    }
    let bytes = source.get(start..end)?;
    let raw_vector =
        std::array::from_fn(|axis| i16::from_le_bytes([bytes[axis * 2], bytes[axis * 2 + 1]]));
    let effective_vector = shifted_vector(raw_vector, coordinate_shift);
    // 00490900 with the explicitly proven identity basis leaves these signed
    // integers unchanged. Each is exact in x87 and the nearest-even FISTP.
    let dot_product = native_dot(world_point, eye_root, effective_vector);
    let command_high = i32::from(instruction.raw_command as i16) >> 8;
    Some(ModelDiagnosticViewPlaneDecision {
        instruction_span: instruction.span,
        vector_source_span: ByteSpan { start, end },
        vertex_slot,
        vector_index,
        raw_vector,
        effective_vector,
        world_point,
        eye_root,
        coordinate_shift,
        dot_product,
        jump_taken: i32::from(dot_product < 0) != command_high,
    })
}

fn shifted_vector(vector: [i16; 3], shift: u8) -> [i32; 3] {
    let shift = u32::from(shift & 31);
    vector.map(|value| i32::from(value).wrapping_shl(shift) >> shift)
}

fn native_dot(point: [i32; 3], eye: [i32; 3], vector: [i32; 3]) -> i32 {
    let products: [i32; 3] = std::array::from_fn(|axis| {
        point[axis]
            .wrapping_sub(eye[axis])
            .wrapping_mul(vector[axis])
    });
    products[2]
        .wrapping_add(products[1])
        .wrapping_add(products[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_matches_original_004a0ce0_fixtures() {
        // Executed unchanged PE 004A0CE0 + 00490900 with synthetic identity
        // state, x87 027F. The untracked oracle also checked 4096 random cases.
        let cases = [
            (
                [-128, 256, 0],
                [-128, 256, 1024],
                [2, 226, 1372],
                16,
                [2, 226, 1372],
                -1_404_928,
            ),
            (
                [-128, 256, 0],
                [-128, 256, -1024],
                [2, 226, 1371],
                0,
                [2, 226, 1371],
                1_403_904,
            ),
            (
                [-128, 256, 0],
                [-128, 256, 0],
                [2, 226, 1370],
                0,
                [2, 226, 1370],
                0,
            ),
            (
                [i32::MAX, i32::MIN, 12],
                [i32::MIN, i32::MAX, i32::MIN],
                [-32768, 32767, -17],
                0,
                [-32768, 32767, -17],
                -2_147_418_317,
            ),
            (
                [16_777_217, -33_554_435, 67_108_869],
                [-100_000_003, 123_456_789, -200_000_007],
                [32767, -32768, 23456],
                20,
                [-1, 0, -1120],
                1_368_992_380,
            ),
            ([1, 2, 3], [4, 5, 6], [1, 2, 3], 31, [-1, 0, -1], 6),
            ([1, 2, 3], [4, 5, 6], [1, 2, 3], 255, [-1, 0, -1], 6),
            ([1, 2, 3], [4, 5, 6], [1, 2, 3], 32, [1, 2, 3], -18),
        ];
        for (point, eye, raw, shift, vector, dot) in cases {
            assert_eq!(shifted_vector(raw, shift), vector);
            assert_eq!(native_dot(point, eye, vector), dot);
        }
    }
}
