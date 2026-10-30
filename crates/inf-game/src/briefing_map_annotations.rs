//! Independent map annotation tail of 00472D30, after the ordinary actor pass.
//! Selector 87 owns labels/radius/RGB; these inputs retain one wrapped tile.

use thiserror::Error;

use crate::{
    briefing_markers::BriefingMarkerTile,
    object_runtime::{ObjectArena, ObjectError, ObjectId},
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingMapAnnotation {
    pub position_words: Option<[u16; 3]>,
    pub label: u32,
    pub radius: u16,
    pub rgb: [u8; 3],
}

impl BriefingMapAnnotation {
    pub fn from_arena(arena: &ObjectArena, id: ObjectId) -> Result<Self, ObjectError> {
        let annotation = arena.map_annotation(id)?;
        Ok(Self {
            position_words: arena.object(id)?.base.position_words,
            label: annotation.label,
            radius: annotation.radius,
            rgb: annotation.rgb,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingAnnotationInputs {
    pub tile: BriefingMarkerTile,
    /// C390 supplies screen age in milliseconds shifted left by two.
    pub scan_age: i32,
    /// High byte of native final argument, preserved when radius RGB is copied.
    /// The phase-3 caller supplies a zero DWORD, independently of text styles.
    pub radius_high_byte: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BriefingAnnotationDraw {
    Radius(DirectSpriteQuad),
    Label {
        /// Native "%d" reads the entire DWORD; command 2 writes a zero-extended WORD.
        label: i32,
        position_pixels: [i32; 2],
        color_word: u32,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BriefingAnnotationError<E> {
    #[error("map annotation has unknown object position WORDs")]
    MissingPosition,
    #[error("map annotation style lookup failed: {0}")]
    Resource(E),
}

/// Plan one annotation. The enclosing pass resolves H4/0 and H6/0 first,
/// even for an empty list. Only a nonzero label then consumes H6/24.
/// Submit this result before advancing the retained annotation list.
pub fn briefing_map_annotation<E>(
    annotation: BriefingMapAnnotation,
    inputs: BriefingAnnotationInputs,
    mut label_style: impl FnMut() -> Result<u32, E>,
) -> Result<BriefingAnnotationDraw, BriefingAnnotationError<E>> {
    let [x, _, z] = annotation
        .position_words
        .ok_or(BriefingAnnotationError::MissingPosition)?;
    let tile = inputs.tile;
    let projected = |word: u16, size: i32| {
        (i32::from(word) / 2).wrapping_mul(size).wrapping_shr(15) as i16 as i32
    };
    let center = [
        tile.origin[0].wrapping_add(projected(x, tile.size[0])),
        tile.origin[1]
            .wrapping_sub(projected(z, tile.size[1]))
            .wrapping_add(tile.size[1]),
    ];
    if annotation.label != 0 {
        let scan = i32::from(z)
            .wrapping_sub(inputs.scan_age.wrapping_shl(6))
            .wrapping_sub(0x8000)
            & 0xffff;
        let style = label_style().map_err(BriefingAnnotationError::Resource)?;
        let rgb = std::array::from_fn::<_, 3, _>(|channel| {
            (((style >> (channel * 8)) & 0xff) * scan as u32 >> 15) as u8
        });
        return Ok(BriefingAnnotationDraw::Label {
            label: annotation.label as i32,
            position_pixels: center,
            color_word: (style & 0xff00_0000)
                | u32::from(rgb[0])
                | (u32::from(rgb[1]) << 8)
                | (u32::from(rgb[2]) << 16),
        });
    }
    // 4733FE wraps the DWORD product before signed division by 40000.
    // 4736E0 consumes only the low WORDs of center and radius.
    let radius = i32::from(annotation.radius).wrapping_mul(tile.size[1]) / 40000;
    let rgb = annotation.rgb;
    Ok(BriefingAnnotationDraw::Radius(DirectSpriteQuad {
        sprite_slot: 7,
        corners: [
            [
                center[0].wrapping_sub(radius),
                center[1].wrapping_sub(radius),
            ],
            [
                center[0].wrapping_add(radius),
                center[1].wrapping_sub(radius),
            ],
            [
                center[0].wrapping_add(radius),
                center[1].wrapping_add(radius),
            ],
            [
                center[0].wrapping_sub(radius),
                center[1].wrapping_add(radius),
            ],
        ]
        .map(|pair| pair.map(|value| value as i16)),
        vertex_depths: [64; 4],
        descriptor_word: 0,
        mode: DirectSpriteMode::Effect {
            color_word: u32::from(rgb[0])
                | (u32::from(rgb[1]) << 8)
                | (u32::from(rgb[2]) << 16)
                | (u32::from(inputs.radius_high_byte) << 24),
        },
    }))
}

#[cfg(test)]
#[path = "briefing_map_annotations_tests.rs"]
mod tests;
