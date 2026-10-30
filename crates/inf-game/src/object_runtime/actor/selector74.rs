//! Selector 74, Helniazine's zone: the shared 0047A7A0 with the final table
//! 00521764 (case 0x4A at 0046157D, 0x158 bytes, through 0042D550, which
//! clears the link byte +148 and the link count +14C). Its table is the
//! shared actor's 00527C00 but for +00 (004110E0, the same 0047AA60
//! teardown), +04 (the commands 0042D6F0), +0C (the initializer 0042D760),
//! +1C (the finish 0042D950), +24 (the update 0042D570), +2C (the draw
//! 0042D7D0) and +98 (the messages 0042D6B0).
//!
//! The zone is a box in its own frame, half extents +152 across (X), +150
//! up (Y) and +154 deep (Z), inside the sphere through its corners (squared
//! radius +138). Each update retires (0047CE90) every actor on the list
//! whose +68 answers, the 00455560 family's grenades, charges, missiles,
//! mines, Bombs and implants, inside both. Its draw births particle type 2
//! (00489A90) at +144 points round the circle of radius +140 in its plane,
//! each clamped to the box and set 0x32 over the ground, turned by the
//! timer +13C. A selector 37 or 39 after-load links it (message 7 sets +148
//! and counts +14C) and that target's completion unlinks it (8); a linked
//! zone with no links left retires itself.

use inf_formats::{
    InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView,
    Selector74ConstructorPayload,
};
use thiserror::Error;

use super::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension, DerivedActorClass,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

/// 0042D90C: the draw's points sit this far over the ground.
const DRAW_LIFT: u16 = 0x32;
/// 0042D915: the particle type each point births.
pub const SELECTOR74_PARTICLE: u32 = 2;
/// 0042D9C6..0042D9D6: one draw point per 800 of half width and depth.
const POINT_SPACING: i32 = 800;

/// Case 0x4A's fields after 0047A7A0. 0042D550 writes only +148 and +14C;
/// the initializer writes the extents and the finish the sphere, circle
/// and count, so a built zone holds them all but the timer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector74State {
    /// +138, the squared radius of the sphere through the box's corners.
    pub radius2_138: Option<u32>,
    /// +13C, the draw's turn, which each update advances by `delta >> 6`.
    /// Nothing writes it before the first update adds to it, so retail's
    /// starts from allocator residue (0 in Infest03; 0x3B800000, a float's
    /// bits, in Infest04, whose low WORD, all the draw reads, is 0 too).
    /// The fresh-heap carry policy starts it at zero; a strict arena leaves
    /// it unknown, and then the update advances a known turn only and the
    /// draw holds.
    pub timer_13c: Option<u32>,
    /// +140, the circle's radius.
    pub circle_140: Option<u32>,
    /// +144, the draw's points.
    pub count_144: Option<u32>,
    /// BYTE +148: a link has come (message 7).
    pub linked_148: u8,
    /// +14C, the links: message 7 adds one and 8 takes one, each wrapping.
    pub links_14c: u32,
    /// WORDs +150, +152 and +154: the half height, width and depth.
    pub extents_150: Option<[u16; 3]>,
}

/// The services the update reaches.
pub trait Selector74Host {
    type Error;
    /// 0047CE90 on `actor`: the zone itself, or an actor inside it.
    fn retire(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector74Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("zone {zone:?} reads its unwritten +{offset:03X}")]
    Unwritten { zone: ObjectId, offset: u16 },
    #[error(
        "zone {zone:?}'s draw turns by +13C, which case 0x4A leaves to allocator residue (held)"
    )]
    TimerResidue { zone: ObjectId },
    #[error("zone {zone:?} measures actor {actor:?}, which has no position")]
    Position { zone: ObjectId, actor: ObjectId },
    #[error("zone {zone:?} 0047CE90 on {actor:?}: {error}")]
    Retire {
        zone: ObjectId,
        actor: ObjectId,
        error: E,
    },
}

