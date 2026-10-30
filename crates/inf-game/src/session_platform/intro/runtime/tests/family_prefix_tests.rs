use super::*;
use crate::object_runtime::{
    ActorCommandContext, ActorConstructionRequest, ActorTerrainInputs,
    DerivedActorConstructionRequest, FamilyUpdateClass, FamilyUpdateError, NativeFamilyUpdateError,
    ObjectList,
};
use inf_formats::{
    ByteSpan, InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView, InstanceField,
};

mod brain;
mod creature_pacing;
mod director;
mod ending;
mod selector12_dying;
mod selector35_tail;
mod selector80;
mod selector86;
mod simple_updates;
mod tank_treads;
mod tl717;

/// Supplied classification bytes for the grant messages the updates deliver
/// (the brain's Invulnerability): plain spaces, colons and hyphens.
const PICKUP_TEXT: crate::grant_text::PickupTextData<'static> = crate::grant_text::PickupTextData {
    common: crate::grant_text::GrantTextData {
        null_substitute: None,
        pair_classification: Some(crate::grant_text::GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    },
    hyphen_high: Some(0),
};

/// Real session owners retained across visits. Test scheduling below supplies
/// callback membership/stage deliberately; it does not claim retail timing.
pub(super) struct TickOwners {
    factory_process: ParticleFactoryProcessState,
    behavior_tables: crate::object_runtime::ProcessActorBehaviorTables,
    selector35_process: crate::object_runtime::Selector35RouteProcessState,
    actor_motion: crate::object_runtime::ProcessActorMotionState,
    actor_draw: crate::object_runtime::ObjectDrawProcessState,
    model_process: crate::model_process::ModelProcessOwners,
    clock: crate::frontend_camera::AnimationClock,
    globals: CameraGlobalState,
    inverse_sine: [i16; 1024],
    surface_snap_53a128: u8,
    pub(super) state: SessionState,
    sounds: crate::process_sound::ProcessSoundRuntime,
    tasks: ProcessTaskRuntime,
    particles: ProcessParticleRuntime,
    furniture: FurnitureEffectQueue,
    furniture_query: crate::world_furniture_effect::ProcessFurnitureQueryState,
    furniture_radius: crate::terrain_runtime::ProcessFurnitureRadius,
    furniture_draw: crate::terrain_furniture::draw::FurnitureDrawState,
}

impl TickOwners {
    pub(super) fn new(
        assets: &FrontendAssets,
        world: &mut WorldSession,
        resident: &ResidentRuntime,
    ) -> Self {
        let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let inverse = inf_formats::parse_frontend_inverse_sine_table(
            executable.data(),
            inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
        )
        .unwrap();
        //Declared fixture startup/world-load phase: genuine4508E0 publishes
        //the new fixture owner's radius from this actual mutable H12/H11/H8.
        //It supplies no sampled radius/default or native scheduling claim.
        let mut furniture_radius =
            crate::terrain_runtime::ProcessFurnitureRadius::native_startup(executable).unwrap();
        crate::terrain_runtime::produce_resident_furniture_radius(
            crate::terrain_runtime::TerrainFurnitureRadiusRequest::new(
                &mut world.terrain,
                furniture_radius.word_mut(),
            ),
            resident,
        )
        .unwrap();
        let furniture_draw = crate::terrain_furniture::draw::FurnitureDrawState::new(
            &NativeBasisMath::new(assets.sine_table().values),
        );
        let mut tasks = ProcessTaskRuntime::default();
        tasks
            .retain(world.arena.actor_handles().unwrap().null_handle())
            .unwrap();
        Self {
            selector35_process: crate::object_runtime::Selector35RouteProcessState::default(),
            factory_process: ParticleFactoryProcessState::native_startup(),
            behavior_tables: crate::object_runtime::ProcessActorBehaviorTables::from_executable(
                executable,
            )
            .unwrap(),
            model_process: crate::model_process::ModelProcessOwners::native_startup(executable)
                .unwrap(),
            actor_draw: crate::object_runtime::ObjectDrawProcessState::native_startup(executable)
                .unwrap(),
            actor_motion: crate::object_runtime::ProcessActorMotionState::from_executable(
                executable,
            )
            .unwrap(),
            clock: Default::default(),
            globals: CameraGlobalState::INITIAL,
            inverse_sine: inverse.values,
            surface_snap_53a128: authored_surface_snap(assets),
            state: SessionState::default(),
            sounds: crate::process_sound::ProcessSoundRuntime::new(executable).unwrap(),
            tasks,
            particles: ProcessParticleRuntime::default(),
            furniture: FurnitureEffectQueue::default(),
            furniture_radius,
            furniture_draw,
            furniture_query:
                crate::world_furniture_effect::ProcessFurnitureQueryState::native_startup(
                    executable,
                )
                .unwrap(),
        }
    }

    pub(super) fn tick(
        &mut self,
        world: &mut WorldSession,
        runtime: &ResidentRuntime,
        assets: &FrontendAssets,
        microseconds: u32,
    ) {
        self.tick_paced(world, runtime, assets, microseconds, Some(microseconds));
    }

