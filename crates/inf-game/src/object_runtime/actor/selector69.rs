//! Selector 69, the barrier: the shared 0047A7A0 with the final table
//! 005205D8. Its table is the shared actor's 00527C00 but for +00
//! (004110E0, the shared teardown), +04 (the commands 00411100), +1C (the
//! ground fit 00411150), +24 (the update 00411380), +28 (the light
//! 00411C50), +2C (the draw 00411570) and +98 (the links 00411CE0).
//!
//! A barrier is a line of length +14C along its own X axis. The ground fit
//! finds the highest ground towards each end and stands two posts a sixth
//! of the length either side of the centre. The update kills a modelled
//! actor that crosses the line below it and leaves H13 resource 0x36 where
//! it was. The draw runs +144 bolts along the ground and back along the line,
//! each joined to both posts by a beam, and the light pulses at both posts.
//! The last link's release (message 8) or message 0x15 grants +7A and
//! disposes the barrier.

use inf_formats::native_basis::isqrt_48efc0;
use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};
use thiserror::Error;

use super::derived::kinematics::ftol;
use super::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension, ActorHandle,
};
use crate::model_primitives::{PrimitiveCallback, PrimitiveCommand};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId, RibbonCorner};
use crate::render_resources::RenderResource;
use crate::terrain_runtime::TerrainRuntime;
use crate::world_lighting::TerrainLightRequest;

#[cfg(test)]
#[path = "selector69_tests.rs"]
mod tests;

/// H13 resource 0x36, which 00411548 builds at a killed actor: shared bank
/// local 54, a selector-22 fading visual (model 109).
pub const BARRIER_TRACE_RESOURCE: u32 = 0x36;

/// `0052F628`, the beams' packet mode. Only this draw reads it and nothing
/// writes it; the executable holds 4.
pub const BARRIER_MODE_0052F628: u32 = 0x0052_f628;

/// BarrierAgeResidueApproximation: the constructor never writes the age
/// +148, which the update sums from and the draw and light read as
/// `age >> 11`. Retail's first value is heap residue: Infest01's Level 1
/// barrier held 0x100, Infest02's 0x52FE38. The port starts from the zero a
/// fresh heap holds. Only the bolts' and pulses' phase depends on it: the
/// update's hit test never reads it, and with Level 1's ten bolts eight are
/// on the ground at any age, so every draw births as many sparks.
pub const BARRIER_AGE_RESIDUE_APPROXIMATION: u32 = 0;

/// `00411570`'s `[EBP-28]`: the beams' widest half width.
const BEAM_WIDTH: i32 = 0x32;
/// `00411343`: the posts stand 250 below the line's ends.
const POST_DROP: i32 = 0xfa;

/// The constructor's writes after 0047A7A0 (`00411090..004110D2`) and the
/// fields the barrier's own methods write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector69State {
    /// +144: the bolts the draw runs; 6, or command 0x11.
    pub bolts_144: i32,
    /// +148: the update's sum of its deltas; see
    /// [`BARRIER_AGE_RESIDUE_APPROXIMATION`].
    pub age_148: u32,
    /// +14C: the length; 600, or twice command 0x10's input.
    pub length_14c: i32,
    /// +150: half the length, which the ground fit writes.
    pub half_150: Option<i32>,
    /// +154 and +158: the highest ground found towards the positive and
    /// the negative end, from -32767.
    pub ends_154: [i32; 2],
    /// +15C: the links that hold it (messages 7 and 8).
    pub links_15c: i32,
    /// +138..+143: the posts, which the ground fit writes.
    pub posts_138: Option<[[u16; 3]; 2]>,
}

