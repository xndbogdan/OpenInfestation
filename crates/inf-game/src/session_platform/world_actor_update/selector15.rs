//! The factory's services (`0042EE30`, its shop menu and `0042F2A0`) from
//! the world: one-shot and looped sounds, the player's message, commands
//! and grants (`0046B2B0` with its notice, command and message), the world
//! camera and the shop's executable tables. An evacuation (`0040DCB0`)
//! also reaches the world factory for its scientists, the vehicle's take
//! and the slot flash; a bought item, the factory's `+4C` and the RNG.

use crate::{
    factory_shop::FactoryShopTables,
    grant_text::{GrantTextData, PickupTextData},
    object_runtime::{
        ActorChildRequest, ActorChildResource, CameraSettings, CreationListeners, FactoryHost,
        FactoryStaffHost, LiveGrantContext, ObjectArena, ObjectError, ObjectId, PickupTables,
        PlayerCommandContext, PlayerCommandOperation, PlayerCommandRequest, PlayerId,
        ShownMenuOwner, VehicleCarryContext, VehicleCarryEffect, VehicleCarryHost,
    },
    process_particles::{
        audio::ParticleSoundRequest, factories::ParticleFactoryProcessState, ProcessParticleRuntime,
    },
    process_sound::{ProcessSoundRef, ProcessSoundRuntime, ProcessSoundVoiceTeardown},
    session_platform::world_actor_factory::{
        RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
        WorldFactoryInitialization, WorldFactoryPosition,
    },
    terrain_runtime::TerrainRuntime,
    world_mission::{PlacementDeathRequest, WorldMission},
    world_start::WorldStartState,
    ResidentRuntime,
};
use inf_formats::{native_basis::NativeBasisMath, FrontendExecutable};

use super::CarryEffects;

const FULL: u32 = 0x10000;
/// `0040DD29`: an evacuee is H13 resource 2.
pub(super) const SCIENTIST_RESOURCE: u32 = 2;
/// The slot flash's particle selector.
pub(super) const FLASH_SELECTOR: u32 = 2;

/// What an evacuation and a beam-down reach beyond the shop: the world
/// factory, the take's and the flash's particles, the mission's placement
/// bits and the world RNG.
pub(crate) struct ShopWorld<'a> {
    pub mission: &'a mut WorldMission,
    pub particles: &'a mut ProcessParticleRuntime,
    pub terrain: &'a TerrainRuntime,
    pub world_start: &'a mut WorldStartState,
    pub listeners: &'a mut CreationListeners,
    pub sine: &'a [u16; 4096],
    pub world_flags: u32,
    pub clock: u32,
    pub phase: u32,
    pub rng: &'a mut u32,
}

/// What the factory host borrows, for the drains outside the actor loop.
pub(crate) struct ShopServices<'a> {
    pub residency: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'a>,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub voices: &'a mut dyn ProcessSoundVoiceTeardown,
    pub process: &'a ParticleFactoryProcessState,
    pub camera_settings: CameraSettings,
    pub grant_text: Option<&'a GrantTextData<'a>>,
    pub pickup_text: Option<PickupTextData<'a>>,
    pub world: Option<ShopWorld<'a>>,
    /// The animation clock 005471C0, for a full row's 004693C0.
    pub clock: u32,
}