    /// [`Self::tick`] with the paced classes' own delta
    /// (`WorldTickCarry::paced_delta`): `None` holds their update.
    pub(super) fn tick_paced(
        &mut self,
        world: &mut WorldSession,
        runtime: &ResidentRuntime,
        assets: &FrontendAssets,
        microseconds: u32,
        paced_microseconds: Option<u32>,
    ) {
        self.clock.advance(microseconds);
        tick_world_actor_callbacks(
            world,
            WorldActorTickContext {
                attract: false,
                map: None,
                network_key_countdown: &mut 0,
                network_team_flag: &mut 0,
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                camera_globals: &self.globals,
                grant_text: None,
                pickup_text: Some(PICKUP_TEXT),
                input: &mut crate::input::InputRuntime::default(),
                voices: crate::process_sound::ProcessSoundTeardownLoan::RequiredAtReach,
                residency: runtime,
                model_fallback: None,
                assets,
                clock: self.clock.value(),
                animation_clock: &self.clock,
                inputs: WorldTickPlatformInputs {
                    settings_20: 0,
                    settings_70: 0,
                    camera_settings: crate::object_runtime::CameraSettings {
                        first_person: 0,
                        camera_style: 0,
                    },
                    settings_78: 0,
                    settings_7c: 0,
                    input_mode_08: 0,
                    vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
                    settings_84: 0,
                },
                microseconds,
                paced_microseconds,
                mission_completion: false,
                inverse_sine: &self.inverse_sine,
                up: &[0.0, 1.0, 0.0],
                surface_snap_53a128: &self.surface_snap_53a128,
                factory_process: &mut self.factory_process,
                behavior_tables: &mut self.behavior_tables,
                selector35_process: &mut self.selector35_process,
                actor_motion: &self.actor_motion,
                model_process: &self.model_process,
                actor_shared_basis: &self.actor_draw.shared_basis,
                state: &mut self.state,
                sounds: &mut self.sounds,
                tasks: &mut self.tasks,
                particles: &mut self.particles,
                furniture: &mut self.furniture,
                furniture_query: &mut self.furniture_query,
                furniture_radius: &self.furniture_radius,
                furniture_draw: &self.furniture_draw,
            },
        );
        tick_world_camera(
            world,
            WorldCameraTickContext {
                residency: runtime,
                assets,
                clock: self.clock.value(),
                inputs: WorldTickPlatformInputs {
                    settings_20: 0,
                    settings_70: 0,
                    camera_settings: crate::object_runtime::CameraSettings {
                        first_person: 0,
                        camera_style: 0,
                    },
                    settings_78: 0,
                    settings_7c: 0,
                    input_mode_08: 0,
                    vehicle_position_step: crate::object_runtime::VehiclePositionStep::Retail,
                    settings_84: 0,
                },
                microseconds,
                process: &self.factory_process,
                globals: &mut self.globals,
                inverse_sine: &self.inverse_sine,
                up: [0.0, 1.0, 0.0],
                shake_enabled: 0,
            },
        );
    }
}

pub(super) fn only_callbacks(world: &mut WorldSession, actors: &[ObjectId]) {
    let previous: Vec<_> = world
        .arena
        .list(ObjectList::Flag1Primary)
        .iter()
        .map(|node| node.object)
        .collect();
    for actor in previous {
        world.arena.set_flag1(actor, false).unwrap();
    }
    for &actor in actors {
        world.arena.set_flag1(actor, true).unwrap();
    }
}

fn callback_order(world: &WorldSession) -> Vec<ObjectId> {
    assert!(world.arena.list(ObjectList::Flag1Secondary).is_empty());
    world
        .arena
        .list(ObjectList::Flag1Primary)
        .iter()
        .map(|node| node.object)
        .collect()
}

fn pause_controller(world: &mut WorldSession) {
    world.intro_runtime.as_mut().unwrap().controller_failure =
        Some("supplied test schedule stops unrelated controller".into());
}

/// Obtain a real phase4 SessionState through the session owner. Only these
/// lower screen/timer responses are supplied; world loading already happened
/// in loaded(), and every actor, particle and disposal service is real.
/// Particle allocation admits pool slots only in phase 4 (or networked); a
/// default Title state constructs births into the scratch record instead.
pub(super) fn gameplay_state() -> SessionState {
    use crate::session::{
        ScreenFactory, ScreenKind, SessionController, SessionHost, SessionPhase, TickCallback,
        TransitionEffect,
    };
    struct SuppliedScreen;
    impl SessionHost for SuppliedScreen {
        type Screen = ();
        type ScreenContext = ();
        type Error = std::convert::Infallible;
        fn current_screen(&self) -> Option<()> {
            Some(())
        }
        fn screen_kind(&mut self, _: (), _: &mut SessionState) -> Result<ScreenKind, Self::Error> {
            Ok(ScreenKind::Title)
        }
        fn screen_context(&self, _: ()) {}
        fn reuse_screen(
            &mut self,
            _: (),
            phase: SessionPhase,
            _: &mut SessionState,
        ) -> Result<bool, Self::Error> {
            assert_eq!(phase, SessionPhase::Gameplay);
            Ok(true)
        }
        fn destroy_screen(&mut self, _: (), _: &mut SessionState) -> Result<(), Self::Error> {
            panic!("fixture screen was retained")
        }
        fn network_session_active(&self) -> bool {
            false
        }
        fn transition_effect(
            &mut self,
            effect: TransitionEffect,
            _: &mut SessionState,
        ) -> Result<(), Self::Error> {
            assert!(matches!(
                effect,
                TransitionEffect::SetPlayerTimer {
                    microseconds: 500_000
                } | TransitionEffect::ResetInputBaselines
            ));
            Ok(())
        }
        fn create_screen(
            &mut self,
            _: ScreenFactory,
            _: (),
            _: &mut SessionState,
        ) -> Result<(), Self::Error> {
            panic!("fixture screen was retained")
        }
        fn tick(
            &mut self,
            _: TickCallback,
            _: i32,
            _: i32,
            _: &mut SessionState,
        ) -> Result<(), Self::Error> {
            panic!("fixture supplies initial state only")
        }
    }
    let mut controller = SessionController::default();
    assert!(controller.request_phase(SessionPhase::Gameplay));
    controller.transition(&mut SuppliedScreen).unwrap();
    assert_eq!(controller.state().current_phase(), SessionPhase::Gameplay);
    controller.state().clone()
}

