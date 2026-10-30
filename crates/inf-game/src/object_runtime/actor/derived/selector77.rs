//! Selector 77 construction 00407410 and postinitialization 00408090.
//! Its command, payload, and admission methods are inherited unchanged from
//! 00425E60. update owns the 00407CA0 NULL-target path and reached services;
//! target allocation/control and unknown allocation history remain explicit.
//! Selector 79, world 19's eyes, builds on it (00408740) with its own +04
//! (00408880), +24 (00408AB0), +28 (00408790), +34 (004088C0) and +98
//! (00408A70); its other virtuals are selector 77's. Selector 80, the members
//! world 19's spawner raises (00408FC0), has its own +24 (00409070) and +D4
//! (00409010).

use inf_formats::{
    native_basis::{isqrt_48efc0, orient_from_forward},
    InstanceCommandPayload, InstanceCommandView,
};

use super::super::allocation::ActorAllocationDwordCarry;
use super::{DerivedActorClass, DerivedActorConstructionRequest, DerivedActorError};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        actor::{
            behavior::select_actor_behavior, ActorCommandContext, ActorCommandError,
            ActorConstructionError,
        },
        ActorHandle, ObjectArena, ObjectError, ObjectId,
    },
};

/// 0055F280: the CRT-initialized world up vector.
const WORLD_UP: [f32; 3] = [0.0, 1.0, 0.0];

mod contact;
mod death;
pub use death::{EyeDeathError, EyeDeathHost, EYE_DEATH_CHILD, EYE_STALK_MODEL};
mod update;
pub use update::{
    Selector77Shot, Selector77TickRequest, Selector77UpdateError, Selector77UpdateHost,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector77State {
    /// 004959E0 zeroes exactly fourteen bytes starting at +184.
    pub words_184_190: [u16; 7],
    /// Written by +1C after the shared extent initialization. Allocation
    /// position remains unknown until a supported caller supplies it.
    pub position_192: Option<[u16; 3]>,
    /// A second process RNG output, zero-extended then shifted left twice.
    pub word_198: u32,
    pub word_19c: Option<u16>,
    pub word_19e: Option<u16>,
    pub word_1a0: Option<u32>,
    pub word_1a4: u32,
    pub word_1a8: u32,
    pub word_1ac: Option<u32>,
    pub handle_1b0: ActorHandle,
    /// Selector 79's fields (00408740); none for selector 77's 0051FBB0.
    pub pair: Option<Box<Selector79State>>,
    /// Selector 80's fields (00408FC0).
    pub member: Option<Selector80State>,
}

/// A member world 19's spawner raises (selector 80, table 0051FD68): it rises
/// out of the liquid, aims as selector 77 does, and sinks after 15 s up, or
/// 5 s up without a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector80State {
    /// +1BC: 1 rising, 0 up, -1 sinking.
    pub rise_1bc: i32,
    /// +1B8: rising, whether it has been put under the liquid; up, the time
    /// up.
    pub up_1b8: u32,
    /// +1B4, the time up without a target.
    pub idle_1b4: u32,
}

/// World 19's eyes (selector 79, table 0051FC8C): each names its partner
/// (command 19), the first after-load makes it the pair's leader, and the
/// pair strikes what crosses between them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector79State {
    /// WORDs +1B4..+1B8, the midpoint to the partner (the leader's after-load).
    pub midpoint_1b4: Option<[u16; 3]>,
    /// WORD +1BA, three quarters of the distance to the partner.
    pub reach_1ba: Option<u16>,
    /// WORD +1BC, the target's side of the pair's plane at its last update.
    pub side_1bc: Option<u16>,
    /// +1C0, the time aimed at the current target.
    pub aimed_1c0: Option<u32>,
    /// +1C4: the partner's placement reference, its handle after the after-load.
    pub partner_1c4: u32,
    /// +1C8, the target's handle.
    pub target_1c8: ActorHandle,
    /// +1CC, the actor to strike.
    pub struck_1cc: ActorHandle,
    /// +1D0, the strike clock: 0, arming to 1000000, then cooling to 7500000.
    pub strike_1d0: u32,
    /// BYTE +1D4: 1 while the eye aims for itself, 0 while it follows.
    pub leader_1d4: u8,
}

/// What an eye's +98 leaves to its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector79Message {
    Done,
    /// 00408AA6: the follower strikes (00408DA0).
    Strike,
}

