//! Production subset of 004875A0: native selector88 callback, its helpers,
//! admitted family +24 prefixes, constructor77 NULL-target updates and complete
//! constructor93 update. Shared
//! callbacks are independent of an intro controller; camera follows separately.
//! Each unowned actor/effect remains an explicit skip or reached feature boundary.

use inf_formats::native_basis::NativeBasisMath;

use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorWaterPlane, ArenaIntroInputs, ArenaIntroRequest, ArenaIntroServices,
        CreationListenerEffects, CreationListeners, FamilyUpdateClass, FamilyUpdateProgress,
        FamilyUpdateRequest, IntroAdmissionInputs, ObjectArena, ObjectId, Selector12TailRequest,
        Selector36TailPrefixRequest, Selector77TickRequest, Selector90MotionMode,
        Selector93TickRequest,
    },
    process_particles::{factories::ParticleFactoryProcessState, ProcessParticleRuntime},
    process_tasks::ProcessTaskRuntime,
    session::SessionState,
    session_platform::{PortPlatform, PortSession, WorldTickPlatformInputs},
    world_actor_collision::WorldActorCollisionHost,
    world_actor_terrain::WorldActorTerrainHost,
    world_backend::{FamilyPrefixStop, FamilyPrefixStopReason, WorldSession},
    world_completion::{
        complete_world, WorldCompletionInputs, WorldCompletionOutcome, WorldCompletionRequest,
    },
    world_furniture_effect::FurnitureEffectQueue,
    ResidentRuntime,
};

use super::retirement::{ProductionRetirementHost, RetirementOwners};
mod disposal;
use crate::session_platform::world_actor_behavior::WorldFamilyBehaviorHost;
use crate::session_platform::world_actor_factory::{
    fresh_world_allocation, RequiredListenerEffects,
};
use crate::session_platform::{
    particles::damage::{BehaviorFactoryLoan, WorldDamageOwners},
    world_actor_update::{WorldFamilyUpdateHost, WorldWreckHost},
};
use disposal::drain_world_disposal;

/// The process values a player's message reads, sampled per world tick.
struct PlayerMessageProcess {
    network: u32,
    owner_byte: bool,
    pause_on_hints: bool,
    attract: bool,
    ui_state: u32,
    cheats: crate::object_runtime::ComputerCheats,
}

/// UI states the players' messages asked for (004391B0 -> 00438380): a hint
/// asks for the paused phase 6 and an expired or dismissed hint under 6 for
/// phase 4. Each is a native first-request-wins phase request.
pub(in crate::session_platform) fn request_player_message_phases(
    world: &mut crate::world_backend::WorldSession,
    state: &mut SessionState,
) {
    for effect in world.arena.take_player_message_effects() {
        let crate::object_runtime::PlayerMessageEffect::RequestUiState(id) = effect;
        match crate::session::SessionPhase::ALL
            .into_iter()
            .find(|phase| u32::from(phase.id()) == id)
        {
            Some(phase) => {
                state.request_phase(phase);
            }
            None => eprintln!("Player message requests unknown UI state {id}; skipped"),
        }
    }
}

/// The UI state 004391B0 returns, as the message reads it.
pub(in crate::session_platform) fn sync_player_message_ui_state(
    world: &mut crate::world_backend::WorldSession,
    state: &SessionState,
) {
    if let Some(environment) = world.arena.player_message_environment_mut() {
        environment.ui_state = u32::from(state.current_phase().id());
    }
}

/// Installs the message boxes' static menus and basis math once per world,
/// then refreshes the process values the message reads.
fn refresh_player_message_environment(
    world: &mut crate::world_backend::WorldSession,
    assets: &FrontendAssets,
    process: PlayerMessageProcess,
) {
    use crate::object_runtime::PlayerMessageEnvironment;
    let visibility = crate::menu::MenuVisibility::from_owner_flag(process.owner_byte);
    if let Some(environment) = world.arena.player_message_environment_mut() {
        environment.network = process.network;
        environment.visibility = visibility;
        environment.pause_on_hints = process.pause_on_hints;
        environment.attract = process.attract;
        environment.ui_state = process.ui_state;
        environment.cheats = process.cheats;
        return;
    }
    let menus = match crate::object_runtime::PlayerMessageMenus::parse(assets) {
        Ok(menus) => menus,
        Err(error) => {
            eprintln!("Player message menus unavailable: {error}");
            return;
        }
    };
    world
        .arena
        .set_player_message_environment(PlayerMessageEnvironment {
            menus,
            math: crate::menu_basis::MenuBasisMath::new(assets.sine_table().values),
            network: process.network,
            visibility,
            pause_on_hints: process.pause_on_hints,
            attract: process.attract,
            ui_state: process.ui_state,
            cheats: process.cheats,
        });
}

struct WorldActorTickContext<'a> {
    /// Byte 0055F14C, the attract playback the Siren's sound checks.
    attract: bool,
    residency: &'a ResidentRuntime,
    /// Pelagia's borrowed Command Centre bank, for the models world 1's
    /// own does not register (its ships' 665), as the draw borrows them.
    model_fallback: Option<&'a ResidentRuntime>,
    assets: &'a FrontendAssets,
    camera_globals: &'a crate::object_runtime::CameraGlobalState,
    clock: u32,
    animation_clock: &'a dyn crate::frontend_camera::AnimationClockSource,
    inputs: WorldTickPlatformInputs,
    microseconds: u32,
    /// The scientists', the alien ships' and the creatures' delta this tick,
    /// or `None` when [`PortFixes::paced_actors`](crate::frontend_settings::PortFixes)
    /// holds their update to the next.
    paced_microseconds: Option<u32>,
    /// [`PortFixes::mission_completion`](crate::frontend_settings::PortFixes).
    mission_completion: bool,
    inverse_sine: &'a [i16; 1024],
    up: &'a [f32; 3],
    surface_snap_53a128: &'a u8,
    factory_process: &'a mut ParticleFactoryProcessState,
    behavior_tables: &'a mut crate::object_runtime::ProcessActorBehaviorTables,
    selector35_process: &'a mut crate::object_runtime::Selector35RouteProcessState,
    actor_motion: &'a crate::object_runtime::ProcessActorMotionState,
    model_process: &'a crate::model_process::ModelProcessOwners,
    actor_shared_basis: &'a inf_formats::native_basis::NativeBasis,
    state: &'a mut SessionState,
    sounds: &'a mut crate::process_sound::ProcessSoundRuntime,
    voices: crate::process_sound::ProcessSoundTeardownLoan<'a>,
    tasks: &'a mut ProcessTaskRuntime,
    particles: &'a mut ProcessParticleRuntime,
    furniture: &'a mut FurnitureEffectQueue,
    furniture_query: &'a mut crate::world_furniture_effect::ProcessFurnitureQueryState,
    furniture_radius: &'a crate::terrain_runtime::ProcessFurnitureRadius,
    furniture_draw: &'a crate::terrain_furniture::draw::FurnitureDrawState,
    grant_text: Option<&'a crate::grant_text::GrantTextData<'a>>,
    /// The pickup text inputs a course gate's repair message formats with.
    pickup_text: Option<crate::grant_text::PickupTextData<'a>>,
    /// The session's input owner; 0046A260 reads the player's references.
    input: &'a mut crate::input::InputRuntime,
    /// The spacing globals 005372F0/F4 that temporary fine scopes borrow.
    terrain_policy: &'a mut crate::terrain_runtime::ProcessTerrainPolicy,
    /// G+1B4's jitter, which the vehicle's radar jam writes.
    map: Option<&'a mut crate::world_map::WorldMapState>,
    /// 0054ED18, the network keys' countdown, and 0054E81C, the host's team
    /// flag, which the player's network keys keep.
    network_key_countdown: &'a mut u32,
    network_team_flag: &'a mut u8,
}

impl<P: PortPlatform> PortSession<P> {
    pub(in crate::session_platform) fn tick_world_actor_runtime(
        &mut self,
        microseconds: u32,
        state: &mut SessionState,
    ) {
        let paced_microseconds = self
            .world_tick_carry
            .paced_delta(self.platform.actor_pacing(), microseconds);
        let mission_completion = self.platform.mission_completion();
        let Some(world) = self.world.as_mut() else {
            return;
        };
        // The start block's RNG word and animation clock (the lab's
        // comparisons with retail) seed the first world tick, once.
        if let Some(start) = self.harness_start.as_mut() {
            if let Some(rng) = start.rng.take() {
                world.rng = rng;
            }
            if let Some(micros) = start.clock_us.take() {
                self.process.reset_animation();
                self.process.advance_animation(micros);
            }
            if let Some(cache) = start.camera_cache.take() {
                self.camera_globals.cached_position = Some(cache);
            }
            // Capture nodes taken before the start, by the first player's
            // vehicle, as a scientist's touch (0043C150) takes them.
            if let Some(nodes) = start.captured_nodes.take() {
                let vehicle = world
                    .arena
                    .first_player()
                    .and_then(|player| world.arena.player_actor(player).ok().flatten());
                let all: Vec<_> = world
                    .arena
                    .actor_list()
                    .iter()
                    .copied()
                    .filter(|&actor| world.arena.selector84(actor).is_ok())
                    .collect();
                for index in nodes {
                    let taken = match (all.get(index), vehicle) {
                        (Some(&node), Some(_)) => world
                            .arena
                            .touch_selector84(node, vehicle)
                            .map(|()| node)
                            .map_err(|error| error.to_string()),
                        (None, _) => Err(format!("node {index} is past the world's {}", all.len())),
                        (_, None) => {
                            Err("the first player has no vehicle at the first tick".into())
                        }
                    };
                    match taken {
                        Ok(node) => eprintln!(
                            "--start captured_nodes: node {index} at {:?} taken",
                            world
                                .arena
                                .object(node)
                                .ok()
                                .and_then(|node| node.base.position_words)
                        ),
                        Err(error) => eprintln!("--start captured_nodes: {error}"),
                    }
                }
            }
            // A recorded vehicle's drive state, into the first player's vehicle.
            if let Ok(Some(bytes)) = start.vehicle_bytes() {
                start.vehicle = None;
                let dword = |offset: u32| {
                    let at = offset as usize;
                    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
                };
                let vehicle = world
                    .arena
                    .first_player()
                    .and_then(|player| world.arena.player_actor(player).ok().flatten());
                match vehicle {
                    Some(vehicle) => {
                        if let Err(error) = world.arena.inject_vehicle_drive_state(vehicle, dword) {
                            eprintln!("--start vehicle: {error}");
                        }
                    }
                    None => eprintln!(
                        "--start vehicle: the first player has no vehicle at the first tick"
                    ),
                }
            }
        }
        crate::replay_controls::tick();
        // --replay-rng: retail's word at this tick's start (the lab's lockstep runs).
        if let Some(word) = crate::replay_controls::rng_word() {
            world.rng = word;
        }
        crate::event_log::tick(self.process.animation().value(), world.rng, microseconds);
        crate::crash_log::step(crate::crash_log::Step::WorldTick);
        // 00478F44 clears the byte that 00479827 sets after a normal object
        // draw, so draw-time readers such as selector10's trails see zero.
        self.object_draw_process.completion_53b890 = 0;
        let inputs = self.platform.world_tick_inputs();
        refresh_player_message_environment(
            world,
            self.catalog.assets(),
            PlayerMessageProcess {
                network: self.factory_process.network_flag,
                owner_byte: self.owner_byte,
                pause_on_hints: inputs.settings_78 == 1,
                attract: self.attract,
                ui_state: u32::from(state.current_phase().id()),
                cheats: self.computer_cheats,
            },
        );
        // The protocol's state is the world's while it updates.
        let mut team_flag = world
            .arena
            .network_game_mut()
            .map_or(0, |hold| hold.game.host_flag_23);
        let voices = self.platform.sound_voice_teardown();
        let map = self.map_load.current();
        let grant_text = crate::session_platform::world_owners::current_grant_text(
            &self.profile_name_classification,
        );
        let pickup_text = crate::grant_text::PickupTextData {
            common: grant_text,
            hyphen_high: self
                .profile_name_classification
                .high_bytes
                .as_ref()
                .and_then(|bytes| bytes[b'-' as usize]),
        };
        tick_world_actor_callbacks(
            world,
            WorldActorTickContext {
                attract: self.attract,
                camera_globals: &self.camera_globals,
                residency: &self.residency,
                model_fallback: self.pelagia_models.as_ref().map(|models| &models.runtime),
                assets: self.catalog.assets(),
                clock: self.process.animation().value(),
                animation_clock: &self.process,
                inputs,
                microseconds,
                paced_microseconds,
                mission_completion,
                inverse_sine: &self.camera_inverse_sine,
                up: &self.process_up,
                surface_snap_53a128: &self.surface_snap_53a128,
                factory_process: &mut self.factory_process,
                behavior_tables: &mut self.actor_behavior_tables,
                selector35_process: &mut self.selector35_process,
                actor_motion: &self.actor_motion,
                model_process: &self.model_process,
                actor_shared_basis: &self.object_draw_process.shared_basis,
                state,
                sounds: &mut self.sounds,
                voices,
                tasks: &mut self.tasks,
                particles: &mut self.particles,
                furniture: &mut self.furniture,
                furniture_query: &mut self.furniture_query,
                furniture_radius: &self.furniture_radius,
                furniture_draw: &self.furniture_draw,
                grant_text: Some(&grant_text),
                pickup_text: Some(pickup_text),
                input: &mut self.input,
                terrain_policy: &mut self.terrain_policy,
                map: map.and_then(|map| self.maps.jitter_mut(map).ok()),
                network_key_countdown: &mut self.network_key_countdown,
                network_team_flag: &mut team_flag,
            },
        );
        if let Some(world) = self.world.as_mut() {
            if let Some(hold) = world.arena.network_game_mut() {
                hold.game.host_flag_23 = team_flag;
            }
            request_player_message_phases(world, state);
            // 0048C407: a gate's cheat bits go to the live settings word.
            let bits = std::mem::take(&mut world.actor_updates.cheat_flags_84);
            if bits != 0 && !self.platform.grant_cheat_flag(bits) {
                eprintln!("World gate cheat flags {bits:#x} skipped: no settings are lent");
            }
        }
    }

    /// 00438950, the tick of phases 5, 6 and 7 offline: the first player's
    /// message updates with flag 1, so only the hint box ages, and only
    /// under UI state 6. The world does not update. The controlled unit's
    /// behavior gives up its looping weapon sound (+7C, 0048D000); the
    /// caller then runs the music service 00487B90.
    pub(in crate::session_platform) fn tick_paused_player(
        &mut self,
        microseconds: i32,
        state: &mut SessionState,
    ) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        if let Some(player) = world.arena.first_player() {
            sync_player_message_ui_state(world, state);
            if let Err(error) = world
                .arena
                .update_player_message(player, microseconds, true)
            {
                if world
                    .draw_omissions_logged
                    .insert(format!("paused message {error}"))
                {
                    eprintln!("Paused player message update stopped: {error}");
                }
            }
            // 00438961..00438995: the unit (0046AE60) and its +7C behavior's
            // held loop, deleted through 00487FE0.
            let released = world
                .arena
                .player_actor(player)
                .map_err(|error| error.to_string())
                .and_then(|unit| match unit {
                    Some(unit) => world
                        .arena
                        .take_unit_behavior_sound(unit)
                        .map_err(|error| error.to_string()),
                    None => Ok(None),
                })
                .and_then(|sound| match sound {
                    Some(sound) => self
                        .sounds
                        .delete_record(sound, &mut self.platform.sound_voice_teardown())
                        .map_err(|error| format!("00487FE0: {error:?}")),
                    None => Ok(()),
                });
            if let Err(error) = released {
                if world
                    .draw_omissions_logged
                    .insert(format!("paused unit sound {error}"))
                {
                    eprintln!("Paused tick: the unit's sound stop (0048D000) failed: {error}");
                }
            }
        }
        // The shown menu's queued commands and destroyed menus, as in the
        // world tick: its controller ticks while paused too.
        self.drain_shown_menu_commands(state);
        let Some(world) = self.world.as_mut() else {
            return;
        };
        request_player_message_phases(world, state);
    }
}

impl<P: PortPlatform> PortSession<P> {
    /// The shown menus' commands the keys queued (`00413950`) and the
    /// destructors of the menus a faded controller took with it. Native runs
    /// them in the key callbacks, so the gameplay tick drains them before
    /// the world's update as well as at its start.
    pub(in crate::session_platform) fn drain_shown_menu_commands(&mut self, state: &SessionState) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        if let Ok(executable) =
            inf_formats::FrontendExecutable::parse(self.catalog.assets().executable_bytes())
        {
            let grant_text = crate::session_platform::world_owners::current_grant_text(
                &self.profile_name_classification,
            );
            let pickup_text = crate::grant_text::PickupTextData {
                common: grant_text,
                hyphen_high: self
                    .profile_name_classification
                    .high_bytes
                    .as_ref()
                    .and_then(|bytes| bytes[b'-' as usize]),
            };
            let camera_settings = self.platform.world_tick_inputs().camera_settings;
            let mut voices = self.platform.sound_voice_teardown();
            crate::session_platform::world_actor_update::drain_shop_menus(
                &mut world.arena,
                &mut world.actor_updates.shop_stop,
                crate::session_platform::world_actor_update::ShopServices {
                    residency: &self.residency,
                    executable,
                    sounds: &mut self.sounds,
                    voices: &mut voices,
                    process: &self.factory_process,
                    camera_settings,
                    grant_text: Some(&grant_text),
                    pickup_text: Some(pickup_text),
                    world: Some(crate::session_platform::world_actor_update::ShopWorld {
                        mission: &mut world.mission,
                        particles: &mut self.particles,
                        terrain: &world.terrain,
                        world_start: &mut world.world_start,
                        listeners: &mut world.listeners,
                        sine: &self.catalog.assets().sine_table().values,
                        world_flags: world.world_flags,
                        clock: self.process.animation().value(),
                        phase: u32::from(state.current_phase().id()),
                        rng: &mut world.rng,
                    }),
                    clock: self.process.animation().value(),
                },
            );
        }
    }
}

/// The world's owners an evacuation or a beam-down from the shop reaches.
struct ShopWorldOwners<'a> {
    mission: &'a mut crate::world_mission::WorldMission,
    terrain: &'a crate::terrain_runtime::TerrainRuntime,
    world_start: &'a mut crate::world_start::WorldStartState,
    listeners: &'a mut crate::object_runtime::CreationListeners,
    world_flags: u32,
    rng: &'a mut u32,
}

