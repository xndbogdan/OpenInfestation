//! Actual loaded vehicle through the world manager and shared model executor.
//! Supplied view/mode isolate the constructor domain; active scratch is an
//! explicit custody fault fixture. Neither establishes visible vehicle pixels,
//! native camera/update mode publication, or complete retail frame acceptance.

use super::*;
use crate::{
    model_scratch::{ModelScratchDrawState, ModelScratchError, ProcessModelScratch},
    object_runtime::{
        Flag0ObjectDrawError, ObjectDrawProcessState, ObjectList, ObjectModelCallbacks,
    },
    render_resources::{RenderResourceData, RenderResourceKind, RenderResourceRequest},
};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScratchFixture {
    Ready,
    ActiveDraw,
}

#[test]
fn loaded_vehicle_constructor_model_completes_in_both_tiers_and_lanes() {
    exercise_loaded_vehicle(ScratchFixture::Ready);
}

#[test]
fn loaded_vehicle_scratch_custody_fault_preserves_actual_world_prefix() {
    exercise_loaded_vehicle(ScratchFixture::ActiveDraw);
}

fn exercise_loaded_vehicle(scratch_fixture: ScratchFixture) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let zero = executable.loader_zero_data(0x0056_4728, 0xfa004).unwrap();
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        for mode in [0, 1] {
            let (residency, mut world, _maps, _map_load) = load_first_world_at_tier(tier);
            // Actual loader result selects identity, then actual typed state
            // authenticates class. H8/model/arena index never select the class.
            let source = world.respawn.as_ref().unwrap().vehicle.unwrap();
            let state = world.arena.selector2_actor(source).unwrap();
            assert_eq!(state.dword_228(), None);
            let pose = world
                .arena
                .object(source)
                .unwrap()
                .base
                .position_words
                .unwrap();
            let captured = entity_submission(&world.arena, source).unwrap();
            let resources = RenderResources::new(&residency);
            let model_resource = resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Model,
                    argument: i32::from(captured.model_slot),
                })
                .unwrap();
            let RenderResourceData::Model(model) = model_resource.data else {
                panic!("actual vehicle H8 binding")
            };
            assert!(model.header.vertex_slot_count > 0);
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(&model_resource.source.bytes[model.span.start..model.span.end])
                ),
                "b945f86866b9da46a4f488130d40ed2ac4a8136abae527a6ae43c82d90e3fc9d"
            );
            assert_eq!(model.program.instructions.len(), 27);
            // The loaded constructor's +1E8=0 maps to callback1=0 through the
            // original 00530578 table. Its wrapper's equipment branches skip
            // visible child packets. Zero queue commands is this finite domain,
            // not evidence of a full vehicle render or an outer frustum cull.
            assert_eq!(state.dwords_1e8_1f4[0], 0);
            assert_eq!(
                ObjectModelCallbacks {
                    arena: &world.arena,
                    source,
                    animation: &AnimationClock::default(),
                    executable,
                }
                .packed_value(1)
                .unwrap(),
                0
            );
            // Supplied manager isolation, via existing real setters. Actual
            // vehicle payload, XYZ, basis, model, extent and terrain stay intact.
            let active: Vec<_> = world
                .arena
                .list(ObjectList::Flag0Primary)
                .iter()
                .map(|node| node.object)
                .collect();
            for object in active {
                world.arena.set_flag0(object, false).unwrap();
            }
            world.arena.set_flag0(source, true).unwrap();
            assert!(world.arena.apply_actor_view_mode(source, mode).unwrap());
            if mode == 1 {
                let flags = world.arena.object(source).unwrap().base.flags;
                world
                    .arena
                    .apply_payload_flags(source, flags | 0x0100_0001)
                    .unwrap();
            }
            assert_eq!(world.arena.list(ObjectList::Flag0Primary).len(), 1);
            let view = SceneView {
                eye: [
                    i32::from(pose[0] as i16),
                    i32::from(pose[1] as i16),
                    i32::from(pose[2] as i16) - 1024,
                ],
                basis: MenuBasis::INITIAL,
            };
            let mut config = frontend_startup(
                &residency,
                crate::presentation::NativeFrame::original(dimensions),
            )
            .unwrap()
            .scene;
            config.environment.view = view;
            //Headless fixture publishes the full explicit model projection;
            //startup CPU globals alone do not establish viewport/focal/center.
            config.environment.projection = ModelProjection {
                focal: [512; 2],
                viewport: dimensions.map(|v| v as i32),
                center: dimensions.map(|v| (v / 2) as i32),
                fade: [1, 65_000],
            };
            config.environment.coordinate_shift = 0; // explicit supplied fixture units
            let transform =
                crate::model_context::ModelContext::new(captured.position, captured.basis)
                    .publish(view, config.environment.coordinate_shift);
            assert_eq!(transform.view_position, [0, 0, 1024]);
            assert_eq!(
                transform.admission(model.header, &config.environment.projection),
                Some(crate::model_context::ModelDispatch::Far)
            );
            config.execution_limits.instructions = 100_000;
            let mut context = crate::object_runtime::ObjectDrawContext {
                view,
                near: 1,
                far: 65_000,
                far_color: config.far_color,
            };
            let scratch = ProcessModelScratch::native_loader_zero(&zero).unwrap();
            let mut frame = SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
            let mut scene = SoftwareScene::new(SoftwareSceneRequest {
                model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                    scratch.clone(),
                    (config).environment,
                ),
                resources,
                tables: &tables,
                frame: &mut frame,
                config,
            })
            .unwrap();
            scene.set_terrain(Some(world.terrain.clone()));
            scene.install_terrain_surface_callbacks(world.world_flags, 0);
            scene.begin_frame().unwrap();
            let process =
                crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
            let mut draw_process = ObjectDrawProcessState::native_startup(executable).unwrap();
            let clock = AnimationClock::default();
            let grid = crate::world_lighting::WorldLightGrid::default();
            let before_tint = world.presentation.tint_word;
            let mut modulated = before_tint.to_le_bytes();
            let shade = u32::from(world.terrain.cell_at([pose[0], pose[2]]).shade4().min(8));
            for channel in &mut modulated[..3] {
                *channel = ((u32::from(*channel) * shade) >> 3) as u8;
            }
            let queue_before = scene.queue.state();
            let rng_before = world.rng;
            draw_process.completion_53b890 = 0; // supplied preceding-update byte
            let active_scope = match scratch_fixture {
                ScratchFixture::Ready => None,
                ScratchFixture::ActiveDraw => Some(scratch.begin_draw().unwrap()),
            };
            let scratch_before = scratch.state().unwrap();
            let result = submit_world_object_models(
                &mut world,
                &mut scene,
                &mut WorldObjectDrawRequest {
                    weapon_tables: None,
                    pickup_text_data: None,
                    particles: &mut Default::default(),
                    sounds: None,
                    birth_inputs: crate::process_particles::births::ParticleBirthInputs {
                        network_active: false,
                        phase: Some(4),
                        process: &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
                    },
                    context: &mut context,
                    process: &mut draw_process,
                    residency: &residency,
                    light_grid: &grid,
                    executable,
                    network_flag: &process.network_flag,
                    animation: &clock,
                    attract_demo: &false,
                    camera_settings: crate::object_runtime::CameraSettings { first_person: 0, camera_style: 0 },
                    settings_7c: 0,
                    settings_70: 0,
                    network_initials: &[[0; 6]; crate::network::game::SLOTS],
                    network_team_flag: 0,
                    map: None,
                },
                ObjectPass::Manager(crate::object_runtime::ObjectDrawCone::Retail),
            );
            let after228 = world.arena.selector2_actor(source).unwrap().dword_228();
            assert_eq!(
                scene.process_rng(),
                Some(world.rng),
                "actual all-return world commit"
            );
            assert_eq!(entity_submission(&world.arena, source).unwrap(), captured,
                "this currently unowned vehicle custom domain has no invented pose/basis/model writes");
            assert_eq!(world.rng, rng_before);
            assert_eq!(scene.queue.state(), queue_before);
            assert_eq!(scene.stats.model_commands, 0);
            assert_eq!(scratch.cursor().unwrap(), 0);
            match scratch_fixture {
                ScratchFixture::Ready => {
                    assert_eq!(world.arena.list(ObjectList::Flag0Primary).len(), 1);
                    assert!(world.arena.list(ObjectList::Flag0Secondary).is_empty());
                    let outcome = result.unwrap();
                    assert_eq!((outcome.visited, outcome.near_callbacks), (1, 1));
                    assert!(outcome.omissions.is_empty());
                    assert_eq!(
                        (
                            outcome.common_models,
                            outcome.late_models,
                            outcome.empty_behaviors
                        ),
                        if mode == 0 { (1, 0, 0) } else { (0, 1, 1) }
                    );
                    assert_eq!(after228, Some(0));
                    assert_eq!(world.presentation.tint_word, before_tint);
                    assert_eq!(draw_process.completion_53b890, 1);
                    assert_eq!(scene.stats.models, 1);
                    assert_eq!(scratch.state().unwrap(), ModelScratchDrawState::Ready);
                }
                ScratchFixture::ActiveDraw => {
                    assert!(
                        matches!(
                            result,
                            Err(Flag0ObjectDrawError::Host(SoftwareSceneError::Model(
                                crate::model_execution::ModelExecutionError::Scratch(
                                    ModelScratchError::DrawState {
                                        expected: "Ready",
                                        actual: ModelScratchDrawState::Running(_)
                                    }
                                )
                            )))
                        ),
                        "{result:?}"
                    );
                    assert!(world.arena.list(ObjectList::Flag0Primary).is_empty());
                    assert_eq!(world.arena.list(ObjectList::Flag0Secondary).len(), 1);
                    assert_eq!(after228, None);
                    assert_eq!(draw_process.completion_53b890, 0);
                    assert_eq!(scene.stats.models, 0);
                    assert_eq!(scratch.state().unwrap(), scratch_before,
                        "the enclosing active draw is never reset/closed by model-feature containment");
                    assert!(active_scope.is_some());
                    assert_eq!(
                        world.presentation.tint_word,
                        if mode == 0 {
                            u32::from_le_bytes(modulated)
                        } else {
                            before_tint
                        }
                    );
                    if mode == 0 {
                        assert_eq!(
                            draw_process.profiling.counters(13, 1).unwrap().1,
                            0,
                            "failed common model never executes the vehicle End suffix"
                        );
                    }
                }
            }
            if mode == 1 {
                assert_eq!(draw_process.profiling.counters(13, 1).unwrap(), (0, 0));
            }
        }
    }
}
