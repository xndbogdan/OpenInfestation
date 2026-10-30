//! Production virtual+B4 dispatch for the owned selector11 vtable0051F080.
//! Its 004283E0 prefix is shared across worlds. Reached live-health services
//! remain errors, preserving preceding arena writes. Source component+108
//! follows the actual final tables, as every damage host does.

use thiserror::Error;

use super::{ProcessActorDamageServices, ProcessDamageContext};
use crate::object_runtime::{
    ActorDamageCallback, ActorDamageReactionEffect, ActorDamageReactionError,
    ActorDamageReactionHost, ActorHandle, DerivedActorError, ObjectArena, ObjectError, ObjectId,
};

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum RequiredDamageReactionService {
    #[error("required0047CBC0 network-active query")]
    NetworkActive,
    #[error("required0048E7C0 difficulty multiplier")]
    DifficultyMultiplier,
    #[error("required0046B090 source player bonuses")]
    SourceBonuses,
    #[error("required005471C0 animation clock")]
    AnimationTick,
    #[error("required actor virtual+68 status exclusion")]
    ExcludesStatus,
    #[error("required actor virtual+64 recent damage source")]
    RecentSource,
    #[error("required recent source+5A8 animation timestamp")]
    RecentSourceTick,
    #[error("required recent source+5A6 actor owner")]
    RecentSourceOwner,
    #[error("required actor damage reaction effect {0:?}")]
    Effect(ActorDamageReactionEffect),
}

#[derive(Debug, Error)]
pub enum NativeProcessActorDamageError {
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Reaction(#[from] ActorDamageReactionError<RequiredDamageReactionService>),
    #[error(transparent)]
    Object(#[from] ObjectError),
}

/// The shared process RNG is consumed only by reached native reaction code.
/// ProcessActorDamageHost separately supplies the retained task/statistics
/// context; neither this dispatch nor its required services reset those owners.
pub struct NativeProcessActorDamageServices<'a> {
    rng: &'a mut u32,
    process: &'a crate::process_particles::factories::ParticleFactoryProcessState,
}

impl<'a> NativeProcessActorDamageServices<'a> {
    pub fn new(
        rng: &'a mut u32,
        process: &'a crate::process_particles::factories::ParticleFactoryProcessState,
    ) -> Self {
        Self { rng, process }
    }
}

impl ProcessActorDamageServices for NativeProcessActorDamageServices<'_> {
    type Error = NativeProcessActorDamageError;

    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.process.process_null_handle
    }

    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
        _: ProcessDamageContext<'_>,
    ) -> Result<(), Self::Error> {
        // This class check admits only the recovered0051F080/+B4 policy.
        // Other derived classes may have additional virtual reaction suffixes.
        arena.selector11_actor(actor)?;
        arena.react_derived_actor_damage(
            actor,
            callback,
            self.rng,
            &mut RequiredReaction {
                process: self.process,
            },
        )?;
        Ok(())
    }

    fn source_component_108(
        &mut self,
        arena: &mut ObjectArena,
        component: ObjectId,
        target: ObjectId,
        _: ProcessDamageContext<'_>,
    ) -> Result<(), Self::Error> {
        Ok(arena.source_component_108(component, target)?)
    }
}

struct RequiredReaction<'a> {
    process: &'a crate::process_particles::factories::ParticleFactoryProcessState,
}

impl ActorDamageReactionHost for RequiredReaction<'_> {
    type Error = RequiredDamageReactionService;
    type RecentSource = ();

    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.process.process_null_handle
    }

    fn network_active(&mut self) -> Result<bool, Self::Error> {
        Err(RequiredDamageReactionService::NetworkActive)
    }
    fn difficulty_multiplier(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<i16, Self::Error> {
        Err(RequiredDamageReactionService::DifficultyMultiplier)
    }
    fn source_bonuses(
        &mut self,
        _: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<[i8; 6]>, Self::Error> {
        Err(RequiredDamageReactionService::SourceBonuses)
    }
    fn animation_tick(&mut self) -> Result<u32, Self::Error> {
        Err(RequiredDamageReactionService::AnimationTick)
    }
    /// Settings `+10` (`00463AB0`), the game's mode.
    fn network_role(&mut self) -> Result<i32, Self::Error> {
        Ok(self.process.settings[0x10 / 4] as i32)
    }
    fn local_player_byte(&mut self) -> Result<u8, Self::Error> {
        Ok(self.process.local_slot)
    }
    fn excludes_status(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<bool, Self::Error> {
        Err(RequiredDamageReactionService::ExcludesStatus)
    }
    fn recent_damage_source(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<()>, Self::Error> {
        Err(RequiredDamageReactionService::RecentSource)
    }
    fn recent_source_tick(&mut self, _: &mut ObjectArena, _: ()) -> Result<u32, Self::Error> {
        Err(RequiredDamageReactionService::RecentSourceTick)
    }
    fn recent_source_owner_word(&mut self, _: &mut ObjectArena, _: ()) -> Result<u16, Self::Error> {
        Err(RequiredDamageReactionService::RecentSourceOwner)
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: ActorDamageReactionEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        Err(RequiredDamageReactionService::Effect(effect))
    }
}
