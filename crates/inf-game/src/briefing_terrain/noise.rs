//! 0040AF50's streaming static strips, shared by the normal pulse and
//! 0040AAC0's disabled-camera pane. Resolve each selected original H3 width
//! before advancing the second RNG draw, then submit before the next strip.

use thiserror::Error;

use crate::{
    name_entry::name_entry_rng_next,
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BriefingNoiseRows {
    /// 0040AAC0 passes -1: visit every row, with no row-selection RNG.
    All,
    /// The world-view branch passes unsigned screen +1CC divided by 2000.
    Random { count: u32 },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BriefingNoiseError<E> {
    #[error("camera-static sprite width lookup failed: {0}")]
    Resource(E),
    #[error("camera-static sprite {slot} has zero width; native traversal cannot advance")]
    ZeroWidth { slot: u16 },
}

/// Screen WORD boundaries retain the native exclusive right/bottom loop
/// gates. The renderer's clip separately uses inclusive edges.
pub struct BriefingNoisePass {
    clip: [i16; 4],
    rows: BriefingNoiseRows,
    row_index: i32,
    row_count: i32,
    current_row: Option<i32>,
    x: i32,
}

impl BriefingNoisePass {
    pub fn new(clip: [i16; 4], rows: BriefingNoiseRows) -> Self {
        Self {
            clip,
            rows,
            row_index: 0,
            row_count: match rows {
                BriefingNoiseRows::All => i32::from(clip[3]) - i32::from(clip[1]),
                BriefingNoiseRows::Random { count } => count as i32,
            },
            current_row: None,
            x: i32::from(clip[0]),
        }
    }

    pub fn next<E>(
        &mut self,
        rng: &mut u32,
        mut sprite_width: impl FnMut(u16) -> Result<u16, E>,
    ) -> Result<Option<DirectSpriteQuad>, BriefingNoiseError<E>> {
        loop {
            if self.current_row.is_none() {
                if self.row_index >= self.row_count {
                    return Ok(None);
                }
                let top = i32::from(self.clip[1]);
                self.current_row = Some(match self.rows {
                    BriefingNoiseRows::All => top.wrapping_add(self.row_index),
                    BriefingNoiseRows::Random { .. } => {
                        let draw = name_entry_rng_next(rng) as i32;
                        let height = i32::from(self.clip[3]) - top;
                        top.wrapping_add(draw.wrapping_mul(height) >> 16)
                    }
                });
                self.x = i32::from(self.clip[0]);
            }
            if self.x >= i32::from(self.clip[2]) {
                self.current_row = None;
                self.row_index = self.row_index.wrapping_add(1);
                continue;
            }
            let slot = 0xbf + ((name_entry_rng_next(rng) >> 4) & 3) as u16;
            let width = sprite_width(slot).map_err(BriefingNoiseError::Resource)?;
            if width == 0 {
                return Err(BriefingNoiseError::ZeroWidth { slot });
            }
            let forward = name_entry_rng_next(rng) & 0x10 != 0;
            let right = self.x.wrapping_add(i32::from(width));
            let [first, second] = if forward {
                [self.x, right]
            } else {
                [right, self.x]
            };
            let y = self.current_row.expect("row selected before strip");
            let quad = DirectSpriteQuad {
                sprite_slot: slot,
                corners: [[first, y], [second, y], [second, y + 1], [first, y + 1]]
                    .map(|pair| pair.map(|word| word as i16)),
                vertex_depths: [0x8001; 4],
                descriptor_word: 0,
                mode: DirectSpriteMode::Textured,
            };
            self.x = right;
            return Ok(Some(quad));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_rows_use_authored_width_and_keep_the_last_strip_unclipped() {
        let mut rng = 0;
        let mut expected = rng;
        let mut pass = BriefingNoisePass::new([10, 20, 16, 22], BriefingNoiseRows::All);
        let mut quads = Vec::new();
        for row in 20..22 {
            for x in [10, 14] {
                let slot = 0xbf + ((name_entry_rng_next(&mut expected) >> 4) & 3) as u16;
                let flip = name_entry_rng_next(&mut expected) & 0x10 == 0;
                let quad = pass.next(&mut rng, |_| Ok::<_, ()>(4)).unwrap().unwrap();
                assert_eq!(quad.sprite_slot, slot);
                assert_eq!(quad.corners[0], [if flip { x + 4 } else { x }, row]);
                assert_eq!(quad.corners[2], [if flip { x } else { x + 4 }, row + 1]);
                assert_eq!(quad.vertex_depths, [0x8001; 4]);
                assert_eq!(quad.mode, DirectSpriteMode::Textured);
                assert_eq!(quad.descriptor_word, 0);
                quads.push(quad);
            }
        }
        assert!(pass.next(&mut rng, |_| Ok::<_, ()>(4)).unwrap().is_none());
        assert_eq!(quads.len(), 4);
        assert_eq!(rng, expected, "All rows add no random-row draws");
    }

    #[test]
    fn random_rows_select_y_before_sprite_and_flip_with_signed_wrapping_product() {
        let mut rng = 0x1357_2468;
        let mut expected = rng;
        let mut pass = BriefingNoisePass::new(
            [-10, -30_000, -4, 30_000],
            BriefingNoiseRows::Random { count: 2 },
        );
        for _ in 0..2 {
            let draw = name_entry_rng_next(&mut expected) as i32;
            let y = (-30_000_i32).wrapping_add(draw.wrapping_mul(60_000) >> 16);
            let slot = 0xbf + ((name_entry_rng_next(&mut expected) >> 4) & 3) as u16;
            name_entry_rng_next(&mut expected);
            let quad = pass.next(&mut rng, |_| Ok::<_, ()>(8)).unwrap().unwrap();
            assert_eq!(quad.sprite_slot, slot);
            assert_eq!(quad.corners[0][1], y as i16);
        }
        assert!(pass.next(&mut rng, |_| Ok::<_, ()>(8)).unwrap().is_none());
        assert_eq!(rng, expected);
    }

    #[test]
    fn failures_preserve_the_reached_rng_prefix_without_reading_flip() {
        for rows in [
            BriefingNoiseRows::All,
            BriefingNoiseRows::Random { count: 1 },
        ] {
            let mut rng = 7;
            let mut expected = rng;
            if matches!(rows, BriefingNoiseRows::Random { .. }) {
                name_entry_rng_next(&mut expected);
            }
            name_entry_rng_next(&mut expected);
            let mut pass = BriefingNoisePass::new([0, 0, 10, 10], rows);
            assert_eq!(
                pass.next(&mut rng, |_| Err::<u16, _>("missing")),
                Err(BriefingNoiseError::Resource("missing"))
            );
            assert_eq!(rng, expected);
        }
        let mut pass = BriefingNoisePass::new([0, 0, 1, 1], BriefingNoiseRows::All);
        let mut rng = 0;
        assert!(matches!(
            pass.next(&mut rng, |_| Ok::<_, ()>(0)),
            Err(BriefingNoiseError::ZeroWidth { .. })
        ));
    }

    #[test]
    fn empty_rows_skip_rng_but_random_empty_width_still_selects_each_row() {
        let mut rng = 0;
        let mut pass = BriefingNoisePass::new([0, 0, 1, 1], BriefingNoiseRows::Random { count: 0 });
        assert!(pass.next(&mut rng, |_| Ok::<_, ()>(1)).unwrap().is_none());
        assert_eq!(rng, 0);
        let mut expected = rng;
        for _ in 0..3 {
            name_entry_rng_next(&mut expected);
        }
        let mut pass = BriefingNoisePass::new([1, 0, 1, 4], BriefingNoiseRows::Random { count: 3 });
        assert!(pass.next(&mut rng, |_| Ok::<_, ()>(1)).unwrap().is_none());
        assert_eq!(rng, expected);
    }
}
