//! Actor messages from live updates: a +98 to one actor (`0047DD80`'s
//! delivery), to every actor near the sender (`0047DAF0`'s radius arm
//! under `0047DD20`, and `0047DBF0`) or to every actor (its zero-radius
//! arm), and the effects the receivers leave to
//! the world: the vehicle's text, damage and radar jam (the map's jitter,
//! when it is lent), a smoked factory's text, and retirement (`0047CE90`),
//! for the Powerball's message 5 its sender's too.

use crate::{
    object_runtime::{
        ActorMessage, ActorMessageReply, ActorMessageWalk, GrantOwner, LiveGrantContext,
        LiveGrantOperation, LiveGrantRequest, ObjectArena, ObjectError, ObjectId,
        PlayerActorLookupContext, VehicleMessageEffect, POWERBALL_PARTNER_RESOURCE,
    },
    session_platform::intro::retirement::ProductionRetirementHost,
    session_platform::particles::damage::{apply_world_actor_damage, WorldDamageOwners},
    world_map::WorldMapState,
};

/// `00411D0E`, the barrier's release inside its `+98`: a nonzero `+7A` goes
/// to the first player's grants +9C (`00488AA0`, `004750C0`), then the
/// barrier disposes itself (`00479FA0`).
pub(crate) fn release_world_barrier(
    arena: &mut ObjectArena,
    barrier: ObjectId,
    grant: u16,
    context: &LiveGrantContext<'_>,
) -> Result<(), String> {
    if grant != 0 {
        if let Some(player) = arena.first_player() {
            arena
                .mutate_live_grants(
                    LiveGrantRequest {
                        target: GrantOwner::Player9c(player),
                        operation: LiveGrantOperation::SetRaw {
                            id: u32::from(grant),
                        },
                    },
                    context,
                )
                .map_err(|error| format!("00411D29 004750C0({grant:#x}): {error}"))?;
        }
    }
    arena
        .dispose(barrier)
        .map_err(|error| format!("00411D30 00479FA0: {error}"))
}

pub(crate) fn deliver_world_actor_message(
    arena: &mut ObjectArena,
    target: ObjectId,
    message: ActorMessage,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), String> {
    receive_world_actor_message(arena, target, message, owners, None, rng).map(|_| ())
}

/// One receiver's +98 and its effects; the full EAX it returned.
pub(crate) fn receive_world_actor_message(
    arena: &mut ObjectArena,
    target: ObjectId,
    message: ActorMessage,
    owners: &mut WorldDamageOwners<'_, '_>,
    map: Option<&mut WorldMapState>,
    rng: &mut u32,
) -> Result<u32, String> {
    let network_active = owners.factory_process.network_flag != 0;
    let reply = arena
        .receive_actor_message(
            target,
            message,
            PlayerActorLookupContext {
                process: Some(&*owners.factory_process),
            },
            network_active,
        )
        .map_err(|error| error.to_string())?;
    match reply {
        ActorMessageReply::Done(result) => Ok(result),
        ActorMessageReply::Vehicle(effect) => {
            match effect {
                VehicleMessageEffect::None => {}
                VehicleMessageEffect::PlayerText { player, string } => arena
                    .deliver_player_text(player, string, owners.residency)
                    .map_err(|error| format!("004135D0 text {string:#x}: {error}"))?,
                VehicleMessageEffect::Damage { packet, source } => {
                    apply_world_actor_damage(arena, target, packet, source, owners, rng)
                        .map_err(|error| format!("0047C6E0: {error}"))?
                }
                VehicleMessageEffect::MapJitter(write) => map
                    .or(owners.map.as_deref_mut())
                    .ok_or_else(|| format!("00420BE0 radar jam {write:?} needs the world's map"))?
                    .apply(write)
                    .map_err(|error| format!("00420BE0 radar jam: {error}"))?,
            }
            Ok(0)
        }
        ActorMessageReply::PlayerText { player, string } => arena
            .deliver_player_text(player, string, owners.residency)
            .map(|()| 0)
            .map_err(|error| format!("004135D0 text {string:#x}: {error}")),
        ActorMessageReply::Release { grant } => release_world_barrier(
            arena,
            target,
            grant,
            &LiveGrantContext {
                process: Some(&*owners.factory_process),
                executable: Some(owners.executable),
                resident: Some(owners.residency),
                text_data: owners.grant_text,
            },
        )
        .map(|()| 0),
        ActorMessageReply::Retire => retire_world_actor(arena, target, owners, rng).map(|()| 0),
        ActorMessageReply::RetireWithSender { sender } => {
            retire_world_actor(arena, target, owners, rng)?;
            let resource = arena
                .object(sender)
                .map_err(|error| format!("004864C0 sender +30: {error}"))?
                .base
                .resource_word;
            if resource == POWERBALL_PARTNER_RESOURCE {
                retire_world_actor(arena, sender, owners, rng)?;
            }
            Ok(0)
        }
    }
}