/// `0042D880..0042D8B7`: a WORD above the limit (signed) becomes the limit;
/// one below its negation (sign-extended) becomes the negation's low WORD.
/// A negative limit sends every value to one end or the other.
fn clamp(value: u16, limit: u16) -> u16 {
    if value as i16 > limit as i16 {
        limit
    } else if i32::from(value as i16) >= -i32::from(limit as i16) {
        value
    } else {
        limit.wrapping_neg()
    }
}

/// The inline quarter wave of `0042D7FA..0042D822`: the WORD at the index
/// the second quarter mirrors, negated in the second half, times the radius,
/// shifted right 15.
fn wave(sine: &[u16; 4096], angle: i32, radius: u32) -> u16 {
    let mut index = angle & 0x3ffc;
    if angle & 0x4000 != 0 {
        index = 0x3ffc - index;
    }
    let mut value = i32::from(sine[(index >> 2) as usize]);
    if angle & 0x8000 != 0 {
        value = value.wrapping_neg();
    }
    ((radius as i32).wrapping_mul(value) >> 15) as u16
}

/// `0042D615..0042D632`: the DWORD sum of the signed WORD squares.
fn squared(delta: [i16; 3]) -> u32 {
    delta.iter().fold(0_u32, |sum, &axis| {
        sum.wrapping_add((i32::from(axis) * i32::from(axis)) as u32)
    })
}

impl ObjectArena {
    /// Case 0x4A: 0x158 bytes, the shared 0047A7A0, then 0042D550's zero
    /// link byte +148 and link count +14C.
    pub fn allocate_selector74(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        let timer_13c = self
            .actor_allocation
            .initial_dword(super::allocation::ActorAllocationDwordCarry::Selector74Timer13c);
        self.actor_mut(object)?.extension = ActorExtension::Selector74(Box::new(Selector74State {
            radius2_138: None,
            timer_13c,
            circle_140: None,
            count_144: None,
            linked_148: 0,
            links_14c: 0,
            extents_150: None,
        }));
        Ok(object)
    }