impl ObjectArena {
    /// 00407410 calls 00425E60 first. Both constructors consume the same
    /// caller-owned RNG word, one step each; this class has no private seed.
    pub fn allocate_selector77_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_derived_actor(request, rng)?;
        let handle_1b0 = self.actor_handles().unwrap().null_handle();
        let word_198 = name_entry_rng_next(rng) << 2;
        let word_19c = self.actor_allocation.initial_word();
        let word_1a0 = self
            .actor_allocation
            .initial_dword(ActorAllocationDwordCarry::Eye1a0);
        self.derived_actor_mut(object)?.class =
            DerivedActorClass::Selector77(Box::new(Selector77State {
                words_184_190: [0; 7],
                position_192: None,
                word_198,
                word_19c,
                word_19e: None,
                word_1a0,
                word_1a4: 0,
                word_1a8: 0,
                word_1ac: None,
                handle_1b0,
                pair: None,
                member: None,
            }));
        Ok(object)
    }

    /// 00408FC0: 00407410, then +1BC = 1 (rising), +1B4 = +1B8 = 0.
    pub fn allocate_selector80_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_selector77_actor(request, rng)?;
        let DerivedActorClass::Selector77(state) = &mut self.derived_actor_mut(object)?.class
        else {
            unreachable!("selector 77 allocated above");
        };
        state.member = Some(Selector80State {
            rise_1bc: 1,
            up_1b8: 0,
            idle_1b4: 0,
        });
        Ok(object)
    }

    pub fn selector80(&self, object: ObjectId) -> Result<Selector80State, DerivedActorError> {
        self.selector77_actor(object)?
            .member
            .ok_or(DerivedActorError::NotSelector80(object))
    }

    pub(in crate::object_runtime) fn selector80_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector80State, DerivedActorError> {
        match &mut self.derived_actor_mut(object)?.class {
            DerivedActorClass::Selector77(state) => state
                .member
                .as_mut()
                .ok_or(DerivedActorError::NotSelector80(object)),
            _ => Err(DerivedActorError::NotSelector80(object)),
        }
    }

    /// 00408740: 00407410, then +1C4 = +1C8 = +1CC = 0054E894, +1D0 = 0
    /// and BYTE +1D4 = 1.
    pub fn allocate_selector79_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_selector77_actor(request, rng)?;
        let null = self.actor_handles().unwrap().null_handle();
        let pair = Selector79State {
            midpoint_1b4: None,
            reach_1ba: None,
            side_1bc: self.actor_allocation.initial_extended_word(),
            aimed_1c0: self
                .actor_allocation
                .initial_dword(ActorAllocationDwordCarry::Selector79Aimed1c0),
            partner_1c4: null.raw(),
            target_1c8: null,
            struck_1cc: null,
            strike_1d0: 0,
            leader_1d4: 1,
        };
        let DerivedActorClass::Selector77(state) = &mut self.derived_actor_mut(object)?.class
        else {
            unreachable!("selector 77 allocated above");
        };
        state.pair = Some(Box::new(pair));
        Ok(object)
    }

    pub fn selector79(&self, object: ObjectId) -> Result<&Selector79State, DerivedActorError> {
        self.selector77_actor(object)?
            .pair
            .as_deref()
            .ok_or(DerivedActorError::NotSelector79(object))
    }

    pub(in crate::object_runtime) fn selector79_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector79State, DerivedActorError> {
        match &mut self.derived_actor_mut(object)?.class {
            DerivedActorClass::Selector77(state) => state
                .pair
                .as_deref_mut()
                .ok_or(DerivedActorError::NotSelector79(object)),
            _ => Err(DerivedActorError::NotSelector79(object)),
        }
    }

    /// 00408880, the eye's +04: codes through 18 are selector 77's
    /// (00426030), 19 names the partner's placement, and any other refuses.
    pub fn apply_selector79_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        self.selector79(object)?;
        match (command.code.value, &command.payload) {
            (..=18, _) => self.apply_derived_actor_command(object, command, context),
            (19, InstanceCommandPayload::Selector79Link(value)) => {
                self.selector79_mut(object)?.partner_1c4 = value.value;
                Ok(true)
            }
            (19, _) => Err(ActorCommandError::Payload(19)),
            _ => Ok(false),
        }
    }

    /// 004088C0, the eye's after-load: +1C4 becomes the partner's handle
    /// (0047F7B0), and an eye without a partner disposes itself (00479FA0).
    /// A leader then faces across the line to its partner (00490370 with
    /// the world's up), keeps the midpoint and three quarters of the
    /// distance, and makes the partner its follower (its +98, message 3).
    pub fn initialize_selector79_after_load(
        &mut self,
        object: ObjectId,
    ) -> Result<(), DerivedActorError> {
        let reference = self.selector79(object)?.partner_1c4;
        if reference != 0 {
            let handle = self.actor_handle_by_placement(reference as i32)?;
            self.selector79_mut(object)?.partner_1c4 = handle.raw();
        }
        let partner = self.selector79(object)?.partner_1c4;
        let Some(partner) = self.resolve_actor_handle(ActorHandle::from_raw(partner))? else {
            self.dispose(object)?;
            return Ok(());
        };
        if self.selector79(object)?.leader_1d4 == 0 {
            return Ok(());
        }
        let position = |arena: &Self, actor: ObjectId| {
            arena
                .object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))
        };
        let own = position(self, object)?;
        let theirs = position(self, partner)?;
        let d: [i16; 3] = std::array::from_fn(|axis| theirs[axis].wrapping_sub(own[axis]) as i16);
        let squared = d.iter().fold(0_i32, |sum, &axis| {
            sum.wrapping_add(i32::from(axis).wrapping_mul(i32::from(axis)))
        });
        // A sum with bit 31 set is quartered before the root, then doubled.
        let distance = if squared < 0 {
            isqrt_48efc0(((squared as u32) >> 2) as i32) << 1
        } else {
            isqrt_48efc0(squared)
        } as i32;
        // 00408975..004089F2 under the CRT's 53-bit precision: the length
        // is stored as an f32, and so is each quotient.
        let (x, z) = (f64::from(d[0]), f64::from(d[2].wrapping_neg()));
        let length = f64::from(((x * x) + (z * z)).sqrt() as f32);
        let forward = if length == 0.0 {
            [1.0, 0.0, 0.0]
        } else {
            [(z / length) as f32, 0.0, (x / length) as f32]
        };
        orient_from_forward(
            &mut self.actor_mut(object)?.rotating.basis,
            forward,
            WORLD_UP,
            false,
        );
        let half = |axis: usize| own[axis].wrapping_add((i32::from(d[axis]) / 2) as u16);
        let midpoint = [half(0), half(1), half(2)];
        let pair = self.selector79_mut(object)?;
        pair.reach_1ba = Some((distance.wrapping_mul(3) / 4) as u16);
        pair.midpoint_1b4 = Some(midpoint);
        // Every partner in the campaign is an eye; another's +98 stops here.
        self.selector79_message(partner, Some(object), 3)?;
        Ok(())
    }

    /// 00408A70, the eye's +98: message 3 makes it a follower, and message 2
    /// names the actor a follower strikes (00408DA0, the update's to run).
    /// Every message answers 0.
    pub(in crate::object_runtime) fn selector79_message(
        &mut self,
        object: ObjectId,
        sender: Option<ObjectId>,
        code: i32,
    ) -> Result<Selector79Message, DerivedActorError> {
        let leader = self.selector79(object)?.leader_1d4;
        Ok(match (code, sender) {
            (3, _) => {
                self.selector79_mut(object)?.leader_1d4 = 0;
                Selector79Message::Done
            }
            (2, Some(sender)) if leader == 0 => {
                let handle = self.actor(sender)?.handle;
                self.selector79_mut(object)?.struck_1cc = handle;
                Selector79Message::Strike
            }
            _ => Selector79Message::Done,
        })
    }

    #[cfg(test)]
    pub(crate) fn supply_selector77_middle_pose_fixture(
        &mut self,
        object: ObjectId,
        word_190: u16,
    ) -> Result<(), DerivedActorError> {
        self.selector77_actor(object)?;
        let DerivedActorClass::Selector77(state) = &mut self.derived_actor_mut(object)?.class
        else {
            unreachable!("typed fixture checked before mutation");
        };
        state.words_184_190[6] = word_190;
        Ok(())
    }

    pub fn selector77_actor(
        &self,
        object: ObjectId,
    ) -> Result<&Selector77State, DerivedActorError> {
        match &self.derived_actor(object)?.class {
            DerivedActorClass::Selector77(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector77(object)),
        }
    }

    /// 00408090 reselects an existing behavior after commands may change
    /// +B4, invokes shared +1C, then copies exactly the raw position WORDs.
    /// It does not publish the factory's initialized flag or clear dirty bits.
    pub fn finish_selector77_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), DerivedActorError> {
        self.selector77_actor(object)?;
        let flags = self.object(object)?.base.flags;
        let selector_b4 = self.actor(object)?.word_b4;
        if let Some(behavior) = &mut self.derived_actor_mut(object)?.behavior_140 {
            select_actor_behavior(behavior, flags, selector_b4);
        }
        self.finish_actor_initialization(object)?;
        let position = self.object(object)?.base.position_words;
        let DerivedActorClass::Selector77(state) = &mut self.derived_actor_mut(object)?.class
        else {
            unreachable!("class checked before postinitialization");
        };
        state.position_192 = position;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector77_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "selector77_native_tests.rs"]
mod native_tests;