/// The shown menus' queued commands (`00413950` from the keys or the `+54`
/// repeat), then the destructors of the menus a faded controller took with
/// it: a shop menu that no rebuild replaced undocks its factory
/// (`004315E0`). A failure stops the shop for the rest of the world.
pub(crate) fn drain_shop_menus(
    arena: &mut ObjectArena,
    stop: &mut Option<String>,
    services: ShopServices<'_>,
) {
    if stop.is_some() {
        return;
    }
    let commands = arena.take_shown_menu_commands();
    let destroyed = arena.take_destroyed_menus();
    if commands.is_empty() && destroyed.is_empty() {
        return;
    }
    let result = (|| -> Result<(), String> {
        let tables = FactoryShopTables::parse(services.executable).map_err(|e| e.to_string())?;
        let mut host = WorldFactoryHost {
            residency: services.residency,
            sounds: services.sounds,
            voices: services.voices,
            executable: services.executable,
            process: services.process,
            camera_settings: services.camera_settings,
            grant_text: services.grant_text,
            pickup_text: services.pickup_text,
            tables: &tables,
            world: services.world,
            clock: services.clock,
        };
        let visibility = arena
            .player_message_environment_mut()
            .map(|environment| environment.visibility)
            .ok_or("the shown menu's command needs the message environment")?;
        for (player, code) in commands {
            arena
                .run_shown_menu_command(player, code, visibility, &mut host)
                .map_err(|e| e.to_string())?;
        }
        for (_, owner) in destroyed.into_iter().chain(arena.take_destroyed_menus()) {
            match owner {
                ShownMenuOwner::Shop { factory } => arena
                    .destroy_shop_menu(factory, &mut host)
                    .map_err(|e| e.to_string())?,
                // 00443400: the phase it was built over, 6 as 4.
                ShownMenuOwner::InWorld { return_phase } => {
                    arena.request_message_ui_state(if return_phase == 6 { 4 } else { return_phase })
                }
                ShownMenuOwner::Plain => {}
                // 00414480 and 004147D0 request phase 4.
                ShownMenuOwner::Database { .. } | ShownMenuOwner::HintList { .. } => {
                    arena.request_message_ui_state(4)
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        stop_shown_menus(stop, error);
    }
}

/// A shop drain's failure stops every shown menu's commands for the rest of
/// the world. The game runs on with no menu answering the keys, which looks
/// like a freeze, so the crash log is the player's account of it.
fn stop_shown_menus(stop: &mut Option<String>, error: String) {
    eprintln!("Factory shop stopped after reached writes: {error}");
    let report = [
        "The shown menus stopped: the computer's and the factories' menus no longer answer the keys for the rest of this world."
            .to_string(),
        format!("Factory shop stopped after reached writes: {error}"),
    ];
    if let Some(path) = crate::crash_log::write(&report) {
        eprintln!("A crash log was saved to: {}", path.display());
    }
    *stop = Some(error);
}

pub(crate) struct WorldFactoryHost<'a> {
    pub residency: &'a ResidentRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub voices: &'a mut dyn ProcessSoundVoiceTeardown,
    pub executable: FrontendExecutable<'a>,
    pub process: &'a ParticleFactoryProcessState,
    pub camera_settings: CameraSettings,
    pub grant_text: Option<&'a GrantTextData<'a>>,
    pub pickup_text: Option<PickupTextData<'a>>,
    pub tables: &'a FactoryShopTables,
    /// None where no evacuation can run (the factory's own update).
    pub world: Option<ShopWorld<'a>>,
    /// The animation clock 005471C0, for a full row's 004693C0.
    pub clock: u32,
}

const NO_WORLD: &str = "the factory reaches the world, which this host does not lend";

fn position(arena: &ObjectArena, actor: ObjectId) -> Result<[u16; 3], ObjectError> {
    arena
        .object(actor)?
        .base
        .position_words
        .ok_or(ObjectError::ActorPositionUnknown(actor))
}

impl WorldFactoryHost<'_> {
    fn commands(&self) -> PlayerCommandContext<'_> {
        PlayerCommandContext {
            process: Some(self.process),
            executable: Some(self.executable),
            resident: Some(self.residency),
        }
    }
}

impl FactoryStaffHost for WorldFactoryHost<'_> {
    type Error = String;

    fn process(&self) -> &ParticleFactoryProcessState {
        self.process
    }

    fn grants(&self) -> LiveGrantContext<'_> {
        LiveGrantContext {
            process: Some(self.process),
            executable: Some(self.executable),
            resident: Some(self.residency),
            text_data: self.grant_text,
        }
    }

    fn shop_tables(&self) -> Result<&FactoryShopTables, String> {
        Ok(self.tables)
    }

    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        CarryEffects {
            particles: &mut *world.particles,
            sounds: &mut *self.sounds,
            residency: self.residency,
            executable: self.executable,
            process: self.process,
            phase: world.phase,
        }
        .effect(
            arena,
            VehicleCarryEffect::Particle {
                selector: FLASH_SELECTOR,
                position,
                flags: 0,
            },
            world.rng,
        )
    }

    fn complete_placement(&mut self, index: i8) -> Result<(), String> {
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let inhibit_byte_08 = world.mission.inhibit_byte_08;
        world
            .mission
            .record_placement_death(PlacementDeathRequest {
                index: i32::from(index),
                inhibit_byte_08,
                force: 0,
            })
            .map(|_| ())
            .map_err(|error| format!("0048EAD0 placement {index}: {error}"))
    }
}