fn shop_services<'a>(
    context: &'a mut WorldActorTickContext<'_>,
    owners: ShopWorldOwners<'a>,
) -> Option<crate::session_platform::world_actor_update::ShopServices<'a>> {
    let assets = context.assets;
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).ok()?;
    let clock = context.animation_clock.animation_value();
    let phase = u32::from(context.state.current_phase().id());
    Some(crate::session_platform::world_actor_update::ShopServices {
        residency: context.residency,
        executable,
        sounds: &mut *context.sounds,
        voices: &mut context.voices,
        process: &*context.factory_process,
        camera_settings: context.inputs.camera_settings,
        grant_text: context.grant_text,
        pickup_text: context.pickup_text,
        world: Some(crate::session_platform::world_actor_update::ShopWorld {
            mission: owners.mission,
            particles: &mut *context.particles,
            terrain: owners.terrain,
            world_start: owners.world_start,
            listeners: owners.listeners,
            sine: &assets.sine_table().values,
            world_flags: owners.world_flags,
            clock,
            phase,
            rng: owners.rng,
        }),
        clock,
    })
}

fn tick_world_actor_callbacks(world: &mut WorldSession, mut context: WorldActorTickContext<'_>) {
    // The shown menus' commands the keys queued since the last tick run
    // first, as 00413950 ran them during the input poll.
    let owners = ShopWorldOwners {
        mission: &mut world.mission,
        terrain: &world.terrain,
        world_start: &mut world.world_start,
        listeners: &mut world.listeners,
        world_flags: world.world_flags,
        rng: &mut world.rng,
    };
    if let Some(services) = shop_services(&mut context, owners) {
        crate::session_platform::world_actor_update::drain_shop_menus(
            &mut world.arena,
            &mut world.actor_updates.shop_stop,
            services,
        );
    }
    // 00478F3E: the tick's delta, 0055F0E0, for the draw's lights.
    world.light_delta = world.light_delta.wrapping_add(context.microseconds);
    //487725 ->478F4B drains the previous disposal queue before player/actor
    //callbacks. Newly queued deaths wait for the next frame's manager pass.
    drain_world_disposal(world, &mut context);
    // 004875A0 calls 00478F20 in every loaded world. Its Flag1 transfer and
    // concrete +24/+3C dispatch do not depend on an intro attachment; only
    // the selector88 controller services below borrow that attachment.
    let attachment = &mut world.intro_runtime;
    if let Some(attachment) = attachment.as_mut().filter(|attachment| {
        !attachment.deadline_approximation_applied && attachment.controller_failure.is_none()
    }) {
        match world
            .arena
            .apply_fresh_intro_deadline_approximation(attachment.controller)
        {
            Ok(()) => {
                attachment.deadline_approximation_applied = true;
                eprintln!("FreshIntroDeadlineApproximation: initial selector88 +140=0 selects the nonpositive first-tick carry branch; allocator history remains unowned and +138/+150 are not initialized early");
            }
            Err(error) => {
                eprintln!("Intro controller attachment skipped: {error}");
                attachment.controller_failure = Some(error.to_string());
            }
        }
    }
    let sine = &context.assets.sine_table().values;
    let math = NativeBasisMath::new(*sine);
    let mut listener_effects = RequiredListenerEffects;
    let arena = &mut world.arena;
    let listeners = &mut world.listeners;
    let callbacks = &mut world.actor_updates;
    let controller = attachment.as_ref().map(|attachment| attachment.controller);
    let first_pass = !callbacks.unowned_callbacks_logged;
    let mut skipped_callbacks = 0;
    let water_plane = ActorWaterPlane {
        height_fixed: world.terrain.plane_base(),
        coordinate_shift: world.terrain.plane_shift(),
    };
    let status_load_factor = world.fade.performance.particle_load_factor as i32;
    // 00478F25..00478F76: 0046B010's first player marks its actor
    // (0046A0E0) and 00479040 moves it to the alternate list, so the loop
    // never reaches the player's vehicle; 0046A260 updates it afterwards.
    let player = arena.first_player();
    let player_actor = match player {
        Some(player) if callbacks.player_update_stop.is_none() => {
            let result = context
                .factory_process
                .process_null_handle
                .ok_or_else(|| "0046A0E0 requires the process NULL 0054E894".to_string())
                .and_then(|null| {
                    // 0046A0E0's remote branch reaches the camera 00478E00.
                    let mut camera_context = crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
                        crate::session_platform::world_actor_factory::WorldCameraInputs {
                            residency: context.residency,
                            sine_quarter: sine,
                            settings: context.inputs.camera_settings,
                            process: Some(&*context.factory_process),
                            animation_clock: context.clock,
                        },
                    );
                    let mut factory =
                        camera_context.factory(&world.terrain, world.world_flags, &mut *listeners);
                    arena
                        .player_pre_update(player, null, &mut factory)
                        .map_err(|error| error.to_string())
                })
                .and_then(|actor| match actor {
                    Some(actor) => arena
                        .append_flag1_alternate(actor)
                        .map(|()| Some(actor))
                        .map_err(|error| error.to_string()),
                    None => Ok(None),
                });
            match result {
                Ok(actor) => actor,
                Err(error) => {
                    let line = format!("World player update stopped: {player:?} {error}");
                    eprintln!("{line}");
                    crate::crash_log::player_stuck(&line);
                    callbacks.player_update_stop = Some(error);
                    None
                }
            }
        }
        _ => None,
    };
    let pass = arena.visit_flag1_update_list(|arena, actor, alternate_3c| {
        crate::crash_log::step_object(crate::crash_log::Step::ActorUpdate, actor.slot());
        match crate::session_platform::world_actor_update::tick_simple_object_callback(
            arena,
            actor,
            alternate_3c,
            context.microseconds,
        ) {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                eprintln!("World simple object callback failed: {actor:?}: {error}");
                return;
            }
        }
        // 0045B5A0 and 0045BDB0, the banner's and the base's +24 (and
        // through 00461C50 their +3C).
        if arena.selector40(actor).is_ok() || arena.is_selector41(actor) {
            if callbacks.selector40_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let null = context
                    .factory_process
                    .process_null_handle
                    .ok_or("the banner requires the process NULL 0054E894")?;
                let math = inf_formats::native_basis::NativeBasisMath::new(*sine);
                let request = crate::object_runtime::BannerRequest {
                    delta: context.microseconds,
                    local_slot: context.factory_process.local_slot,
                    null,
                    math: &math,
                    terrain: &world.terrain,
                };
                let mut host = crate::session_platform::banner::WorldBannerHost {
                    residency: context.residency,
                    executable,
                    process: &*context.factory_process,
                    text: context.pickup_text.as_ref(),
                    clock: context.animation_clock.animation_value(),
                };
                if arena.is_selector41(actor) {
                    arena.tick_selector41(actor, &request, &mut host)
                } else {
                    arena.tick_selector40(actor, &request, &mut host)
                }
                .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World banner stopped after reached writes: {actor:?} {error}");
                callbacks.selector40_stops.push((actor, error));
            }
            return;
        }
        // 00475800, the territory's +24 and (through 00461C50) its +3C.
        if arena.selector46(actor).is_ok() {
            if callbacks.selector46_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let request = crate::object_runtime::TerritoryRequest {
                delta: context.microseconds,
                local_slot: context.factory_process.local_slot,
                host: context.factory_process.host_flag != 0,
                resident: context.residency,
            };
            if let Err(error) = arena.tick_selector46(actor, &request) {
                eprintln!("World territory stopped after reached writes: {actor:?} {error}");
                callbacks.selector46_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00403F40, Powerball's goal's +24 (its +3C is 00405140).
        if arena.goal_state(actor).is_ok() {
            if callbacks.race_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World goal source unavailable: {error}");
                        return;
                    }
                };
            let on_host = context.factory_process.host_flag != 0;
            let limit = context.factory_process.settings[0x30 / 4];
            let mut host = WorldFamilyUpdateHost {
                owners: WorldDamageOwners {
                    grant_text: context.grant_text,
                    camera_settings: Some(context.inputs.camera_settings),
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut *context.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    furniture: &mut *context.furniture,
                    residency: context.residency,
                    executable,
                    factory_process: &mut *context.factory_process,
                    behavior_tables: &mut *context.behavior_tables,
                    animation_clock: context.animation_clock,
                    session: &*context.state,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut *listeners,
                        sine,
                        world_flags: world.world_flags,
                    }),
                    map: context.map.as_deref_mut(),
                },
                rng: &mut world.rng,
                sine_quarter: sine,
            };
            if let Err(error) = arena.tick_goal(actor, on_host, limit, &mut host) {
                eprintln!("World goal stopped after reached writes: {actor:?} {error}");
                callbacks.race_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0046E880, the race's +24, and 0046F8C0, the wrong-way marker's.
        if arena.race_object(actor).is_ok() {
            if callbacks.race_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let null = context.factory_process.process_null_handle.map(|handle| handle.raw());
            let result = match null {
                None => Err("the race requires the process NULL 0054E894".to_string()),
                Some(null) if arena.race_state(actor).is_ok() => {
                    let request = crate::object_runtime::RaceRequest {
                        delta: context.microseconds,
                        local_slot: context.factory_process.local_slot,
                        host: context.factory_process.host_flag != 0,
                        sine,
                        null,
                    };
                    arena
                        .tick_race(actor, &request, &mut world.rng)
                        .map_err(|error| error.to_string())
                }
                Some(null) => arena
                    .tick_wrong_way(actor, null)
                    .map_err(|error| error.to_string()),
            };
            if let Err(error) = result {
                eprintln!("World race stopped after reached writes: {actor:?} {error}");
                callbacks.race_stops.push((actor, error));
            }
            return;
        }
        // 00440520, the selector-64 course gate's own +24; a pass runs its
        // +4C (00440810) with the world's services.
        if !alternate_3c && arena.selector64(actor).is_ok() {
            if callbacks.selector64_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = match arena.tick_selector64(actor, context.microseconds) {
                Ok(None) => Ok(()),
                Ok(Some(crosser)) => {
                    match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                        Ok(executable) => {
                            let mut host = crate::session_platform::world_actor_update::WorldGateHost {
                                owners: WorldDamageOwners {
                                    grant_text: context.grant_text,
                                    camera_settings: Some(context.inputs.camera_settings),
                                    flash: Some(&mut world.flash),
                                    mission: &mut world.mission,
                                    tasks: &mut *context.tasks,
                                    terrain: &mut world.terrain,
                                    particles: &mut *context.particles,
                                    sounds: &mut *context.sounds,
                                    furniture: &mut *context.furniture,
                                    residency: context.residency,
                                    executable,
                                    factory_process: &mut *context.factory_process,
                                    behavior_tables: &mut *context.behavior_tables,
                                    animation_clock: context.animation_clock,
                                    session: &*context.state,
                                    particle_load_factor: world
                                        .fade
                                        .performance
                                        .particle_load_factor,
                                    behavior_factory: Some(BehaviorFactoryLoan {
                                        listeners: &mut *listeners,
                                        sine,
                                        world_flags: world.world_flags,
                                    }),
                                    map: context.map.as_deref_mut(),
                                },
                                pickup_text: context.pickup_text,
                                rng: &mut world.rng,
                            };
                            arena
                                .pass_selector64_gate(actor, crosser, &mut host)
                                .map_err(|error| error.to_string())
                        }
                        Err(error) => Err(error.to_string()),
                    }
                }
                Err(error) => Err(error.to_string()),
            };
            if let Err(error) = result {
                eprintln!("World selector64 gate stopped after reached writes: {actor:?} {error}");
                callbacks.selector64_stops.push((actor, error));
            }
            return;
        }
        // 0042EE30, the factory's own +24: docking, the pull, production;
        // or 00431A90, another machine's factory's +3C: the docking alone.
        if arena.selector15(actor).is_ok() {
            if callbacks.selector15_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            if arena
                .selector15(actor)
                .is_ok_and(|state| state.dock_190.is_none())
                && !callbacks.factory_dock_approximation_logged
            {
                callbacks.factory_dock_approximation_logged = true;
                eprintln!("FactoryDockPointApproximation: a factory's pad test waits for its first draw, whose custom vertex writes the dock point; Infest01's briefing draws had written every dock before gameplay");
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let tables = crate::factory_shop::FactoryShopTables::parse(executable)
                    .map_err(|error| error.to_string())?;
                let mut host = crate::session_platform::world_actor_update::WorldFactoryHost {
                    residency: context.residency,
                    sounds: &mut *context.sounds,
                    voices: &mut context.voices,
                    executable,
                    process: &*context.factory_process,
                    camera_settings: context.inputs.camera_settings,
                    grant_text: context.grant_text,
                    pickup_text: context.pickup_text,
                    tables: &tables,
                    // A bought item's 0042E89E: the factory's +4C and the RNG.
                    world: Some(crate::session_platform::world_actor_update::ShopWorld {
                        mission: &mut world.mission,
                        particles: &mut *context.particles,
                        terrain: &world.terrain,
                        world_start: &mut world.world_start,
                        listeners: &mut *listeners,
                        sine,
                        world_flags: world.world_flags,
                        clock: context.animation_clock.animation_value(),
                        phase: u32::from(context.state.current_phase().id()),
                        rng: &mut world.rng,
                    }),
                    clock: context.animation_clock.animation_value(),
                };
                if alternate_3c {
                    arena.tick_remote_selector15(actor, &mut host)
                } else {
                    arena.tick_selector15(actor, context.microseconds as i32, &mut host)
                }
                .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World factory stopped after reached writes: {actor:?} {error}");
                callbacks.selector15_stops.push((actor, error));
            }
            return;
        }
        // 0043FEB0, the selector-50 hint trigger's own +24.
        if !alternate_3c && arena.selector50(actor).is_ok() {
            if callbacks.selector50_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World selector50 source unavailable: {error}");
                        return;
                    }
                };
            let command = crate::object_runtime::PlayerCommandContext {
                process: Some(&*context.factory_process),
                executable: Some(executable),
                resident: Some(context.residency),
            };
            if let Err(error) = arena.tick_selector50(actor, context.microseconds, &command) {
                eprintln!("World selector50 trigger stopped after reached writes: {actor:?} {error}");
                callbacks.selector50_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0043C8F0, the selector-48 altitude ceiling's own +24.
        if !alternate_3c && arena.selector48(actor).is_ok() {
            if callbacks.selector48_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let mut host = crate::session_platform::world_actor_update::WorldCeilingHost {
                    particles: &mut *context.particles,
                    executable,
                    process: &*context.factory_process,
                    residency: context.residency,
                    phase: u32::from(context.state.current_phase().id()),
                };
                arena
                    .tick_selector48(actor, context.microseconds, &mut world.rng, &mut host)
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World altitude ceiling stopped after reached writes: {actor:?} {error}");
                callbacks.selector48_stops.push((actor, error));
            }
            return;
        }
        // 00454310, the selector-43 timed spawner's own +24.
        if !alternate_3c && arena.selector43(actor).is_ok() {
            if callbacks.selector43_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let mut spawner_effects = RequiredListenerEffects;
                let mut host = crate::session_platform::world_actor_update::WorldSpawnerHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    sine_quarter: sine,
                    world_flags: world.world_flags,
                    clock: context.clock,
                    math: &math,
                    camera_settings: context.inputs.camera_settings,
                    world_start: &mut world.world_start,
                    listener_effects: &mut spawner_effects,
                };
                arena
                    .tick_selector43(actor, context.microseconds, sine, &mut world.rng, &mut host)
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World timed spawner stopped after reached writes: {actor:?} {error}");
                callbacks.selector43_stops.push((actor, error));
            }
            return;
        }
        // 00409390, world 19's spawner's own +24 (selector 81).
        if !alternate_3c && arena.selector81(actor).is_ok() {
            if callbacks.selector81_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let mut spawner_effects = RequiredListenerEffects;
                let mut host = crate::session_platform::world_actor_update::WorldSpawnerHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    sine_quarter: sine,
                    world_flags: world.world_flags,
                    clock: context.clock,
                    math: &math,
                    camera_settings: context.inputs.camera_settings,
                    world_start: &mut world.world_start,
                    listener_effects: &mut spawner_effects,
                };
                arena
                    .tick_selector81(actor, context.microseconds, sine, &mut world.rng, &mut host)
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World spawner stopped after reached writes: {actor:?} {error}");
                callbacks.selector81_stops.push((actor, error));
            }
            return;
        }
        // 00445930, Capture the Banner's missile launcher's own +24
        // (selector 94); its missiles update through the grenade's host.
        if !alternate_3c && arena.selector94(actor).is_ok() {
            if callbacks.selector94_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let mut listener_effects = RequiredListenerEffects;
                let mut host = crate::session_platform::world_actor_update::WorldGrenadeHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    world_flags: world.world_flags,
                    clock: context.clock,
                    sine,
                    math: &math,
                    camera_settings: context.inputs.camera_settings,
                    world_start: &mut world.world_start,
                    listener_effects: &mut listener_effects,
                    settings_7c: context.inputs.settings_7c,
                    inverse_sine: crate::session_platform::world_actor_update::inverse_sine_table(
                        executable,
                    )?,
                    voices: Some(&mut context.voices),
                };
                arena
                    .tick_selector94(actor, context.microseconds, &mut world.rng, &mut host)
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World missile launcher stopped after reached writes: {actor:?} {error}");
                callbacks.selector94_stops.push((actor, error));
            }
            return;
        }
        // 004866A0, the selector67 ribbon's own +24; its +3C is 00405140.
        if !alternate_3c && arena.selector67(actor).is_ok() {
            if callbacks.selector67_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World selector67 source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldRibbonHost {
                factory: crate::object_runtime::Selector67FactoryRequest {
                    runtime: context.residency,
                    process: &*context.factory_process,
                    network_active: context.factory_process.network_flag != 0,
                },
                executable,
            };
            if let Err(error) = arena.tick_selector67(actor, context.microseconds, executable, &mut host)
            {
                eprintln!("World selector67 ribbon stopped after reached writes: {actor:?} {error}");
                callbacks.selector67_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0042DC90, the water splash's own +24.
        if !alternate_3c && arena.selector65(actor).is_ok() {
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let mut host = crate::session_platform::world_actor_update::WorldSplashHost {
                    terrain: &world.terrain,
                    particles: &mut *context.particles,
                    executable,
                    process: &*context.factory_process,
                    phase: u32::from(context.state.current_phase().id()),
                };
                arena
                    .tick_selector65(
                        actor,
                        context.microseconds,
                        sine,
                        &mut world.rng,
                        crate::name_entry::name_entry_rng_next,
                        &mut host,
                    )
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World water splash stopped after reached writes: {actor:?} {error}");
            }
            return;
        }
        // 00486250, the cannon shot's own +24, or another machine's shot's
        // +3C.
        if arena.is_shot32(actor) {
            if callbacks.shot32_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let impact_selectors = world
                    .particle_impact_selectors
                    .ok_or("the shot's ground burst requires H15/0's selectors")?;
                let mut host = crate::session_platform::world_actor_update::WorldShotHost {
                    grant_text: context.grant_text,
                    mission: &mut world.mission,
                    tasks: &mut *context.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    furniture: &mut *context.furniture,
                    residency: context.residency,
                    executable,
                    factory_process: &mut *context.factory_process,
                    behavior_tables: &mut *context.behavior_tables,
                    animation_clock: context.animation_clock,
                    session: &*context.state,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    impact_selectors,
                    listeners: &mut *listeners,
                    sine,
                    world_flags: world.world_flags,
                };
                if alternate_3c {
                    arena.tick_remote_shot32(actor, context.microseconds, &mut world.rng, &mut host)
                } else {
                    arena.tick_shot32(actor, context.microseconds, &mut world.rng, &mut host)
                }
                .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World cannon shot stopped after reached writes: {actor:?} {error}");
                callbacks.shot32_stops.push((actor, error));
            }
            return;
        }
        // 00431FE0, the flamethrower's own +24.
        if !alternate_3c && arena.selector76(actor).is_ok() {
            if callbacks.selector76_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let mut host = crate::session_platform::world_actor_update::WorldFlameHost {
                    owners: crate::session_platform::particles::damage::WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(
                            crate::session_platform::particles::damage::BehaviorFactoryLoan {
                                listeners: &mut *listeners,
                                sine,
                                world_flags: world.world_flags,
                            },
                        ),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                };
                arena
                    .tick_selector76(actor, context.microseconds, &mut host)
                    .map_err(|error| format!("{error:?}"))
            })();
            if let Err(error) = result {
                eprintln!("World flamethrower stopped after reached writes: {actor:?} {error}");
                callbacks.selector76_stops.push((actor, error));
            }
            return;
        }
        // 00455E10, the grenade's own +24, the charge's 00455F30, the mine's
        // 00456860, and the missiles' 00457370 (selector 28) and 00458510
        // (selector 25). Another machine's run their +3C: 004559B0 (the
        // grenade, charge and mine), 00457660 (the missiles) and the shared
        // 0047EBF0 (the Bomb).
        if arena.is_thrown(actor) || arena.is_missile(actor) || arena.is_bomb29(actor) {
            if callbacks.grenade26_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let null = context.factory_process.process_null_handle;
                let mut host = crate::session_platform::world_actor_update::WorldGrenadeHost {
                    owners: crate::session_platform::particles::damage::WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(
                            crate::session_platform::particles::damage::BehaviorFactoryLoan {
                                listeners: &mut *listeners,
                                sine,
                                world_flags: world.world_flags,
                            },
                        ),
                        map: context.map.as_deref_mut(),
                    },
                    world_flags: world.world_flags,
                    clock: context.clock,
                    sine,
                    math: &math,
                    camera_settings: context.inputs.camera_settings,
                    world_start: &mut world.world_start,
                    listener_effects: &mut listener_effects,
                    settings_7c: context.inputs.settings_7c,
                    inverse_sine: crate::session_platform::world_actor_update::inverse_sine_table(
                        executable,
                    )?,
                    voices: Some(&mut context.voices),
                };
                let delta = context.microseconds;
                if alternate_3c && arena.is_bomb29(actor) {
                    use crate::object_runtime::StatusTrailHost;
                    let load = Some(host.status_load_factor());
                    let mut remote = crate::object_runtime::RngRemoteHost {
                        host: &mut host,
                        rng: &mut world.rng,
                    };
                    arena
                        .tick_remote_actor(actor, delta, sine, load, &mut remote)
                        .map_err(|error| error.to_string())
                } else if alternate_3c && arena.is_missile(actor) {
                    arena
                        .tick_remote_missile(actor, delta, sine, &mut world.rng, &mut host)
                        .map_err(|error| error.to_string())
                } else if alternate_3c {
                    arena
                        .tick_remote_thrown(actor, delta, sine, &mut world.rng, &mut host)
                        .map_err(|error| error.to_string())
                } else if arena.is_bomb29(actor) {
                    // 00458C80, the Bomb's fuse while armed.
                    arena
                        .tick_bomb29(actor, context.microseconds, &mut world.rng, &mut host)
                        .map_err(|error| error.to_string())
                } else if arena.is_missile25(actor) {
                    arena
                        .tick_missile25(actor, context.microseconds, sine, &mut world.rng, &mut host)
                        .map_err(|error| error.to_string())
                } else if arena.is_missile(actor) {
                    match arena.behavior_resource(actor).map(|state| state.selector) {
                        Ok(28) => arena
                            .tick_missile_base(
                                actor,
                                context.microseconds,
                                sine,
                                &mut world.rng,
                                &mut host,
                            )
                            .map_err(|error| error.to_string()),
                        // 00458C30: an implant no player holds retires.
                        _ => arena
                            .tick_implant(actor, &mut world.rng, &mut host)
                            .map_err(|error| error.to_string()),
                    }
                } else if arena.is_charge27(actor) {
                    // 00455F6A compares +154 with the live process null.
                    let null = null.ok_or("00455F30 needs the process null 0054E894")?;
                    arena
                        .tick_charge27(
                            actor,
                            context.microseconds,
                            null,
                            sine,
                            &mut world.rng,
                            &mut host,
                        )
                        .map_err(|error| error.to_string())
                } else if arena.is_mine38(actor) {
                    arena
                        .tick_mine38(actor, context.microseconds, sine, &mut world.rng, &mut host)
                        .map_err(|error| error.to_string())
                } else {
                    arena
                        .tick_grenade26(actor, context.microseconds, sine, &mut world.rng, &mut host)
                        .map_err(|error| error.to_string())
                }
            })();
            if let Err(error) = result {
                eprintln!("World grenade stopped after reached writes: {actor:?} {error}");
                callbacks.grenade26_stops.push((actor, error));
            }
            return;
        }
        if !alternate_3c && arena.selector86(actor).is_ok() {
            if arena
                .selector86(actor)
                .is_ok_and(|state| state.update_stopped)
            {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World selector86 source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldSelector86Host {
                residency: context.residency,
                terrain: &world.terrain,
                terrain_source: &world.terrain_source,
                camera: context.camera_globals,
                particles: context.particles,
                process: context.factory_process,
                executable,
                sine,
                phase: u32::from(context.state.current_phase().id()),
            };
            if let Err(error) = arena.tick_selector86(
                actor,
                context.microseconds,
                executable,
                &mut host,
                &mut world.rng,
            ) {
                eprintln!("World selector86 stopped after reached writes: {actor:?} {error}");
            }
            return;
        }
        if !alternate_3c && arena.selector60_actor(actor).is_ok() {
            if arena
                .selector60_actor(actor)
                .is_ok_and(|state| state.update_stopped)
            {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World selector60 source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldSelector60Host {
                factory: crate::session_platform::world_actor_factory::WorldActorChildHost {
                    residency: context.residency,
                    executable,
                    process: &*context.factory_process,
                    terrain: &world.terrain,
                    world_flags: world.world_flags,
                    clock: context.clock,
                    sine,
                    math: &math,
                    camera_settings: Some(context.inputs.camera_settings),
                    world_start: Some(&mut world.world_start),
                    listeners: &mut *listeners,
                    listener_effects: &mut listener_effects,
                },
                terrain_source: &world.terrain_source,
                instances: &world.report.instances,
                mission: &world.mission,
            };
            if let Err(error) =
                arena.run_selector60_update(actor, context.microseconds, &mut host, &mut world.rng)
            {
                eprintln!("World selector60 stopped after reached writes: {actor:?} {error}");
            }
            return;
        }
        if !alternate_3c && arena.selector66(actor).is_ok() {
            if arena.selector66(actor).is_ok_and(|state| state.update_stopped) { return; }
            let executable = match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                Ok(executable) => executable,
                Err(error) => { eprintln!("World selector66 source unavailable: {error}"); return; }
            };
            let mut host = crate::session_platform::world_actor_update::WorldSelector66Host {
                residency: context.residency, terrain: &world.terrain,
                terrain_source: &world.terrain_source, particles: context.particles,
                process: context.factory_process, executable,
                phase: u32::from(context.state.current_phase().id()),
            };
            if let Err(error) = arena.tick_selector66(actor,context.microseconds,&mut host,&mut world.rng) {
                eprintln!("World selector66 stopped after reached writes: {actor:?} {error}");
            }
            return;
        }
        if !alternate_3c && arena.timed_pickup(actor).is_ok() {
            if callbacks.pickup_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let placed = arena
                .timed_pickup(actor)
                .is_ok_and(|pickup| pickup.class == crate::object_runtime::PickupClass::Placed);
            let result = if placed {
                // 0046D950 on the network host's update list.
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string());
                let rng = &mut world.rng;
                let particles = &mut *context.particles;
                let sounds = &mut *context.sounds;
                let residency = context.residency;
                let process = &*context.factory_process;
                let phase = u32::from(context.state.current_phase().id());
                executable.and_then(|executable| {
                    arena.tick_placed_pickup(
                        actor,
                        context.clock,
                        context.inputs.settings_20 != 0,
                        &mut world.mission,
                        &mut |arena, object, effect| {
                            let position = arena
                                .object(object)
                                .map_err(|error| error.to_string())?
                                .base
                                .position_words
                                .ok_or("0046D950 reads the pickup's unwritten +32")?;
                            match effect {
                                crate::object_runtime::PickupCollectionEffect::Particle => particles
                                    .birth_pickup_flash(
                                        crate::process_particles::births::ParticleBirthRequest {
                                            particle_type: 3,
                                            position,
                                            velocity: None,
                                            flags: 0,
                                            inputs: crate::process_particles::births::ParticleBirthInputs {
                                                network_active: process.network_flag != 0,
                                                phase: Some(phase),
                                                process,
                                            },
                                        },
                                        executable,
                                        rng,
                                    )
                                    .map(|_| ())
                                    .map_err(|error| format!("00489A90: {error:?}")),
                                crate::object_runtime::PickupCollectionEffect::Sound {
                                    sound,
                                    volume,
                                    pitch,
                                } => sounds
                                    .enqueue(
                                        residency,
                                        crate::process_particles::audio::ParticleSoundRequest {
                                            sound,
                                            position,
                                            volume,
                                            pitch,
                                            flags: 0,
                                        },
                                    )
                                    .map_err(|error| format!("00487EC0: {error}")),
                                crate::object_runtime::PickupCollectionEffect::MarkCollected { .. } => {
                                    Err("0046D950 does not mark a pickup".into())
                                }
                            }
                        },
                    )
                })
            } else {
                arena
                    .tick_timed_pickup(actor, context.microseconds)
                    .map_err(|error| error.to_string())
            };
            if let Err(error) = result {
                eprintln!("World timed pickup stopped after reached writes: {actor:?} {error}");
                callbacks.pickup_stops.push((actor, error));
            }
            return;
        }
        // 00475420, the save pad's own +24.
        if !alternate_3c && arena.selector58(actor).is_ok() {
            if callbacks.selector58_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let mut host = crate::session_platform::world_actor_update::WorldPadHost {
                residency: context.residency,
                sounds: &mut *context.sounds,
                clock: context.clock,
                math: &math,
                inverse_sine: context.inverse_sine,
            };
            if let Err(error) = arena.tick_selector58(actor, context.microseconds, &mut host) {
                eprintln!("World save pad stopped after reached writes: {actor:?} {error}");
                callbacks.selector58_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0043C3D0, the sinking patch's own +24.
        if !alternate_3c && arena.selector83(actor).is_ok() {
            if callbacks.selector83_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let mut host = crate::session_platform::world_actor_update::WorldPatchHost {
                terrain: &mut world.terrain,
            };
            if let Err(error) = arena.tick_selector83(actor, context.microseconds, &mut host) {
                eprintln!("World sinking patch stopped after reached writes: {actor:?} {error}");
                callbacks.selector83_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 004548E0, the pickup spawner's own +24.
        if !alternate_3c && arena.selector53(actor).is_ok() {
            if callbacks.selector53_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World pickup spawner source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_factory::WorldPickupHost {
                factory: crate::session_platform::world_actor_factory::WorldActorChildHost {
                    residency: context.residency,
                    executable,
                    process: &*context.factory_process,
                    terrain: &world.terrain,
                    world_flags: world.world_flags,
                    clock: context.clock,
                    sine,
                    math: &math,
                    camera_settings: Some(context.inputs.camera_settings),
                    world_start: Some(&mut world.world_start),
                    listeners: &mut *listeners,
                    listener_effects: &mut listener_effects,
                },
                terrain_source: &world.terrain_source,
            };
            if let Err(error) =
                arena.tick_selector53(actor, context.microseconds, &mut host, &mut world.rng)
            {
                eprintln!("World pickup spawner stopped after reached writes: {actor:?} {error}");
                callbacks.selector53_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0046E230, the Prison Unit's own +24.
        if !alternate_3c && arena.selector17(actor).is_ok() {
            if callbacks.selector17_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World Prison Unit source unavailable: {error}");
                        return;
                    }
                };
            let mut host = WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
            };
            if let Err(error) = arena.tick_selector17(
                actor,
                context.microseconds,
                Some(world.fade.performance.particle_load_factor as i32),
                &mut host,
            ) {
                eprintln!("World Prison Unit stopped after reached writes: {actor:?} {error}");
                callbacks.selector17_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00454630, the mining rig's own +24.
        if !alternate_3c && arena.selector52(actor).is_ok() {
            if callbacks.selector52_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World mining rig source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldRigHost {
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            if let Err(error) = arena.tick_selector52(actor, context.microseconds, &mut host) {
                eprintln!("World mining rig stopped after reached writes: {actor:?} {error}");
                callbacks.selector52_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0043C0F0, the capture node's own +24: its clock.
        if !alternate_3c && arena.selector84(actor).is_ok() {
            if let Err(error) = arena.tick_selector84(actor, context.microseconds) {
                eprintln!("World capture node stopped: {actor:?} {error}");
            }
            return;
        }
        // 0042D570, the zone's own +24.
        if !alternate_3c && arena.selector74(actor).is_ok() {
            if callbacks.selector74_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World zone source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldZoneHost {
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            if let Err(error) = arena.tick_selector74(actor, context.microseconds, &mut host) {
                eprintln!("World zone stopped after reached writes: {actor:?} {error}");
                callbacks.selector74_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00412A10, the Pursuit Ship's own +24.
        if !alternate_3c && arena.selector61(actor).is_ok() {
            if callbacks.selector61_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World Pursuit Ship source unavailable: {error}");
                        return;
                    }
                };
            let request = crate::object_runtime::Selector61UpdateRequest {
                microseconds: context.microseconds,
                settings_7c: context.inputs.settings_7c,
                inverse_sine: context.inverse_sine,
                sine_quarter: sine,
            };
            let mut host = crate::session_platform::world_actor_update::WorldPursuitShipHost {
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            if let Err(error) = arena.tick_selector61(actor, request, &mut host) {
                eprintln!("World Pursuit Ship stopped after reached writes: {actor:?} {error}");
                callbacks.selector61_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 004072A0, the Alien Brain's ending director's own +24 until the
        // player takes it; then the player's update runs it.
        if !alternate_3c && arena.selector78(actor).is_ok() {
            if callbacks.selector78_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World director source unavailable: {error}");
                        return;
                    }
                };
            let inputs = ArenaIntroInputs {
                camera_settings: context.inputs.camera_settings,
                settings_7c: context.inputs.settings_7c,
                motion_mode: Selector90MotionMode::FractionalThousandths,
                up: *context.up,
            };
            let (result, phase) = {
                let mut host = crate::session_platform::world_actor_update::WorldDirectorHost {
                    family: WorldFamilyUpdateHost {
                        owners: WorldDamageOwners {
                            grant_text: context.grant_text,
                            camera_settings: Some(context.inputs.camera_settings),
                            flash: Some(&mut world.flash),
                            mission: &mut world.mission,
                            tasks: &mut *context.tasks,
                            terrain: &mut world.terrain,
                            particles: &mut *context.particles,
                            sounds: &mut *context.sounds,
                            furniture: &mut *context.furniture,
                            residency: context.residency,
                            executable,
                            factory_process: &mut *context.factory_process,
                            behavior_tables: &mut *context.behavior_tables,
                            animation_clock: context.animation_clock,
                            session: &*context.state,
                            particle_load_factor: world.fade.performance.particle_load_factor,
                            behavior_factory: Some(BehaviorFactoryLoan {
                                listeners: &mut *listeners,
                                sine,
                                world_flags: world.world_flags,
                            }),
                            map: context.map.as_deref_mut(),
                        },
                        rng: &mut world.rng,
                        sine_quarter: sine,
                    },
                    math: &math,
                    clock: context.clock,
                    world_flags: world.world_flags,
                    camera_settings: context.inputs.camera_settings,
                    cheat_flags_84: &mut callbacks.cheat_flags_84,
                    requested_phase: None,
                    listener_effects: RequiredListenerEffects,
                };
                let result =
                    arena.tick_selector78(actor, context.microseconds, inputs, &mut host);
                (result.map_err(|error| error.to_string()), host.requested_phase)
            };
            if let Some(phase) = phase {
                let _ = context.state.request_phase(phase);
            }
            if let Err(error) = result {
                eprintln!("World director stopped after reached writes: {actor:?} {error}");
                callbacks.selector78_stops.push((actor, error));
            }
            return;
        }
        // 00464050, the Alien Brain's attack's own +24, or the brain's
        // 00405FC0, which runs the attack in its state 2.
        if !alternate_3c && arena.selector62(actor).is_ok() {
            if callbacks.selector62_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World attack source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldBrainAttackHost {
                settings_7c: context.inputs.settings_7c,
                pickup_text: context.pickup_text,
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            let result = if arena.selector85(actor).is_ok() {
                arena
                    .tick_selector85(actor, context.microseconds, sine, &mut host)
                    .map_err(|error| format!("brain: {error}"))
            } else {
                arena
                    .tick_selector62(actor, context.microseconds, &mut host)
                    .map_err(|error| error.to_string())
            };
            if let Err(error) = result {
                eprintln!("World attack stopped after reached writes: {actor:?} {error}");
                callbacks.selector62_stops.push((actor, error));
            }
            return;
        }
        // 00450450, the ECM Unit's own +24.
        if !alternate_3c && arena.selector73(actor).is_ok() {
            if callbacks.selector73_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            if let Err(error) = arena.tick_selector73(actor, context.microseconds, sine) {
                eprintln!("World ECM Unit stopped after reached writes: {actor:?} {error}");
                callbacks.selector73_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00473D70, the Remote Beamer's own +24.
        if !alternate_3c && arena.selector55(actor).is_ok() {
            if callbacks.selector55_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let carry = crate::object_runtime::VehicleCarryContext {
                    process: &*context.factory_process,
                    executable,
                    resident: context.residency,
                    text_data: context.grant_text,
                };
                let mut effects = crate::session_platform::world_actor_update::CarryEffects {
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    residency: context.residency,
                    executable,
                    process: &*context.factory_process,
                    phase: u32::from(context.state.current_phase().id()),
                };
                arena
                    .tick_selector55(actor, context.microseconds, carry, &mut effects, &mut world.rng)
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("World Remote Beamer stopped after reached writes: {actor:?} {error}");
                callbacks.selector55_stops.push((actor, error));
            }
            return;
        }
        // 004863F0, the Powerball's own +24.
        if !alternate_3c && arena.selector57(actor).is_ok() {
            if callbacks.selector57_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World Powerball source unavailable: {error}");
                        return;
                    }
                };
            let status_load_factor = world.fade.performance.particle_load_factor as i32;
            let mut host = WorldFamilyUpdateHost {
                owners: WorldDamageOwners {
                    grant_text: context.grant_text,
                    camera_settings: Some(context.inputs.camera_settings),
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut *context.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    furniture: &mut *context.furniture,
                    residency: context.residency,
                    executable,
                    factory_process: &mut *context.factory_process,
                    behavior_tables: &mut *context.behavior_tables,
                    animation_clock: context.animation_clock,
                    session: &*context.state,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut *listeners,
                        sine,
                        world_flags: world.world_flags,
                    }),
                    map: context.map.as_deref_mut(),
                },
                rng: &mut world.rng,
                sine_quarter: sine,
            };
            if let Err(error) = arena.tick_selector57(
                actor,
                context.microseconds,
                Some(status_load_factor),
                sine,
                &mut host,
            ) {
                eprintln!("World Powerball stopped after reached writes: {actor:?} {error}");
                callbacks.selector57_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00453ED0, the falling rock's own +24.
        if !alternate_3c && arena.selector44(actor).is_ok() {
            if callbacks.selector44_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World rock source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldRockHost {
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            if let Err(error) = arena.tick_selector44(actor, context.microseconds, sine, &mut host) {
                eprintln!("World rock stopped after reached writes: {actor:?} {error}");
                callbacks.selector44_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00403300, Powerball's ball's +24, and 00402DD0, its +3C.
        if arena.is_ball(actor) {
            if callbacks.selector59_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World ball source unavailable: {error}");
                        return;
                    }
                };
            let load = Some(world.fade.performance.particle_load_factor as i32);
            let team_flag = *context.network_team_flag;
            let mut host = WorldFamilyUpdateHost {
                owners: WorldDamageOwners {
                    grant_text: context.grant_text,
                    camera_settings: Some(context.inputs.camera_settings),
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut *context.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    furniture: &mut *context.furniture,
                    residency: context.residency,
                    executable,
                    factory_process: &mut *context.factory_process,
                    behavior_tables: &mut *context.behavior_tables,
                    animation_clock: context.animation_clock,
                    session: &*context.state,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut *listeners,
                        sine,
                        world_flags: world.world_flags,
                    }),
                    map: context.map.as_deref_mut(),
                },
                rng: &mut world.rng,
                sine_quarter: sine,
            };
            let delta = context.microseconds;
            let result = if alternate_3c {
                arena.tick_remote_ball(
                    actor,
                    delta,
                    team_flag,
                    load,
                    sine,
                    context.inverse_sine,
                    &mut host,
                )
            } else {
                arena.tick_ball(actor, delta, team_flag, load, sine, context.inverse_sine, &mut host)
            };
            if let Err(error) = result {
                eprintln!("World ball stopped after reached writes: {actor:?} {error}");
                callbacks.selector59_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 004057D0, the Boulders' own +24.
        if !alternate_3c && arena.selector59(actor).is_ok() {
            if callbacks.selector59_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World boulder source unavailable: {error}");
                        return;
                    }
                };
            let status_load_factor = world.fade.performance.particle_load_factor as i32;
            let mut host = WorldFamilyUpdateHost {
                owners: WorldDamageOwners {
                    grant_text: context.grant_text,
                    camera_settings: Some(context.inputs.camera_settings),
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut *context.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    furniture: &mut *context.furniture,
                    residency: context.residency,
                    executable,
                    factory_process: &mut *context.factory_process,
                    behavior_tables: &mut *context.behavior_tables,
                    animation_clock: context.animation_clock,
                    session: &*context.state,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut *listeners,
                        sine,
                        world_flags: world.world_flags,
                    }),
                    map: context.map.as_deref_mut(),
                },
                rng: &mut world.rng,
                sine_quarter: sine,
            };
            // 004098C0 for selector 82, an eye's remains, else 004057D0.
            let result = if arena
                .selector59(actor)
                .is_ok_and(|state| state.timer_148.is_some())
            {
                arena.tick_selector82(
                    actor,
                    context.microseconds,
                    Some(status_load_factor),
                    sine,
                    &mut host,
                )
            } else {
                arena.tick_selector59(
                    actor,
                    context.microseconds,
                    Some(status_load_factor),
                    sine,
                    &mut host,
                )
            };
            if let Err(error) = result {
                eprintln!("World boulder stopped after reached writes: {actor:?} {error}");
                callbacks.selector59_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00476AE0, the Force Field's own +24.
        if !alternate_3c && arena.selector18(actor).is_ok() {
            if callbacks.selector18_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World force field source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldForceFieldHost {
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            if let Err(error) = arena.tick_selector18(actor, context.microseconds, &mut host) {
                eprintln!("World force field stopped after reached writes: {actor:?} {error}");
                callbacks.selector18_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0043BB80, the course target's own +24.
        if !alternate_3c && arena.selector39(actor).is_ok() {
            if callbacks.selector39_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World course target source unavailable: {error}");
                        return;
                    }
                };
            let mut target_effects = RequiredListenerEffects;
            let mut host = crate::session_platform::world_actor_update::WorldTargetHost {
                world_flags: world.world_flags,
                clock: context.clock,
                math: &math,
                camera_settings: context.inputs.camera_settings,
                world_start: &mut world.world_start,
                listener_effects: &mut target_effects,
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
                status_load_factor,
            };
            if let Err(error) = arena.tick_selector39(actor, context.microseconds, &mut host) {
                eprintln!("World course target stopped after reached writes: {actor:?} {error}");
                callbacks.selector39_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00411380, the barrier's own +24.
        if !alternate_3c && arena.selector69(actor).is_ok() {
            if callbacks.selector69_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World barrier source unavailable: {error}");
                        return;
                    }
                };
            let mut barrier_effects = RequiredListenerEffects;
            let mut host = crate::session_platform::world_actor_update::WorldBarrierHost {
                world_flags: world.world_flags,
                clock: context.clock,
                math: &math,
                camera_settings: context.inputs.camera_settings,
                world_start: &mut world.world_start,
                listener_effects: &mut barrier_effects,
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            if let Err(error) = arena.tick_selector69(actor, context.microseconds, &mut host) {
                eprintln!("World barrier stopped after reached writes: {actor:?} {error}");
                callbacks.selector69_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0047FAC0, the Siren's own +24.
        if !alternate_3c && arena.selector47(actor).is_ok() {
            if callbacks.selector47_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World Siren source unavailable: {error}");
                        return;
                    }
                };
            let status_load_factor = world.fade.performance.particle_load_factor as i32;
            let mut host = crate::session_platform::world_actor_update::WorldSirenHost {
                voices: &mut context.voices,
                status_load_factor,
                attract: context.attract,
                world_flags: world.world_flags,
                flash: &mut world.flash,
                camera_settings: context.inputs.camera_settings,
                animation_clock: context.clock,
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: None,
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            if let Err(error) = arena.tick_selector47(actor, context.microseconds, &mut host) {
                eprintln!("World Siren stopped after reached writes: {actor:?} {error}");
                callbacks.selector47_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 004765A0, the stabiliser's own +24.
        if !alternate_3c && arena.selector49(actor).is_ok() {
            if callbacks.selector49_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World stabiliser source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldStabiliserHost {
                policy: &mut *context.terrain_policy,
                detail: context.inputs.settings_70,
                world_flags: world.world_flags,
                flash: &mut world.flash,
                camera_settings: context.inputs.camera_settings,
                animation_clock: context.clock,
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: None,
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
            };
            if let Err(error) = arena.tick_selector49(actor, context.microseconds, &mut host) {
                eprintln!("World stabiliser stopped after reached writes: {actor:?} {error}");
                callbacks.selector49_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00409A20, the bridge's own +24.
        if !alternate_3c && arena.selector37(actor).is_ok() {
            if callbacks.selector37_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World bridge source unavailable: {error}");
                        return;
                    }
                };
            let mut host = crate::session_platform::world_actor_update::WorldBridgeHost {
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
                status_load_factor,
                policy: &mut *context.terrain_policy,
                detail: context.inputs.settings_70,
                world_flags: world.world_flags,
            };
            if let Err(error) = arena.tick_selector37(actor, context.microseconds, &mut host) {
                eprintln!("World bridge stopped after reached writes: {actor:?} {error}");
                callbacks.selector37_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 00425CD0, the +3C of a selector-10 or 21 actor another machine
        // owns: its helper's +4 alone. A stop is logged once and tried again
        // each pass.
        if alternate_3c && arena.selector10_actor(actor).is_ok() {
            let result = inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                .map_err(|error| error.to_string())
                .and_then(|executable| {
                    arena
                        .tick_remote_selector10(
                            actor,
                            crate::object_runtime::Selector10HelperTickRequest {
                                microseconds: context.microseconds,
                                executable,
                                random_state: &mut world.rng,
                                residency: context.residency,
                                sounds: &mut *context.sounds,
                            },
                        )
                        .map_err(|error| error.to_string())
                });
            if let Err(error) = result {
                if !callbacks.skipped_callbacks.contains(&(actor, true)) {
                    callbacks.skipped_callbacks.push((actor, true));
                    eprintln!("World actor feature stopped after reached writes: {actor:?} virtual+3C: {error}");
                }
            }
            return;
        }
        // 00420F40 and 0047EBF0: the +3C of a vehicle, a camera or a plain
        // actor (00527C00) another machine owns. A stop is logged once and
        // tried again each pass.
        let shared_3c = arena.selector16(actor).is_ok()
            || arena.actor(actor).is_ok_and(|state| {
                matches!(state.extension, crate::object_runtime::ActorExtension::Shared)
            });
        if alternate_3c && (arena.selector2_actor(actor).is_ok() || shared_3c) {
            let result = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let null = context
                    .factory_process
                    .process_null_handle
                    .ok_or("00420F40 requires the process NULL 0054E894")?;
                let world_flags = world.world_flags;
                let mut host = crate::session_platform::world_actor_update::WorldVehicleHost {
                    sounds: &mut *context.sounds,
                    residency: context.residency,
                    model_fallback: context.model_fallback,
                    voices: &mut context.voices,
                    particles: &mut *context.particles,
                    executable,
                    factory_process: &*context.factory_process,
                    session: &*context.state,
                    rng: &mut world.rng,
                    difficulty_10: world.mission.word_10,
                    mission: &mut world.mission,
                    animation: context.animation_clock,
                    grant_text: context.grant_text,
                    behavior_tables: &mut *context.behavior_tables,
                    tasks: &mut *context.tasks,
                    terrain: &world.terrain,
                    terrain_source: &world.terrain_source,
                    world_start: &mut world.world_start,
                    listeners: &mut *listeners,
                    camera_settings: context.inputs.camera_settings,
                    sine,
                    math: &math,
                    world_flags,
                    model_process: context.model_process,
                    furniture_query: &mut *context.furniture_query,
                    furniture_draw: context.furniture_draw,
                    load_factor: status_load_factor,
                    flash: &mut world.flash,
                };
                let load = Some(status_load_factor);
                let delta = context.microseconds;
                if arena.selector2_actor(actor).is_ok() {
                    arena.tick_remote_vehicle(actor, delta, null, sine, load, &mut host)
                } else {
                    arena.tick_remote_shared(actor, delta, sine, load, &mut host)
                }
                .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                if !callbacks.skipped_callbacks.contains(&(actor, true)) {
                    callbacks.skipped_callbacks.push((actor, true));
                    eprintln!("World actor feature stopped after reached writes: {actor:?} virtual+3C: {error}");
                }
            }
            return;
        }
        // 00416090, the vehicle's own +24. The loop never reaches the
        // player's actor, so this is a vehicle the player no longer drives:
        // the wreck, once the death camera holds the player, or a live
        // vehicle the player left for a unit it holds.
        if !alternate_3c && arena.selector2_actor(actor).is_ok() {
            if callbacks.vehicle_dying_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let live = arena
                .actor(actor)
                .is_ok_and(|state| state.word_c4 as i32 >= 3);
            let result = if live {
                (|| -> Result<crate::object_runtime::VehicleUpdateReport, String> {
                    let executable =
                        inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                            .map_err(|error| error.to_string())?;
                    let null = context
                        .factory_process
                        .process_null_handle
                        .ok_or("00416090 requires the process NULL 0054E894")?;
                    let world_flags = world.world_flags;
                    let globals = context.camera_globals;
                    let request = crate::object_runtime::VehicleUpdateRequest {
                        drive: crate::object_runtime::VehicleDriveRequest {
                            delta: context.microseconds,
                            executable,
                            ground: crate::object_runtime::VehicleGround {
                                terrain: &world.terrain,
                                world_flags,
                                clock: context.animation_clock.animation_value(),
                                sine_quarter: sine,
                            },
                            process_null: null,
                            position_step: context.inputs.vehicle_position_step,
                            view_zoom_5c0: &globals.view_zoom_5c0,
                            network: crate::object_runtime::vehicle_network(
                                &context.factory_process,
                            ),
                        },
                        attract: false,
                        current_world: world.mission.current_world_a0,
                        status_load_factor: Some(status_load_factor),
                        command: crate::object_runtime::PlayerCommandContext {
                            process: Some(&*context.factory_process),
                            executable: Some(executable),
                            resident: Some(context.residency),
                        },
                        settings_7c: context.inputs.settings_7c,
                        special_view: context.camera_globals.special_view_active,
                        carry: Some(crate::object_runtime::VehicleCarryContext {
                            process: &*context.factory_process,
                            executable,
                            resident: context.residency,
                            text_data: context.grant_text,
                        }),
                    };
                    let mut host = crate::session_platform::world_actor_update::WorldVehicleHost {
                        sounds: &mut *context.sounds,
                        residency: context.residency,
                        model_fallback: context.model_fallback,
                        voices: &mut context.voices,
                        particles: &mut *context.particles,
                        executable,
                        factory_process: &*context.factory_process,
                        session: &*context.state,
                        rng: &mut world.rng,
                        difficulty_10: world.mission.word_10,
                        mission: &mut world.mission,
                        animation: context.animation_clock,
                        grant_text: context.grant_text,
                        behavior_tables: &mut *context.behavior_tables,
                        tasks: &mut *context.tasks,
                        terrain: &world.terrain,
                        terrain_source: &world.terrain_source,
                        world_start: &mut world.world_start,
                        listeners: &mut *listeners,
                        camera_settings: context.inputs.camera_settings,
                        sine,
                        math: &math,
                        world_flags,
                        model_process: context.model_process,
                        furniture_query: &mut *context.furniture_query,
                        furniture_draw: context.furniture_draw,
                        load_factor: status_load_factor,
                        flash: &mut world.flash,
                    };
                    let mut report = arena
                        .tick_vehicle_unheld(actor, &request, &mut host)
                        .map_err(|error| error.to_string())?;
                    let pending = report.wreck.take();
                    // 004160C6: the deep-water tail, after the wreck's branch
                    // when the update's status retired the vehicle.
                    let water_plane = vehicle_water_plane(&world.terrain);
                    let family = WorldFamilyUpdateHost {
                        owners: WorldDamageOwners {
                            grant_text: context.grant_text,
                            camera_settings: Some(context.inputs.camera_settings),
                            flash: Some(&mut world.flash),
                            mission: &mut world.mission,
                            tasks: &mut *context.tasks,
                            terrain: &mut world.terrain,
                            particles: &mut *context.particles,
                            sounds: &mut *context.sounds,
                            furniture: &mut *context.furniture,
                            residency: context.residency,
                            executable,
                            factory_process: &mut *context.factory_process,
                            behavior_tables: &mut *context.behavior_tables,
                            animation_clock: context.animation_clock,
                            session: &*context.state,
                            particle_load_factor: world.fade.performance.particle_load_factor,
                            behavior_factory: Some(BehaviorFactoryLoan {
                                listeners: &mut *listeners,
                                sine,
                                world_flags,
                            }),
                            map: context.map.as_deref_mut(),
                        },
                        rng: &mut world.rng,
                        sine_quarter: sine,
                    };
                    let mut host = WorldWreckHost {
                        family,
                        voices: &mut context.voices,
                    };
                    let request = crate::object_runtime::VehicleDyingRequest {
                        delta: context.microseconds,
                        attract: false,
                        process_null: null,
                        status_load_factor: Some(status_load_factor),
                        sine_quarter: sine,
                        water_plane,
                        world_flags,
                    };
                    if let Some(pending) = pending {
                        arena
                            .finish_vehicle_wreck(actor, pending, &request, &mut host)
                            .map_err(|error| error.to_string())?;
                    }
                    arena
                        .tick_vehicle_water_tail(actor, &request, &mut host)
                        .map_err(|error| error.to_string())?;
                    Ok(report)
                })()
            } else {
                inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                .map_err(|error| error.to_string())
                .and_then(|executable| {
                    let world_flags = world.world_flags;
                    let water_plane = vehicle_water_plane(&world.terrain);
                    tick_world_vehicle_dying(
                        arena,
                        actor,
                        WorldVehicleDying {
                            owners: WorldDamageOwners {
                                grant_text: context.grant_text,
                                camera_settings: Some(context.inputs.camera_settings),
                                flash: Some(&mut world.flash),
                                mission: &mut world.mission,
                                tasks: &mut *context.tasks,
                                terrain: &mut world.terrain,
                                particles: &mut *context.particles,
                                sounds: &mut *context.sounds,
                                furniture: &mut *context.furniture,
                                residency: context.residency,
                                executable,
                                factory_process: &mut *context.factory_process,
                                behavior_tables: &mut *context.behavior_tables,
                                animation_clock: context.animation_clock,
                                session: &*context.state,
                                particle_load_factor: world.fade.performance.particle_load_factor,
                                behavior_factory: Some(BehaviorFactoryLoan {
                                    listeners: &mut *listeners,
                                    sine,
                                    world_flags: world.world_flags,
                                }),
                                map: context.map.as_deref_mut(),
                            },
                            voices: &mut context.voices,
                            rng: &mut world.rng,
                            sine,
                            water_plane,
                            world_flags,
                            status_load_factor,
                            delta: context.microseconds,
                        },
                        None,
                    )
                })
            };
            if let Err(error) = result {
                eprintln!("World vehicle +24 stopped after reached writes: {actor:?} {error}");
                callbacks.vehicle_dying_stops.push((actor, error));
            }
            return;
        }
        // 00478F20 transfers the node before this callback. Concrete class
        // admission selects 00424830/004679F0/00481710/0043E6D0, never a
        // mission/ref whitelist. Bit80000000 selects virtual+3C instead.
        if !alternate_3c && arena.selector77_actor(actor).is_ok() {
            if callbacks.selector77_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable = match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                Ok(executable) => executable,
                Err(error) => {
                    eprintln!("World actor update source stopped: {actor:?} constructor77: {error}");
                    callbacks.selector77_stops.push((actor, error.to_string()));
                    return;
                }
            };
            let process_null_handle = context.factory_process.process_null_handle;
            let mut host = WorldFamilyUpdateHost {
                owners: WorldDamageOwners {
                    grant_text: context.grant_text,
                    camera_settings: Some(context.inputs.camera_settings),
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut *context.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    furniture: &mut *context.furniture,
                    residency: context.residency,
                    executable,
                    factory_process: &mut *context.factory_process,
                    behavior_tables: &mut *context.behavior_tables,
                    animation_clock: context.animation_clock,
                    session: &*context.state,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut *listeners,
                        sine,
                        world_flags: world.world_flags,
                    }),
                    map: context.map.as_deref_mut(),
                },
                rng: &mut world.rng,
                sine_quarter: sine,
            };
            let request = Selector77TickRequest {
                microseconds: context.microseconds,
                process_null_handle,
                status_load_factor: Some(status_load_factor),
                sine_quarter: sine,
                inverse_sine: context.inverse_sine,
                settings_7c: context.inputs.settings_7c,
                process_velocity: Some(crate::object_runtime::PROCESS_VELOCITY_0055F0F8),
            };
            // 00408AB0, the eyes' own +24 (selector 79), and 00409070, the
            // members' (selector 80).
            let result = if arena.selector79(actor).is_ok() {
                arena.tick_selector79(actor, request, &mut host)
            } else if arena.selector80(actor).is_ok() {
                arena.tick_selector80(actor, request, &mut host)
            } else {
                arena.tick_selector77(actor, request, &mut host)
            };
            if let Err(error) = result {
                eprintln!("World actor feature stopped after reached writes: {actor:?} constructor77 virtual+24: {error}");
                callbacks.selector77_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 004019E0, or (another machine's turret) 00402630.
        if arena.selector11_actor(actor).is_ok() {
            if callbacks.selector11_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let executable = match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                Ok(executable) => executable,
                Err(error) => {
                    eprintln!("World actor update source stopped: {actor:?} constructor11: {error}");
                    callbacks.selector11_stops.push((actor, error.to_string()));
                    return;
                }
            };
            let mut host = crate::session_platform::world_actor_update::WorldSelector11Host {
                family: WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        // The host lends the listeners to its own factory.
                        behavior_factory: None,
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                },
                world_start: &mut world.world_start,
                listeners: &mut *listeners,
                world_flags: world.world_flags,
                camera_settings: context.inputs.camera_settings,
            };
            let request = crate::object_runtime::Selector11TickRequest {
                microseconds: context.microseconds,
                status_load_factor: Some(status_load_factor),
                water_plane,
                sine_quarter: sine,
                inverse_sine: context.inverse_sine,
                settings_7c: context.inputs.settings_7c,
                process_velocity: Some(crate::object_runtime::PROCESS_VELOCITY_0055F0F8),
            };
            let result = if alternate_3c {
                arena.tick_remote_selector11(
                    actor,
                    context.microseconds,
                    sine,
                    Some(status_load_factor),
                    &mut host,
                )
            } else {
                arena.tick_selector11(actor, &request, &mut host)
            };
            if let Err(error) = result {
                eprintln!(
                    "World actor feature stopped after reached writes: {actor:?} constructor11 virtual+{}: {error}",
                    if alternate_3c { "3C" } else { "24" }
                );
                callbacks.selector11_stops.push((actor, error.to_string()));
            }
            return;
        }
        // 0048B3F0, the gate's +24; another machine's gate runs it as its
        // +3C (0048C4E0).
        if arena.selector30_actor(actor).is_ok() {
            if callbacks.selector30_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let Ok(executable) =
                inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
            else {
                return;
            };
            let request = crate::object_runtime::Selector30TickRequest {
                animation_clock: context.animation_clock.animation_value(),
                process_null_handle: context.factory_process.process_null_handle,
                network_active: context.factory_process.network_flag != 0,
                mission_elapsed_ms: world.mission.dword_38,
            };
            let mut host = crate::session_platform::world_actor_update::WorldSelector30Host {
                sounds: &mut *context.sounds,
                residency: context.residency,
                voices: &mut context.voices,
                executable,
                factory_process: &*context.factory_process,
                mission: &mut world.mission,
                state: &mut *context.state,
                grant_text: context.grant_text,
                terrain: &world.terrain,
                flash: &mut world.flash,
                listeners: &mut *listeners,
                camera_settings: context.inputs.camera_settings,
                sine,
                math: &math,
                animation_clock: context.animation_clock.animation_value(),
                world_flags: world.world_flags,
                rng: &mut world.rng,
                cheat_flags: &mut callbacks.cheat_flags_84,
            };
            if let Err(error) = arena.tick_selector30(actor, request, &mut host) {
                eprintln!("World actor feature stopped after reached writes: {actor:?} constructor30 virtual+24: {error}");
                callbacks.selector30_stops.push((actor, error.to_string()));
            }
            return;
        }
        if !alternate_3c && arena.selector93(actor).is_ok() {
            if callbacks.selector93_stops.iter().any(|(stopped, _)| *stopped == actor) {
                return;
            }
            let mut terrain = WorldActorTerrainHost {
                residency: context.residency,
                source: &world.terrain_source,
                prepared: &world.terrain,
            };
            if let Err(error) = arena.tick_selector93(
                actor,
                Selector93TickRequest {
                    microseconds: context.microseconds,
                    settings_7c: context.inputs.settings_7c,
                    math: &math,
                    inverse_sine: context.inverse_sine,
                    sine_quarter: sine,
                    up: Some(context.up),
                },
                &mut world.rng,
                &mut terrain,
            ) {
                eprintln!("World actor feature stopped after reached writes: {actor:?} constructor93 virtual+24: {error}");
                callbacks.selector93_stops.push((actor, error.to_string()));
            }
            return;
        }
        if Some(actor) != controller && !alternate_3c {
            if let Ok(class) = arena.family_update_class(actor) {
                // Port fix: the scientists, the alien ships and the creatures
                // (selectors 10, 21 and 36) update at retail's 30 fps step
                // (`PortFixes::paced_actors`).
                let microseconds = match class {
                    FamilyUpdateClass::Selector12
                    | FamilyUpdateClass::Selector35
                    | FamilyUpdateClass::Selector10
                    | FamilyUpdateClass::Selector36 => {
                        match context.paced_microseconds {
                            Some(microseconds) => microseconds,
                            None => {
                                // A held tank's treads still turn: retail steps
                                // them (00481180) at every update, so at 60 fps
                                // at every tick, and only its model reads them.
                                // Stepped at the paced updates alone they turned
                                // at half that rate, flipping between two of
                                // their three frames.
                                if class == FamilyUpdateClass::Selector36
                                    && !callbacks
                                        .family_prefix_stops
                                        .iter()
                                        .any(|stop| stop.actor == actor)
                                {
                                    if let Err(error) = arena
                                        .publish_selector36_counters::<String>(
                                            actor,
                                            context.actor_motion,
                                        )
                                    {
                                        eprintln!("World actor feature stopped after reached writes: {actor:?} 00481180: {error}");
                                    }
                                }
                                return;
                            }
                        }
                    }
                    _ => context.microseconds,
                };
                if class == FamilyUpdateClass::Selector12 {
                    crate::event_log::scientist(arena, actor);
                } else if class == FamilyUpdateClass::Selector35 {
                    crate::event_log::small_ship(arena, actor);
                }
                if callbacks.family_prefix_stops.iter().any(|stop| stop.actor == actor) {
                    return;
                }
                let executable = match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World actor update source stopped: {actor:?} {class:?}: {error}");
                        callbacks.family_prefix_stops.push(FamilyPrefixStop {
                            actor, class, reason: FamilyPrefixStopReason::BehaviorSourceUnavailable(error),
                        });
                        return;
                    }
                };
                let mut host = WorldFamilyUpdateHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut world.flash),
                        mission: &mut world.mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut world.terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor: world.fade.performance.particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags: world.world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    rng: &mut world.rng,
                    sine_quarter: sine,
                };
                let reason = match arena.update_family_prefix(
                    actor,
                    FamilyUpdateRequest {
                        microseconds: microseconds,
                        water_plane: Some(water_plane),
                        status_load_factor: Some(status_load_factor),
                    },
                    &mut host,
                ) {
                    // Both signed-negative and initially-positive delay arms
                    // return from native+24. The latter has run actual physics
                    // even when it just cleared+144; next visit remains eligible.
                    Ok(FamilyUpdateProgress::Returned) => return,
                    Ok(FamilyUpdateProgress::BeforeBehaviorDispatch { .. }) => {
                        let executable = match inf_formats::FrontendExecutable::parse(
                            context.assets.executable_bytes(),
                        ) {
                            Ok(executable) => executable,
                            Err(error) => {
                                eprintln!("World actor behavior source stopped: {actor:?} {class:?}: {error}");
                                callbacks.family_prefix_stops.push(FamilyPrefixStop {
                                    actor, class, reason: FamilyPrefixStopReason::BehaviorSourceUnavailable(error),
                                });
                                return;
                            }
                        };
                        // Wrapper helper fades and Selector35 timer expiry
                        // precede host callbacks. Reached clears receive the
                        // actual process NULL, separate from registry lookup.
                        let helper_inputs = crate::object_runtime::FamilyHelperHistoryInputs {
                            process_null_handle: context.factory_process.process_null_handle,
                        };
                        let mut behavior = WorldFamilyBehaviorHost {
                            grant_text: context.grant_text,
                            residency: context.residency,
                            executable,
                            animation_clock: context.animation_clock,
                            factory_process: &mut *context.factory_process,
                            session: &mut *context.state,
                            behavior_tables: &mut *context.behavior_tables,
                            selector35_process: &mut *context.selector35_process,
                            tasks: &mut *context.tasks,
                            particles: &mut *context.particles,
                            sounds: &mut *context.sounds,
                            process_rng: &mut world.rng,
                            terrain_source: &world.terrain_source,
                            terrain: &mut world.terrain,
                            dying: crate::session_platform::world_actor_behavior::WorldFamilyDyingContext {
                                mission: &mut world.mission,
                                furniture: &mut *context.furniture,
                                world_start: &mut world.world_start,
                                listeners: &mut *listeners,
                                voices: &mut context.voices,
                                world_flags: world.world_flags,
                                camera_settings: context.inputs.camera_settings,
                                up: context.up,
                                flash: &mut world.flash,
                            },
                            sine_quarter: sine,
                            settings_7c: context.inputs.settings_7c,
                            particle_load_factor: world.fade.performance.particle_load_factor,
                        };
                        match arena.dispatch_actor_family_behavior(
                            actor, microseconds, helper_inputs, &mut behavior,
                        ) {
                            Ok(()) if class == FamilyUpdateClass::Selector12 => {
                                let mut terrain = WorldActorCollisionHost {
                                    model_process: context.model_process,
                                    residency: context.residency,
                                    source: &world.terrain_source,
                                    prepared: &world.terrain,
                                    radius: context.furniture_radius,
                                    process: &mut *context.furniture_query,
                                    furniture: context.furniture_draw,
                                    shared_basis: context.actor_shared_basis,
                                };
                                match arena.tick_selector12_tail(
                                    actor,
                                    Selector12TailRequest {
                                        firing: crate::object_runtime::ActorWeaponFireContext {
                                            tables: context.behavior_tables,
                                            rng: &mut world.rng,
                                            process_velocity: Some(crate::object_runtime::PROCESS_VELOCITY_0055F0F8),
                                        },
                                        microseconds: microseconds,
                                        settings_7c: context.inputs.settings_7c,
                                        surface_snap_53a128: *context.surface_snap_53a128,
                                        math: &math,
                                        inverse_sine: context.inverse_sine,
                                        sine_quarter: sine,
                                        terrain: &mut terrain,
                                    },
                                ) {
                                    Ok(()) => return,
                                    Err(error) => {
                                        #[cfg(test)]
                                        super::required_miss_observer::observe(
                                            arena, actor, &error, context.residency,
                                            &world.terrain_source, &world.terrain,
                                            context.furniture_query,
                                        );
                                        eprintln!("World actor motion tail stopped after reached writes: {actor:?} {class:?} virtual+24: {error}");
                                        FamilyPrefixStopReason::Selector12TailFailed(error)
                                    }
                                }
                            }
                            // 00424830: selector 10's and the family's own (selector
                            // 9), alike but for +118.
                            Ok(()) if matches!(class, FamilyUpdateClass::Selector10 | FamilyUpdateClass::Family4236d0) => {
                                let mut terrain = WorldActorCollisionHost {
                                    model_process: context.model_process,
                                    residency: context.residency, source: &world.terrain_source,
                                    prepared: &world.terrain, radius: context.furniture_radius,
                                    process: &mut *context.furniture_query,
                                    furniture: context.furniture_draw,
                                    shared_basis: context.actor_shared_basis,
                                };
                                match arena.tick_selector10_tail_prefix(actor,
                                    crate::object_runtime::Selector10TailPrefixRequest {
                                        firing: crate::object_runtime::ActorWeaponFireContext {
                                            tables: context.behavior_tables,
                                            rng: &mut world.rng,
                                            process_velocity: Some(crate::object_runtime::PROCESS_VELOCITY_0055F0F8),
                                        },
                                        microseconds: microseconds,
                                        settings_7c: context.inputs.settings_7c,
                                        terrain: &mut terrain,
                                    }) {
                                    Ok(crate::object_runtime::Selector10TailNextPhase::Motion { controls }) => {
                                        let mut terrain = WorldActorTerrainHost {
                                            residency: context.residency, source: &world.terrain_source,
                                            prepared: &world.terrain,
                                        };
                                        let mut header = crate::world_actor_contact::WorldActorContactHeaderHost {
                                            residency: context.residency,
                                        };
                                        let motion = if class == FamilyUpdateClass::Family4236d0 {
                                            arena.tick_family4236d0_motion(actor,
                                                crate::object_runtime::Family4236d0MotionRequest {
                                                    microseconds: microseconds, controls, executable,
                                                    environment: crate::object_runtime::ActorContactEnvironment {
                                                        animation_clock: context.animation_clock,
                                                        plane_policy: &world.terrain, up: context.up, sine_quarter: sine,
                                                    },
                                                }, &mut terrain, &mut header)
                                                .map(|()| crate::object_runtime::Selector10MotionNextPhase::PersistentAudio)
                                                .map_err(|error| {
                                                    eprintln!("World family Motion424CF0 stopped after reached writes: {actor:?} {error}");
                                                    FamilyPrefixStopReason::Family4236d0MotionFailed(error)
                                                })
                                        } else {
                                            arena.tick_selector10_motion(actor,
                                            crate::object_runtime::Selector10MotionRequest {
                                                microseconds: microseconds, controls, executable,
                                                environment: crate::object_runtime::ActorContactEnvironment {
                                                    animation_clock: context.animation_clock,
                                                    plane_policy: &world.terrain, up: context.up, sine_quarter: sine,
                                                },
                                                random_state: &mut world.rng,
                                                residency: context.residency, sounds: &mut *context.sounds,
                                            }, &mut terrain, &mut header)
                                                .map_err(|error| {
                                                    eprintln!("World class10 Motion425400 stopped after reached writes: {actor:?} {error}");
                                                    FamilyPrefixStopReason::Selector10MotionFailed(error)
                                                })
                                                // 00405600 continues selector 21's motion with its swings.
                                                .and_then(|phase| {
                                                    let swinging = arena
                                                        .selector10_actor(actor)
                                                        .is_ok_and(|state| state.swing.is_some());
                                                    if swinging {
                                                        arena.tick_selector21_swing(actor, microseconds, &mut world.rng)
                                                            .map_err(|error| {
                                                                eprintln!("World class21 swing stopped after reached writes: {actor:?} {error}");
                                                                FamilyPrefixStopReason::Selector21SwingFailed(error)
                                                            })?;
                                                    }
                                                    Ok(phase)
                                                })
                                        };
                                        match motion {
                                            Ok(crate::object_runtime::Selector10MotionNextPhase::PersistentAudio) => {
                                                match arena.tick_selector10_persistent_audio(actor,
                                                    context.residency, &mut *context.sounds) {
                                                    Ok(()) => return,
                                                    Err(error) => {
                                                        eprintln!("World class10 audio stopped after reached writes: {actor:?} {error}");
                                                        FamilyPrefixStopReason::Selector10PersistentAudioFailed(error)
                                                    }
                                                }
                                            }
                                            Err(reason) => reason,
                                        }
                                    }
                                    Err(error) => {
                                        eprintln!("World class10 tail stopped after reached writes: {actor:?} {error}");
                                        FamilyPrefixStopReason::Selector10TailFailed(error)
                                    }
                                }
                            }
                            Ok(()) if class == FamilyUpdateClass::Selector35 => {
                                use crate::session_platform::world_actor_update::{
                                    tick_world_selector35_tail, WorldSelector35ClearanceHost,
                                    WorldSelector35TailContext,
                                };
                                let mut camera_context = crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
                                    crate::session_platform::world_actor_factory::WorldCameraInputs {
                                        residency: context.residency, sine_quarter: sine,
                                        settings: context.inputs.camera_settings,
                                        process: Some(&*context.factory_process), animation_clock: context.clock,
                                    });
                                let mut camera = camera_context.factory(&world.terrain, world.world_flags, listeners);
                                match tick_world_selector35_tail(arena, actor, WorldSelector35TailContext {
                                    microseconds: microseconds,
                                    settings_7c: context.inputs.settings_7c,
                                    executable, sine,
                                    clearance: WorldSelector35ClearanceHost {
                                        residency: context.residency, source: &world.terrain_source,
                                        terrain: &world.terrain, furniture: context.furniture_query,
                                    },
                                    firing: crate::object_runtime::ActorWeaponFireContext {
                                        tables: context.behavior_tables, rng: &mut world.rng,
                                        process_velocity: Some(crate::object_runtime::PROCESS_VELOCITY_0055F0F8),
                                    },
                                    camera: &mut camera, sounds: &mut *context.sounds,
                                    voices: &mut context.voices,
                                }) {
                                    Ok(()) => return,
                                    Err(error) => {
                                        eprintln!("World selector35 tail stopped after reached writes: {actor:?} {error}");
                                        FamilyPrefixStopReason::Selector35TailFailed(error)
                                    }
                                }
                            }
                            Ok(()) if class == FamilyUpdateClass::Selector34 => {
                                use crate::session_platform::world_actor_update::{
                                    tick_world_selector34_tail, WorldSelector34TailContext,
                                    WorldSelector35ClearanceHost,
                                };
                                let mut camera_context = crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
                                    crate::session_platform::world_actor_factory::WorldCameraInputs {
                                        residency: context.residency, sine_quarter: sine,
                                        settings: context.inputs.camera_settings,
                                        process: Some(&*context.factory_process), animation_clock: context.clock,
                                    });
                                let mut camera = camera_context.factory(&world.terrain, world.world_flags, listeners);
                                match tick_world_selector34_tail(arena, actor, WorldSelector34TailContext {
                                    microseconds: microseconds,
                                    settings_7c: context.inputs.settings_7c,
                                    executable, sine,
                                    inverse_sine: context.inverse_sine,
                                    clearance: WorldSelector35ClearanceHost {
                                        residency: context.residency, source: &world.terrain_source,
                                        terrain: &world.terrain, furniture: context.furniture_query,
                                    },
                                    firing: crate::object_runtime::ActorWeaponFireContext {
                                        tables: context.behavior_tables, rng: &mut world.rng,
                                        process_velocity: Some(crate::object_runtime::PROCESS_VELOCITY_0055F0F8),
                                    },
                                    camera: &mut camera, sounds: &mut *context.sounds,
                                    voices: &mut context.voices,
                                    ribbon: crate::object_runtime::Selector67FactoryRequest {
                                        runtime: context.residency,
                                        process: &*context.factory_process,
                                        network_active: context.factory_process.network_flag != 0,
                                    },
                                }) {
                                    Ok(()) => return,
                                    Err(error) => {
                                        eprintln!("World selector34 tail stopped after reached writes: {actor:?} {error}");
                                        FamilyPrefixStopReason::Selector34TailFailed(error)
                                    }
                                }
                            }
                            Ok(()) if class == FamilyUpdateClass::Selector36 => {
                                let mut child = crate::session_platform::world_actor_factory::WorldActorChildHost {
                                    residency: context.residency,
                                    executable,
                                    process: &*context.factory_process,
                                    terrain: &world.terrain,
                                    world_flags: world.world_flags,
                                    clock: context.clock,
                                    sine,
                                    math: &math,
                                    camera_settings: Some(context.inputs.camera_settings),
                                    world_start: Some(&mut world.world_start),
                                    listeners: &mut *listeners,
                                    listener_effects: &mut listener_effects,
                                };
                                let mut terrain = WorldActorCollisionHost {
                                    model_process: context.model_process,
                                    residency: context.residency, source: &world.terrain_source,
                                    prepared: &world.terrain, radius: context.furniture_radius,
                                    process: &mut *context.furniture_query,
                                    furniture: context.furniture_draw,
                                    shared_basis: context.actor_shared_basis,
                                };
                                match arena.tick_selector36_tail_prefix(
                                    actor,
                                    Selector36TailPrefixRequest {
                                        behavior_tables: context.behavior_tables,
                                        process_velocity: Some(crate::object_runtime::PROCESS_VELOCITY_0055F0F8),
                                        microseconds: microseconds,
                                        process: context.actor_motion,
                                        settings_7c: context.inputs.settings_7c,
                                        math: &math,
                                        inverse_sine: context.inverse_sine,
                                        shared_basis: context.actor_shared_basis,
                                        random_state: &mut world.rng,
                                        terrain: &mut terrain,
                                        child: &mut child,
                                    },
                                ) {
                                    Ok(crate::object_runtime::Selector36TailNextPhase::Motion { controls }) => {
                                        let mut terrain = WorldActorTerrainHost {
                                            residency: context.residency,
                                            source: &world.terrain_source,
                                            prepared: &world.terrain,
                                        };
                                        let mut header = crate::world_actor_contact::WorldActorContactHeaderHost {
                                            residency: context.residency,
                                        };
                                        match arena.tick_selector36_motion(actor,
                                            crate::object_runtime::Selector36MotionRequest {
                                                microseconds: microseconds,
                                                controls,
                                                executable,
                                                environment: crate::object_runtime::ActorContactEnvironment {
                                                    animation_clock: context.animation_clock,
                                                    plane_policy: &world.terrain,
                                                    up: context.up,
                                                    sine_quarter: sine,
                                                },
                                                // Admitted original BF09 caller on Windows8+ non-LAA:
                                                // the normal buffer is a real native stack address.
                                                stack_policy: crate::object_runtime::ActorContactStackPolicy::Bf09Class36Windows8PlusStack,
                                                math: &math,
                                                inverse_sine: context.inverse_sine,
                                            }, &mut terrain, &mut header) {
                                            Ok(crate::object_runtime::Selector36MotionNextPhase::PersistentAudio481b10 { controls }) => {
                                                let mut camera_context = crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
                                                    crate::session_platform::world_actor_factory::WorldCameraInputs {
                                                        residency: context.residency, sine_quarter: sine,
                                                        settings: context.inputs.camera_settings,
                                                        process: Some(&*context.factory_process), animation_clock: context.clock,
                                                    });
                                                let mut factory = camera_context.factory(&world.terrain, world.world_flags, listeners);
                                                match arena.tick_selector36_persistent_audio(actor,
                                                    crate::object_runtime::Selector36PersistentAudioRequest { controls },
                                                    &mut factory, context.residency, &mut *context.sounds,
                                                    &mut context.voices) {
                                                    Ok(()) => return,
                                                    Err(error) => {
                                                        eprintln!("World Selector36 persistent audio stopped after reached writes: {actor:?} {error}");
                                                        FamilyPrefixStopReason::Selector36PersistentAudioFailed(error)
                                                    }
                                                }
                                            }
                                            Err(error) => {
                                                eprintln!("World actor Selector36 Motion481C10 failed after reached writes: {actor:?} {error}");
                                                FamilyPrefixStopReason::Selector36MotionFailed(error)
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        eprintln!("World actor Selector36 tail failed after reached writes: {actor:?} {error}");
                                        FamilyPrefixStopReason::Selector36TailFailed(error)
                                    }
                                }
                            }
                            Ok(()) => {
                                eprintln!("World actor feature stopped: {actor:?} {class:?} virtual+24 completed virtual+D8; class tail remains required");
                                FamilyPrefixStopReason::AfterBehaviorDispatch
                            }
                            Err(error) => {
                                eprintln!("World actor behavior stopped after reached writes: {actor:?} {class:?} virtual+D8: {error}");
                                FamilyPrefixStopReason::BehaviorFailed(error)
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("World actor feature stopped after reached writes: {actor:?} {class:?} virtual+24: {error}");
                        FamilyPrefixStopReason::Failed(error)
                    }
                };
                callbacks.family_prefix_stops.push(FamilyPrefixStop { actor, class, reason });
                return;
            }
        }
        if Some(actor) != controller || alternate_3c {
            skipped_callbacks += 1;
            if !callbacks
                .skipped_callbacks
                .contains(&(actor, alternate_3c))
            {
                callbacks.skipped_callbacks.push((actor, alternate_3c));
                eprintln!(
                    "World actor callback skipped: {actor:?} virtual+{:02X} remains unowned",
                    if alternate_3c { 0x3c } else { 0x24 }
                );
            }
            return;
        }
        let attachment = attachment.as_mut().expect("actual controller admitted");
        if attachment.controller_failure.is_some() {
            return;
        }
        let executable =
            match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                Ok(executable) => executable,
                Err(error) => {
                    attachment.controller_failure = Some(error.to_string());
                    eprintln!("Intro retirement executable unavailable: {error}");
                    return;
                }
            };
        let phase = Some(u32::from(context.state.current_phase().id()));
        let mut services = ProductionIntroServices {
            retirement: ProductionRetirementHost {
                owners: RetirementOwners {
                    grant_text: context.grant_text,
                    mission: &mut world.mission,
                    terrain: &mut world.terrain,
                    tasks: &mut *context.tasks,
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    furniture: &mut *context.furniture,
                    residency: context.residency,
                    executable,
                    phase,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    factory_process: &mut *context.factory_process,
                    // The intro lends no world damage owners.
                    world: None,
                },
                rng: &mut world.rng,
            },
            state: &mut *context.state,
            math: &math,
            sine,
            clock: context.clock,
            world_flags: world.world_flags,
            camera_settings: context.inputs.camera_settings,
            listeners: &mut *listeners,
            listener_effects: &mut listener_effects,
        };
        if let Err(error) = arena.tick_selector88_intro_live(
            actor,
            context.microseconds,
            ArenaIntroRequest {
                runtime: context.residency,
                inputs: ArenaIntroInputs {
                    camera_settings: context.inputs.camera_settings,
                    settings_7c: context.inputs.settings_7c,
                    // Executable 00538BC0 starts at one; all current-up
                    // consumers borrow the retained CRT-initialized owner.
                    motion_mode: Selector90MotionMode::FractionalThousandths,
                    up: *context.up,
                },
                services: &mut services,
            },
        ) {
            eprintln!("Intro controller feature stopped after reached writes: {error:?}");
            attachment.controller_failure = Some(format!("{error:?}"));
        }
    });
    if let Err(error) = pass {
        if callbacks.list_failure.is_none() {
            eprintln!("World update-list feature stopped: {error}");
            callbacks.list_failure = Some(error.to_string());
            if let Some(attachment) = attachment.as_mut() {
                if attachment.controller_failure.is_none() {
                    attachment.controller_failure = Some(error.to_string());
                }
            }
        }
    }
    if first_pass {
        eprintln!("World update-list: {skipped_callbacks} unowned callbacks skipped");
        callbacks.unowned_callbacks_logged = true;
    }
    if pass.is_ok() {
        // 00478FE7..00478FF8, then the header swap at 00478FFD.
        if let (Some(player), Some(actor)) = (player, player_actor) {
            tick_world_player(
                WorldPlayerOwners {
                    arena: &mut *arena,
                    constructions: &world.constructions,
                    updates: &mut *callbacks,
                    terrain: &mut world.terrain,
                    world_flags: world.world_flags,
                    current_world: world.mission.current_world_a0,
                    status_load_factor,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    rng: &mut world.rng,
                    difficulty_10: world.mission.word_10,
                    mission: &mut world.mission,
                    terrain_source: &world.terrain_source,
                    world_start: &mut world.world_start,
                    listeners: &mut *listeners,
                    math: &math,
                    respawn_header: world.respawn_header,
                    vehicle_loss_limit: world.briefing_milestone_count,
                    settings_84: context.inputs.settings_84,
                    flash: &mut world.flash,
                    instances: &world.report.instances,
                },
                player,
                actor,
                &mut context,
            );
        }
        arena.swap_flag1_lists();
    }
    // 00479022: 00478F20 runs 004791F0 after its update list and swap.
    match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
        Ok(executable) => {
            let network_active = context.factory_process.network_flag != 0;
            let mut host = crate::session_platform::world_contact::WorldContactPassHost {
                damage: WorldDamageOwners {
                    grant_text: context.grant_text,
                    camera_settings: Some(context.inputs.camera_settings),
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut *context.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut *context.particles,
                    sounds: &mut *context.sounds,
                    furniture: &mut *context.furniture,
                    residency: context.residency,
                    executable,
                    factory_process: &mut *context.factory_process,
                    behavior_tables: &mut *context.behavior_tables,
                    animation_clock: context.animation_clock,
                    session: &*context.state,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut *listeners,
                        sine,
                        world_flags: world.world_flags,
                    }),
                    map: context.map.as_deref_mut(),
                },
                rng: &mut world.rng,
                model_fallback: context.model_fallback,
                model_process: context.model_process,
                furniture_query: &mut *context.furniture_query,
                furniture_radius: context.furniture_radius,
                furniture_draw: context.furniture_draw,
                shared_basis: context.actor_shared_basis,
                terrain_source: &world.terrain_source,
                world_flags: world.world_flags,
                sine_quarter: sine,
                network_active,
                world_start: &mut world.world_start,
                camera_settings: context.inputs.camera_settings,
                paced_skip: context.paced_microseconds.is_none(),
                mission_completion: context.mission_completion && !network_active,
                grant_text: context.grant_text,
                factory_tables: std::cell::OnceCell::new(),
                math: &math,
            };
            crate::crash_log::step(crate::crash_log::Step::WorldContact);
            crate::session_platform::world_contact::run_world_contact_pass(
                &mut world.arena,
                &mut world.actor_updates,
                &mut host,
            );
            // 004875A0 runs the pair pass 004790C0 after 00478F20 returns.
            crate::session_platform::world_contact::run_world_pair_pass(
                &mut world.arena,
                &mut world.actor_updates,
                &mut host,
            );
        }
        Err(error) => eprintln!("World contact pass source unavailable: {error}"),
    }
}

/// The owners a vehicle's dying update `00416090` reaches.
struct WorldVehicleDying<'a, 'exe> {
    owners: WorldDamageOwners<'a, 'exe>,
    /// The platform's voice teardown, for the weapon loop the wreck releases.
    voices: &'a mut dyn crate::process_sound::ProcessSoundVoiceTeardown,
    rng: &'a mut u32,
    sine: &'a [u16; 4096],
    water_plane: i16,
    world_flags: u32,
    status_load_factor: i32,
    delta: u32,
}

/// The water plane `0055EE3C` shifted by `005483BC`, as a signed WORD.
fn vehicle_water_plane(terrain: &crate::terrain_runtime::TerrainRuntime) -> i16 {
    (terrain.plane_base() >> terrain.plane_shift()) as i16
}

/// `00416090` with the world's services: the wreck's dying update, or with
/// `pending` the wreck's branch that ends a controlled update whose status
/// retired the vehicle.
fn tick_world_vehicle_dying(
    arena: &mut ObjectArena,
    vehicle: ObjectId,
    world: WorldVehicleDying<'_, '_>,
    pending: Option<crate::object_runtime::VehicleWreckPending>,
) -> Result<crate::object_runtime::VehicleUpdateReport, String> {
    let process_null = world
        .owners
        .factory_process
        .process_null_handle
        .ok_or("00416090 requires the process NULL 0054E894")?;
    let request = crate::object_runtime::VehicleDyingRequest {
        delta: world.delta,
        attract: false,
        process_null,
        status_load_factor: Some(world.status_load_factor),
        sine_quarter: world.sine,
        water_plane: world.water_plane,
        world_flags: world.world_flags,
    };
    let mut host = WorldWreckHost {
        family: WorldFamilyUpdateHost {
            owners: world.owners,
            rng: world.rng,
            sine_quarter: world.sine,
        },
        voices: world.voices,
    };
    match pending {
        Some(pending) => arena
            .finish_vehicle_wreck(vehicle, pending, &request, &mut host)
            .map(|()| Default::default()),
        None => arena.tick_vehicle_dying(vehicle, &request, &mut host),
    }
    .map_err(|error| error.to_string())
}

/// What `0048E170` builds the restart's placements with.
struct PlacementOwners<'a> {
    instances: &'a crate::world_loader::WorldInstanceAllocation,
    terrain: &'a crate::terrain_runtime::TerrainRuntime,
    world_flags: u32,
    math: &'a NativeBasisMath,
    world_start: &'a mut crate::world_start::WorldStartState,
    listeners: &'a mut crate::object_runtime::CreationListeners,
    rng: &'a mut u32,
}

/// `0046A61C..0046A637`: each banner the host's arena restart sent home,
/// built again from its placement in mode 0 (`0048E170` with the process
/// NULL); one past the placements builds nothing.
fn build_placements(
    arena: &mut ObjectArena,
    placements: &[i16],
    owners: PlacementOwners<'_>,
    context: &mut WorldActorTickContext<'_>,
) -> Result<(), String> {
    use crate::session_platform::world_actor_factory::{
        placement_request, RequiredListenerEffects, WorldActorChildHost,
    };
    let PlacementOwners {
        instances,
        terrain,
        world_flags,
        math,
        world_start,
        listeners,
        rng,
    } = owners;
    let executable = inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
        .map_err(|error| error.to_string())?;
    let mut factory = WorldActorChildHost {
        residency: context.residency,
        executable,
        process: &*context.factory_process,
        terrain,
        world_flags,
        clock: context.clock,
        sine: &context.assets.sine_table().values,
        math,
        camera_settings: Some(context.inputs.camera_settings),
        world_start: Some(world_start),
        listeners,
        listener_effects: &mut RequiredListenerEffects,
    };
    for &placement in placements {
        let index = i32::from(placement) as u32;
        if let Some(request) = placement_request(instances, terrain, index, None)? {
            factory
                .create_object(arena, request, rng)
                .map_err(|error| format!("0048E170 for placement {index}: {error}"))?;
        }
    }
    Ok(())
}

/// The world fields the player block reads and writes, borrowed apart from
/// the listeners the contact pass still holds.
struct WorldPlayerOwners<'a> {
    arena: &'a mut ObjectArena,
    constructions: &'a [crate::object_runtime::WorldPlayerConstruction],
    updates: &'a mut crate::world_backend::WorldActorUpdateState,
    /// Mutable for the wreck's radius effect, which damages furniture.
    terrain: &'a mut crate::terrain_runtime::TerrainRuntime,
    world_flags: u32,
    current_world: u32,
    status_load_factor: i32,
    particle_load_factor: u32,
    rng: &'a mut u32,
    /// Mission +10, the damage multiplier `0048E7C0` returns.
    difficulty_10: Option<u16>,
    /// The shop's beam-down records a filled factory's placement.
    mission: &'a mut crate::world_mission::WorldMission,
    /// The vehicle drop's attachment (`00405420`) reaches the world factory.
    terrain_source: &'a crate::world_actor_terrain::WorldTerrainSource,
    world_start: &'a mut crate::world_start::WorldStartState,
    listeners: &'a mut crate::object_runtime::CreationListeners,
    math: &'a NativeBasisMath,
    /// The held camera's respawn `0046A890` reads these world inputs.
    respawn_header: crate::world_backend::WorldRespawnHeader,
    vehicle_loss_limit: u32,
    settings_84: u32,
    /// The world flash a steered missile's lost launcher starts.
    flash: &'a mut crate::world_flash::WorldFlashState,
    /// The world's placements, which the host's arena restart builds from.
    instances: &'a crate::world_loader::WorldInstanceAllocation,
}

/// 0046A260 for the first player, after the update loop: the block comes
/// from the player's own input references, then the vehicle's +44
/// (00416170) runs with it. Omitted vehicle sub-features are logged once per
/// world; a stopped vehicle update freezes the vehicle but keeps the block.
fn tick_world_player(
    owners: WorldPlayerOwners<'_>,
    player: crate::object_runtime::PlayerId,
    actor: ObjectId,
    context: &mut WorldActorTickContext<'_>,
) {
    use crate::object_runtime::{
        PlayerCommandContext, PlayerUpdateInputs, PlayerVehicleCall, VehicleDriveRequest,
        VehicleGround, VehicleUpdateRequest,
    };
    let WorldPlayerOwners {
        arena,
        constructions,
        updates,
        terrain,
        world_flags,
        current_world,
        status_load_factor,
        particle_load_factor,
        rng,
        difficulty_10,
        mission,
        terrain_source,
        world_start,
        listeners,
        math,
        respawn_header,
        vehicle_loss_limit,
        settings_84,
        flash,
        instances,
    } = owners;
    if updates.player_update_stop.is_some() {
        return;
    }
    let result = (|| -> Result<crate::object_runtime::PlayerControlsUpdate, String> {
        let consumers = constructions
            .iter()
            .find(|construction| construction.player() == player)
            .and_then(|construction| construction.input_consumers())
            .ok_or("the player's input references are not held")?;
        let flags = arena
            .object(actor)
            .map_err(|error| error.to_string())?
            .base
            .flags;
        let lifecycle = arena
            .actor(actor)
            .map_err(|error| error.to_string())?
            .word_c4 as i32;
        arena
            .update_player_controls(
                player,
                PlayerUpdateInputs {
                    delta_micros: context.microseconds as i32,
                    slow_input_5c5: context.camera_globals.special_view_active,
                    input_mode_08: context.inputs.input_mode_08,
                    network: context.factory_process.network_flag,
                },
                context.input,
                &consumers,
                Some((flags, lifecycle)),
                Some(actor),
                Some(crate::object_runtime::PlayerNetworkState {
                    key_countdown_ed18: &mut *context.network_key_countdown,
                    team_flag_81c: &mut *context.network_team_flag,
                    mode: context.factory_process.settings[0x10 / 4],
                    host: context.factory_process.host_flag != 0,
                }),
            )
            .map_err(|error| error.to_string())
    })();
    let call = match result {
        Ok(update) => {
            updates.log_key_omissions(update.omissions);
            if !update.placements.is_empty() {
                if let Err(error) = build_placements(
                    arena,
                    &update.placements,
                    PlacementOwners {
                        instances,
                        terrain: &*terrain,
                        world_flags,
                        math,
                        world_start: &mut *world_start,
                        listeners: &mut *listeners,
                        rng: &mut *rng,
                    },
                    context,
                ) {
                    let line = format!("World player update stopped: {player:?} {error}");
                    eprintln!("{line}");
                    crate::crash_log::player_stuck(&line);
                    updates.player_update_stop = Some(error);
                    return;
                }
            }
            if update.restart_powerball {
                let result = (|| -> Result<(), String> {
                    let executable =
                        inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                            .map_err(|error| error.to_string())?;
                    let sine = &context.assets.sine_table().values;
                    let mut host = WorldFamilyUpdateHost {
                        owners: WorldDamageOwners {
                            grant_text: context.grant_text,
                            camera_settings: Some(context.inputs.camera_settings),
                            flash: Some(&mut *flash),
                            mission: &mut *mission,
                            tasks: &mut *context.tasks,
                            terrain: &mut *terrain,
                            particles: &mut *context.particles,
                            sounds: &mut *context.sounds,
                            furniture: &mut *context.furniture,
                            residency: context.residency,
                            executable,
                            factory_process: &mut *context.factory_process,
                            behavior_tables: &mut *context.behavior_tables,
                            animation_clock: context.animation_clock,
                            session: &*context.state,
                            particle_load_factor,
                            behavior_factory: Some(BehaviorFactoryLoan {
                                listeners: &mut *listeners,
                                sine,
                                world_flags,
                            }),
                            map: context.map.as_deref_mut(),
                        },
                        rng: &mut *rng,
                        sine_quarter: sine,
                    };
                    arena
                        .restart_powerball(&mut *context.network_key_countdown, &mut host)
                        .map_err(|error| error.to_string())
                })();
                if let Err(error) = result {
                    let line = format!("World player update stopped: {player:?} {error}");
                    eprintln!("{line}");
                    crate::crash_log::player_stuck(&line);
                    updates.player_update_stop = Some(error);
                    return;
                }
            }
            // The +54 repeat's commands ran inside 0046A260.
            let owners = ShopWorldOwners {
                mission: &mut *mission,
                terrain: &*terrain,
                world_start: &mut *world_start,
                listeners: &mut *listeners,
                world_flags,
                rng: &mut *rng,
            };
            if let Some(services) = shop_services(context, owners) {
                crate::session_platform::world_actor_update::drain_shop_menus(
                    arena,
                    &mut updates.shop_stop,
                    services,
                );
            }
            update.call
        }
        Err(error) => {
            let line = format!("World player update stopped: {player:?} {error}");
            eprintln!("{line}");
            crate::crash_log::player_stuck(&line);
            updates.player_update_stop = Some(error);
            return;
        }
    };
    match call {
        PlayerVehicleCall::Update44
            if updates.player_vehicle_stop.is_none() && arena.selector16(actor).is_ok() =>
        {
            // 0040F890, the camera's +44 while the player holds it.
            let outcome = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let sine = &context.assets.sine_table().values;
                let grants = inf_formats::parse_grant_table(executable)
                    .map_err(|error| error.to_string())?;
                let texts = inf_formats::parse_grant_texts(executable)
                    .map_err(|error| error.to_string())?;
                let limits = inf_formats::parse_vehicle_upgrade_limits(executable)
                    .map_err(|error| error.to_string())?;
                let vehicle_table = inf_formats::parse_vehicle_behavior_table(executable)
                    .map_err(|error| error.to_string())?;
                let mut controls = arena
                    .world_player(player)
                    .map_err(|error| error.to_string())?
                    .controls;
                let mut camera_context =
                    crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
                        crate::session_platform::world_actor_factory::WorldCameraInputs {
                            residency: context.residency,
                            sine_quarter: sine,
                            settings: context.inputs.camera_settings,
                            process: Some(&*context.factory_process),
                            animation_clock: context.clock,
                        },
                    );
                let mut factory = camera_context.factory(&*terrain, world_flags, &mut *listeners);
                let mut host = crate::session_platform::world_actor_update::WorldRespawnHost {
                    residency: context.residency,
                    terrain: &*terrain,
                    world_start: &mut *world_start,
                    rng: &mut *rng,
                    process: &*context.factory_process,
                    sounds: &mut *context.sounds,
                    particles: &mut *context.particles,
                    executable,
                    grant_text: context.grant_text,
                    pickup_text: context.pickup_text.as_ref(),
                    sine,
                    clock: context.clock,
                    world_flags,
                    camera_settings: context.inputs.camera_settings,
                    phase: u32::from(context.state.current_phase().id()),
                    header: respawn_header,
                    loss_limit: vehicle_loss_limit,
                    mission: &mut *mission,
                    current_world_a0: current_world,
                    settings_84,
                    pickups: crate::object_runtime::PickupTables {
                        grants: &grants,
                        texts: &texts,
                        upgrade_limits: &limits,
                        network_active: context.factory_process.network_flag != 0,
                    },
                    vehicle_table: &vehicle_table,
                    inverse_sine: context.inverse_sine,
                };
                let outcome = arena.tick_held_camera(
                    actor,
                    context.microseconds,
                    &mut controls,
                    &mut factory,
                    &mut host,
                );
                // Reached control writes stay even when the update stops.
                arena
                    .set_player_controls(player, controls)
                    .map_err(|error| error.to_string())?;
                outcome.map_err(|error| error.to_string())
            })();
            if let Err(error) = outcome {
                let line = format!("Held camera {actor:?} stopped after reached writes: {error}");
                eprintln!("{line}");
                crate::crash_log::player_stuck(&line);
                updates.player_vehicle_stop = Some(error);
            }
        }
        PlayerVehicleCall::Update44
            if updates.player_vehicle_stop.is_none() && arena.is_missile(actor) =>
        {
            // 00457800, the missiles' +44: the player steers the implant.
            let outcome = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let sine = &context.assets.sine_table().values;
                let controls = arena
                    .world_player(player)
                    .map_err(|error| error.to_string())?
                    .controls;
                let mut listener_effects = RequiredListenerEffects;
                let mut host = crate::session_platform::world_actor_update::WorldGrenadeHost {
                    owners: WorldDamageOwners {
                        grant_text: context.grant_text,
                        camera_settings: Some(context.inputs.camera_settings),
                        flash: Some(&mut *flash),
                        mission: &mut *mission,
                        tasks: &mut *context.tasks,
                        terrain: &mut *terrain,
                        particles: &mut *context.particles,
                        sounds: &mut *context.sounds,
                        furniture: &mut *context.furniture,
                        residency: context.residency,
                        executable,
                        factory_process: &mut *context.factory_process,
                        behavior_tables: &mut *context.behavior_tables,
                        animation_clock: context.animation_clock,
                        session: &*context.state,
                        particle_load_factor,
                        behavior_factory: Some(BehaviorFactoryLoan {
                            listeners: &mut *listeners,
                            sine,
                            world_flags,
                        }),
                        map: context.map.as_deref_mut(),
                    },
                    world_flags,
                    clock: context.clock,
                    sine,
                    math,
                    camera_settings: context.inputs.camera_settings,
                    world_start: &mut *world_start,
                    listener_effects: &mut listener_effects,
                    settings_7c: context.inputs.settings_7c,
                    inverse_sine: crate::session_platform::world_actor_update::inverse_sine_table(
                        executable,
                    )?,
                    voices: Some(&mut context.voices),
                };
                arena
                    .tick_missile_steered(
                        actor,
                        context.microseconds,
                        &controls,
                        sine,
                        &mut *rng,
                        &mut host,
                    )
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = outcome {
                let line =
                    format!("Steered missile {actor:?} stopped after reached writes: {error}");
                eprintln!("{line}");
                crate::crash_log::player_stuck(&line);
                updates.player_vehicle_stop = Some(error);
            }
        }
        PlayerVehicleCall::Update44
            if updates.player_vehicle_stop.is_none() && arena.selector36_actor(actor).is_ok() =>
        {
            // 00481A50, selector 36's +44: the player drives the creature the
            // implant took, through the class's own phases in native order.
            let outcome = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let sine = &context.assets.sine_table().values;
                let controls = arena
                    .world_player(player)
                    .map_err(|error| error.to_string())?
                    .controls;
                let delta = context.microseconds;
                let water_plane = ActorWaterPlane {
                    height_fixed: terrain.plane_base(),
                    coordinate_shift: terrain.plane_shift(),
                };
                macro_rules! family_host {
                    () => {
                        WorldFamilyUpdateHost {
                            owners: WorldDamageOwners {
                                grant_text: context.grant_text,
                                camera_settings: Some(context.inputs.camera_settings),
                                flash: Some(&mut *flash),
                                mission: &mut *mission,
                                tasks: &mut *context.tasks,
                                terrain: &mut *terrain,
                                particles: &mut *context.particles,
                                sounds: &mut *context.sounds,
                                furniture: &mut *context.furniture,
                                residency: context.residency,
                                executable,
                                factory_process: &mut *context.factory_process,
                                behavior_tables: &mut *context.behavior_tables,
                                animation_clock: context.animation_clock,
                                session: &*context.state,
                                particle_load_factor,
                                behavior_factory: Some(BehaviorFactoryLoan {
                                    listeners: &mut *listeners,
                                    sine,
                                    world_flags,
                                }),
                                map: context.map.as_deref_mut(),
                            },
                            rng: &mut *rng,
                            sine_quarter: sine,
                        }
                    };
                }
                // 00481A5D: the water (00429D60).
                arena
                    .tick_actor_water(actor, delta, water_plane, &mut family_host!())
                    .map_err(|error| format!("00429D60: {error}"))?;
                // 00481A76: the block's intents (00428490).
                let intents = arena
                    .family_player_intents(actor, &controls, executable)
                    .map_err(|error| format!("00428490: {error}"))?;
                // 00481A7B..00481AD1: the weapon (0048D030, 0048D0D0).
                arena
                    .tick_family_player_weapon(
                        actor,
                        delta,
                        &controls,
                        &mut crate::object_runtime::ActorWeaponFireContext {
                            tables: context.behavior_tables,
                            rng: &mut *rng,
                            process_velocity: Some(
                                crate::object_runtime::PROCESS_VELOCITY_0055F0F8,
                            ),
                        },
                    )
                    .map_err(|error| format!("00481A7B weapon: {error}"))?;
                // 00481AE0: +118, the motion (00481C10) on the intents.
                let crate::object_runtime::Selector36MotionNextPhase::PersistentAudio481b10 {
                    controls: audio,
                } = {
                    let mut terrain_host = WorldActorTerrainHost {
                        residency: context.residency,
                        source: terrain_source,
                        prepared: &*terrain,
                    };
                    let mut header = crate::world_actor_contact::WorldActorContactHeaderHost {
                        residency: context.residency,
                    };
                    arena
                        .tick_selector36_motion(
                            actor,
                            crate::object_runtime::Selector36MotionRequest {
                                microseconds: delta,
                                controls: intents,
                                executable,
                                environment: crate::object_runtime::ActorContactEnvironment {
                                    animation_clock: context.animation_clock,
                                    plane_policy: &*terrain,
                                    up: context.up,
                                    sine_quarter: sine,
                                },
                                stack_policy: crate::object_runtime::ActorContactStackPolicy::Bf09Class36Windows8PlusStack,
                                math,
                                inverse_sine: context.inverse_sine,
                            },
                            &mut terrain_host,
                            &mut header,
                        )
                        .map_err(|error| format!("00481C10: {error}"))?
                };
                // 00481AE8: the counters (00481180).
                arena
                    .publish_selector36_counters::<String>(actor, context.actor_motion)
                    .map_err(|error| format!("00481180: {error}"))?;
                // 00481AF5: the engine sound (00481B10).
                {
                    let mut camera_context =
                        crate::session_platform::world_actor_factory::WorldCameraFactoryContext::new(
                            crate::session_platform::world_actor_factory::WorldCameraInputs {
                                residency: context.residency,
                                sine_quarter: sine,
                                settings: context.inputs.camera_settings,
                                process: Some(&*context.factory_process),
                                animation_clock: context.clock,
                            },
                        );
                    let mut factory =
                        camera_context.factory(&*terrain, world_flags, &mut *listeners);
                    arena
                        .tick_selector36_persistent_audio(
                            actor,
                            crate::object_runtime::Selector36PersistentAudioRequest {
                                controls: audio,
                            },
                            &mut factory,
                            context.residency,
                            &mut *context.sounds,
                            &mut context.voices,
                        )
                        .map_err(|error| format!("00481B10: {error}"))?;
                }
                // 00481AFD: the status (0047C990).
                arena
                    .tick_actor_status(actor, delta, Some(status_load_factor), &mut family_host!())
                    .map_err(|error| format!("0047C990: {error}"))?;
                Ok(())
            })();
            if let Err(error) = outcome {
                let line = format!("Held creature {actor:?} stopped after reached writes: {error}");
                eprintln!("{line}");
                crate::crash_log::player_stuck(&line);
                updates.player_vehicle_stop = Some(error);
            }
        }
        PlayerVehicleCall::Update44
            if updates.player_vehicle_stop.is_none() && arena.family4236d0_actor(actor).is_ok() =>
        {
            // 00424C40, the +44 of selectors 9, 10 and 21: the player drives
            // the truck, assault craft or creature the implant took. It is
            // 00481A50's water, intents and weapon, then the class's own +118
            // on the intents and the status; no counters and no engine sound.
            let outcome = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let sine = &context.assets.sine_table().values;
                let controls = arena
                    .world_player(player)
                    .map_err(|error| error.to_string())?
                    .controls;
                let delta = context.microseconds;
                let water_plane = ActorWaterPlane {
                    height_fixed: terrain.plane_base(),
                    coordinate_shift: terrain.plane_shift(),
                };
                macro_rules! family_host {
                    () => {
                        WorldFamilyUpdateHost {
                            owners: WorldDamageOwners {
                                grant_text: context.grant_text,
                                camera_settings: Some(context.inputs.camera_settings),
                                flash: Some(&mut *flash),
                                mission: &mut *mission,
                                tasks: &mut *context.tasks,
                                terrain: &mut *terrain,
                                particles: &mut *context.particles,
                                sounds: &mut *context.sounds,
                                furniture: &mut *context.furniture,
                                residency: context.residency,
                                executable,
                                factory_process: &mut *context.factory_process,
                                behavior_tables: &mut *context.behavior_tables,
                                animation_clock: context.animation_clock,
                                session: &*context.state,
                                particle_load_factor,
                                behavior_factory: Some(BehaviorFactoryLoan {
                                    listeners: &mut *listeners,
                                    sine,
                                    world_flags,
                                }),
                                map: context.map.as_deref_mut(),
                            },
                            rng: &mut *rng,
                            sine_quarter: sine,
                        }
                    };
                }
                // 00424C4D: the water (00429D60).
                arena
                    .tick_actor_water(actor, delta, water_plane, &mut family_host!())
                    .map_err(|error| format!("00429D60: {error}"))?;
                // 00424C66: the block's intents (00428490).
                let intents = arena
                    .family_player_intents(actor, &controls, executable)
                    .map_err(|error| format!("00428490: {error}"))?;
                // 00424C6B..00424CC1: the weapon (0048D030, 0048D0D0).
                arena
                    .tick_family_player_weapon(
                        actor,
                        delta,
                        &controls,
                        &mut crate::object_runtime::ActorWeaponFireContext {
                            tables: context.behavior_tables,
                            rng: &mut *rng,
                            process_velocity: Some(
                                crate::object_runtime::PROCESS_VELOCITY_0055F0F8,
                            ),
                        },
                    )
                    .map_err(|error| format!("00424C6B weapon: {error}"))?;
                // 00424CD0: +118 on the intents: selector 9's 00424CF0,
                // selector 10's 00425400 or selector 21's 00405600 (the
                // same with its swings).
                {
                    let mut terrain_host = WorldActorTerrainHost {
                        residency: context.residency,
                        source: terrain_source,
                        prepared: &*terrain,
                    };
                    let mut header = crate::world_actor_contact::WorldActorContactHeaderHost {
                        residency: context.residency,
                    };
                    let environment = crate::object_runtime::ActorContactEnvironment {
                        animation_clock: context.animation_clock,
                        plane_policy: &*terrain,
                        up: context.up,
                        sine_quarter: sine,
                    };
                    if arena.selector10_actor(actor).is_ok() {
                        arena
                            .tick_selector10_motion(
                                actor,
                                crate::object_runtime::Selector10MotionRequest {
                                    microseconds: delta,
                                    controls: intents,
                                    executable,
                                    environment,
                                    random_state: &mut *rng,
                                    residency: context.residency,
                                    sounds: &mut *context.sounds,
                                },
                                &mut terrain_host,
                                &mut header,
                            )
                            .map_err(|error| format!("00425400: {error}"))?;
                        if arena
                            .selector10_actor(actor)
                            .is_ok_and(|state| state.swing.is_some())
                        {
                            arena
                                .tick_selector21_swing(actor, delta, &mut *rng)
                                .map_err(|error| format!("00405600: {error}"))?;
                        }
                    } else {
                        arena
                            .tick_family4236d0_motion(
                                actor,
                                crate::object_runtime::Family4236d0MotionRequest {
                                    microseconds: delta,
                                    controls: intents,
                                    executable,
                                    environment,
                                },
                                &mut terrain_host,
                                &mut header,
                            )
                            .map_err(|error| format!("00424CF0: {error}"))?;
                    }
                }
                // 00424CD9: the status (0047C990).
                arena
                    .tick_actor_status(actor, delta, Some(status_load_factor), &mut family_host!())
                    .map_err(|error| format!("0047C990: {error}"))?;
                Ok(())
            })();
            if let Err(error) = outcome {
                let line = format!("Held creature {actor:?} stopped after reached writes: {error}");
                eprintln!("{line}");
                crate::crash_log::player_stuck(&line);
                updates.player_vehicle_stop = Some(error);
            }
        }
        PlayerVehicleCall::Update44 | PlayerVehicleCall::Update24
            if updates.player_vehicle_stop.is_none() && arena.selector78(actor).is_ok() =>
        {
            // 00407340, the ending director's +44 once the player has taken
            // it, runs its +24 (004072A0), as the player's update does below
            // lifecycle 3.
            if updates
                .selector78_stops
                .iter()
                .any(|(stopped, _)| *stopped == actor)
            {
                return;
            }
            let executable =
                match inf_formats::FrontendExecutable::parse(context.assets.executable_bytes()) {
                    Ok(executable) => executable,
                    Err(error) => {
                        eprintln!("World director source unavailable: {error}");
                        return;
                    }
                };
            let sine = &context.assets.sine_table().values;
            let inputs = crate::object_runtime::ArenaIntroInputs {
                camera_settings: context.inputs.camera_settings,
                settings_7c: context.inputs.settings_7c,
                motion_mode: crate::object_runtime::Selector90MotionMode::FractionalThousandths,
                up: *context.up,
            };
            let (result, phase) = {
                let mut host = crate::session_platform::world_actor_update::WorldDirectorHost {
                    family: WorldFamilyUpdateHost {
                        owners: WorldDamageOwners {
                            grant_text: context.grant_text,
                            camera_settings: Some(context.inputs.camera_settings),
                            flash: Some(&mut *flash),
                            mission: &mut *mission,
                            tasks: &mut *context.tasks,
                            terrain: &mut *terrain,
                            particles: &mut *context.particles,
                            sounds: &mut *context.sounds,
                            furniture: &mut *context.furniture,
                            residency: context.residency,
                            executable,
                            factory_process: &mut *context.factory_process,
                            behavior_tables: &mut *context.behavior_tables,
                            animation_clock: context.animation_clock,
                            session: &*context.state,
                            particle_load_factor,
                            behavior_factory: Some(BehaviorFactoryLoan {
                                listeners: &mut *listeners,
                                sine,
                                world_flags,
                            }),
                            map: context.map.as_deref_mut(),
                        },
                        rng: &mut *rng,
                        sine_quarter: sine,
                    },
                    math,
                    clock: context.clock,
                    world_flags,
                    camera_settings: context.inputs.camera_settings,
                    cheat_flags_84: &mut updates.cheat_flags_84,
                    requested_phase: None,
                    listener_effects: RequiredListenerEffects,
                };
                let result = arena.tick_selector78(actor, context.microseconds, inputs, &mut host);
                (
                    result.map_err(|error| error.to_string()),
                    host.requested_phase,
                )
            };
            if let Some(phase) = phase {
                let _ = context.state.request_phase(phase);
            }
            if let Err(error) = result {
                eprintln!("World director stopped after reached writes: {actor:?} {error}");
                updates.selector78_stops.push((actor, error));
            }
        }
        PlayerVehicleCall::Update44
            if updates.player_vehicle_stop.is_none() && arena.selector12_actor(actor).is_ok() =>
        {
            // 00467DA0, the +44 of selectors 12 and 13: the player walks the
            // scientist the implant took: the water (00429D60), the block's
            // intents over its +290 table (00428490), its own +118 (00467DF0)
            // on them and the status (0047C990); no weapon.
            let outcome = (|| -> Result<(), String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let sine = &context.assets.sine_table().values;
                let controls = arena
                    .world_player(player)
                    .map_err(|error| error.to_string())?
                    .controls;
                let delta = context.microseconds;
                let water_plane = ActorWaterPlane {
                    height_fixed: terrain.plane_base(),
                    coordinate_shift: terrain.plane_shift(),
                };
                macro_rules! family_host {
                    () => {
                        WorldFamilyUpdateHost {
                            owners: WorldDamageOwners {
                                grant_text: context.grant_text,
                                camera_settings: Some(context.inputs.camera_settings),
                                flash: Some(&mut *flash),
                                mission: &mut *mission,
                                tasks: &mut *context.tasks,
                                terrain: &mut *terrain,
                                particles: &mut *context.particles,
                                sounds: &mut *context.sounds,
                                furniture: &mut *context.furniture,
                                residency: context.residency,
                                executable,
                                factory_process: &mut *context.factory_process,
                                behavior_tables: &mut *context.behavior_tables,
                                animation_clock: context.animation_clock,
                                session: &*context.state,
                                particle_load_factor,
                                behavior_factory: Some(BehaviorFactoryLoan {
                                    listeners: &mut *listeners,
                                    sine,
                                    world_flags,
                                }),
                                map: context.map.as_deref_mut(),
                            },
                            rng: &mut *rng,
                            sine_quarter: sine,
                        }
                    };
                }
                arena
                    .tick_actor_water(actor, delta, water_plane, &mut family_host!())
                    .map_err(|error| format!("00429D60: {error}"))?;
                let [turn, forward] = arena
                    .family_player_intents(actor, &controls, executable)
                    .map_err(|error| format!("00428490: {error}"))?;
                let math = inf_formats::native_basis::NativeBasisMath::new(*sine);
                let mut prepared: &crate::terrain_runtime::TerrainRuntime = &*terrain;
                arena
                    .tick_selector12_motion(
                        actor,
                        crate::object_runtime::Selector12MotionRequest {
                            microseconds: delta,
                            turn_control: turn,
                            forward_control: forward,
                            surface_snap_53a128: *context.surface_snap_53a128,
                            math: &math,
                            inverse_sine: context.inverse_sine,
                            sine_quarter: sine,
                            terrain: &mut prepared,
                        },
                    )
                    .map_err(|error| format!("00467DF0: {error}"))?;
                arena
                    .tick_actor_status(actor, delta, Some(status_load_factor), &mut family_host!())
                    .map_err(|error| format!("0047C990: {error}"))?;
                Ok(())
            })();
            if let Err(error) = outcome {
                let line = format!("Held creature {actor:?} stopped after reached writes: {error}");
                eprintln!("{line}");
                crate::crash_log::player_stuck(&line);
                updates.player_vehicle_stop = Some(error);
            }
        }
        PlayerVehicleCall::Update44 if updates.player_vehicle_stop.is_none() => {
            let outcome = (|| -> Result<_, String> {
                let executable =
                    inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                        .map_err(|error| error.to_string())?;
                let null = context
                    .factory_process
                    .process_null_handle
                    .ok_or("00416170 requires the process NULL 0054E894")?;
                let sine = &context.assets.sine_table().values;
                let mut controls = arena
                    .world_player(player)
                    .map_err(|error| error.to_string())?
                    .controls;
                // --replay-controls: a retail recording's held times for this tick.
                if let Some([turn, pitch, thrust, fire, grenade]) =
                    crate::replay_controls::current()
                {
                    controls.turn_14 = turn;
                    controls.pitch_18 = pitch;
                    controls.thrust_1c = thrust;
                    controls.fire_20 = fire;
                    controls.grenade_24 = grenade;
                }
                let globals = context.camera_globals;
                let request = VehicleUpdateRequest {
                    drive: VehicleDriveRequest {
                        delta: context.microseconds,
                        executable,
                        ground: VehicleGround {
                            terrain: &*terrain,
                            world_flags,
                            clock: context.animation_clock.animation_value(),
                            sine_quarter: sine,
                        },
                        process_null: null,
                        position_step: context.inputs.vehicle_position_step,
                        view_zoom_5c0: &globals.view_zoom_5c0,
                        network: crate::object_runtime::vehicle_network(&context.factory_process),
                    },
                    attract: false,
                    current_world,
                    status_load_factor: Some(status_load_factor),
                    command: PlayerCommandContext {
                        process: Some(&*context.factory_process),
                        executable: Some(executable),
                        resident: Some(context.residency),
                    },
                    settings_7c: context.inputs.settings_7c,
                    special_view: context.camera_globals.special_view_active,
                    carry: Some(crate::object_runtime::VehicleCarryContext {
                        process: &*context.factory_process,
                        executable,
                        resident: context.residency,
                        text_data: context.grant_text,
                    }),
                };
                let mut host = crate::session_platform::world_actor_update::WorldVehicleHost {
                    sounds: &mut *context.sounds,
                    residency: context.residency,
                    model_fallback: context.model_fallback,
                    voices: &mut context.voices,
                    particles: &mut *context.particles,
                    executable,
                    factory_process: &*context.factory_process,
                    session: &*context.state,
                    rng: &mut *rng,
                    difficulty_10,
                    mission: &mut *mission,
                    animation: context.animation_clock,
                    grant_text: context.grant_text,
                    behavior_tables: &mut *context.behavior_tables,
                    tasks: &mut *context.tasks,
                    terrain: &*terrain,
                    terrain_source,
                    world_start: &mut *world_start,
                    listeners: &mut *listeners,
                    camera_settings: context.inputs.camera_settings,
                    sine,
                    math,
                    world_flags,
                    model_process: context.model_process,
                    furniture_query: &mut *context.furniture_query,
                    furniture_draw: context.furniture_draw,
                    load_factor: status_load_factor,
                    flash: &mut *flash,
                };
                let outcome = arena.tick_vehicle_update(actor, &mut controls, &request, &mut host);
                // Reached control writes stay even when the update stops.
                arena
                    .set_player_controls(player, controls)
                    .map_err(|error| error.to_string())?;
                let mut report = outcome.map_err(|error| error.to_string())?;
                if let Some(pending) = report.wreck.take() {
                    let water_plane = vehicle_water_plane(terrain);
                    tick_world_vehicle_dying(
                        arena,
                        actor,
                        WorldVehicleDying {
                            owners: WorldDamageOwners {
                                grant_text: context.grant_text,
                                camera_settings: Some(context.inputs.camera_settings),
                                flash: Some(&mut *flash),
                                mission: &mut *mission,
                                tasks: &mut *context.tasks,
                                terrain: &mut *terrain,
                                particles: &mut *context.particles,
                                sounds: &mut *context.sounds,
                                furniture: &mut *context.furniture,
                                residency: context.residency,
                                executable,
                                factory_process: &mut *context.factory_process,
                                behavior_tables: &mut *context.behavior_tables,
                                animation_clock: context.animation_clock,
                                session: &*context.state,
                                particle_load_factor,
                                behavior_factory: Some(BehaviorFactoryLoan {
                                    listeners: &mut *listeners,
                                    sine,
                                    world_flags,
                                }),
                                map: context.map.as_deref_mut(),
                            },
                            voices: &mut context.voices,
                            rng: &mut *rng,
                            sine,
                            water_plane,
                            world_flags,
                            status_load_factor,
                            delta: context.microseconds,
                        },
                        Some(pending),
                    )?;
                }
                Ok(report)
            })();
            match outcome {
                Ok(report) => {
                    // 00417922..00417932: infrared vision's request.
                    if report.toggles_infrared {
                        let globals = context.camera_globals;
                        globals.infrared_5c4.set(!globals.infrared_5c3);
                    }
                    if report.steps_lamps {
                        if let Err(error) = arena.step_vehicle_lamps(actor, context.camera_globals)
                        {
                            eprintln!("The lamps' step skipped: {error}");
                        }
                    }
                }
                Err(error) => {
                    let line =
                        format!("Vehicle {actor:?} update stopped after reached writes: {error}");
                    eprintln!("{line}");
                    crate::crash_log::player_stuck(&line);
                    updates.player_vehicle_stop = Some(error);
                }
            }
        }
        PlayerVehicleCall::Update24 if updates.player_vehicle_stop.is_none() => {
            // 00416090, the vehicle's +24 below lifecycle 3.
            let water_plane = vehicle_water_plane(terrain);
            let outcome = inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
                .map_err(|error| error.to_string())
                .and_then(|executable| {
                    tick_world_vehicle_dying(
                        arena,
                        actor,
                        WorldVehicleDying {
                            owners: WorldDamageOwners {
                                grant_text: context.grant_text,
                                camera_settings: Some(context.inputs.camera_settings),
                                flash: Some(&mut *flash),
                                mission: &mut *mission,
                                tasks: &mut *context.tasks,
                                terrain: &mut *terrain,
                                particles: &mut *context.particles,
                                sounds: &mut *context.sounds,
                                furniture: &mut *context.furniture,
                                residency: context.residency,
                                executable,
                                factory_process: &mut *context.factory_process,
                                behavior_tables: &mut *context.behavior_tables,
                                animation_clock: context.animation_clock,
                                session: &*context.state,
                                particle_load_factor,
                                behavior_factory: Some(BehaviorFactoryLoan {
                                    listeners: &mut *listeners,
                                    sine: &context.assets.sine_table().values,
                                    world_flags,
                                }),
                                map: context.map.as_deref_mut(),
                            },
                            voices: &mut context.voices,
                            rng: &mut *rng,
                            sine: &context.assets.sine_table().values,
                            water_plane,
                            world_flags,
                            status_load_factor,
                            delta: context.microseconds,
                        },
                        None,
                    )
                });
            if let Err(error) = outcome {
                let line =
                    format!("Vehicle {actor:?} dying update stopped after reached writes: {error}");
                eprintln!("{line}");
                crate::crash_log::player_stuck(&line);
                updates.player_vehicle_stop = Some(error);
            }
        }
        _ => {}
    }
    if let Err(error) = arena.finish_player_update(player, context.microseconds as i32) {
        let line =
            format!("World player update stopped after its vehicle call: {player:?} {error}");
        eprintln!("{line}");
        crate::crash_log::player_stuck(&line);
        updates.player_update_stop = Some(error.to_string());
    }
    // 00413870 ends 0046A260: a faded controller's menus are destroyed
    // there, and a shop menu undocks its factory.
    let owners = ShopWorldOwners {
        mission,
        terrain: &*terrain,
        world_start,
        listeners,
        world_flags,
        rng,
    };
    if let Some(services) = shop_services(context, owners) {
        crate::session_platform::world_actor_update::drain_shop_menus(
            arena,
            &mut updates.shop_stop,
            services,
        );
    }
}