fn set_delay(world: &mut WorldSession, runtime: &ResidentRuntime, actor: ObjectId, delay: i32) {
    // Existing 00426030 command18 owns the write and flag side effects. A
    // supplied command packet avoids exposing mutation-only production APIs.
    let command = InstanceCommandView {
        target: InstanceCommandTarget::DerivedActor,
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: 18,
        },
        proven_method: Some(0x0042_6030),
        payload_consumer: Some(0x0042_6030),
        payload: InstanceCommandPayload::ActorNegatedDword144(InstanceField {
            span: ByteSpan { start: 4, end: 8 },
            value: delay.wrapping_neg() as u32,
        }),
    };
    assert!(world
        .arena
        .apply_derived_actor_command(
            actor,
            &command,
            ActorCommandContext {
                runtime,
                first_player_grants: None,
                terrain: None,
            }
        )
        .unwrap());
}

fn exit_actors(world: &mut WorldSession) -> Vec<ObjectId> {
    let controller = world.intro_runtime.as_ref().unwrap().controller;
    let script = world.arena.selector88_intro_script(controller).unwrap();
    (0..script
        .reference_count(crate::intro_sequence::IntroActorList::Exit2c)
        .unwrap() as u32)
        .map(|index| {
            let raw = script
                .reference_handle(crate::intro_sequence::IntroActorList::Exit2c, index)
                .unwrap();
            world
                .arena
                .resolve_actor_handle(crate::object_runtime::ActorHandle::from_raw(raw))
                .unwrap()
                .unwrap()
        })
        .collect()
}

fn constructed_selector35(
    world: &mut WorldSession,
    assets: &FrontendAssets,
    position: Option<[u16; 3]>,
) -> ObjectId {
    world
        .arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: position,
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: world.terrain.plane_shift(),
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &assets.sine_table().values,
                    },
                },
                world_mask_limits: [0, 0],
            },
            &mut world.rng,
        )
        .unwrap()
}

/// Native 00429D60 expectation over actual loaded signed WORD-Y, H12 plane
/// and unsigned extent. This two-visit schedule remains below the damage gate.
fn selected_water_next(world: &WorldSession, actor: ObjectId, microseconds: u32) -> u16 {
    let base = &world.arena.object(actor).unwrap().base;
    let position = base.position_words.expect("actual loaded actor XYZ");
    let plane_y = (world.terrain.plane_base() >> (world.terrain.plane_shift() & 31)) as i16;
    let separation = i32::from(position[1] as i16) - i32::from(plane_y);
    let previous = world
        .arena
        .family429e20_actor(actor)
        .unwrap()
        .word_230
        .expect("selected production carry supplies WORD230");
    assert!(
        previous <= 4000,
        "this schedule does not supply a drowning service"
    );
    if separation < 0 && separation < -(i32::from(base.word_28) / 2) {
        previous.wrapping_add((microseconds >> 10) as u16)
    } else if previous != 0 {
        previous.wrapping_sub((microseconds >> 9) as u16)
    } else {
        0
    }
}

/// Clone only the selected expected object. Actual arena storage is never
/// rewritten; the negative-delay water callback can change only its timer.
fn expected_water_object(
    before: &crate::object_runtime::ObjectState,
    timer: u16,
) -> crate::object_runtime::ObjectState {
    use crate::object_runtime::{
        ActorExtension, DerivedActorClass, Family428650Class, ObjectClass,
    };
    let mut expected = before.clone();
    let ObjectClass::Actor(actor) = &mut expected.class else {
        panic!("actual actor")
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("actual derived actor")
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("actual family")
    };
    let Family428650Class::Family429e20(water) = &mut family.class else {
        panic!("actual water family")
    };
    water.word_230 = Some(timer);
    expected
}

