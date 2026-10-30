//! 00486B30, the ribbon's +2C. Each segment between consecutive nodes is one
//! quad: both ends are projected through the context's local projection at
//! the context position (0048FE60), widened across the screen direction by a
//! depth-scaled half width and faded along the trail. The packets carry grey
//! corner levels; the far D6 quad 0044EA70 also carries both ends' fog, else
//! it is the near effect quad 0040EF30.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

use crate::model_primitives::{
    PrimitiveCallback, PrimitiveCommand, TextureSortError, TextureSortIdentity,
};
use crate::model_projection::ModelProjectionBackend;
use crate::model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use crate::render_resources::{RenderResource, RenderResourceData, RenderResourceError};

#[cfg(test)]
mod tests;

/// One packed corner: XY WORDs and the sign-extended low WORD of its depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RibbonCorner {
    pub xy: u32,
    pub depth: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RibbonQuad {
    /// 0044EA70 when either end carries fog, else 0040EF30.
    pub far: bool,
    /// The ends' average depth, before material sorting.
    pub depth: u32,
    /// Native order: previous - offset, current - offset, current + offset,
    /// previous + offset (+00..+1F).
    pub corners: [RibbonCorner; 4],
    /// Grey levels at +28, +2C, +30 and +34, each stored as R, G and B.
    pub levels: [u8; 4],
    /// +3C..+3F of a far quad: previous, current, current, previous.
    pub fog: [u8; 4],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RibbonDrawError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("selector67 ribbon {0:?} divides by its zero kept length +54")]
    ZeroLength(ObjectId),
    #[error("selector67 ribbon {0:?} divides by a zero screen length")]
    ZeroScreenLength(ObjectId),
    #[error("selector67 ribbon {0:?} reads unwritten projection byte {1}")]
    UnwrittenProjection(ObjectId, usize),
}

/// [0053C568], the packets' mode word; it also picks the sorting selector.
pub const RIBBON_MODE_0053C568: u32 = 0x0053_c568;

impl RibbonQuad {
    /// 00486F7A..00486FFD (far) and 004870CF..00487134 (near): four
    /// blocks for both kinds, although the near consumer reads three.
    pub fn command<'r>(
        &self,
        sort: u32,
        mode: u32,
        texture: Option<RenderResource<'r>>,
        far_color: u32,
    ) -> PrimitiveCommand<'r> {
        let callback = if self.far {
            PrimitiveCallback::D6QuadFar
        } else {
            PrimitiveCallback::EffectQuadNear
        };
        let mut command = PrimitiveCommand::new(callback, sort, 4);
        for (index, corner) in self.corners.iter().enumerate() {
            command.write_u32(index * 8, corner.xy);
            command.write_u32(index * 8 + 4, corner.depth as u32);
        }
        command.write_u32(0x20, mode);
        command.write_texture(0x24, texture);
        for (index, level) in self.levels.iter().enumerate() {
            command.write_rgb(0x28 + index * 4, [*level; 3]);
        }
        if self.far {
            command.write_u32(0x38, far_color);
            for (index, fog) in self.fog.iter().enumerate() {
                command.write_byte(0x3c + index, *fog);
            }
        }
        command
    }
}

#[derive(Debug, Error)]
pub enum RibbonSortError<E> {
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Sort(#[from] TextureSortError),
    #[error("ribbon sort identity failed: {0}")]
    Host(E),
}

/// 00486EC7..00486F48: with material sorting and a texture, the average
/// depth becomes its translucent half or the descriptor's opaque key. The
/// selector byte sits at the entry + (mode & 1).
pub fn ribbon_sort_key<'r, E>(
    depth: u32,
    mode: u32,
    texture: Option<RenderResource<'r>>,
    sorting: bool,
    identity: impl FnOnce(RenderResource<'r>) -> Result<TextureSortIdentity, E>,
) -> Result<u32, RibbonSortError<E>> {
    let Some(texture) = texture.filter(|_| sorting) else {
        return Ok(depth);
    };
    let RenderResourceData::Sprite(entry) = texture.data else {
        return Err(RenderResourceError::Registration {
            request: texture.request,
        }
        .into());
    };
    let selector = if mode & 1 == 0 {
        entry.selectors.mode0
    } else {
        entry.selectors.mode1
    };
    ribbon_sort_word(
        depth,
        selector,
        |selector| {
            entry
                .variants
                .get(usize::from(selector))
                .map(|variant| variant.flags.raw())
                .ok_or(RenderResourceError::Registration {
                    request: texture.request,
                })
        },
        || identity(texture),
    )
}

/// The decision itself: selector FF or variant flags 39 keep the
/// translucent half `(depth >> 1) + C0000000`; otherwise the descriptor's
/// opaque sort word.
pub fn ribbon_sort_word<E>(
    depth: u32,
    selector: u8,
    flags: impl FnOnce(u8) -> Result<u16, RenderResourceError>,
    identity: impl FnOnce() -> Result<TextureSortIdentity, E>,
) -> Result<u32, RibbonSortError<E>> {
    let translucent = (depth >> 1).wrapping_add(0xc000_0000);
    if selector == 255 || flags(selector)? & 0x39 != 0 {
        return Ok(translucent);
    }
    Ok(identity()
        .map_err(RibbonSortError::Host)?
        .sort_word(selector)?)
}

/// The context fields 00486B30 reads.
pub struct RibbonDrawInput {
    /// Context+50's projection with 0048FE60's transform at the context
    /// position: identity object basis, current view.
    pub projector: ModelProjectionBackend,
    /// Context+04/+08/+0C low WORDs, subtracted from each node as WORDs.
    pub origin: [u16; 3],
    /// Context+38, the horizontal focal length that scales the width.
    pub focal_x: i32,
}

struct Projected {
    xy: u32,
    depth: i32,
    clip: u8,
    fog: Option<u8>,
}

impl ObjectArena {
    /// 00486B62: the H3 sprite slot by +44.
    pub fn selector67_texture_slot(&self, ribbon: ObjectId) -> Result<i32, ObjectError> {
        Ok(match self.selector67(ribbon)?.word_44 as i16 {
            0 => 0x29d,
            1 => 0x38b,
            _ => 1,
        })
    }