struct ProductionIntroServices<'a> {
    retirement: ProductionRetirementHost<'a>,
    state: &'a mut SessionState,
    math: &'a NativeBasisMath,
    sine: &'a [u16; 4096],
    clock: u32,
    world_flags: u32,
    camera_settings: crate::object_runtime::CameraSettings,
    listeners: &'a mut CreationListeners,
    listener_effects: &'a mut dyn CreationListenerEffects,
}
impl ProductionIntroServices<'_> {
    fn admission(&mut self) -> IntroAdmissionInputs<'_> {
        let allocation = fresh_world_allocation(
            self.retirement.owners.terrain,
            self.clock,
            self.world_flags,
            self.sine,
            crate::object_runtime::ActorConstructorNull::Process(
                self.retirement.owners.factory_process,
            ),
        );
        IntroAdmissionInputs {
            process: Some(self.retirement.owners.factory_process),
            runtime: self.retirement.owners.residency,
            math: self.math,
            helper_allocation: allocation,
            camera_allocation: allocation,
            local_owner: self.retirement.owners.factory_process.local_slot,
            camera_settings: self.camera_settings,
            network_active: self.retirement.owners.factory_process.network_flag != 0,
            network_world_active: self.retirement.owners.factory_process.network_flag != 0,
            listeners: &mut *self.listeners,
            listener_effects: &mut *self.listener_effects,
        }
    }
}
impl crate::object_runtime::TargetNullHandleSource for ProductionIntroServices<'_> {
    fn target_null_handle(&self) -> Option<crate::object_runtime::ActorHandle> {
        // Read the actual process owner only when a reached constructor asks;
        // this loan remains independent from the arena's registry sentinel.
        self.retirement.owners.factory_process.process_null_handle
    }
}
impl ArenaIntroServices for ProductionIntroServices<'_> {
    type Error = String;
    fn create_helper(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, String> {
        self.admission()
            .create_helper(arena)
            .map_err(|error| error.to_string())
    }
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, String> {
        self.admission()
            .acquire_camera(arena)
            .map_err(|error| error.to_string())
    }
    fn retire_actor(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        arena
            .retire_actor(
                actor,
                self.retirement.owners.factory_process.process_null_handle,
                &mut self.retirement,
            )
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }
    fn world_present(&mut self) -> Result<bool, String> {
        Ok(true)
    }
    fn complete_mission(
        &mut self,
        arena: &mut ObjectArena,
        selection: u32,
        start_ordinal: u16,
        argument: u32,
    ) -> Result<(), String> {
        if argument != 0 {
            return Err("unowned nonnull intro completion actor".into());
        }
        let outcome = complete_world(
            self.retirement.owners.mission,
            arena,
            WorldCompletionRequest {
                next_selection: selection,
                start_ordinal,
                actor: None,
            },
            WorldCompletionInputs {
                network_active: self.retirement.owners.factory_process.network_flag != 0,
                process: Some(self.retirement.owners.factory_process),
            },
            &mut *self.retirement.owners.sounds,
        )
        .map_err(|error| error.to_string())?;
        if let WorldCompletionOutcome::Accepted { request_phase } = outcome {
            let _ = self.state.request_phase(request_phase);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