impl FactoryHost for WorldFactoryHost<'_> {
    fn camera_settings(&self) -> CameraSettings {
        self.camera_settings
    }

    /// 00478E00: the world camera, which gameplay always has.
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<ObjectId, String> {
        let handle = arena
            .camera_handle()
            .ok_or("00478E00 would construct a camera; gameplay always has one")?;
        arena
            .resolve_actor_handle(handle)
            .map_err(|error| format!("{error:?}"))?
            .ok_or_else(|| "the world camera's handle no longer resolves".to_string())
    }

    fn sound(
        &mut self,
        _: &ObjectArena,
        sound: u32,
        position: [u16; 3],
        volume: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.sounds
            .enqueue(
                self.residency,
                ParticleSoundRequest {
                    sound,
                    position,
                    volume,
                    pitch,
                    flags: 0,
                },
            )
            .map_err(|error| format!("{error:?}"))
    }

    fn create_loop(
        &mut self,
        arena: &ObjectArena,
        factory: ObjectId,
        sound: u32,
    ) -> Result<Option<ProcessSoundRef>, String> {
        self.sounds
            .create_persistent(self.residency, sound as i16, FULL, FULL, 0, || {
                position(arena, factory)
            })
            .map(Some)
            .map_err(|error| format!("{error:?}"))
    }

    fn update_loop(
        &mut self,
        arena: &ObjectArena,
        factory: ObjectId,
        reference: ProcessSoundRef,
        volume: u32,
    ) -> Result<(), String> {
        self.sounds
            .records_mut()
            .update(reference, volume, FULL, Some(|| position(arena, factory)))
            .map_err(|error| format!("{error:?}"))
    }

    fn release_loop(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        self.sounds
            .delete_record(reference, &mut *self.voices)
            .map_err(|error| format!("{error:?}"))
    }

    fn text(&mut self, arena: &mut ObjectArena, player: PlayerId, text: u16) -> Result<(), String> {
        arena
            .deliver_player_text(player, text, self.residency)
            .map_err(|error| error.to_string())
    }

    fn command(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        raw_id: u32,
        silent: u32,
    ) -> Result<(), String> {
        arena
            .apply_player_command(
                PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set { raw_id, silent },
                },
                &self.commands(),
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// 0046B2B0 as the course gate's repair runs it: the grant, then its
    /// notice, the class case's command and the pickup message (offline).
    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
        amount: i16,
    ) -> Result<(), String> {
        let grants = inf_formats::parse_grant_table(self.executable).map_err(|e| e.to_string())?;
        let texts = inf_formats::parse_grant_texts(self.executable).map_err(|e| e.to_string())?;
        let limits = inf_formats::parse_vehicle_upgrade_limits(self.executable)
            .map_err(|e| e.to_string())?;
        let result = arena
            .grant_player_pickup(
                player,
                id,
                amount,
                PickupTables {
                    grants: &grants,
                    texts: &texts,
                    upgrade_limits: &limits,
                    network_active: self.process.network_flag != 0,
                },
            )
            .map_err(|e| e.to_string())?;
        let flags = inf_formats::parse_grant_flags(self.executable).map_err(|e| e.to_string())?;
        arena
            .deliver_pickup_outcome(
                player,
                result,
                &crate::object_runtime::PickupDelivery {
                    online: self.process.network_flag != 0,
                    flags: &flags,
                    residency: self.residency,
                    text: self.pickup_text.as_ref(),
                    commands: &self.commands(),
                    clock: self.clock,
                },
            )
            .map_err(|e| e.to_string())
    }

    fn create_scientist(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, String> {
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let math = NativeBasisMath::new(*world.sine);
        WorldActorChildHost {
            residency: self.residency,
            executable: self.executable,
            process: self.process,
            terrain: world.terrain,
            world_flags: world.world_flags,
            clock: world.clock,
            sine: world.sine,
            math: &math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *world.world_start),
            listeners: &mut *world.listeners,
            listener_effects: &mut RequiredListenerEffects,
        }
        .create_object(
            arena,
            WorldActorFactoryRequest {
                resource: ActorChildResource::Full(SCIENTIST_RESOURCE),
                position: WorldFactoryPosition::Local(position),
                spawn_mode_1: false,
                network_handle: None,
                initialization: WorldFactoryInitialization::Bare,
            },
            world.rng,
        )
        .map_err(|error| format!("00460310 evacuee: {error}"))
    }

    fn carry_scientist(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        scientist: ObjectId,
    ) -> Result<bool, String> {
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let context = VehicleCarryContext {
            process: self.process,
            executable: self.executable,
            resident: self.residency,
            text_data: self.grant_text,
        };
        let mut effects = CarryEffects {
            particles: &mut *world.particles,
            sounds: &mut *self.sounds,
            residency: self.residency,
            executable: self.executable,
            process: self.process,
            phase: world.phase,
        };
        arena
            .attach_vehicle_item(vehicle, scientist, context, &mut effects, world.rng)
            .map_err(|error| format!("00421810 evacuee: {error}"))
    }

    fn random(&mut self) -> Result<u32, String> {
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        Ok(crate::name_entry::name_entry_rng_next(world.rng))
    }

    fn create_item(
        &mut self,
        arena: &mut ObjectArena,
        factory: ObjectId,
        resource: ActorChildResource,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, String> {
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let math = NativeBasisMath::new(*world.sine);
        let mut host = WorldActorChildHost {
            residency: self.residency,
            executable: self.executable,
            process: self.process,
            terrain: world.terrain,
            world_flags: world.world_flags,
            clock: world.clock,
            sine: world.sine,
            math: &math,
            camera_settings: Some(self.camera_settings),
            world_start: Some(&mut *world.world_start),
            listeners: &mut *world.listeners,
            listener_effects: &mut RequiredListenerEffects,
        };
        let mut item = None;
        let built = arena
            .create_actor_child(
                factory,
                ActorChildRequest {
                    resource,
                    position: Some(position),
                },
                Some(&mut item),
                &mut host,
                world.rng,
            )
            .map_err(|error| format!("0047AAF0 item {resource:?}: {error}"))?;
        Ok(built.and(item))
    }

    fn broadcast_skipped(&mut self, receivers: &[String]) {
        for receiver in receivers {
            eprintln!("Factory shop's 004137D0 broadcast skipped {receiver}");
        }
    }
}