    /// The quads 00486B30 submits, in order. A list without a second node
    /// draws nothing.
    pub fn selector67_draw_quads(
        &self,
        ribbon: ObjectId,
        input: &mut RibbonDrawInput,
    ) -> Result<Vec<RibbonQuad>, RibbonDrawError> {
        let state = self.selector67(ribbon)?;
        let nodes = &state.nodes;
        if nodes.len() < 2 {
            return Ok(Vec::new());
        }
        let total = state.dword_54 as i32;
        let mut project = |point: [u16; 3]| -> Result<Projected, RibbonDrawError> {
            let local = std::array::from_fn(|axis| {
                i32::from(point[axis].wrapping_sub(input.origin[axis]) as i16)
            });
            let mut output = ModelVertexProjection::default();
            let depth = match input
                .projector
                .project(ModelVertexInput::Local(local), &mut output)
            {
                Ok(depth) => depth as i32,
                Err(error) => match error {},
            };
            Ok(Projected {
                xy: output
                    .read_u32(0)
                    .ok_or(RibbonDrawError::UnwrittenProjection(ribbon, 0))?,
                depth,
                clip: output.bytes[8].ok_or(RibbonDrawError::UnwrittenProjection(ribbon, 8))?,
                fog: output.bytes[9],
            })
        };
        let mut previous = project(nodes[0].point)?;
        // 00486C81 adds the head's length before the loop, and the first
        // iteration adds it again before it advances.
        let mut walked = i32::from(nodes[0].length);
        let mut previous_level = 0x80;
        let mut offset = [0_u16; 2];
        let mut quads = Vec::new();
        for index in 0..nodes.len() - 1 {
            walked = walked.wrapping_add(i32::from(nodes[index].length));
            let previous_offset = offset;
            if total == 0 {
                return Err(RibbonDrawError::ZeroLength(ribbon));
            }
            let level = (0x80 - walked.wrapping_mul(130) / total).max(0);
            let current = project(nodes[index + 1].point)?;
            let factor = if walked < total {
                (total.wrapping_sub(walked) << 7) / total + 1
            } else {
                1
            };
            let half = if current.depth > 0 {
                (input.focal_x.wrapping_mul(factor) / current.depth + 1).min(200)
            } else {
                200
            };
            // The screen direction turned a quarter: (previous Y - current Y,
            // current X - previous X), scaled to the half width.
            let across = (previous.xy >> 16).wrapping_sub(current.xy >> 16) as u16 as i16;
            let along = current.xy.wrapping_sub(previous.xy) as u16 as i16;
            offset = [across as u16, along as u16];
            let squared = i32::from(across)
                .wrapping_mul(i32::from(across))
                .wrapping_add(i32::from(along).wrapping_mul(i32::from(along)));
            if squared != 0 {
                let length = isqrt_48efc0(squared) as i32;
                if length == 0 {
                    return Err(RibbonDrawError::ZeroScreenLength(ribbon));
                }
                offset = [
                    (i32::from(across).wrapping_mul(half) / length) as u16,
                    (i32::from(along).wrapping_mul(half) / length) as u16,
                ];
            }
            let drawn = (previous.clip | current.clip) != 0
                && previous.depth > 0x40
                && current.depth > 0x40;
            if drawn {
                let fog = [previous.fog, current.fog];
                let [Some(previous_fog), Some(current_fog)] = fog else {
                    return Err(RibbonDrawError::UnwrittenProjection(ribbon, 9));
                };
                let corner = |xy: u32, offset: [u16; 2], sign: bool, depth: i32| {
                    let apply = |value: u16, delta: u16| {
                        if sign {
                            value.wrapping_add(delta)
                        } else {
                            value.wrapping_sub(delta)
                        }
                    };
                    RibbonCorner {
                        xy: u32::from(apply(xy as u16, offset[0]))
                            | (u32::from(apply((xy >> 16) as u16, offset[1])) << 16),
                        depth: i32::from(depth as i16),
                    }
                };
                quads.push(RibbonQuad {
                    far: (previous_fog | current_fog) != 0,
                    depth: (previous.depth.wrapping_add(current.depth) / 2) as u32,
                    corners: [
                        corner(previous.xy, previous_offset, false, previous.depth),
                        corner(current.xy, offset, false, current.depth),
                        corner(current.xy, offset, true, current.depth),
                        corner(previous.xy, previous_offset, true, previous.depth),
                    ],
                    levels: [
                        previous_level as u8,
                        level as u8,
                        level as u8,
                        previous_level as u8,
                    ],
                    fog: [previous_fog, current_fog, current_fog, previous_fog],
                });
            }
            previous = current;
            previous_level = level;
        }
        Ok(quads)
    }
}