/// `0047CE90` on `actor` with the world's lifecycle owners.
fn retire_world_actor(
    arena: &mut ObjectArena,
    actor: ObjectId,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), String> {
    let mut host = ProductionRetirementHost {
        owners: crate::session_platform::particles::damage::world_retirement_owners(owners),
        rng,
    };
    let null = host.owners.factory_process.process_null_handle;
    arena
        .retire_actor(actor, null, &mut host)
        .map(|_| ())
        .map_err(|error| format!("0047CE90 retirement: {error:?}"))
}

/// A delivery's failure inside a walk.
struct WalkFailure(String);

impl From<ObjectError> for WalkFailure {
    fn from(error: ObjectError) -> Self {
        Self(error.to_string())
    }
}

/// `0047DAF0`'s radius arm or `0047DBF0` from `source`: each receiver's
/// +98(source, distance², code, argument); the results add up.
pub(crate) fn broadcast_world_actor_message(
    arena: &mut ObjectArena,
    source: ObjectId,
    walk: ActorMessageWalk,
    code: i32,
    argument: u32,
    owners: &mut WorldDamageOwners<'_, '_>,
    mut map: Option<&mut WorldMapState>,
    rng: &mut u32,
) -> Result<u32, String> {
    arena
        .walk_message_receivers(source, walk, |arena, actor, distance_squared| {
            receive_world_actor_message(
                arena,
                actor,
                ActorMessage {
                    sender: Some(source),
                    distance_squared,
                    code,
                    argument,
                },
                owners,
                map.as_deref_mut(),
                rng,
            )
            .map_err(WalkFailure)
        })
        .map_err(|WalkFailure(error)| error)
}

/// `0047DD20(radius, 0xF, 0)` from a mining rig the vehicle drops, outside
/// the world's actor tick: the mines (selector 53) answer it, every other
/// receiver within the radius returns 0; an answer that would need the
/// world's owners stops the call.
pub(crate) fn call_world_mines(
    arena: &mut ObjectArena,
    rig: ObjectId,
    radius: u32,
    process: &crate::process_particles::factories::ParticleFactoryProcessState,
) -> Result<u32, String> {
    let network_active = process.network_flag != 0;
    arena
        .walk_message_receivers(
            rig,
            ActorMessageWalk::Radius { radius },
            |arena, actor, distance_squared| {
                let reply = arena
                    .receive_actor_message(
                        actor,
                        ActorMessage {
                            sender: Some(rig),
                            distance_squared,
                            code: crate::object_runtime::SELECTOR52_MINE_CALL,
                            argument: 0,
                        },
                        PlayerActorLookupContext {
                            process: Some(process),
                        },
                        network_active,
                    )
                    .map_err(|error| WalkFailure(error.to_string()))?;
                match reply {
                    ActorMessageReply::Done(answer) => Ok(answer),
                    ActorMessageReply::Vehicle(VehicleMessageEffect::None) => Ok(0),
                    reply => Err(WalkFailure(format!(
                        "the dropped rig's call to the mines reached {actor:?}'s {reply:?}"
                    ))),
                }
            },
        )
        .map_err(|WalkFailure(error)| error)
}

/// `0047DAF0`'s zero-radius arm with no sender (ECX NULL), as the timed
/// spawner raises its eruption: every actor's +98(NULL, 0, code, argument);
/// the results add up.
pub(crate) fn broadcast_world_event(
    arena: &mut ObjectArena,
    code: i32,
    argument: u32,
    owners: &mut WorldDamageOwners<'_, '_>,
    mut map: Option<&mut WorldMapState>,
    rng: &mut u32,
) -> Result<u32, String> {
    arena
        .walk_every_message_receiver(|arena, actor| {
            receive_world_actor_message(
                arena,
                actor,
                ActorMessage {
                    sender: None,
                    distance_squared: 0,
                    code,
                    argument,
                },
                owners,
                map.as_deref_mut(),
                rng,
            )
            .map_err(WalkFailure)
        })
        .map_err(|WalkFailure(error)| error)
}

/// `0047DD80(target, range, code, argument)` from `sender`: the squared
/// distance from the sender to the target (signed WORD deltas, wrapping
/// sums); a nonzero range beyond it sends nothing and answers 0.
pub(crate) fn send_world_actor_message(
    arena: &mut ObjectArena,
    sender: ObjectId,
    target: ObjectId,
    range: u32,
    code: i32,
    argument: u32,
    owners: &mut WorldDamageOwners<'_, '_>,
    map: Option<&mut WorldMapState>,
    rng: &mut u32,
) -> Result<u32, String> {
    let position = |arena: &ObjectArena, actor: ObjectId| {
        arena
            .object(actor)
            .map_err(|error| error.to_string())?
            .base
            .position_words
            .ok_or_else(|| format!("0047DD80 needs {actor:?}'s position"))
    };
    let from = position(arena, sender)?;
    let to = position(arena, target)?;
    let distance_squared = (0..3).fold(0_i32, |squares, axis| {
        let delta = i32::from(from[axis].wrapping_sub(to[axis]) as i16);
        squares.wrapping_add(delta.wrapping_mul(delta))
    }) as u32;
    if range != 0 && distance_squared > range.wrapping_mul(range) {
        return Ok(0);
    }
    receive_world_actor_message(
        arena,
        target,
        ActorMessage {
            sender: Some(sender),
            distance_squared,
            code,
            argument,
        },
        owners,
        map,
        rng,
    )
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
