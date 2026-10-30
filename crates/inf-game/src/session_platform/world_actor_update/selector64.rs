//! The course gate's pass services (00440810) from the world tick: the
//! 0046B2B0 kind-4 repair with its notice and pickup text, the gate's
//! sound 1, the command context for a warpgate's command and the linked
//! actor's message with the world's effects.

use crate::object_runtime::{
    ActorMessage, ObjectArena, ObjectId, PickupTables, PlayerCommandContext, PlayerId,
    Selector64PassHost,
};
use crate::process_particles::audio::ParticleSoundRequest;
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::session_platform::particles::damage::WorldDamageOwners;

pub(crate) struct WorldGateHost<'a, 'exe> {
    pub owners: WorldDamageOwners<'a, 'exe>,
    pub pickup_text: Option<crate::grant_text::PickupTextData<'a>>,
    pub rng: &'a mut u32,
}

/// 0046B2B0's repair kind and the gate's amount.
const REPAIR_KIND: u16 = 4;
const REPAIR_AMOUNT: i16 = 5000;
/// 00487F10's sound selector at 00440849.
const GATE_SOUND: u32 = 1;

impl Selector64PassHost for WorldGateHost<'_, '_> {
    fn message(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        message: ActorMessage,
    ) -> Result<(), String> {
        super::message::deliver_world_actor_message(
            arena,
            target,
            message,
            &mut self.owners,
            self.rng,
        )
    }

    fn process(&self) -> Option<&ParticleFactoryProcessState> {
        Some(&*self.owners.factory_process)
    }

    fn repair(&mut self, arena: &mut ObjectArena, player: PlayerId) -> Result<(), String> {
        let executable = self.owners.executable;
        let grants = inf_formats::parse_grant_table(executable).map_err(|e| e.to_string())?;
        let texts = inf_formats::parse_grant_texts(executable).map_err(|e| e.to_string())?;
        let limits =
            inf_formats::parse_vehicle_upgrade_limits(executable).map_err(|e| e.to_string())?;
        let result = arena
            .grant_player_pickup(
                player,
                REPAIR_KIND,
                REPAIR_AMOUNT,
                PickupTables {
                    grants: &grants,
                    texts: &texts,
                    upgrade_limits: &limits,
                    network_active: self.owners.factory_process.network_flag != 0,
                },
            )
            .map_err(|e| e.to_string())?;
        if let Some(operation) = result.command {
            arena
                .apply_player_command(
                    crate::object_runtime::PlayerCommandRequest { player, operation },
                    &self.command_context(),
                )
                .map_err(|e| e.to_string())?;
        }
        if self.owners.factory_process.network_flag != 0 {
            return Ok(());
        }
        // 004750C0's notice comes first, as it does for collected pickups.
        if let Some(notification) = result.notification {
            let flags = inf_formats::parse_grant_flags(executable).map_err(|e| e.to_string())?;
            arena
                .deliver_grant_notification(
                    player,
                    notification,
                    flags[usize::from(notification.id)],
                    self.owners.residency,
                    self.pickup_text.as_ref().map(|data| &data.common),
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(message) = result.message {
            arena
                .deliver_pickup_message(
                    player,
                    message,
                    self.owners.residency,
                    self.pickup_text.as_ref(),
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn sound(&mut self, position: [u16; 3]) -> Result<(), String> {
        self.owners
            .sounds
            .enqueue(
                self.owners.residency,
                ParticleSoundRequest {
                    sound: GATE_SOUND,
                    position,
                    volume: 0x10000,
                    pitch: 0x10000,
                    flags: 0,
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn command_context(&self) -> PlayerCommandContext<'_> {
        PlayerCommandContext {
            process: Some(&*self.owners.factory_process),
            executable: Some(self.owners.executable),
            resident: Some(self.owners.residency),
        }
    }
}
