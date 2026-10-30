//! Briefing map backdrop at 0040C390 → 00471F70.
//!
//! This caller uses original H3 effect tiles and 00472D30 markers, not the
//! separate indexed-terrain/00471D40 palette renderer at 00470170.

use crate::{
    briefing_markers::BriefingMarkerTile,
    briefing_runtime::{BriefingPanel, BriefingPanelAnimation},
    briefing_scene::{panel_center_pixels, panel_visible, BriefingSceneFrame},
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
};

#[derive(Clone, Copy, Debug)]
pub struct BriefingMapInputs {
    pub frame: BriefingSceneFrame,
    pub animation: BriefingPanelAnimation,
    pub map_scale_word: u16,
    /// Screen +160/+164, not the selector-16 camera actor's position.
    pub camera_xz_words: [u16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingMapBackdrop {
    pub clip_rect: [i16; 4],
    /// 00471F70 interleaves 00472D30 marker calls after each backdrop tile.
    pub tiles: [BriefingMapTile; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingMapTile {
    pub backdrop: DirectSpriteQuad,
    /// Keep the original DWORDs independently of sprite-corner WORD stores.
    pub markers: BriefingMarkerTile,
}

/// Original signed-DWORD arithmetic and unsigned camera WORD thresholds.
/// The viewport clip shrinks only at stage0 opening and stage1→0 closing.
pub fn briefing_map_backdrop(
    inputs: BriefingMapInputs,
    style_word: u32,
) -> Option<BriefingMapBackdrop> {
    let state = inputs.animation;
    if !panel_visible(state) {
        return None;
    }
    let [width, _] = inputs.frame.projection.viewport;
    let mut clip_width = width.wrapping_mul(45) / 100;
    if state.current_stage == 0 {
        let percent = state.duration_micros.wrapping_sub(state.remaining_micros)
            / 10
            / (state.duration_micros / 1000);
        clip_width = percent.wrapping_mul(clip_width) / 100;
    } else if state.current_stage == 1 && state.requested_stage == 0 {
        let percent = state.duration_micros.wrapping_sub(state.remaining_micros)
            / 10
            / (state.duration_micros / 1000);
        clip_width = 100i32.wrapping_sub(percent).wrapping_mul(clip_width) / 100;
    }
    let [fx, fy] = inputs.frame.projection.focal;
    let clip_height = fy.wrapping_mul(clip_width) / fx;
    let [cx, cy] =
        panel_center_pixels(BriefingPanel::Map, inputs.frame.projection.viewport).map(i32::from);
    let clip_rect = [
        cx - clip_width / 2,
        cy - clip_height / 2,
        cx + clip_width / 2,
        cy + clip_height / 2,
    ]
    .map(|v| v as i16);
    let size =
        width.wrapping_mul(((u32::from(inputs.map_scale_word) * 100) >> 16) as i32 + 40) / 100;
    let tiles = map_tiles([cx, cy], [size, size], inputs.camera_xz_words, style_word);
    Some(BriefingMapBackdrop { clip_rect, tiles })
}

/// `00471F70`'s four wrapped tiles about `center`: each is an H3 `0x5B7`
/// effect quad in the caller's H6 `0x3B` word, at depth `0x8000` with
/// descriptor `0x100`, followed by its `00472D30` marker pass. The X origin
/// uses the first size and the Y origin the second; the camera WORDs are
/// unsigned, their products wrapping DWORDs. Its sixth argument is unread.
pub fn map_tiles(
    center: [i32; 2],
    size: [i32; 2],
    camera_xz_words: [u16; 2],
    style_word: u32,
) -> [BriefingMapTile; 4] {
    let [cx, cy] = center;
    let [width, height] = size;
    let [camera_x, camera_z] = camera_xz_words;
    let mut x = cx.wrapping_sub(i32::from(camera_x).wrapping_mul(width) >> 16);
    let mut y = (i32::from(camera_z).wrapping_mul(height) >> 16)
        .wrapping_sub(height)
        .wrapping_add(cy);
    let tile = |x: i32, y: i32| BriefingMapTile {
        markers: BriefingMarkerTile {
            origin: [x, y],
            size,
        },
        backdrop: DirectSpriteQuad {
            sprite_slot: 0x5b7,
            corners: [
                [x, y],
                [x.wrapping_add(width), y],
                [x.wrapping_add(width), y.wrapping_add(height)],
                [x, y.wrapping_add(height)],
            ]
            .map(|p| p.map(|v| v as i16)),
            vertex_depths: [0x8000; 4],
            descriptor_word: 0x100,
            mode: DirectSpriteMode::Effect {
                color_word: style_word,
            },
        },
    };
    let mut tiles = [tile(x, y); 4];
    for row in 0..2 {
        tiles[row * 2] = tile(x, y);
        x = if camera_x < 0x7fff {
            x.wrapping_sub(width)
        } else {
            x.wrapping_add(width)
        };
        tiles[row * 2 + 1] = tile(x, y);
        x = if camera_x < 0x7fff {
            x.wrapping_add(width)
        } else {
            x.wrapping_sub(width)
        };
        y = if camera_z > 0x7fff {
            y.wrapping_sub(height)
        } else {
            y.wrapping_add(height)
        };
    }
    tiles
}

#[cfg(test)]
#[path = "briefing_map_tests.rs"]
mod tests;
