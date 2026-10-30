//! The respawn `0046A890` the held camera `0040F890` reaches mid-world: the
//! world's start (or saved spawn), a fresh vehicle through the mode-1
//! factory, the equipment, the pickups and their messages.

use inf_formats::FrontendExecutable;

use crate::{
    grant_text::{GrantTextData, PickupTextData},
    object_runtime::{
        ActorConstructorNull, CameraFactoryInputs, CameraSettings, HeldCameraHost, NetworkRespawn,
        ObjectArena, PickupTables, PlayerId, PlayerRespawnInputs, RespawnMessage,
    },
    process_particles::{factories::ParticleFactoryProcessState, ProcessParticleRuntime},
    process_sound::ProcessSoundRuntime,
    session_platform::world_actor_factory::fresh_world_allocation,
    terrain_runtime::TerrainRuntime,
    world_backend::WorldRespawnHeader,
    world_loader::{WorldEquipment, WorldLoadEquipmentInputs},
    world_mission::WorldMission,
    world_start::{WorldStartContext, WorldStartState},
    ResidentRuntime,
};

pub(crate) struct WorldRespawnHost<'a, 'exe> {
    pub residency: &'a ResidentRuntime,
    pub terrain: &'a TerrainRuntime,
    pub world_start: &'a mut WorldStartState,
    pub rng: &'a mut u32,
    pub process: &'a ParticleFactoryProcessState,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub particles: &'a mut ProcessParticleRuntime,
    pub executable: FrontendExecutable<'exe>,
    pub grant_text: Option<&'a GrantTextData<'a>>,
    pub pickup_text: Option<&'a PickupTextData<'a>>,
    pub sine: &'a [u16; 4096],
    /// The animation clock the fresh vehicle allocation samples terrain at.
    pub clock: u32,
    pub world_flags: u32,
    pub camera_settings: CameraSettings,
    /// Session +64C, read when the particle pool is full.
    pub phase: u32,
    pub header: WorldRespawnHeader,
    /// H14 header +24, the limit on the player's lost vehicles.
    pub loss_limit: u32,
    /// The refused respawn starts the mission's doom countdown.
    pub mission: &'a mut WorldMission,
    /// Mission +A0 (G+250).
    pub current_world_a0: u32,
    /// Settings +84, the cheat word, as the world tick reads it.
    pub settings_84: u32,
    pub pickups: PickupTables<'a>,
    pub vehicle_table: &'a [u32; inf_formats::VEHICLE_BEHAVIOR_TABLE_DWORDS],
    /// 0048F4D0's table, for a network respawn at the camera.
    pub inverse_sine: &'a [i16; 1024],
}

impl HeldCameraHost for WorldRespawnHost<'_, '_> {
    type Error = String;

    fn respawn(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        factory: &mut CameraFactoryInputs<'_>,
    ) -> Result<(), String> {
        // 0040FA61: the byte argument is the network flag, so offline the
        // loss limit applies; at it 0046A8C6..0046A8D7 start the doom.
        if self.process.network_flag == 0
            && arena
                .respawn_loss_limit_reached(player, self.loss_limit)
                .map_err(|error| error.to_string())?
        {
            self.mission.start_doom_countdown(1);
            return Ok(());
        }
        if arena
            .world_player(player)
            .map_err(|error| error.to_string())?
            .byte_ed
            != 0
        {
            eprintln!("SavedSpawnRotationApproximation: the respawn at the saved pad leaves 0046A890's factory rotation words unwritten; the port turns the fresh vehicle by zero angles, so only its basis counter advances before 004911F0 replaces every row with the saved angles");
        }
        let mut equipment = WorldEquipment {
            inputs: WorldLoadEquipmentInputs {
                executable: self.executable,
                text_data: self.grant_text,
                actor_clock: self.clock,
                sine: self.sine,
                phase: Some(self.phase),
            },
            residency: self.residency,
            process: self.process,
            terrain: self.terrain,
            world_flags: self.world_flags,
            camera_settings: self.camera_settings,
            sounds: Some(&mut *self.sounds),
            particles: Some(&mut *self.particles),
        };
        let respawn = arena
            .respawn_player(
                player,
                PlayerRespawnInputs {
                    start: WorldStartContext {
                        state: &mut *self.world_start,
                        runtime: self.residency,
                        terrain: self.terrain,
                    },
                    world_header_18: self.header.vehicle_selection_18,
                    vehicle_allocation: fresh_world_allocation(
                        self.terrain,
                        self.clock,
                        self.world_flags,
                        self.sine,
                        ActorConstructorNull::Process(self.process),
                    ),
                    world_mask_limits: self.header.actor_mask_limits,
                    rng: &mut *self.rng,
                    vehicle_table: self.vehicle_table,
                    current_world_a0: self.current_world_a0,
                    settings_84: self.settings_84,
                    pickups: self.pickups,
                    equipment: Some(&mut equipment),
                    network: (self.process.network_flag != 0).then_some(NetworkRespawn {
                        local_slot: self.process.local_slot,
                        host: self.process.host_flag != 0,
                        settings: &self.process.settings,
                        inverse_sine: self.inverse_sine,
                    }),
                },
                factory,
            )
            .map_err(|error| error.to_string())?;
        // The grants' notices and the pickups' messages, in call order.
        for message in respawn.messages {
            match message {
                RespawnMessage::Notice(notice) => {
                    let flags = inf_formats::parse_grant_flags(self.executable)
                        .map_err(|error| error.to_string())?;
                    arena.deliver_grant_notification(
                        player,
                        notice,
                        flags[usize::from(notice.id)],
                        self.residency,
                        self.pickup_text.map(|data| &data.common),
                    )
                }
                RespawnMessage::Pickup(message) => {
                    arena.deliver_pickup_message(player, message, self.residency, self.pickup_text)
                }
            }
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
