//! Selector11 virtual+AC 00402250. The actor's state1/health0 prefix belongs
//! to 0047D670; these ordered services retain their own live world custody.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use crate::{
    object_runtime::{
        ActorHandle, DerivedActorClass, DerivedActorError, ObjectArena, ObjectError, ObjectId,
    },
    process_particles::births::ParticleBurst,
    world_radius_effect::{RadiusEffectBlock, WorldRadiusEffectRequest},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector11RadiusBlock {
    /// Original actor+1A0 block, including its constructor-unwritten byte3.
    Actor1A0,
    /// Original ten-byte table00531250, not a copied actor block.
    Executable531250,
}

/// Both calls retain the actor position allocation; callbacks can mutate it
/// while the terrain/furniture, actor and network consumers execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector11RadiusOrigin {
    pub actor: ObjectId,
    pub block: Selector11RadiusBlock,
}

#[derive(Debug, Error)]
pub enum Selector11RadiusError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("selector11 radius effect reads unwritten actor position {0:?}")]
    MissingPosition(ObjectId),
}

impl Selector11RadiusOrigin {
    /// Live source read shared by terrain/furniture, actor and network phases.
    /// The block's byte3 stays unknown until an actual writer supplies it.
    pub fn effect_block(
        self,
        arena: &ObjectArena,
        executable: FrontendExecutable<'_>,
    ) -> Result<RadiusEffectBlock, Selector11RadiusError> {
        Ok(match self.block {
            Selector11RadiusBlock::Actor1A0 => {
                let state = arena.selector11_actor(self.actor)?;
                RadiusEffectBlock {
                    strength: state.word_1a0 as i16,
                    channel: state.byte_1a2,
                    byte_03: state.byte_1a3,
                    inner_radius: state.word_1a4,
                    outer_radius: state.word_1a6,
                    impulse: state.word_1a8,
                }
            }
            Selector11RadiusBlock::Executable531250 => RadiusEffectBlock::from_authored_bytes(
                executable
                    .mapped(0x0053_1250, 10)?
                    .bytes
                    .try_into()
                    .expect("ten-byte effect"),
            ),
        })
    }

    pub fn position_words(self, arena: &ObjectArena) -> Result<[u16; 3], Selector11RadiusError> {
        arena
            .object(self.actor)?
            .base
            .position_words
            .ok_or(Selector11RadiusError::MissingPosition(self.actor))
    }
}

pub trait Selector11RetirementHost {
    type Error;

    /// Complete 004887D0, including reached network delivery. Return count is
    /// ignored by 00402250. No successful empty default is provided.
    fn world_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Selector11RadiusOrigin>,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;

    /// Complete 0048A8B0 admission against actual process pool/RNG/audio. The
    /// stack record is a snapshot made after the first radius call returns.
    fn birth_burst(
        &mut self,
        arena: &mut ObjectArena,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;

    /// Actual 0047E190 on current actor XYZ and mutable H12. Native argument2
    /// means three RNG-selected cell operations after its terrain-height gate.
    fn scatter_terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        argument: i32,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;

    /// Actual 0048EAD0 on current mission custody; byte78 is freshly sign
    /// extended after scatter. The second argument is native force zero.
    fn placement_death(
        &mut self,
        arena: &mut ObjectArena,
        index: i8,
        force: i32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector11RetirementError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("selector11 retirement reads unwritten actor position {0:?}")]
    MissingPosition(ObjectId),
    #[error("required selector11 retirement service failed")]
    Host(E),
}

impl ObjectArena {
    ///00402250 preserves reached host/actor writes on error. It performs no
    /// lifecycle transition and does not skip a required service to succeed.
    pub fn retire_selector11<H: Selector11RetirementHost>(
        &mut self,
        actor: ObjectId,
        source: ActorHandle,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector11RetirementError<H::Error>> {
        self.selector11_actor(actor)?;
        if self.object(actor)?.base.flags & 0x8000_0000 == 0 {
            host.world_radius_effect(
                self,
                WorldRadiusEffectRequest {
                    origin: Selector11RadiusOrigin {
                        actor,
                        block: Selector11RadiusBlock::Actor1A0,
                    },
                    damage_source: source,
                    replicate: true,
                },
                rng,
            )
            .map_err(Selector11RetirementError::Host)?;
        }
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(Selector11RetirementError::MissingPosition(actor))?;
        host.birth_burst(
            self,
            ParticleBurst {
                debris_count: 16,
                position,
                placement_radius: 20,
                central_type: 1,
                debris_types: [1, 1],
                speed: 8,
                flags: 0,
            },
            rng,
        )
        .map_err(Selector11RetirementError::Host)?;
        //4022DB rereads flags after the burst; the two admission gates are
        //independent. Its EDI still identifies the live actor position.
        if self.object(actor)?.base.flags & 0x8000_0000 == 0 {
            host.world_radius_effect(
                self,
                WorldRadiusEffectRequest {
                    origin: Selector11RadiusOrigin {
                        actor,
                        block: Selector11RadiusBlock::Executable531250,
                    },
                    damage_source: source,
                    replicate: true,
                },
                rng,
            )
            .map_err(Selector11RetirementError::Host)?;
        }
        let DerivedActorClass::Selector11(state) = &mut self.derived_actor_mut(actor)?.class else {
            return Err(DerivedActorError::NotSelector11(actor).into());
        };
        state.word_1b2 = 2000;
        host.scatter_terrain(self, actor, 2, rng)
            .map_err(Selector11RetirementError::Host)?;
        let index = self.actor(actor)?.byte_78 as i8;
        host.placement_death(self, index, 0)
            .map_err(Selector11RetirementError::Host)
    }
}

#[cfg(test)]
mod tests;
