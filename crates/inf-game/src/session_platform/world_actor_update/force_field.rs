//! The Force Field's update `00476AE0` with the world's services: the
//! bounce's message 5 (400 damage to the vehicle), the vehicle's notices
//! (code 4, through `0047DD80` with no range), the hint (`0046C760`) and a
//! released field's grant (`004750C0`).

use crate::object_runtime::{
    ActorMessage, GrantOwner, LiveGrantContext, LiveGrantOperation, LiveGrantRequest, ObjectArena,
    ObjectId, PlayerActorLookupContext, PlayerActorLookupRequest, PlayerCommandContext,
    PlayerCommandOperation, PlayerCommandRequest, PlayerId, Selector18Host,
    FORCE_FIELD_BOUNCE_CODE,
};

use super::{deliver_world_actor_message, WorldFamilyUpdateHost};

pub(crate) struct WorldForceFieldHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
}

fn position(arena: &ObjectArena, object: ObjectId) -> Result<[u16; 3], String> {
    arena
        .object(object)
        .map_err(|error| error.to_string())?
        .base
        .position_words
        .ok_or_else(|| format!("{object:?}'s position is unknown"))
}

impl Selector18Host for WorldForceFieldHost<'_, '_> {
    type Error = String;

    fn find_player(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<PlayerId>, String> {
        let handle = arena
            .actor(actor)
            .map_err(|error| error.to_string())?
            .handle;
        arena
            .find_player_for_actor_handle(
                PlayerActorLookupRequest { handle },
                PlayerActorLookupContext {
                    process: Some(&*self.family.owners.factory_process),
                },
            )
            .map_err(|error| format!("0046B090: {error}"))
    }

    fn first_player(&mut self, arena: &mut ObjectArena) -> Result<Option<PlayerId>, String> {
        Ok(arena.first_player())
    }

    fn player_vehicle(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, String> {
        arena
            .first_player_primary_actor()
            .map_err(|error| format!("0046B030: {error}"))
    }

    fn owns_grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        grant: u16,
    ) -> Result<bool, String> {
        arena
            .player_upgrade_owned(player, u32::from(grant))
            .map_err(|error| format!("0046BDC0({grant:#x}): {error}"))
    }

    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        grant: u16,
    ) -> Result<(), String> {
        let owners = &self.family.owners;
        arena
            .mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::SetRaw {
                        id: u32::from(grant),
                    },
                },
                &LiveGrantContext {
                    process: Some(&*owners.factory_process),
                    executable: Some(owners.executable),
                    resident: Some(owners.residency),
                    text_data: owners.grant_text,
                },
            )
            .map_err(|error| format!("004750C0({grant:#x}): {error}"))
    }

    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        command: u32,
    ) -> Result<(), String> {
        let owners = &self.family.owners;
        arena
            .apply_player_command(
                PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set {
                        raw_id: command,
                        silent: 0,
                    },
                },
                &PlayerCommandContext {
                    process: Some(&*owners.factory_process),
                    executable: Some(owners.executable),
                    resident: Some(owners.residency),
                },
            )
            .map(|_| ())
            .map_err(|error| format!("0046C760({command:#x}, 0): {error}"))
    }

    fn bounce(
        &mut self,
        arena: &mut ObjectArena,
        field: ObjectId,
        actor: ObjectId,
        correction: Option<[u16; 3]>,
    ) -> Result<(), String> {
        // 00405490, the dying actor's +98, moves it by the correction's
        // WORDs. Every other receiver ignores a code-5 argument but the
        // vehicle, whose packet takes the argument slot's top byte: a stack
        // address's, zero, as for NULL.
        if let Some(offset) = correction {
            if arena.selector66(actor).is_ok() {
                return arena
                    .offset_selector66(actor, FORCE_FIELD_BOUNCE_CODE as u32, Some(offset))
                    .map(|_| ())
                    .map_err(|error| format!("00405490: {error}"));
            }
        }
        deliver_world_actor_message(
            arena,
            actor,
            ActorMessage {
                sender: Some(field),
                distance_squared: 0,
                code: FORCE_FIELD_BOUNCE_CODE,
                argument: 0,
            },
            &mut self.family.owners,
            self.family.rng,
        )
    }

    fn tell(
        &mut self,
        arena: &mut ObjectArena,
        field: ObjectId,
        vehicle: ObjectId,
        text: u16,
    ) -> Result<(), String> {
        // 0047DD80 with no range measures, then delivers whatever the
        // distance.
        let (own, theirs) = (position(arena, field)?, position(arena, vehicle)?);
        let squared = (0..3).fold(0_u32, |sum, axis| {
            let word = i32::from(own[axis].wrapping_sub(theirs[axis]) as i16);
            sum.wrapping_add(word.wrapping_mul(word) as u32)
        });
        deliver_world_actor_message(
            arena,
            vehicle,
            ActorMessage {
                sender: Some(field),
                distance_squared: squared,
                code: 4,
                argument: u32::from(text),
            },
            &mut self.family.owners,
            self.family.rng,
        )
    }
}