#[test]
fn production_family_prefix_preserves_class_specific_stops_and_the_live_selector12() {
    let (runtime, assets, mut world, _maps) = loaded();
    pause_controller(&mut world);
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let matrix = [
        (FamilyUpdateClass::Selector10, 0x0052_0cc8, 0x0042_4830),
        (FamilyUpdateClass::Selector12, 0x0052_7058, 0x0046_79f0),
        (FamilyUpdateClass::Selector36, 0x0052_7ee0, 0x0048_1710),
    ];
    let mut actors = Vec::new();
    let mut water_expected = Vec::new();
    for (class, vtable, method) in matrix {
        assert_eq!(executable.word(vtable + 0x24).unwrap(), method);
        let actor = world
            .arena
            .actor_list()
            .iter()
            .copied()
            .find(|&actor| world.arena.family_update_class(actor) == Ok(class))
            .unwrap();
        assert_eq!(
            world.arena.family429e20_actor(actor).unwrap().word_230,
            Some(0)
        );
        assert_eq!(
            world.arena.family428650_actor(actor).unwrap().word_220,
            Some(0)
        );
        // Real command18 supplies only this callback schedule. It does not
        // choose class policy or rewrite actual allocation carries.
        set_delay(&mut world, &runtime, actor, 0);
        let status = world.arena.actor(actor).unwrap();
        assert_eq!(
            (status.word_cc, status.word_ce),
            (0, 0),
            "actual status has no required trail/damage effect"
        );
        water_expected.push(selected_water_next(&world, actor, 40_000));
        actors.push(actor);
    }
    only_callbacks(&mut world, &actors);
    let live = actors[1];
    assert_eq!(
        world.arena.family_update_class(live),
        Ok(FamilyUpdateClass::Selector12)
    );
    let cache_before = world
        .arena
        .family429e20_actor(live)
        .unwrap()
        .probe_234
        .as_ref()
        .expect("actual loaded +234 header")
        .decoded_cache()
        .clone();
    let cache_address = std::ptr::from_ref(
        &**world
            .arena
            .family429e20_actor(live)
            .unwrap()
            .probe_234
            .as_ref()
            .unwrap(),
    );
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.tick(&mut world, &runtime, &assets, 40_000);
    // 00478F20 leaves the list reversed; the second visit restores it.
    let reversed: Vec<_> = actors.iter().rev().copied().collect();
    assert_eq!(callback_order(&world), reversed);
    assert!(world.actor_updates.skipped_callbacks.is_empty());
    let stops = &world.actor_updates.family_prefix_stops;
    assert_eq!(
        stops.iter().map(|stop| stop.actor).collect::<Vec<_>>(),
        [actors[0]]
    );
    for (index, &actor) in actors.iter().enumerate() {
        let reached = stops
            .iter()
            .filter(|stop| stop.actor == actor)
            .collect::<Vec<_>>();
        match matrix[index].0 {
            FamilyUpdateClass::Selector12 => {
                assert!(
                    reached.is_empty(),
                    "the real miss/avoidance/motion returned normally"
                );
                let cache = world
                    .arena
                    .family429e20_actor(actor)
                    .unwrap()
                    .probe_234
                    .as_ref()
                    .unwrap();
                assert_eq!(
                    std::ptr::from_ref(&**cache),
                    cache_address,
                    "normal completion retains the actual installed +234 header"
                );
                assert_ne!(cache.decoded_cache(), &cache_before);
                assert!(
                    cache.decoded_cache().words().iter().any(|word| *word != 0),
                    "normal callbacks completed real packed samples, without probing again"
                );
                assert_eq!(
                    world.arena.family428650_actor(actor).unwrap().word_22e & 0x8000,
                    0,
                    "429EE0 restored XYZ and cleared its surviving guard before the live return"
                );
                assert!(world
                    .arena
                    .object(actor)
                    .unwrap()
                    .base
                    .position_words
                    .is_some());
            }
            FamilyUpdateClass::Selector10 => {
                assert_eq!(reached.len(), 1);
                assert_eq!(reached[0].class, FamilyUpdateClass::Selector10);
                use crate::object_runtime::{
                    FamilyAvoidanceError, FamilyObstacleScanError, FamilyTerrainProbeMissError,
                    FurnitureCollisionPrefixError, Selector10TailPrefixError,
                    Selector12TailProbeError,
                };
                // This fixture has not run the renderer's projection selection.
                // The new tail reaches its real first avoidance/collision query.
                let FamilyPrefixStopReason::Selector10TailFailed(
                    Selector10TailPrefixError::Avoidance(FamilyAvoidanceError::Host {
                        error:
                            FamilyObstacleScanError::Host {
                                ordinal: 1,
                                error:
                                    Selector12TailProbeError::Miss {
                                        error: FamilyTerrainProbeMissError::Host { error, .. },
                                        ..
                                    },
                            },
                        ..
                    }),
                ) = &reached[0].reason
                else {
                    panic!(
                        "expected the actual class10 collision prefix: {:#?}",
                        reached[0]
                    );
                };
                let crate::world_actor_collision::WorldActorCollisionError::Collision(
                    FurnitureCollisionPrefixError::Host {
                        error:
                            crate::world_actor_collision::ResidentFurnitureCollisionError::Auxiliary(
                                crate::model_auxiliary::AuxiliaryModelError::VertexExecution(
                                    crate::model_vertices::ModelVertexError::Projection(error),
                                ),
                            ),
                        ..
                    },
                ) = error
                else {
                    panic!("expected the actual unselected projection callback: {error:#?}");
                };
                assert!(matches!(
                    error,
                    crate::process_model_graphics::ProcessModelGraphicsError::NativeNullProjection {
                        table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
                        slot: crate::process_model_graphics::SelectedProjectionSlot::Local1c,
                    }
                ));
            }
            FamilyUpdateClass::Selector36 => {
                assert!(reached.is_empty(), "supplied class36 matrix must complete audio or retain its actual typed boundary: {reached:?}");
                assert!(
                    world.arena.derived_actor(actor).unwrap().word_138.is_some(),
                    "normalMotion includes real contact scalar before478E00/audio"
                );
            }
            class => panic!("unexpected supplied matrix class {class:?}"),
        }
        assert_eq!(world.arena.derived_actor(actor).unwrap().word_144, 0);
        assert_eq!(
            world.arena.family429e20_actor(actor).unwrap().word_230,
            Some(water_expected[index])
        );
        assert_eq!(
            world.arena.family428650_actor(actor).unwrap().word_220,
            Some(0)
        );
    }
    let stopped_before =
        [actors[0]].map(|actor| (actor, world.arena.object(actor).unwrap().clone()));
    let soldier_before = world.arena.object(actors[2]).unwrap().clone();
    let stops_before = format!("{:?}", world.actor_updates.family_prefix_stops);
    let live_before = world.arena.object(live).unwrap().clone();
    let live_family = world.arena.family428650_actor(live).unwrap();
    let elapsed_before = live_family.words_188_19c[0].expect("normal D8 published elapsed");
    //This supplied second visit stays in genuine retained timer branches,
    //which bypass periodic/descriptor-selection RNG. No timer is rewritten.
    assert!(live_family.word_21c > 40_000);
    assert!(live_family.words_188_19c[5].expect("entered descriptor duration") > 40_000);
    assert_eq!(live_family.word_22e & 0x0d, 0);
    let live_water_expected = selected_water_next(&world, live, 40_000);
    let before_rng = world.rng;
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(world.rng, before_rng);
    assert_eq!(callback_order(&world), actors);
    assert!(world.actor_updates.skipped_callbacks.is_empty());
    assert_eq!(
        format!("{:?}", world.actor_updates.family_prefix_stops),
        stops_before,
        "stopped10 collision prefix is not replayed; normal12/36 callbacks remain eligible"
    );
    for (actor, before) in stopped_before {
        assert_eq!(world.arena.object(actor).unwrap(), &before);
    }
    assert_ne!(
        world.arena.object(actors[2]).unwrap(),
        &soldier_before,
        "the normal class36 movement/audio callback remains eligible on its next list visit"
    );
    assert_ne!(
        world.arena.object(live).unwrap(),
        &live_before,
        "the normal class12 callback remains eligible for the second native list pass"
    );
    assert_eq!(
        world.arena.family428650_actor(live).unwrap().words_188_19c[0],
        Some(elapsed_before.wrapping_add(40_000))
    );
    assert_eq!(
        world.arena.family429e20_actor(live).unwrap().word_230,
        Some(live_water_expected)
    );
    assert_eq!(
        world.arena.family428650_actor(live).unwrap().word_22e & 0x8000,
        0
    );
    assert_eq!(
        std::ptr::from_ref(
            &**world
                .arena
                .family429e20_actor(live)
                .unwrap()
                .probe_234
                .as_ref()
                .unwrap()
        ),
        cache_address
    );
}