impl Selector69State {
    fn new() -> Self {
        Self {
            bolts_144: 6,
            age_148: BARRIER_AGE_RESIDUE_APPROXIMATION,
            length_14c: 600,
            half_150: None,
            ends_154: [-32767; 2],
            links_15c: 0,
            posts_138: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum Selector69Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("barrier {0:?} has no position")]
    Position(ObjectId),
    #[error("barrier {0:?} reads its unwritten half length +150")]
    UnwrittenHalf(ObjectId),
    #[error("barrier {0:?} reads its unwritten posts +138")]
    UnwrittenPosts(ObjectId),
    #[error("barrier {0:?}'s ground fit needs H12/0")]
    Terrain(ObjectId),
    #[error("barrier {object:?} divides by zero at {address:08X}")]
    DivideByZero { object: ObjectId, address: u32 },
    #[error("barrier {0:?} actor {1:?} has no position")]
    VictimPosition(ObjectId, ObjectId),
    #[error("barrier host: {0}")]
    Host(E),
}

impl<E> Selector69Error<E> {
    /// The host's own error, or this stop without it.
    pub fn split_host(self) -> Result<Selector69Error<std::convert::Infallible>, E> {
        Ok(match self {
            Self::Host(error) => return Err(error),
            Self::Object(error) => Selector69Error::Object(error),
            Self::Position(object) => Selector69Error::Position(object),
            Self::UnwrittenHalf(object) => Selector69Error::UnwrittenHalf(object),
            Self::UnwrittenPosts(object) => Selector69Error::UnwrittenPosts(object),
            Self::Terrain(object) => Selector69Error::Terrain(object),
            Self::DivideByZero { object, address } => {
                Selector69Error::DivideByZero { object, address }
            }
            Self::VictimPosition(barrier, actor) => Selector69Error::VictimPosition(barrier, actor),
        })
    }
}

/// The ground fit's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector69Fit {
    Fitted,
    /// A zero length or bolt count disposes the barrier (00479FA0).
    Disposed,
}

/// What the update's kill reaches, in order.
pub trait Selector69Host {
    type Error;
    /// `0047D670(0, barrier +84, 1)` on the victim: down to state 1 with
    /// the barrier as the source.
    fn kill(
        &mut self,
        arena: &mut ObjectArena,
        victim: ObjectId,
        source: ActorHandle,
    ) -> Result<(), Self::Error>;
    /// `00460310` with resource 0x36 at the victim's position, NULL
    /// command and rotation, placement -1, mode 0 and the current NULL.
    fn trace(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), Self::Error>;
}

/// What the barrier's +98 leaves to its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector69Message {
    /// Returned 0 with nothing else to do.
    Done,
    /// `00411D0E`: a nonzero `+7A` goes to the first player's grants +9C
    /// (`00488AA0`, `004750C0`), then the barrier disposes itself
    /// (`00479FA0`). Returns 0.
    Release { grant: u16 },
}

/// One bolt of the draw: where it is, and whether it births a spark there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector69Bolt {
    pub point: [u16; 3],
    /// A bolt on the ground births `004894A0(0, point, zero velocity, 0, 0)`
    /// after its height is read.
    pub spark: bool,
}

/// The fields `00411570` reads for its bolts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector69Geometry {
    pub barrier: ObjectId,
    pub position: [u16; 3],
    /// The X axis, basis words 1..3.
    pub axis: [u32; 3],
    pub length: i32,
    pub bolts: i32,
    pub ends: [i32; 2],
    pub age: u32,
    pub half: i32,
}

impl Selector69Geometry {
    /// `00411570`'s bolts before projection, in order: `+144` of them, the
    /// i-th at `t = (i * period / bolts + (age >> 11)) % period` with the
    /// period five quarters of the length. Under the length a bolt runs on
    /// the ground at `t - half` along X (`height(x, z)`, `00450A30` over
    /// H12/0, acquired at each such bolt) and births a spark; past it, it
    /// flies back along the straight line between the ends' heights at
    /// `5 * length - 4t - half`.
    pub fn bolts<E>(
        &self,
        mut height: impl FnMut([u16; 2]) -> Result<i16, E>,
    ) -> Result<Vec<Selector69Bolt>, Selector69Error<E>> {
        let [x, _, z] = self.position;
        let divide = |address| Selector69Error::DivideByZero {
            object: self.barrier,
            address,
        };
        let period = self.length.wrapping_mul(5) / 4;
        let mut points = Vec::new();
        let mut walked = 0_i32;
        for _ in 0..self.bolts.max(0) {
            let step = walked
                .checked_div(self.bolts)
                .ok_or_else(|| divide(0x0041_16d7))?;
            let t = (step as u32)
                .wrapping_add(self.age >> 11)
                .checked_rem(period as u32)
                .ok_or_else(|| divide(0x0041_16e6))?;
            if (t as i32) < self.length {
                let along = (t as i32).wrapping_sub(self.half) as i16;
                let at = [
                    x.wrapping_add(scaled(along, self.axis[0])),
                    z.wrapping_add(scaled(along, self.axis[2])),
                ];
                let y = height(at).map_err(Selector69Error::Host)? as u16;
                points.push(Selector69Bolt {
                    point: [at[0], y, at[1]],
                    spark: true,
                });
            } else {
                let back = self
                    .length
                    .wrapping_mul(5)
                    .wrapping_sub((t as i32).wrapping_mul(4));
                let along = back.wrapping_sub(self.half) as i16;
                let rise = self.ends[0]
                    .wrapping_sub(self.ends[1])
                    .wrapping_mul(back)
                    .checked_div(self.length)
                    .ok_or_else(|| divide(0x0041_180e))? as u16;
                points.push(Selector69Bolt {
                    point: [
                        x.wrapping_add(scaled(along, self.axis[0])),
                        rise.wrapping_add(self.ends[1] as u16),
                        z.wrapping_add(scaled(along, self.axis[2])),
                    ],
                    spark: false,
                });
            }
            walked = walked.wrapping_add(period);
        }
        Ok(points)
    }
}

