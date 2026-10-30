//! Selector35 +120/+11C dying and +E0/+DC dormant callbacks. These do not
//! share the staged family10/36 dying program. Stack trail/burst positions
//! are captured here; radius and terrain services consume the live actor.
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorHandle, ActorServiceAllocationRequest, ActorServiceResourceKind,
        ActorStateChangeRequest, DerivedActorError, FixedPositionTarget, NativeTarget,
        NativeTargetClass, NativeTargetError, ObjectArena, ObjectError, ObjectId,
        TargetNullHandleSource,
    },
    process_particles::births::ParticleBurst,
};
use thiserror::Error;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector35LifecycleRequest {
    DyingChange { entering: bool },
    DyingUpdate,
    DormantChange { entering: bool },
    DormantUpdate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector35LifecycleEffect {
    Burst(ParticleBurst),
    Radius {
        block_address: u32,
        source: ActorHandle,
    },
    Trail {
        kind: u32,
        position: [u16; 3],
        velocity: [u16; 3],
    },
    TerrainScatter,
    ChangeState(ActorStateChangeRequest),
}
pub trait Selector35LifecycleHost: TargetNullHandleSource {
    type Error;
    fn particle_load_factor(&mut self) -> Result<u32, Self::Error>;
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: Selector35LifecycleEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}
#[derive(Debug, Error)]
pub enum Selector35LifecycleError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("selector35 {actor:?} requires field +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("selector35 requires current process NULL at {address:08X}")]
    MissingProcessNull { address: u32 },
    #[error("selector35 native zero-divisor trap at {address:08X}")]
    Division { address: u32 },
    #[error("selector35 lifecycle service at {address:08X} failed: {error}")]
    Service { address: u32, error: E },
}
impl ObjectArena {
    pub fn run_selector35_lifecycle<H: Selector35LifecycleHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        request: Selector35LifecycleRequest,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector35LifecycleError<H::Error>> {
        // 43DEB0 tests its entry byte before touching the receiver. Dormant
        // 43E120 has no such gate: false entry still runs burst and radius.
        if request == (Selector35LifecycleRequest::DyingChange { entering: false }) {
            return Ok(());
        }
        self.selector35_actor(actor)?;
        match request {
            Selector35LifecycleRequest::DyingChange { .. } => {
                self.family428650_actor_mut(actor)?.word_228 = 0x530d3c;
                let target = if let Some(reference) = self
                    .allocate_actor_service(ActorServiceAllocationRequest {
                        kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
                    })
                    .map_err(NativeTargetError::from)?
                {
                    // Native source capture follows allocation and precedes XYZ.
                    let source = host.target_null_handle().ok_or(
                        Selector35LifecycleError::MissingProcessNull { address: 0x43ded8 },
                    )?;
                    let position = self.selector35_lifecycle_position(actor)?;
                    self.construct_native_target(
                        reference,
                        NativeTarget::FixedPosition(FixedPositionTarget::from_point(
                            source,
                            Some(position),
                        )),
                    )?;
                    Some(reference)
                } else {
                    None
                };
                self.derived_actor_mut(actor)?.target_148 = target;
                self.selector35_actor_mut(actor)?.byte_238 = 0;
                self.selector35_actor_mut(actor)?.word_230 = 0;
                for axis in 0..3 {
                    let add = name_entry_rng_next(rng) as u16 & 255;
                    let value = self.actor(actor)?.words_8e_92[axis].wrapping_add(add);
                    self.actor_mut(actor)?.words_8e_92[axis] = value;
                }
                for axis in 0..3 {
                    let add = name_entry_rng_next(rng) & 255;
                    let value = self.actor(actor)?.words_94_9c[axis].wrapping_add(add);
                    self.actor_mut(actor)?.words_94_9c[axis] = value;
                }
                Ok(())
            }
            // 0043DFA0, the +11C dying update.
            Selector35LifecycleRequest::DyingUpdate => {
                let elapsed = self.family428650_actor(actor)?.words_188_19c[0].ok_or(
                    Selector35LifecycleError::RequiredField {
                        actor,
                        offset: 0x188,
                        address: 0x43dfa6,
                    },
                )?;
                if elapsed as i32 > 15_000_000 {
                    let source = host.target_null_handle().ok_or(
                        Selector35LifecycleError::MissingProcessNull { address: 0x43dfb2 },
                    )?;
                    return self.selector35_lifecycle_effect(
                        actor,
                        Selector35LifecycleEffect::ChangeState(ActorStateChangeRequest {
                            target_state: 0,
                            argument: 0,
                            source,
                        }),
                        0x43dfbc,
                        host,
                        rng,
                    );
                }
                let position = self.selector35_lifecycle_position(actor)?;
                let reference = self.derived_actor(actor)?.target_148.ok_or(
                    Selector35LifecycleError::RequiredField {
                        actor,
                        offset: 0x148,
                        address: 0x43dfcc,
                    },
                )?;
                let mut target = self.native_target(reference)?;
                if target.kind_word() == 8 {
                    target.set_focus_08_words([position[0], 0xec78, position[2]]);
                    self.write_native_target(reference, target)?;
                }
                let mut position = self.selector35_lifecycle_position(actor)?;
                if name_entry_rng_next(rng) & 3 == 0 {
                    self.actor_mut(actor)?.word_c8 = 0x7a11;
                    self.actor_mut(actor)?.word_ca = Some(0x37);
                }
                let size = self.object(actor)?.base.word_28;
                let kind = if size > 200 { 10 } else { 9 };
                let draw = name_entry_rng_next(rng) as u16;
                if size == 0 {
                    return Err(Selector35LifecycleError::Division { address: 0x43e058 });
                }
                position[0] = position[0].wrapping_add((draw % size).wrapping_sub(size >> 1));
                let size = self.object(actor)?.base.word_28;
                let draw = name_entry_rng_next(rng) as u16;
                if size == 0 {
                    return Err(Selector35LifecycleError::Division { address: 0x43e07b });
                }
                position[2] = position[2].wrapping_add((draw % size).wrapping_sub(size >> 1));
                let mut velocity = self.actor(actor)?.words_8e_92;
                velocity[0] = velocity[0]
                    .wrapping_add((name_entry_rng_next(rng) as u16 & 127).wrapping_sub(64));
                velocity[1] = velocity[1].wrapping_add(name_entry_rng_next(rng) as u16 & 127);
                velocity[2] = velocity[2]
                    .wrapping_add((name_entry_rng_next(rng) as u16 & 127).wrapping_sub(64));
                self.selector35_lifecycle_effect(
                    actor,
                    Selector35LifecycleEffect::Trail {
                        kind,
                        position,
                        velocity,
                    },
                    0x43e104,
                    host,
                    rng,
                )?;
                self.selector35_lifecycle_effect(
                    actor,
                    Selector35LifecycleEffect::TerrainScatter,
                    0x43e10d,
                    host,
                    rng,
                )
            }
            Selector35LifecycleRequest::DormantChange { entering } => {
                let burst = ParticleBurst {
                    debris_count: 16,
                    position: self.selector35_lifecycle_position(actor)?,
                    placement_radius: 20,
                    central_type: 2,
                    debris_types: [1, 1],
                    speed: 8,
                    flags: 0,
                };
                self.selector35_lifecycle_effect(
                    actor,
                    Selector35LifecycleEffect::Burst(burst),
                    0x43e17f,
                    host,
                    rng,
                )?;
                if self.object(actor)?.base.flags & 0x8000_0000 == 0 {
                    let source = self.actor(actor)?.handle;
                    self.selector35_lifecycle_effect(
                        actor,
                        Selector35LifecycleEffect::Radius {
                            block_address: 0x531250,
                            source,
                        },
                        0x43e1a2,
                        host,
                        rng,
                    )?;
                }
                // Complete shared 428630 suffix, with no flags or list writes.
                if entering {
                    self.family428650_actor_mut(actor)?.word_228 = 0x530d44;
                }
                Ok(())
            }
            Selector35LifecycleRequest::DormantUpdate => {
                let position = self.selector35_lifecycle_position(actor)?;
                let velocity = [0, name_entry_rng_next(rng) as u16 & 63, 0];
                let load = host.particle_load_factor().map_err(|error| {
                    Selector35LifecycleError::Service {
                        address: 0x43e1f3,
                        error,
                    }
                })?;
                let divisor = (0xc000u32.wrapping_sub(load) as i32) / 2048;
                if divisor >= 2 && (name_entry_rng_next(rng) as u16 as i32) % divisor != 0 {
                    return Ok(());
                }
                self.selector35_lifecycle_effect(
                    actor,
                    Selector35LifecycleEffect::Trail {
                        kind: 6,
                        position,
                        velocity,
                    },
                    0x43e240,
                    host,
                    rng,
                )
            }
        }
    }
    fn selector35_lifecycle_position<E>(
        &self,
        actor: ObjectId,
    ) -> Result<[u16; 3], Selector35LifecycleError<E>> {
        self.object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor).into())
    }
    fn selector35_lifecycle_effect<H: Selector35LifecycleHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        effect: Selector35LifecycleEffect,
        address: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector35LifecycleError<H::Error>> {
        host.effect(self, actor, effect, rng)
            .map_err(|error| Selector35LifecycleError::Service { address, error })
    }
}
