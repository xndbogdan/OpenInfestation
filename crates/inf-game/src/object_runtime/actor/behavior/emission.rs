//! Shared retained firing drain 0048CDB0 and reached emitter 0048C890.
//! Queued direction edits and completed effects survive a later service limit.
//! The node is unlinked and released only after normal emitter return.

use inf_formats::FrontendExecutable;
use thiserror::Error;

use super::damage_cache::storage::{read_row_byte, read_row_word};
use super::{
    ActorBehaviorPositionError, ActorBehaviorTable, BehaviorCacheStorageError,
    BehaviorRowSelection, ProcessActorBehaviorTables,
};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorServiceHeapError, DerivedActorError, NativeHeapRef, NativeWeaponFireNode, ObjectArena,
        ObjectError, ObjectId,
    },
    process_particles::{
        audio::ParticleSoundRequest,
        factories::{
            ParticleCreationNotice, ParticleFactoryContext, ParticleFactoryError,
            ParticleFactoryHost, ParticleFactoryImpactRequest, ParticleFactoryKind,
            ParticleFactoryProcessState, ParticleFactoryRequest, ParticleFactorySources,
            ParticleVelocitySource,
        },
        ParticleVectorBytes, ProcessParticleRuntime,
    },
    process_sound::{
        ProcessSoundRecordError, ProcessSoundRuntime, RequiredProcessSoundVoiceTeardown,
    },
    world_mission::RECORD_0C_BYTES,
    world_statistics::{ActorStatisticCategory, WorldStatistics},
    ResidentRuntime,
};

pub struct WeaponFiringContext<'a> {
    pub tables: &'a mut ProcessActorBehaviorTables,
    /// The current object-pass loan, including previous model RNG draws.
    pub rng: &'a mut u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponEmissionRequest {
    pub actor: ObjectId,
    pub node: NativeHeapRef,
    pub position: [u16; 3],
}