/// One projected end of a beam: the projection's XY DWORD, its depth and
/// its bytes +8 (clip flags) and +9 (fog, which the far projection writes
/// only from depth 64).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector69Projected {
    pub xy: u32,
    pub depth: i32,
    pub clip: u8,
    pub fog: Option<u8>,
}

/// A beam that would read a fog byte its projection left unwritten. Both
/// ends are deeper than 0x40 where it is read, so a projection that writes
/// fog from depth 64 never reaches this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("barrier beam reads an unwritten fog byte +9")]
pub struct Selector69UnwrittenFog;

/// One beam quad from a post to a bolt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector69Quad {
    /// 00411D50 when either end carries fog, else 0040EF30.
    pub far: bool,
    /// The ends' average depth, before material sorting.
    pub depth: u32,
    /// Native order: post - offset, bolt - offset, bolt + offset,
    /// post + offset (+00..+1F).
    pub corners: [RibbonCorner; 4],
    /// +30..+33 of a far quad: post, bolt, bolt, post.
    pub fog: [u8; 4],
}

impl Selector69Quad {
    /// `00411A72..00411AD8` (far) and `00411BB4..00411BF5` (near): mode at
    /// +20, the texture at +24, the effect color `[0055F26C]` at +28
    /// (`00490360`), and for a far quad the context's far color at +2C and
    /// the fog bytes.
    pub fn command<'r>(
        &self,
        sort: u32,
        mode: u32,
        texture: Option<RenderResource<'r>>,
        effect_color: u32,
        far_color: u32,
    ) -> PrimitiveCommand<'r> {
        let (callback, blocks) = if self.far {
            (PrimitiveCallback::EffectQuadFar, 4)
        } else {
            (PrimitiveCallback::EffectQuadNear, 3)
        };
        let mut command = PrimitiveCommand::new(callback, sort, blocks);
        for (index, corner) in self.corners.iter().enumerate() {
            command.write_u32(index * 8, corner.xy);
            command.write_u32(index * 8 + 4, corner.depth as u32);
        }
        command.write_u32(0x20, mode);
        command.write_texture(0x24, texture);
        command.write_u32(0x28, effect_color);
        if self.far {
            command.write_u32(0x2c, far_color);
            for (index, fog) in self.fog.iter().enumerate() {
                command.write_byte(0x30 + index, *fog);
            }
        }
        command
    }
}

/// `__ftol` of a signed WORD scale times one basis float, the low WORD kept.
fn scaled(scale: i16, word: u32) -> u16 {
    ftol(f64::from(scale) * f64::from(f32::from_bits(word))) as u16
}

impl ObjectArena {
    /// `0046144A..00461475`: 0x160 bytes, the shared 0047A7A0, then
    /// `00411090`'s bolts 6, length 600, ends -32767 and no links.
    pub fn allocate_selector69(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector69(Box::new(Selector69State::new()));
        Ok(object)
    }