/// What a key's unit broadcast (`0047DD20` from Tab's computer and from
/// Escape) reaches: the factory's `0042F380` reads the process NULL, and
/// sending Facilities back to the Options rebuilds the shop from the
/// executable's tables. Its arms reach no grant, flash or placement.
pub(crate) struct KeyBroadcastHost<'a> {
    pub process: &'a ParticleFactoryProcessState,
    pub executable: FrontendExecutable<'a>,
    pub tables: FactoryShopTables,
}

const KEY_BROADCAST_ONLY: &str = "a key's unit broadcast reaches no grant, flash or placement";

impl FactoryStaffHost for KeyBroadcastHost<'_> {
    type Error = String;

    fn process(&self) -> &ParticleFactoryProcessState {
        self.process
    }

    fn grants(&self) -> LiveGrantContext<'_> {
        LiveGrantContext {
            process: Some(self.process),
            executable: Some(self.executable),
            resident: None,
            text_data: None,
        }
    }

    fn shop_tables(&self) -> Result<&FactoryShopTables, String> {
        Ok(&self.tables)
    }

    fn flash(&mut self, _: &mut ObjectArena, _: [u16; 3]) -> Result<(), String> {
        Err(KEY_BROADCAST_ONLY.into())
    }

    fn complete_placement(&mut self, _: i8) -> Result<(), String> {
        Err(KEY_BROADCAST_ONLY.into())
    }
}

/// The unit broadcasts a key callback just sent, run before the next input
/// event as retail ran them inside the callback: Tab's computer re-arms a
/// factory within 0x300 of the unit, and Escape sends a factory's shop back
/// to its Options or hides the message. A receiver whose +98 is not ported is
/// skipped and logged once. A failure stops the shop like its other drains.
pub(crate) fn run_key_broadcasts(
    arena: &mut ObjectArena,
    stop: &mut Option<String>,
    logged: &mut std::collections::BTreeSet<String>,
    process: &ParticleFactoryProcessState,
    executable: FrontendExecutable<'_>,
) {
    let broadcasts = arena.take_unit_broadcasts();
    if broadcasts.is_empty() || stop.is_some() {
        return;
    }
    let result = (|| -> Result<(), String> {
        let mut host = KeyBroadcastHost {
            process,
            executable,
            tables: FactoryShopTables::parse(executable).map_err(|e| e.to_string())?,
        };
        for broadcast in broadcasts {
            let outcome = arena
                .run_unit_broadcast(broadcast, &mut host)
                .map_err(|e| e.to_string())?;
            for receiver in outcome.skipped {
                if logged.insert(format!("unit broadcast {receiver}")) {
                    eprintln!("The unit's 0047DD20 broadcast skipped {receiver}");
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        stop_shown_menus(stop, error);
    }
}