#[test]
fn production_family_delayed_returns_repeat_expire_then_dispatch_on_the_carried_timer() {
    let (runtime, assets, mut world, _maps) = loaded();
    pause_controller(&mut world);
    let actor = exit_actors(&mut world)[0];
    assert_eq!(
        world.arena.family_update_class(actor),
        Ok(FamilyUpdateClass::Selector35)
    );
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    assert_eq!(executable.word(0x0052_273c + 0x24).unwrap(), 0x0043_e6d0);
    set_delay(&mut world, &runtime, actor, 120_000);
    world
        .arena
        .add_actor_linear_velocity(actor, [1000, 500, 100])
        .unwrap();
    world
        .arena
        .add_actor_angular_velocity(actor, [1000, 2000, 3000])
        .unwrap();
    only_callbacks(&mut world, &[actor]);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    let initial_basis = world.arena.actor(actor).unwrap().rotating.basis;
    for remaining in [80_000, 40_000, 0] {
        let before = world.arena.object(actor).unwrap().base.position_words;
        owners.tick(&mut world, &runtime, &assets, 40_000);
        assert_eq!(
            world.arena.derived_actor(actor).unwrap().word_144,
            remaining
        );
        assert_ne!(
            world.arena.object(actor).unwrap().base.position_words,
            before
        );
        assert!(world.actor_updates.family_prefix_stops.is_empty());
        assert_eq!(callback_order(&world), [actor]);
    }
    assert_ne!(
        world.arena.actor(actor).unwrap().rotating.basis,
        initial_basis
    );
    assert_eq!(
        world.arena.family428650_actor(actor).unwrap().word_220,
        Some(0)
    );
    owners.tick(&mut world, &runtime, &assets, 40_000);
    // The labeled zero +19C carry admits selection on the first post-delay
    // visit: this authored ship enters Roam and its whole update completes.
    assert!(world.actor_updates.family_prefix_stops.is_empty());
    let family = world.arena.family428650_actor(actor).unwrap();
    assert_eq!(family.word_184, 4);
    assert_eq!(family.words_188_19c[0], Some(40_000));
    assert_ne!(
        family.words_188_19c[5],
        Some(0),
        "entered duration replaced the carry"
    );
    assert_eq!(callback_order(&world), [actor]);
}

#[test]
fn production_family_behavior_custody_and_alternate_callback_remain_distinct() {
    let (runtime, assets, mut world, _maps) = loaded();
    pause_controller(&mut world);
    let actor = constructed_selector35(&mut world, &assets, Some([1, 2, 3]));
    let alternate = exit_actors(&mut world)[0];
    set_delay(&mut world, &runtime, alternate, 120_000);
    let flags = world.arena.object(alternate).unwrap().base.flags | 0x8000_0000;
    world.arena.apply_payload_flags(alternate, flags).unwrap();
    only_callbacks(&mut world, &[alternate, actor]);
    let alternate_before = world.arena.object(alternate).unwrap().clone();
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    for visit in 0..2 {
        owners.tick(&mut world, &runtime, &assets, 40_000);
        let order = if visit == 0 {
            [actor, alternate]
        } else {
            [alternate, actor]
        };
        assert_eq!(callback_order(&world), order);
        assert_eq!(world.arena.object(alternate).unwrap(), &alternate_before);
        let attachment = &world.actor_updates;
        assert_eq!(attachment.family_prefix_stops.len(), 1);
        assert_eq!(attachment.family_prefix_stops[0].actor, actor);
        // The carried zero +19C reaches scoring; the payload-less fixture
        // then lacks the retained +23C that candidate bias +110 rereads.
        assert!(matches!(
            attachment.family_prefix_stops[0].reason,
            FamilyPrefixStopReason::BehaviorFailed(
                crate::object_runtime::FamilyBehaviorError::Derived(
                    crate::object_runtime::DerivedActorError::MissingSelector35Payload
                )
            )
        ));
        assert_eq!(attachment.skipped_callbacks, [(alternate, true)]);
        assert_eq!(
            world.arena.family428650_actor(actor).unwrap().word_220,
            Some(1)
        );
    }
}