    pub fn selector69(&self, object: ObjectId) -> Result<&Selector69State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector69(state) => Ok(state),
            _ => Err(ObjectError::NotSelector69(object)),
        }
    }

    pub fn selector69_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector69State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector69(state) => Ok(state),
            _ => Err(ObjectError::NotSelector69(object)),
        }
    }

    /// `00411100`: the shared 0047ADA0 through code 15; code 16 doubles its
    /// DWORD into the length +14C, code 17 stores its DWORD in the bolts
    /// +144, and a higher code refuses.
    pub fn apply_selector69_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(69) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector69(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let state = self.selector69_mut(object)?;
        match (code, &command.payload) {
            (16, InstanceCommandPayload::Selector69DoubledDword14c(value)) => {
                state.length_14c = value.value.wrapping_shl(1);
            }
            (17, InstanceCommandPayload::Selector69Dword144(value)) => {
                state.bolts_144 = value.value;
            }
            (16 | 17, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// `00411150`, the barrier's +1C. Half the length goes to +150 and its
    /// low WORD to the radius +28. A zero length or bolt count disposes it.
    /// Otherwise it samples the ground every 0x40 from the half length
    /// down, at that distance along its X axis either way, keeping the
    /// highest heights in +154 (positive end) and +158 (negative end); it
    /// stands at their mean and puts the posts a sixth of the length either
    /// way along X, 250 below the line's ends.
    pub fn finish_selector69_initialization(
        &mut self,
        object: ObjectId,
        terrain: Option<&TerrainRuntime>,
    ) -> Result<Selector69Fit, Selector69Error<std::convert::Infallible>> {
        // A missing H12/0 stops at the first sample, after the half length
        // and the zero-length and zero-bolt disposal.
        self.fit_selector69(object, |xz| {
            terrain.map(|terrain| terrain.height_at(xz)).ok_or(())
        })
        .map_err(|error| {
            error
                .split_host()
                .unwrap_or(Selector69Error::Terrain(object))
        })
    }

    /// [`Self::finish_selector69_initialization`] over a supplied height
    /// (`00450A30`), read at each sample in order.
    pub fn fit_selector69<E>(
        &mut self,
        object: ObjectId,
        mut height: impl FnMut([u16; 2]) -> Result<i16, E>,
    ) -> Result<Selector69Fit, Selector69Error<E>> {
        let length = self.selector69(object)?.length_14c;
        let bolts = self.selector69(object)?.bolts_144;
        let half = length / 2;
        self.selector69_mut(object)?.half_150 = Some(half);
        self.owned_mut(object)?.state.base.word_28 = half as u16;
        if length == 0 || bolts == 0 {
            self.dispose(object)?;
            return Ok(Selector69Fit::Disposed);
        }
        let [x, _, z] = self
            .object(object)?
            .base
            .position_words
            .ok_or(Selector69Error::Position(object))?;
        let axis = self.actor(object)?.rotating.basis.words;
        let mut ends = self.selector69(object)?.ends_154;
        let mut along = (half as u32) & 0xffff;
        while along != 0 && along as i32 > 0 {
            let scale = along as u16 as i16;
            let dx = scaled(scale, axis[1]);
            let dz = scaled(scale, axis[3]);
            for (end, sign) in ends.iter_mut().zip([false, true]) {
                let at = if sign {
                    [x.wrapping_sub(dx), z.wrapping_sub(dz)]
                } else {
                    [x.wrapping_add(dx), z.wrapping_add(dz)]
                };
                let height = i32::from(height(at).map_err(Selector69Error::Host)?);
                if height > *end {
                    *end = height;
                }
            }
            along = along.wrapping_sub(0x40);
            if along as i32 <= 0 {
                break;
            }
        }
        let y = (ends[1].wrapping_add(ends[0]) / 2) as u16;
        let sixth = (length / 6) as i16;
        let offset = [
            scaled(sixth, axis[1]),
            scaled(sixth, axis[2]),
            scaled(sixth, axis[3]),
        ];
        let lift = ends[0].wrapping_sub(ends[1]) / 6;
        let mut posts = [
            [
                x.wrapping_add(offset[0]),
                y.wrapping_add(offset[1]),
                z.wrapping_add(offset[2]),
            ],
            [
                x.wrapping_sub(offset[0]),
                y.wrapping_sub(offset[1]),
                z.wrapping_sub(offset[2]),
            ],
        ];
        posts[0][1] = posts[0][1].wrapping_add(lift.wrapping_sub(POST_DROP) as u16);
        posts[1][1] = posts[1][1].wrapping_add((-POST_DROP).wrapping_sub(lift) as u16);
        let state = self.selector69_mut(object)?;
        state.ends_154 = ends;
        state.posts_138 = Some(posts);
        // 00411150 replaces the shared +1C 0047AAD0 without calling it, so
        // +74 keeps the constructor's value.
        if let Some(position) = &mut self.owned_mut(object)?.state.base.position_words {
            position[1] = y;
        }
        Ok(Selector69Fit::Fitted)
    }

    /// `00411380`, the barrier's +24. The age +148 takes the delta. Then,
    /// over the actor list (G+48) in order, an actor modelled with flags
    /// 0x8 and 0x80 clear (`(+2C & 0x8C) == 4`), lower than the higher end,
    /// within the half length plus 0x40 across the ground and alive (state
    /// 3) is measured in the barrier's frame (`0047A660`). It is killed
    /// when it lies within the half length along X, within its own radius
    /// +40 plus the distance its velocity across the line covers this frame
    /// (`|vel . Z| * (delta >> 5) >> 15`, a signed WORD) along Z, and below
    /// the line between the ends' heights. The kill is `0047D670(0, +84,
    /// 1)`, then resource 0x36 at its position.
    pub fn tick_selector69<H: Selector69Host>(
        &mut self,
        barrier: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector69Error<H::Error>> {
        let state = self.selector69_mut(barrier)?;
        state.age_148 = state.age_148.wrapping_add(delta);
        let half = state
            .half_150
            .ok_or(Selector69Error::UnwrittenHalf(barrier))?;
        let length = state.length_14c;
        let ends = state.ends_154;
        let reach = half.wrapping_add(0x40);
        let reach = reach.wrapping_mul(reach) as u32;
        let top = if ends[0] > ends[1] { ends[0] } else { ends[1] };
        let source = self.actor(barrier)?.handle;
        let mut index = 0;
        while let Some(&actor) = self.actor_list().get(index) {
            index += 1;
            if self.construction_is_stopped(actor)? || self.disposal_is_stopped(actor)? {
                continue;
            }
            let base = &self.object(actor)?.base;
            if base.flags & 0x8c != 4 {
                continue;
            }
            let Some(theirs) = base.position_words else {
                return Err(Selector69Error::VictimPosition(barrier, actor));
            };
            if i32::from(theirs[1] as i16) >= top {
                continue;
            }
            let own = self
                .object(barrier)?
                .base
                .position_words
                .ok_or(Selector69Error::Position(barrier))?;
            let dx = own[0].wrapping_sub(theirs[0]) as i16;
            let dz = own[2].wrapping_sub(theirs[2]) as i16;
            let squared =
                (i32::from(dx) * i32::from(dx)).wrapping_add(i32::from(dz) * i32::from(dz));
            if squared as u32 >= reach {
                continue;
            }
            let victim = self.actor(actor)?;
            if victim.word_c4 != 3 {
                continue;
            }
            let velocity = victim.words_8e_92;
            let radius = victim.rotating.visual.radius_word;
            let dy = own[1].wrapping_sub(theirs[1]) as i16;
            let basis = self.actor(barrier)?.rotating.basis;
            let local = basis.inverse_words([dx, dy, dz]);
            if i32::from(local[0]).abs() >= half {
                continue;
            }
            let across = ftol(
                f64::from(velocity[2] as i16) * f64::from(f32::from_bits(basis.words[9]))
                    + f64::from(velocity[0] as i16) * f64::from(f32::from_bits(basis.words[7])),
            );
            let bound =
                i32::from((across.wrapping_abs().wrapping_mul((delta >> 5) as i32) >> 15) as i16)
                    + i32::from(radius);
            if i32::from(local[2]).abs() >= bound {
                continue;
            }
            let slope = ends[1]
                .wrapping_sub(ends[0])
                .wrapping_mul(i32::from(local[0]));
            let line = slope
                .checked_div(length)
                .ok_or(Selector69Error::DivideByZero {
                    object: barrier,
                    address: 0x0041_150d,
                })?
                .wrapping_add(i32::from(own[1] as i16));
            if i32::from(theirs[1] as i16) >= line {
                continue;
            }
            host.kill(self, actor, source)
                .map_err(Selector69Error::Host)?;
            let at = self
                .object(actor)?
                .base
                .position_words
                .ok_or(Selector69Error::VictimPosition(barrier, actor))?;
            host.trace(self, at).map_err(Selector69Error::Host)?;
        }
        Ok(())
    }

    /// `00411C50`, the barrier's +28: one pulse at each post (X/Z), of
    /// radius `phase + half/2` where the phase folds `(age >> 11) % half`
    /// back from the middle; its level `radius * 64 / half` is blue, and
    /// green from 0x20 up.
    pub fn selector69_lights(
        &self,
        barrier: ObjectId,
    ) -> Result<[TerrainLightRequest; 2], Selector69Error<std::convert::Infallible>> {
        let state = self.selector69(barrier)?;
        let half = state
            .half_150
            .ok_or(Selector69Error::UnwrittenHalf(barrier))?;
        let posts = state
            .posts_138
            .ok_or(Selector69Error::UnwrittenPosts(barrier))?;
        let divide = |address| Selector69Error::DivideByZero {
            object: barrier,
            address,
        };
        let mut phase = (state.age_148 >> 11)
            .checked_rem(half as u32)
            .ok_or_else(|| divide(0x0041_1c75))?;
        let middle = half / 2;
        if phase as i32 > middle {
            phase = (half as u32).wrapping_sub(phase);
        }
        let radius = (phase as i32).wrapping_add(middle);
        let level = radius
            .wrapping_shl(6)
            .checked_div(half)
            .ok_or_else(|| divide(0x0041_1c92))?;
        let green = if level as u32 > 0x20 {
            (level as u8).wrapping_sub(0x20)
        } else {
            0
        };
        let rgb = [0, green as i8, level as u8 as i8];
        Ok(posts.map(|post| TerrainLightRequest {
            world_xz: [i32::from(post[0] as i16), i32::from(post[2] as i16)],
            radius,
            rgb,
        }))
    }

    /// The fields `00411570` reads for its bolts, captured before any terrain
    /// read.
    pub fn selector69_geometry(
        &self,
        barrier: ObjectId,
    ) -> Result<Selector69Geometry, Selector69Error<std::convert::Infallible>> {
        let state = self.selector69(barrier)?;
        let words = self.actor(barrier)?.rotating.basis.words;
        Ok(Selector69Geometry {
            barrier,
            position: self
                .object(barrier)?
                .base
                .position_words
                .ok_or(Selector69Error::Position(barrier))?,
            axis: [words[1], words[2], words[3]],
            length: state.length_14c,
            bolts: state.bolts_144,
            ends: state.ends_154,
            age: state.age_148,
            half: state
                .half_150
                .ok_or(Selector69Error::UnwrittenHalf(barrier))?,
        })
    }

    /// The barrier's two posts, which the draw projects first.
    pub fn selector69_posts(
        &self,
        barrier: ObjectId,
    ) -> Result<[[u16; 3]; 2], Selector69Error<std::convert::Infallible>> {
        self.selector69(barrier)?
            .posts_138
            .ok_or(Selector69Error::UnwrittenPosts(barrier))
    }

    /// `00411CE0`, the barrier's +98: message 7 adds a link; message 8
    /// takes one away and, leaving none, releases it; 0x15 releases it; any
    /// other code returns 0.
    pub fn selector69_message(
        &mut self,
        barrier: ObjectId,
        code: i32,
    ) -> Result<Selector69Message, ObjectError> {
        let grant = self.actor(barrier)?.word_7a;
        let state = self.selector69_mut(barrier)?;
        match code {
            7 => {
                state.links_15c = state.links_15c.wrapping_add(1);
                Ok(Selector69Message::Done)
            }
            8 => {
                state.links_15c = state.links_15c.wrapping_sub(1);
                if state.links_15c >= 1 {
                    Ok(Selector69Message::Done)
                } else {
                    Ok(Selector69Message::Release { grant })
                }
            }
            0x15 => Ok(Selector69Message::Release { grant }),
            _ => Ok(Selector69Message::Done),
        }
    }
}

/// The beam sizes `00411666..004116A2` and `0041185D..00411874`: a positive
/// depth scales the widest half width by the focal length, plus one, at
/// most the widest; zero or less keeps the widest.
pub fn selector69_beam_width(focal_x: i32, depth: i32) -> i32 {
    if depth < 1 {
        return BEAM_WIDTH;
    }
    (focal_x.wrapping_mul(BEAM_WIDTH) / depth + 1).min(BEAM_WIDTH)
}

/// `0041187F..00411BF5` for one bolt: a beam from each post in turn, drawn
/// when either end is on screen (byte +8), the screen step is longer than
/// one and the post is deeper than 0x40. The bolt itself must be deeper
/// than 0x40, which the caller checks first. Each end widens across the
/// step by its own size; the quad is far when either end carries fog.
pub fn selector69_beams(
    posts: &[(Selector69Projected, i32); 2],
    bolt: Selector69Projected,
    size: i32,
) -> Result<Vec<Selector69Quad>, Selector69UnwrittenFog> {
    let mut quads = Vec::new();
    for &(post, post_size) in posts {
        if (post.clip | bolt.clip) == 0 {
            continue;
        }
        // 0041188D..004118A2 compares the fog bytes' AND with 0x100, which
        // a byte never equals: nothing is skipped and nothing depends on it.
        let rise = (bolt.xy >> 16).wrapping_sub(post.xy >> 16) as u16 as i16;
        let run = post.xy.wrapping_sub(bolt.xy) as u16 as i16;
        let squared = i32::from(rise)
            .wrapping_mul(i32::from(rise))
            .wrapping_add(i32::from(run).wrapping_mul(i32::from(run)));
        if squared as u32 <= 1 || post.depth <= 0x40 {
            continue;
        }
        let (Some(post_fog), Some(bolt_fog)) = (post.fog, bolt.fog) else {
            return Err(Selector69UnwrittenFog);
        };
        let length = isqrt_48efc0(squared) as i32;
        let part = |width: i32, value: i16| (width.wrapping_mul(i32::from(value)) / length) as u16;
        let post_offset = [part(post_size, rise), part(post_size, run)];
        let bolt_offset = [part(size, rise), part(size, run)];
        let corner = |xy: u32, offset: [u16; 2], plus: bool, depth: i32| {
            let apply = |value: u16, delta: u16| {
                if plus {
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
        quads.push(Selector69Quad {
            far: (post_fog | bolt_fog) != 0,
            depth: (post.depth.wrapping_add(bolt.depth) / 2) as u32,
            corners: [
                corner(post.xy, post_offset, false, post.depth),
                corner(bolt.xy, bolt_offset, false, bolt.depth),
                corner(bolt.xy, bolt_offset, true, bolt.depth),
                corner(post.xy, post_offset, true, post.depth),
            ],
            fog: [post_fog, bolt_fog, bolt_fog, post_fog],
        });
    }
    Ok(quads)
}

/// Where `selector69_draw` stopped.
#[derive(Debug, Error)]
pub enum Selector69DrawError<E> {
    #[error(transparent)]
    Fog(#[from] Selector69UnwrittenFog),
    #[error("barrier draw host: {0}")]
    Host(E),
}

/// `00411570` after its bolts: both posts are projected and sized first,
/// then each bolt in order births its spark (a bolt on the ground), is
/// projected and, deeper than 0x40, sized and joined to the posts.
pub fn selector69_draw<E>(
    posts: [[u16; 3]; 2],
    bolts: &[Selector69Bolt],
    focal_x: i32,
    mut project: impl FnMut([u16; 3]) -> Result<Selector69Projected, E>,
    mut spark: impl FnMut([u16; 3]) -> Result<(), E>,
    mut beam: impl FnMut(Selector69Quad) -> Result<(), E>,
) -> Result<(), Selector69DrawError<E>> {
    let mut ends = [(
        Selector69Projected {
            xy: 0,
            depth: 0,
            clip: 0,
            fog: None,
        },
        0,
    ); 2];
    for (end, post) in ends.iter_mut().zip(posts) {
        let projected = project(post).map_err(Selector69DrawError::Host)?;
        *end = (projected, selector69_beam_width(focal_x, projected.depth));
    }
    for bolt in bolts {
        if bolt.spark {
            spark(bolt.point).map_err(Selector69DrawError::Host)?;
        }
        let projected = project(bolt.point).map_err(Selector69DrawError::Host)?;
        if projected.depth <= 0x40 {
            continue;
        }
        let size = selector69_beam_width(focal_x, projected.depth);
        for quad in selector69_beams(&ends, projected, size)? {
            beam(quad).map_err(Selector69DrawError::Host)?;
        }
    }
    Ok(())
}