    pub fn selector74(&self, object: ObjectId) -> Result<&Selector74State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector74(state) => Ok(state),
            _ => Err(ObjectError::NotSelector74(object)),
        }
    }

    pub(in crate::object_runtime) fn selector74_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector74State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector74(state) => Ok(state),
            _ => Err(ObjectError::NotSelector74(object)),
        }
    }

    /// A recorded timer, for the tests that replay retail's draws.
    #[cfg(test)]
    pub(crate) fn selector74_set_timer_for_tests(
        &mut self,
        zone: ObjectId,
        timer: u32,
    ) -> Result<(), ObjectError> {
        self.selector74_mut(zone)?.timer_13c = Some(timer);
        Ok(())
    }

    /// 0042D780 after the shared 0047AC10: the half width +152 and half
    /// depth +154 from the payload's DWORDs +48 and +4C, halved toward zero,
    /// the half height +150 from its WORD +44, and +28 from the half width.
    pub(in crate::object_runtime) fn initialize_selector74_payload(
        &mut self,
        zone: ObjectId,
        payload: Option<&Selector74ConstructorPayload>,
    ) -> Result<(), ObjectError> {
        let payload = payload.ok_or(ObjectError::MissingSelector74Payload(zone))?;
        let half = |value: u32| ((value as i32) / 2) as u16;
        let width = half(payload.dword_48.value);
        self.selector74_mut(zone)?.extents_150 =
            Some([payload.word_44.value, width, half(payload.dword_4c.value)]);
        self.owned_mut(zone)?.state.base.word_28 = width;
        Ok(())
    }

    /// 0042D6F0, the zone's +04: the shared 0047ADA0 through code 15
    /// (signed). Every other code reads its WORD first; 16 halves it
    /// (signed, toward zero) into the half width +152, 17 into the half
    /// depth +154, and 18 stores it at the half height +150; others refuse.
    pub fn apply_selector74_command(
        &mut self,
        zone: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(74) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector74(zone)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(zone, command, context);
        }
        let half = |value: u16| ((value as i16) / 2) as u16;
        let (axis, value) = match (code, &command.payload) {
            (16, InstanceCommandPayload::Selector74Width(value)) => (1, half(value.value)),
            (17, InstanceCommandPayload::Selector74Depth(value)) => (2, half(value.value)),
            (18, InstanceCommandPayload::Selector74Height(value)) => (0, value.value),
            (16..=18, _) => return Err(ActorCommandError::Payload(code)),
            (_, InstanceCommandPayload::Selector74Refused(_)) => return Ok(false),
            _ => return Err(ActorCommandError::Payload(code)),
        };
        let extents = self
            .selector74_mut(zone)?
            .extents_150
            .as_mut()
            .ok_or(ObjectError::Selector74ExtentsUnwritten(zone))?;
        extents[axis] = value;
        Ok(true)
    }

    /// 0042D950, the zone's +1C in place of the shared 0047AAD0 (so +74
    /// keeps the constructor's): the sphere +138 is the DWORD sum of the
    /// three signed extents' squares and +28 its root (0048EFC0); the circle
    /// +140 is the root of the width's and depth's; the count +144 is their
    /// signed sum over 800, toward zero.
    pub fn finish_selector74_initialization(&mut self, zone: ObjectId) -> Result<(), ObjectError> {
        let [height, width, depth] = self
            .selector74(zone)?
            .extents_150
            .ok_or(ObjectError::Selector74ExtentsUnwritten(zone))?
            .map(|extent| extent as i16);
        let radius2 = squared([height, depth, width]);
        let circle = squared([0, depth, width]);
        let root = |value: u32| inf_formats::native_basis::isqrt_48efc0(value as i32);
        self.owned_mut(zone)?.state.base.word_28 = root(radius2) as u16;
        let state = self.selector74_mut(zone)?;
        state.radius2_138 = Some(radius2);
        state.circle_140 = Some(root(circle));
        state.count_144 = Some(((i32::from(depth) + i32::from(width)) / POINT_SPACING) as u32);
        Ok(())
    }

    /// 0042D6B0, the zone's +98: message 7 marks it linked and counts a
    /// link, 8 takes one, each wrapping; every message answers 0.
    pub fn selector74_message(&mut self, zone: ObjectId, code: i32) -> Result<(), ObjectError> {
        let state = self.selector74_mut(zone)?;
        match code {
            7 => {
                state.linked_148 = 1;
                state.links_14c = state.links_14c.wrapping_add(1);
            }
            8 => state.links_14c = state.links_14c.wrapping_sub(1),
            _ => {}
        }
        Ok(())
    }

    /// Virtual +68 of an actor the zone meets: the 00455560 family's
    /// tables answer with the actor (00404610); every other actor table
    /// binds the null 00401300.
    fn selector74_answers_68(&self, actor: ObjectId) -> Result<bool, ObjectError> {
        Ok(match &self.actor(actor)?.extension {
            ActorExtension::BehaviorResource(state) => state.component_68(),
            ActorExtension::Derived(state) => match &state.class {
                DerivedActorClass::Base
                | DerivedActorClass::Family428650(_)
                | DerivedActorClass::Selector2(_)
                | DerivedActorClass::Selector11(_)
                | DerivedActorClass::Selector60(_)
                | DerivedActorClass::Selector77(_) => false,
            },
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector93(_) => false,
        })
    }

    fn selector74_position<E>(
        &self,
        zone: ObjectId,
        actor: ObjectId,
    ) -> Result<[u16; 3], Selector74Error<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(Selector74Error::Position { zone, actor })
    }

    /// 0042D570, the zone's +24. The timer gains `delta >> 6`. A linked
    /// zone with no links left retires itself and stops. Otherwise, over
    /// the actor list (G+48) in order, an actor whose +68 answers is
    /// retired when its squared distance (the signed WORD differences of
    /// the zone's position less its own) is below +138 (unsigned) and,
    /// turned into the zone's frame (0047A660), it lies strictly inside the
    /// box: |Z| < +154, |X| < +152 and |Y| < +150 (signed). The next node is
    /// read after each retirement.
    pub fn tick_selector74<H: Selector74Host>(
        &mut self,
        zone: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector74Error<H::Error>> {
        let state = self.selector74_mut(zone)?;
        state.timer_13c = state.timer_13c.map(|timer| timer.wrapping_add(delta >> 6));
        if state.linked_148 != 0 && state.links_14c == 0 {
            return host
                .retire(self, zone)
                .map_err(|error| Selector74Error::Retire {
                    zone,
                    actor: zone,
                    error,
                });
        }
        let unwritten = |offset| Selector74Error::Unwritten { zone, offset };
        let radius2 = state.radius2_138.ok_or(unwritten(0x138))?;
        let [height, width, depth] = state
            .extents_150
            .ok_or(unwritten(0x150))?
            .map(|extent| i32::from(extent as i16));
        let mut index = 0;
        while let Some(&actor) = self.actor_list().get(index) {
            index += 1;
            if !self.selector74_answers_68(actor)? {
                continue;
            }
            let own = self.selector74_position(zone, zone)?;
            let theirs = self.selector74_position(zone, actor)?;
            let difference: [i16; 3] =
                std::array::from_fn(|axis| own[axis].wrapping_sub(theirs[axis]) as i16);
            if squared(difference) >= radius2 {
                continue;
            }
            let local = self.actor(zone)?.rotating.basis.inverse_words(difference);
            let inside = |axis: usize, extent: i32| i32::from(local[axis]).abs() < extent;
            if !(inside(2, depth) && inside(0, width) && inside(1, height)) {
                continue;
            }
            host.retire(self, actor)
                .map_err(|error| Selector74Error::Retire { zone, actor, error })?;
            index = self
                .actor_list()
                .iter()
                .position(|&current| current == actor)
                .ok_or(ObjectError::LeftActorList(actor))?
                + 1;
        }
        Ok(())
    }

    /// 0042D7D0's points, the zone's +2C, before the ground: point `i` of
    /// +144 sits at the angle `i * 0x10000 / count` (signed) plus the timer
    /// on the circle +140, X from the sine and Z from the cosine, each
    /// clamped to the box's half width and depth, turned into the world
    /// (0047A6C0) and moved by the zone's X and Z. The caller sets each on
    /// the ground (00450A30) plus 0x32 and births it (00489A90(2, point, 0)).
    pub fn selector74_draw_points(
        &self,
        zone: ObjectId,
        sine: &[u16; 4096],
    ) -> Result<Vec<[u16; 2]>, Selector74Error<std::convert::Infallible>> {
        let state = self.selector74(zone)?;
        let unwritten = |offset| Selector74Error::Unwritten { zone, offset };
        let count = state.count_144.ok_or(unwritten(0x144))? as i32;
        if count <= 0 {
            return Ok(Vec::new());
        }
        let timer = state
            .timer_13c
            .ok_or(Selector74Error::TimerResidue { zone })?;
        let circle = state.circle_140.ok_or(unwritten(0x140))?;
        let [_, width, depth] = state.extents_150.ok_or(unwritten(0x150))?;
        let [x, _, z] = self.selector74_position(zone, zone)?;
        (0..count)
            .map(|point| {
                let angle = (point << 16).wrapping_div(count).wrapping_add(timer as i32);
                let local = [
                    clamp(wave(sine, angle, circle), width),
                    0,
                    clamp(wave(sine, angle.wrapping_add(0x4000), circle), depth),
                ];
                let world = self.transform_visual_vector_words(zone, local)?;
                Ok([world[0].wrapping_add(x), world[2].wrapping_add(z)])
            })
            .collect()
    }
}

/// `0042D90C`: a draw point's height over the ground under it (00450A30),
/// whose low WORD the draw keeps.
pub fn selector74_point_height(ground: i16) -> u16 {
    (ground as u16).wrapping_add(DRAW_LIFT)
}

#[cfg(test)]
#[path = "selector74_tests.rs"]
mod tests;