#[test]
fn production_family_physics_error_preserves_partial_writes_without_replaying_them() {
    let (runtime, assets, mut world, _maps) = loaded();
    pause_controller(&mut world);
    let actor = constructed_selector35(&mut world, &assets, None);
    only_callbacks(&mut world, &[actor]);
    set_delay(&mut world, &runtime, actor, -1);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    let before = world.arena.object(actor).unwrap().clone();
    owners.tick(&mut world, &runtime, &assets, 1024);
    assert_eq!(world.arena.object(actor).unwrap(), &before);
    assert!(world.actor_updates.family_prefix_stops.is_empty());

    set_delay(&mut world, &runtime, actor, 1024);
    world
        .arena
        .add_actor_linear_velocity(actor, [3, 4, 0])
        .unwrap();
    world
        .arena
        .add_actor_angular_velocity(actor, [3, 4, 0])
        .unwrap();
    owners.tick(&mut world, &runtime, &assets, 1024);
    assert_eq!(world.arena.derived_actor(actor).unwrap().word_144, 0);
    assert_eq!(world.arena.actor(actor).unwrap().words_8e_92, [2, 2, 0]);
    assert_eq!(world.arena.actor(actor).unwrap().words_94_9c, [1, 2, 0]);
    assert_eq!(world.arena.object(actor).unwrap().base.position_words, None);
    let stops = &world.actor_updates.family_prefix_stops;
    assert_eq!(stops.len(), 1);
    assert!(
        matches!(&stops[0].reason, FamilyPrefixStopReason::Failed(FamilyUpdateError::Host {
        effect: crate::object_runtime::FamilyUpdateEffect::DelayedPhysics { microseconds: 1024, mask: 0x1f },
        error: NativeFamilyUpdateError::Physics(crate::object_runtime::ActorMotionError::MissingPosition(stopped)),
    }) if *stopped == actor)
    );
    let before = world.arena.object(actor).unwrap().clone();
    owners.tick(&mut world, &runtime, &assets, 1024);
    assert_eq!(world.arena.object(actor).unwrap(), &before);
    assert_eq!(world.actor_updates.family_prefix_stops.len(), 1);
    assert_eq!(callback_order(&world), [actor]);
}

#[test]
fn production_intro_stage9_admits_exit_prefixes_to_the_same_native_list_pass() {
    let (runtime, assets, mut world, _maps) = loaded();
    let controller = world.intro_runtime.as_ref().unwrap().controller;
    only_callbacks(&mut world, &[controller]);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert!(world
        .intro_runtime
        .as_ref()
        .unwrap()
        .controller_failure
        .is_none());
    let exits = exit_actors(&mut world);
    assert!(!exits.is_empty());
    let mut initial = Vec::new();
    for &actor in &exits {
        assert_eq!(
            world.arena.family_update_class(actor),
            Ok(FamilyUpdateClass::Selector35)
        );
        let delay = world.arena.derived_actor(actor).unwrap().word_144;
        assert!(delay < 0, "authored exit command18 starts disabled");
        initial.push(delay);
    }
    // Supplied stage/time schedules the original transition8->9. Completing
    // the sequential index isolates activation, avoiding invented retirement.
    let script = world.arena.selector88_intro_script(controller).unwrap();
    let mut sequence = world.arena.selector88_intro_sequence(controller).unwrap();
    sequence.stage_148 = 8;
    sequence.timer_144 = 0;
    sequence.deadline_140 = 10_000;
    sequence.sequential_index_14c = script
        .reference_count(crate::intro_sequence::IntroActorList::Sequential1c)
        .unwrap();
    world
        .arena
        .publish_selector88_intro_sequence(controller, &sequence)
        .unwrap();
    only_callbacks(&mut world, &[controller]);
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert!(world
        .intro_runtime
        .as_ref()
        .unwrap()
        .controller_failure
        .is_none());
    assert_eq!(
        world
            .arena
            .selector88_intro_sequence(controller)
            .unwrap()
            .stage_148,
        9
    );
    // Visited as controller then the admitted exits, so left reversed.
    let mut expected_order = vec![controller];
    expected_order.extend_from_slice(&exits);
    expected_order.reverse();
    assert_eq!(callback_order(&world), expected_order);
    for (&actor, delay) in exits.iter().zip(initial) {
        let activated = delay.wrapping_mul(-1000);
        assert!(activated > 0);
        assert_eq!(
            world.arena.derived_actor(actor).unwrap().word_144,
            if activated > 40_000 {
                activated.wrapping_sub(40_000)
            } else {
                0
            }
        );
        assert_eq!(
            world.arena.object(actor).unwrap().base.flags & 0x0420_0000,
            0
        );
        assert!(!world
            .actor_updates
            .family_prefix_stops
            .iter()
            .any(|stop| stop.actor == actor));
        assert_eq!(
            world.arena.family428650_actor(actor).unwrap().word_220,
            Some(0)
        );
    }
}