/// The drain owns the prefix and normal-return unlink. Tests may stop at the
/// actual emitter boundary; production supplies WeaponFiringRuntime below.
pub trait WeaponFiringEmitter {
    type Error;
    fn emit(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut WeaponFiringContext<'_>,
        request: WeaponEmissionRequest,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponFiringDrainSummary {
    pub emitted: usize,
}

#[derive(Debug, Error)]
pub enum WeaponFiringDrainError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Position(#[from] ActorBehaviorPositionError),
    #[error(transparent)]
    Storage(#[from] BehaviorCacheStorageError),
    #[error(transparent)]
    Heap(#[from] ActorServiceHeapError),
    #[error("actor {0:?} requires its current +140 firing behavior")]
    MissingBehavior(ObjectId),
    #[error("firing node {0:?} was stopped during a drain; replay is blocked")]
    PreviouslyStopped(NativeHeapRef),
    #[error("0048C890 emission stopped for {node:?}: {source}")]
    Emission { node: NativeHeapRef, source: E },
}

fn queued_row(
    arena: &ObjectArena,
    actor: ObjectId,
    selection: u32,
) -> Result<BehaviorRowSelection, DerivedActorError> {
    let table = arena
        .actor_behavior_storage(actor)?
        .as_ref()
        .expect("behavior checked by entry")
        .table_14;
    // This consumer masks a WORD, unlike signed BYTE current selection.
    let offset = (selection & 0xffff).wrapping_mul(44);
    Ok(if table == ActorBehaviorTable::Selector2Copy {
        BehaviorRowSelection::VehicleCopy {
            owner: actor,
            byte_offset: offset as i32,
        }
    } else {
        BehaviorRowSelection::SharedAddress {
            address: (table as u32).wrapping_add(offset),
        }
    })
}

impl ObjectArena {
    pub fn drain_actor_firing<H: WeaponFiringEmitter>(
        &mut self,
        actor: ObjectId,
        context: &mut WeaponFiringContext<'_>,
        emitter: &mut H,
    ) -> Result<WeaponFiringDrainSummary, WeaponFiringDrainError<H::Error>> {
        let behavior = self
            .actor_behavior_storage(actor)?
            .as_ref()
            .ok_or(WeaponFiringDrainError::MissingBehavior(actor))?;
        if let Some(stopped) = behavior.firing_stop_00 {
            return Err(WeaponFiringDrainError::PreviouslyStopped(stopped));
        }
        let mut reached = None;
        let result = self.drain_actor_firing_inner(actor, context, emitter, &mut reached);
        if result.is_err() {
            if let Some(reference) = reached {
                // Stop this actor's chain on every reached failure, including
                // direction/position prefixes and a free failure after head
                // publication. Newly prepended nodes cannot restart it.
                if let Some(behavior) = self.actor_behavior_storage_mut(actor)?.as_mut() {
                    behavior.firing_stop_00 = Some(reference);
                }
            }
        }
        result
    }

    fn drain_actor_firing_inner<H: WeaponFiringEmitter>(
        &mut self,
        actor: ObjectId,
        context: &mut WeaponFiringContext<'_>,
        emitter: &mut H,
        reached: &mut Option<NativeHeapRef>,
    ) -> Result<WeaponFiringDrainSummary, WeaponFiringDrainError<H::Error>> {
        let mut summary = WeaponFiringDrainSummary::default();
        loop {
            let behavior = self
                .actor_behavior_storage(actor)?
                .as_ref()
                .ok_or(WeaponFiringDrainError::MissingBehavior(actor))?;
            let Some(reference) = behavior.fire_head_00 else {
                return Ok(summary);
            };
            *reached = Some(reference);
            let lead = behavior.word_20 as i32;
            let mut node = self.actor_service_heap().weapon_fire_node(reference)?;
            if lead != 0 && node.selection & 0x10000 == 0 {
                let row = queued_row(self, actor, node.selection)?;
                let speed = i32::from(read_row_word(self, context.tables, row, 2)? as i16);
                let divisor = if lead <= 64 {
                    speed.wrapping_shl(6)
                } else {
                    speed.wrapping_mul(lead)
                };
                if divisor != 0 {
                    let factor = f64::from(i32::MAX / divisor);
                    let position = self.actor_behavior_group_position(actor, node.group)?;
                    let origin = self
                        .object(actor)?
                        .base
                        .position_words
                        .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                    for axis in 0..3 {
                        let delta = f64::from(position[axis].wrapping_sub(origin[axis]) as i16);
                        // FPCW027F uses double precision between the explicit
                        // single stores. 00528B08 is the exact power 2^-21.
                        let correction = (delta * factor) * f64::from(f32::from_bits(0x3500_0000));
                        node.direction_bits[axis] =
                            ((f64::from(f32::from_bits(node.direction_bits[axis])) - correction)
                                as f32)
                                .to_bits();
                        self.actor_service_heap_mut()
                            .write_weapon_fire_node(reference, node)?;
                    }
                }
            }
            let mut position = self.actor_behavior_group_position(actor, node.group)?;
            if node.compensation != 0 {
                // Native SHR32 (not signed SAR) precedes low-DWORD IMUL/SAR15.
                let compensation = (node.compensation as u32 >> 5) as i32;
                let velocity = self.actor(actor)?.words_8e_92;
                for axis in 0..3 {
                    let delta = i32::from(velocity[axis] as i16).wrapping_mul(compensation) >> 15;
                    position[axis] = position[axis].wrapping_sub(delta as u16);
                }
            }
            if let Err(source) = emitter.emit(
                self,
                context,
                WeaponEmissionRequest {
                    actor,
                    node: reference,
                    position,
                },
            ) {
                return Err(WeaponFiringDrainError::Emission {
                    node: reference,
                    source,
                });
            }
            // 48CF42 rereads next only after the synchronous emitter returns.
            let next = self.actor_service_heap().weapon_fire_node(reference)?.next;
            self.actor_behavior_storage_mut(actor)?
                .as_mut()
                .ok_or(WeaponFiringDrainError::MissingBehavior(actor))?
                .fire_head_00 = next;
            self.actor_service_heap_mut().release(reference)?;
            summary.emitted += 1;
        }
    }
}

/// Reached services with no admitted implementation. Construction of526EFC
/// remains owned; its later update/draw and489600 impact are distinct limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum WeaponEmissionLimit {
    #[error("actor virtual+48 velocity basis required at0048C8ED")]
    ActorVelocityBasis,
    #[error("unowned firing factory {0:08X} at0048C9AC")]
    Factory(u32),
    #[error("actor-resource firing {resource} flags{flags:02X} required at0048C9CE")]
    ActorResource { resource: u32, flags: u8 },
    /// 0048CA02: a live earlier shot of an unported class answers +98.
    #[error("weapon group's earlier shot {shot:?} needs its class's virtual +98 at0048CA02")]
    ShotRetrigger { shot: ObjectId },
    /// 0048CA37: resource 0x82 goes to 00431EF0, which needs the world's
    /// factory for its registry object.
    #[error("firing resource 0x82 requires 00431EF0 at0048CA37")]
    Resource82,
    /// 0048CB21: with upgrade 0x26 the detonator sends its pulse 0048D980,
    /// whose map jitter and flash need the world's owners, here not lent.
    #[error("the detonator's pulse 0048D980 (upgrade 0x26) needs the world's map and flash")]
    Pulse,
    #[error("particle network notice00492500 required")]
    NetworkNotice,
    #[error("particle impact00489600 required")]
    Impact489600,
}

#[derive(Debug, Error)]
pub enum WeaponEmissionError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Storage(#[from] BehaviorCacheStorageError),
    #[error(transparent)]
    Heap(#[from] ActorServiceHeapError),
    #[error(transparent)]
    Limit(#[from] WeaponEmissionLimit),
    #[error(transparent)]
    Factory(#[from] ParticleFactoryError<WeaponEmissionLimit>),
    #[error(transparent)]
    Sound(#[from] ProcessSoundRecordError),
    #[error("actor {0:?} requires its current +140 firing behavior")]
    MissingBehavior(ObjectId),
    #[error("the shot's construction stopped: {0}")]
    Shot(String),
    #[error("0048CB71 extra particle: {0}")]
    ExtraParticle(String),
    #[error("the detonator 0048CB00: {0}")]
    Detonator(String),
}

/// The world's part of Sonic Shock's pulse `0048D980`, after its event 9.
pub trait DetonatorPulseHost {
    /// `0048D9AB`: the map's jitter, `G+1B4` `+24 = 0x1FFF`.
    fn jam_map(&mut self) -> Result<(), String>;
    /// `0048D9B8`: `00487D70(position, 0xF00, 0)`, the world flash.
    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String>;
}

pub struct WeaponFiringRuntime<'a, 'exe> {
    pub executable: FrontendExecutable<'exe>,
    pub factory_process: &'a ParticleFactoryProcessState,
    pub particles: &'a mut ProcessParticleRuntime,
    /// Captured G+1BC owner; None is the native null-statistics branch.
    pub statistics: Option<&'a mut [u8; RECORD_0C_BYTES]>,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub residency: &'a ResidentRuntime,
    pub phase: Option<u32>,
    /// The owner's +4C (0047AAF0) for the actor-resource arm; None keeps
    /// that arm at its boundary.
    pub children: Option<
        &'a mut dyn crate::object_runtime::ActorChildHost<
            Error = crate::object_runtime::Selector36ChildServiceError,
        >,
    >,
    /// Sonic Shock's map jitter and flash; None keeps the pulse at its
    /// boundary.
    pub pulse: Option<&'a mut dyn DetonatorPulseHost>,
}

/// WeaponHandleResidueApproximation: behavior +24 is allocated without its
/// words being written, so a weapon group's first 0048C9EA read is heap
/// residue. Infest01's first cannon shot read 03842EB0, which 0047AD90
/// resolved to no actor, as every first read recorded did; the port treats an
/// unwritten word as no earlier shot.
fn earlier_shot(
    arena: &mut ObjectArena,
    entry: Option<u32>,
) -> Result<Option<ObjectId>, ObjectError> {
    match entry {
        Some(raw) => arena
            .resolve_actor_handle(crate::object_runtime::ActorHandle::from_raw(raw))
            .map_err(|_| ObjectError::MissingActorRegistry),
        None => Ok(None),
    }
}

/// 0048CB2C: the detonator's reach, and 0048CB3A's hint.
const DETONATOR_RADIUS: u32 = 4000;
const DETONATOR_HINT: u32 = 0x30;
/// 0048CB12: Sonic Shock, the detonator's upgrade.
const SONIC_SHOCK: u32 = 0x26;
/// 0048D989: the pulse's event, which sets off any explosive.
const PULSE_EVENT: i32 = 9;

impl WeaponFiringRuntime<'_, '_> {
    /// 0048CB00, the detonator (registry kind 23, resource 0). The owner's
    /// player (0046B090) with upgrade 0x26 sends the pulse 0048D980 instead.
    /// Otherwise event 10 goes to the actors within 4000 of the owner
    /// (0047DD20), so the grenades and mines it owns mark themselves to
    /// detonate, and its player gets hint 0x30 (0046C760).
    fn detonate(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), WeaponEmissionError> {
        let failed =
            |error: &dyn std::fmt::Display| WeaponEmissionError::Detonator(error.to_string());
        let lookup = crate::object_runtime::PlayerActorLookupContext {
            process: Some(self.factory_process),
        };
        let handle = arena.actor(actor)?.handle;
        let player = arena
            .find_player_for_actor_handle(
                crate::object_runtime::PlayerActorLookupRequest { handle },
                lookup,
            )
            .map_err(|error| failed(&error))?;
        if let Some(player) = player {
            if arena
                .player_upgrade_owned(player, SONIC_SHOCK)
                .map_err(|error| failed(&error))?
            {
                return self.sonic_shock(arena, actor, lookup, rng);
            }
        }
        arena
            .notify_actor_owner_event_10(actor, DETONATOR_RADIUS, lookup)
            .map_err(|error| failed(&error))?;
        // 0048CB3E calls 0046C760 on the player it found; without one retail
        // would fault there.
        let player = player.ok_or_else(|| failed(&"the hint needs the owner's player"))?;
        arena
            .apply_player_command(
                crate::object_runtime::PlayerCommandRequest {
                    player,
                    operation: crate::object_runtime::PlayerCommandOperation::Set {
                        raw_id: DETONATOR_HINT,
                        silent: 0,
                    },
                },
                &crate::object_runtime::PlayerCommandContext {
                    process: Some(self.factory_process),
                    executable: Some(self.executable),
                    resident: Some(self.residency),
                },
            )
            .map(|_| ())
            .map_err(|error| failed(&error))
    }
}

impl WeaponFiringRuntime<'_, '_> {
    /// 0048D980, Sonic Shock's pulse from `owner`: event 9 to every actor
    /// within 4000 (0047DD20), which sets off grenades, mines, missiles and
    /// Bombs whoever owns them, then the map's jitter and the world flash at
    /// the owner. There is no hint.
    pub(crate) fn sonic_shock(
        &mut self,
        arena: &mut ObjectArena,
        owner: ObjectId,
        lookup: crate::object_runtime::PlayerActorLookupContext<'_>,
        rng: &mut u32,
    ) -> Result<(), WeaponEmissionError> {
        use crate::object_runtime::{
            ActorMessage, ActorMessageReply, ActorMessageWalk, VehicleMessageEffect,
        };
        let failed = |error: String| WeaponEmissionError::Detonator(error);
        let network = self.factory_process.network_flag != 0;
        arena.walk_message_receivers(
            owner,
            ActorMessageWalk::Radius {
                radius: DETONATOR_RADIUS,
            },
            |arena, receiver, distance_squared| {
                let reply = arena
                    .receive_actor_message(
                        receiver,
                        ActorMessage {
                            sender: Some(owner),
                            distance_squared,
                            code: PULSE_EVENT,
                            argument: 0,
                        },
                        lookup,
                        network,
                    )
                    .map_err(|error| failed(error.to_string()))?;
                match reply {
                    ActorMessageReply::Done(value) => Ok(value),
                    ActorMessageReply::Vehicle(VehicleMessageEffect::None) => Ok(0),
                    other => Err(failed(format!(
                        "event 9 leaves {receiver:?}'s {other:?} to the world"
                    ))),
                }
            },
        )?;
        let host = self
            .pulse
            .as_deref_mut()
            .ok_or(WeaponEmissionLimit::Pulse)?;
        host.jam_map().map_err(failed)?;
        let position = arena
            .object(owner)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(owner))?;
        host.flash(arena, position, rng).map_err(failed)?;
        // 0048D9E0: a local owner's pulse goes to every machine (20).
        if network && arena.object(owner)?.base.flags & 0x8000_0000 == 0 {
            let handle = arena.actor(owner)?.handle.raw();
            arena.queue_network_send(20, crate::network::protocol::GameSend::Dword(handle));
        }
        Ok(())
    }
}

/// 00431F68: the flamethrower's registry resource.
const FLAMETHROWER_RESOURCE: u16 = 0x82;

impl WeaponFiringRuntime<'_, '_> {
    /// 00431EF0, resource 0x82's flamethrower (selector 76): the registry
    /// object built with the process null (004600F0). A local owner's flame
    /// goes to every machine first (00431F5F, message 25). The object keeps
    /// the owner's handle at +6C and runs its update once with no time,
    /// which only follows the owner. A refused construction builds nothing.
    pub(crate) fn throw_flame(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), WeaponEmissionError> {
        if arena.object(actor)?.base.flags & 0x8000_0000 == 0
            && self.factory_process.network_flag != 0
        {
            let handle = arena.actor(actor)?.handle.raw();
            arena.queue_network_send(25, crate::network::protocol::GameSend::Dword(handle));
        }
        let children = self
            .children
            .as_deref_mut()
            .ok_or(WeaponEmissionLimit::Resource82)?;
        let flame = children
            .create_registry_object(arena, FLAMETHROWER_RESOURCE)
            .map_err(|error| WeaponEmissionError::Shot(error.to_string()))?;
        if let Some(flame) = flame {
            let handle = arena.actor(actor)?.handle;
            arena.arm_selector76(flame, handle)?;
            arena.selector76_follow(flame)?;
        }
        Ok(())
    }
}

/// The selectors whose shots the port builds: the cannon's (32), the
/// Lasers (31), the grenade (26), the row-12 charge (27), the mine (38),
/// the missiles (25 and 28) and the Cranial Implanter's implant (68), whose
/// update, light, draw and contacts are ported.
const PORTED_SHOT_SELECTORS: [u16; 8] = [25, 26, 27, 28, 31, 32, 38, 68];
/// 00520A2C: the scale of a registry-flag-8 launch along the owner's basis.
const BASIS_LAUNCH: f64 = -128.0;

struct FactoryServices {
    phase: Option<u32>,
}
impl ParticleFactoryHost for FactoryServices {
    type Error = WeaponEmissionLimit;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase)
    }
    fn network_notice(
        &mut self,
        notice: ParticleCreationNotice,
        particles: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        crate::process_particles::factories::queue_creation_notice(particles, notice, sources)
            .map_err(|_| WeaponEmissionLimit::NetworkNotice)
    }
    fn impact_489600(
        &mut self,
        _: ParticleFactoryImpactRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        Err(WeaponEmissionLimit::Impact489600)
    }
}

fn registry_byte(
    tables: &ProcessActorBehaviorTables,
    address: u32,
) -> Result<u8, BehaviorCacheStorageError> {
    tables.read_process_bytes(address, 1)?[0].ok_or(BehaviorCacheStorageError::Unknown { address })
}

impl WeaponFiringEmitter for WeaponFiringRuntime<'_, '_> {
    type Error = WeaponEmissionError;
    fn emit(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut WeaponFiringContext<'_>,
        request: WeaponEmissionRequest,
    ) -> Result<(), Self::Error> {
        let WeaponEmissionRequest {
            actor,
            node: reference,
            position,
        } = request;
        if arena.actor_behavior_storage(actor)?.is_none() {
            return Err(WeaponEmissionError::MissingBehavior(actor));
        }
        let node: NativeWeaponFireNode = arena.actor_service_heap().weapon_fire_node(reference)?;
        let row = queued_row(arena, actor, node.selection)?;
        let kind = i32::from(read_row_word(arena, context.tables, row, 0)? as i16);
        let registry = 0x0053_dba8_u32.wrapping_add(kind.wrapping_mul(16) as u32);
        let mut speed = i32::from(read_row_word(arena, context.tables, row, 2)? as i16);
        let inherited = arena.actor(actor)?.words_8e_92;
        let velocity: [u16; 3] = if registry_byte(context.tables, registry + 12)? & 8 != 0 {
            // 0048C8E9..0048C962: the owner's +48 basis (+44), its row at
            // +10 times -128.0 (00520A2C), each axis truncated; neither the
            // node's direction nor the row's speed.
            let basis = arena.actor(actor)?.rotating.basis;
            std::array::from_fn(|axis| {
                let value = f64::from(f32::from_bits(basis.words[4 + axis])) * BASIS_LAUNCH;
                (super::super::derived::native_trunc_low32(value) as u16)
                    .wrapping_add(inherited[axis])
            })
        } else {
            if node.speed_scale != 0 {
                speed = ((node.speed_scale as i32).wrapping_mul(speed) >> 16) as i16 as i32;
            }
            std::array::from_fn(|axis| {
                let value = f64::from(speed) * f64::from(f32::from_bits(node.direction_bits[axis]));
                (super::super::derived::native_trunc_low32(value) as u16)
                    .wrapping_add(inherited[axis])
            })
        };
        let callback = context.tables.read_dword(registry)?;
        if callback != 0 {
            let kind = match callback {
                0x0046_65d0 => ParticleFactoryKind::Factory4665D0,
                0x0046_6710 => ParticleFactoryKind::Factory466710,
                0x0046_6850 => ParticleFactoryKind::Factory466850,
                _ => return Err(WeaponEmissionLimit::Factory(callback).into()),
            };
            let selector = context.tables.read_dword(registry + 4)?;
            let source_handle = arena.actor(actor)?.handle;
            let mut sources = ParticleFactorySources {
                position: ParticleVectorBytes::from_words(position),
                separate_velocity: Some(ParticleVectorBytes::from_words(velocity)),
            };
            // The admitted factory/deleter kernels only read these two process
            // words. Their sole mutating extension points (network and impact)
            // stop before side effects here. This local API adaptation therefore
            // has no state to restore; the actual RNG is always the live loan.
            let mut process = *self.factory_process;
            self.particles.create_factory_particle(
                ParticleFactoryRequest {
                    kind,
                    selector,
                    source_handle,
                    flags: 0,
                    velocity_source: ParticleVelocitySource::Separate,
                },
                &mut ParticleFactoryContext {
                    process: &mut process,
                    rng: context.rng,
                    executable: self.executable,
                },
                &mut sources,
                &mut FactoryServices { phase: self.phase },
            )?;
            if let Some(record) = self.statistics.as_deref_mut() {
                let category = ActorStatisticCategory::from_actor(
                    arena.object(actor)?.base.flags,
                    arena.actor(actor)?.word_b4,
                );
                WorldStatistics::from_record(record).record_particle_firing(category);
            }
        } else {
            let resource = context.tables.read_dword(registry + 8)?;
            if resource == 0 {
                // 0048C9D3 -> 0048CB00: the detonator, no shot.
                self.detonate(arena, actor, context.rng)?;
            } else {
                let flags = registry_byte(context.tables, registry + 12)?;
                let group = node.group as usize;
                let mut retriggered = false;
                if flags & 2 != 0 {
                    // 0048C9DF: the shot this weapon group made last, by the
                    // handle behavior +24 keeps for the group.
                    let entry = *arena
                        .actor_behavior_storage(actor)?
                        .as_ref()
                        .ok_or(WeaponEmissionError::MissingBehavior(actor))?
                        .words_24
                        .get(group)
                        .ok_or(WeaponEmissionLimit::ActorResource { resource, flags })?;
                    if let Some(shot) = earlier_shot(arena, entry)? {
                        // +98(owner, 0, 2, 0). A grenade or mine thrown
                        // in the group holds its handle; 004557F0 answers 0.
                        retriggered = if arena.is_shot32(shot) {
                            arena.retrigger_shot32(shot, 2)?
                        } else if arena.is_thrown(shot) {
                            arena.grenade26_message(shot, Some(actor), 2)? != 0
                        } else if arena.is_missile(shot) {
                            // 00456AD0 answers event 2 with 0.
                            arena.missile_message(shot, 2)? != 0
                        } else {
                            return Err(WeaponEmissionLimit::ShotRetrigger { shot }.into());
                        };
                    }
                } else if let Some(record) = self.statistics.as_deref_mut() {
                    let category = ActorStatisticCategory::from_actor(
                        arena.object(actor)?.base.flags,
                        arena.actor(actor)?.word_b4,
                    );
                    WorldStatistics::from_record(record).record_actor_firing(category);
                }
                if !retriggered && resource == 0x82 {
                    // 0048CA35: 00431EF0 builds the flamethrower, then the
                    // tail; no group handle and no emission velocity.
                    self.throw_flame(arena, actor)?;
                } else if !retriggered {
                    let selector = crate::object_runtime::resident_constructor_selector(
                        arena,
                        self.residency,
                        resource,
                    )
                    .map_err(|_| WeaponEmissionLimit::ActorResource { resource, flags })?;
                    if !PORTED_SHOT_SELECTORS.contains(&selector) {
                        return Err(WeaponEmissionLimit::ActorResource { resource, flags }.into());
                    }
                    let children = self
                        .children
                        .as_deref_mut()
                        .ok_or(WeaponEmissionLimit::ActorResource { resource, flags })?;
                    // 0048CA54: owner +4C with the resource WORD in DX (the high
                    // half is the register's residue) and the shot's position.
                    let mut shot = None;
                    arena
                        .create_actor_child(
                            actor,
                            crate::object_runtime::ActorChildRequest {
                                resource:
                                    crate::object_runtime::ActorChildResource::RegisterLowWord(
                                        resource as u16,
                                    ),
                                position: Some(position),
                            },
                            Some(&mut shot),
                            children,
                            context.rng,
                        )
                        .map_err(|error| WeaponEmissionError::Shot(error.to_string()))?;
                    if let Some(shot) = shot {
                        // 0048CA63..0048CA7C: the group keeps the shot's handle and
                        // the shot takes the emission velocity.
                        let handle = arena.actor(shot)?.handle.raw();
                        arena
                            .actor_behavior_storage_mut(actor)?
                            .as_mut()
                            .ok_or(WeaponEmissionError::MissingBehavior(actor))?
                            .words_24[group] = Some(handle);
                        let shot_actor = arena.actor_mut(shot)?;
                        for axis in 0..3 {
                            shot_actor.words_8e_92[axis] =
                                shot_actor.words_8e_92[axis].wrapping_add(velocity[axis]);
                        }
                        // +6C answers the shot: it takes the group, and flag 0x40
                        // turns a positive strength into -strength / 10.
                        if arena.is_shot32(shot) {
                            arena.set_shot32_group(shot, group as u16, flags & 0x40 != 0)?;
                        }
                        if arena.object(actor)?.base.flags & 0x10 != 0
                            && arena.object(shot)?.base.flags & 0x200 != 0
                        {
                            // 0048CADC..0048CAF9: the owner's player (0046B090)
                            // takes the shot (0046AE90); only the implant has
                            // flag 0x200.
                            let owner = arena.actor(actor)?.handle;
                            if let Some(player) = arena.player_holding(owner)? {
                                let handle = arena.actor(shot)?.handle;
                                children.take_shot(arena, player, handle).map_err(|error| {
                                    WeaponEmissionError::Shot(error.to_string())
                                })?;
                            }
                        }
                    }
                }
            }
        }
        if registry_byte(context.tables, registry + 12)? & 0x10 != 0
            && name_entry_rng_next(context.rng) & 1 == 0
        {
            // 0048CB71: 00489D90(2) at the emission point, moving with the
            // owner (+8E).
            let owner_velocity = arena.actor(actor)?.words_8e_92;
            self.particles
                .birth_trail(
                    crate::process_particles::births::ParticleBirthRequest {
                        particle_type: 2,
                        position,
                        velocity: Some(owner_velocity),
                        flags: 0,
                        inputs: crate::process_particles::births::ParticleBirthInputs {
                            network_active: self.factory_process.network_flag != 0,
                            phase: self.phase,
                            process: self.factory_process,
                        },
                    },
                    self.executable,
                    context.rng,
                )
                .map_err(|error| WeaponEmissionError::ExtraParticle(format!("{error:?}")))?;
        }
        // Native recaptures current +14, then stores +38 before audio reads.
        let row = queued_row(arena, actor, node.selection)?;
        arena
            .actor_behavior_storage_mut(actor)?
            .as_mut()
            .unwrap()
            .word_38 = 500_000;
        let persistent = read_row_byte(arena, context.tables, row, 0x20)? != 0;
        let sound = i32::from(read_row_word(arena, context.tables, row, 4)? as i16);
        if !persistent {
            self.sounds.enqueue(
                self.residency,
                ParticleSoundRequest {
                    sound: sound as u32,
                    position,
                    volume: 0x10000,
                    pitch: 0x10000,
                    flags: 0,
                },
            )?;
        } else if arena
            .actor_behavior_storage(actor)?
            .as_ref()
            .unwrap()
            .word_2c
            != sound as u32
        {
            if let Some(old) = arena
                .actor_behavior_storage(actor)?
                .as_ref()
                .unwrap()
                .sound_28
            {
                self.sounds
                    .delete_record(old, &mut RequiredProcessSoundVoiceTeardown)?;
                arena
                    .actor_behavior_storage_mut(actor)?
                    .as_mut()
                    .unwrap()
                    .sound_28 = None;
            }
            arena
                .actor_behavior_storage_mut(actor)?
                .as_mut()
                .unwrap()
                .word_2c = sound as u32;
            let flag = read_row_byte(arena, context.tables, row, 0x23)?;
            let sound_ref = self
                .sounds
                .create_persistent(
                    self.residency,
                    sound as i16,
                    0x10000,
                    0x10000,
                    u32::from(flag),
                    || Ok::<_, std::convert::Infallible>(position),
                )
                .map_err(|error| match error {
                    crate::process_sound::ProcessSoundConstructionError::Record(error) => {
                        WeaponEmissionError::Sound(error)
                    }
                    crate::process_sound::ProcessSoundConstructionError::Source(never) => {
                        match never {}
                    }
                })?;
            arena
                .actor_behavior_storage_mut(actor)?
                .as_mut()
                .unwrap()
                .sound_28 = Some(sound_ref);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