/// Real ordinary Level1 allocations and authored payloads, with explicit saved
/// import fixture policy. Supplied command18 timing below is not retail timing.
fn loaded_ordinary_family_world() -> (
    ResidentRuntime,
    FrontendAssets,
    WorldSession,
    crate::world_map::ProcessWorldMapRuntime,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let grants = inf_formats::parse_grant_table(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let offline = crate::offline_prepare::offline_game_with_selection(&grants, Some(18)).unwrap();
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut owner = crate::world_backend::WorldLoadOwner::import_saved(
        crate::world_backend::OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        },
    )
    .unwrap();
    let mut listener_effects = crate::world_backend::EmptyListenerEffects;
    let process = ParticleFactoryProcessState::native_startup();
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let metadata = crate::world_backend::load_offline_world(
        &mut runtime,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut owner,
            listener_effects: &mut listener_effects,
            policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets: &assets,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            process: Some(&process),
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let world = crate::world_backend::WorldSession::from_loaded(owner, metadata);
    assert_eq!(world.mission.current_world_a0, 18);
    assert!(world.intro_runtime.is_none());
    assert!(!world
        .arena
        .actor_list()
        .iter()
        .any(|&actor| world.arena.selector93(actor).is_ok()));
    (runtime, assets, world, maps)
}

#[test]
fn ordinary_family_delay_returns_run_real_physics_then_retain_the_required_tail_stop() {
    let (runtime, assets, mut world, _maps) = loaded_ordinary_family_world();
    // This world has no live authored selector35: the imported one is already
    // queued for destruction. Supply a real constructor-owned timing fixture
    // and leave that existing disposal work to the production manager.
    let actor = constructed_selector35(&mut world, &assets, Some([1000, 1000, 1000]));
    let unrelated = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&other| {
            other != actor
                && !world
                    .arena
                    .list(ObjectList::Disposal)
                    .iter()
                    .any(|node| node.object == other)
        })
        .unwrap();
    only_callbacks(&mut world, &[actor]);
    let unrelated_before = world.arena.object(unrelated).unwrap().clone();
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    set_delay(&mut world, &runtime, actor, -1);
    let disabled = world.arena.object(actor).unwrap().clone();
    let disabled_rng = world.rng;
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(world.arena.object(actor).unwrap(), &disabled);
    assert_eq!(world.rng, disabled_rng);
    assert!(world.actor_updates.family_prefix_stops.is_empty());
    assert_eq!(callback_order(&world), [actor]);

    set_delay(&mut world, &runtime, actor, 120_000);
    world
        .arena
        .add_actor_linear_velocity(actor, [1000, 500, 100])
        .unwrap();
    world
        .arena
        .add_actor_angular_velocity(actor, [1000, 2000, 3000])
        .unwrap();
    let initial_basis = world.arena.actor(actor).unwrap().rotating.basis;
    for remaining in [80_000, 40_000, 0] {
        let position = world.arena.object(actor).unwrap().base.position_words;
        owners.tick(&mut world, &runtime, &assets, 40_000);
        assert_eq!(
            world.arena.derived_actor(actor).unwrap().word_144,
            remaining
        );
        assert_ne!(
            world.arena.object(actor).unwrap().base.position_words,
            position
        );
        assert!(world.actor_updates.family_prefix_stops.is_empty());
        assert!(world.actor_updates.skipped_callbacks.is_empty());
        assert_eq!(callback_order(&world), [actor]);
    }
    assert_ne!(
        world.arena.actor(actor).unwrap().rotating.basis,
        initial_basis
    );
    assert_eq!(
        world.arena.family428650_actor(actor).unwrap().word_220,
        Some(0)
    );
    owners.tick(&mut world, &runtime, &assets, 40_000);
    let stop = &world.actor_updates.family_prefix_stops[0];
    assert_eq!(world.actor_updates.family_prefix_stops.len(), 1);
    assert_eq!(stop.actor, actor);
    assert_eq!(stop.class, FamilyUpdateClass::Selector35);
    assert!(matches!(
        &stop.reason,
        FamilyPrefixStopReason::BehaviorFailed(
            crate::object_runtime::FamilyBehaviorError::Derived(
                crate::object_runtime::DerivedActorError::MissingSelector35Payload
            )
        )
    ));
    let stopped = world.arena.object(actor).unwrap().clone();
    let stopped_rng = world.rng;
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert_eq!(world.arena.object(actor).unwrap(), &stopped);
    assert_eq!(world.rng, stopped_rng);
    assert_eq!(world.actor_updates.family_prefix_stops.len(), 1);
    assert_eq!(
        world.arena.family428650_actor(actor).unwrap().word_220,
        Some(1)
    );
    assert_eq!(world.arena.object(unrelated).unwrap(), &unrelated_before);
    assert!(world.intro_runtime.is_none());
    assert_eq!(callback_order(&world), [actor]);
}

#[test]
fn ordinary_selected_water_returns_and_alternate_callbacks_preserve_the_following_live_family() {
    let (runtime, assets, mut world, _maps) = loaded_ordinary_family_world();
    let water: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            world.arena.family_update_class(actor) == Ok(FamilyUpdateClass::Selector12)
        })
        .collect();
    assert!(!water.is_empty());
    // Explicit healthy constructor fixture; the loaded selector35 awaiting
    // disposal must not be reactivated as this following callback.
    let fellow = constructed_selector35(&mut world, &assets, Some([1000, 1000, 1000]));
    let alternate = constructed_selector35(&mut world, &assets, Some([1, 2, 3]));
    set_delay(&mut world, &runtime, alternate, 120_000);
    let flags = world.arena.object(alternate).unwrap().base.flags | 0x8000_0000;
    world.arena.apply_payload_flags(alternate, flags).unwrap();
    set_delay(&mut world, &runtime, fellow, 120_000);
    let mut order = water.clone();
    order.extend([alternate, fellow]);
    only_callbacks(&mut world, &order);
    for &actor in &water {
        assert_eq!(
            world.arena.family429e20_actor(actor).unwrap().word_230,
            Some(0)
        );
        assert_eq!(
            world.arena.family428650_actor(actor).unwrap().word_220,
            Some(0)
        );
        // Water precedes this real command18 negative-delay return. Status,
        // physics and virtual+D8 stay unread on this supplied callback arm.
        set_delay(&mut world, &runtime, actor, -1);
    }
    let alternate_before = world.arena.object(alternate).unwrap().clone();
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    let mut expected_order = order.clone();
    for remaining in [80_000, 40_000] {
        let water_expected: Vec<_> = water
            .iter()
            .map(|&actor| {
                let next = selected_water_next(&world, actor, 40_000);
                expected_water_object(world.arena.object(actor).unwrap(), next)
            })
            .collect();
        owners.tick(&mut world, &runtime, &assets, 40_000);
        expected_order.reverse();
        assert_eq!(callback_order(&world), expected_order);
        assert!(world.actor_updates.family_prefix_stops.is_empty());
        for (&actor, expected) in water.iter().zip(water_expected) {
            assert_eq!(world.arena.object(actor).unwrap(), &expected);
            assert_eq!(world.arena.derived_actor(actor).unwrap().word_144, -1);
            assert_eq!(
                world.arena.family428650_actor(actor).unwrap().word_220,
                Some(0)
            );
        }
        assert_eq!(world.arena.object(alternate).unwrap(), &alternate_before);
        assert_eq!(world.actor_updates.skipped_callbacks, [(alternate, true)]);
        assert_eq!(
            world.arena.derived_actor(fellow).unwrap().word_144,
            remaining
        );
        assert!(!world
            .actor_updates
            .family_prefix_stops
            .iter()
            .any(|stop| stop.actor == fellow));
        assert!(world.intro_runtime.is_none());
    }
}

#[test]
fn production_selector12_composes_real_behavior_and_null_target_motion() {
    let (runtime, assets, mut world, _maps) = loaded();
    pause_controller(&mut world);
    let actor = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.family_update_class(actor) == Ok(FamilyUpdateClass::Selector12))
        .unwrap();
    assert_eq!(world.arena.actor(actor).unwrap().word_c4, 3);
    set_delay(&mut world, &runtime, actor, 0);
    //Explicit supplied retained score coefficients exercise the real D8 host
    //without choosing a descriptor. They are not original intro behavior.
    let bytes = vec![0_u8; 32];
    let allocation = std::sync::Arc::new(
        crate::object_runtime::RuntimeAllocation::new(bytes.into(), ByteSpan::new(0, 32)).unwrap(),
    );
    world
        .arena
        .set_selector12_retained(actor, &allocation, 0)
        .unwrap();
    let mut position = world
        .arena
        .object(actor)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let sampled_height = world.terrain.height_at([position[0], position[2]]);
    // Keep the sample point fixed: intrinsic tests cover displaced Y and
    // final physics; this composition fixture checks genuine D8-to-motion.
    position[1] = sampled_height as u16;
    world.arena.copy_actor_position(actor, position).unwrap();
    world
        .arena
        .add_actor_linear_velocity(actor, [0, 500, 0])
        .unwrap();
    only_callbacks(&mut world, &[actor]);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    assert_eq!(owners.surface_snap_53a128, 1);
    let before_rng = world.rng;
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert!(world.actor_updates.family_prefix_stops.is_empty());
    assert_eq!(callback_order(&world), [actor]);
    assert_ne!(world.rng, before_rng, "real D8 periodic/selection RNG ran");
    let family = world.arena.family428650_actor(actor).unwrap();
    assert_eq!(family.word_184, -1);
    assert_eq!(family.words_188_19c[0], Some(40_000));
    assert_eq!(
        family.words_188_19c[5],
        Some(0),
        "an unchanged -1 selection leaves the labeled zero carry in place"
    );
    let derived = world.arena.derived_actor(actor).unwrap();
    assert_eq!((derived.target_148, derived.target_154), (None, None));
    assert_eq!(
        (
            derived.word_14c,
            derived.word_150,
            derived.word_158,
            derived.word_15c
        ),
        (Some(0), Some(0), None, None),
        "selected extended allocation backing supplies fresh14C and 150; adjacent holes stay unknown"
    );
    assert_eq!(
        world
            .arena
            .object(actor)
            .unwrap()
            .base
            .position_words
            .unwrap()[1],
        sampled_height as u16,
        "actual +118 terrain snap ran after normal D8 and native NULL controls"
    );
    assert_eq!(world.arena.actor(actor).unwrap().words_8e_92[1], 0);
    //Without a periodic hit the next visit reads the carried zero timer,
    //rescoring instead of stopping; zero coefficients keep the selection.
    let before_rng = world.rng;
    owners.tick(&mut world, &runtime, &assets, 40_000);
    assert!(world.actor_updates.family_prefix_stops.is_empty());
    assert_ne!(world.rng, before_rng, "rescoring consumed live RNG");
    let family = world.arena.family428650_actor(actor).unwrap();
    assert_eq!(family.word_184, -1);
    assert_eq!(family.words_188_19c[0], Some(80_000));
    assert_eq!(family.words_188_19c[5], Some(0));
}
