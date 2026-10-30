//! Original loaded intro eye through Flag0 and the real software object loan.
//! Supplied view/fade and two declared middle-span poses isolate rendering.
//! Only the paired fixture writes WORD190; actual XYZ/basis, behavior backing
//! and unknown constructor fields remain retained. No native77-update/camera
//! timeline/pixel-acceptance claim.
use super::*;
use crate::{
    frontend_scene::{SceneCommand, SceneView},
    model_context::{ModelContext, ModelDispatch},
    model_execution::ModelRootMode,
    model_scratch::{ModelScratchDrawState, ProcessModelScratch},
    object_runtime::{
        Flag0ObjectDrawHost, ObjectDrawCallbackBoundary, ObjectDrawServiceError, ObjectList,
        ObjectModelDrawSubmission,
    },
    render_resources::{RenderResourceData, RenderResourceKind, RenderResourceRequest},
    software_scene::{SoftwareObjectDrawHost, SoftwareSceneError},
};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
struct EyeState {
    captured: Option<[u16; 3]>,
    height: u16,
    group_words: Option<Vec<[Option<u16>; 6]>>,
    group_flags: Option<Vec<u32>>,
}
fn eye_state(arena: &ObjectArena, source: ObjectId) -> EyeState {
    EyeState {
        captured: arena.selector77_actor(source).unwrap().position_192,
        height: arena.actor(source).unwrap().word_74,
        group_words: arena
            .derived_actor(source)
            .unwrap()
            .behavior_140
            .as_ref()
            .map(|b| b.groups_1c.iter().map(|g| g.coordinates_00_0a).collect()),
        group_flags: arena
            .derived_actor(source)
            .unwrap()
            .behavior_140
            .as_ref()
            .map(|b| b.groups_1c.iter().map(|g| g.word_0c).collect()),
    }
}
#[derive(Debug)]
struct EyePacket {
    packet: ObjectModelPacket,
    root: ModelRootMode,
    before: EyeState,
    after: EyeState,
    commands_before: usize,
    commands_after: usize,
    exact_unowned2: bool,
    error: Option<String>,
}
struct EyeObserver<'a, 'f, 'r> {
    actual: SoftwareObjectDrawHost<'a, 'f, 'r>,
    source: ObjectId,
    packets: Vec<EyePacket>,
}
impl Flag0ObjectDrawHost for EyeObserver<'_, '_, '_> {
    type Error = SoftwareSceneError;
    fn scene(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.actual.scene(command)
    }
    fn attract_demo(&mut self) -> u8 {
        self.actual.attract_demo()
    }
    fn material_color(
        &mut self,
        argument: i32,
    ) -> Result<u32, ObjectDrawServiceError<Self::Error>> {
        self.actual.material_color(argument)
    }
    fn model(
        &mut self,
        request: ObjectModelDrawSubmission<'_>,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        if request.packet.source != self.source {
            return self.actual.model(request);
        }
        // Actual typed identity selected from the loader, then independently
        // inspect packet model/root. The request is forwarded without mutation.
        assert!(request
            .arena
            .selector77_actor(request.packet.source)
            .is_ok());
        let before = eye_state(request.arena, self.source);
        let commands_before = self.actual.scene.stats.model_commands;
        let packet = request.packet;
        let root = request.root;
        let result = self.actual.model(ObjectModelDrawSubmission {
            arena: &mut *request.arena,
            packet,
            root,
            context: request.context,
        });
        let exact_unowned2 = matches!(&result,
            Err(ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::CustomVertex {
                    object,selector:2,reason:crate::object_runtime::ObjectCustomVertexError::Unowned {
                        object:callback_source,selector:2,
                    },
                }
            )) if *object==self.source && *callback_source==self.source);
        self.packets.push(EyePacket {
            packet,
            root,
            before,
            after: eye_state(request.arena, self.source),
            commands_before,
            commands_after: self.actual.scene.stats.model_commands,
            exact_unowned2,
            error: result.as_ref().err().map(|e| format!("{e:?}")),
        });
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EyePoseFixture {
    Constructor,
    MiddleQuarterSpan,
}
impl EyePoseFixture {
    fn label(self) -> &'static str {
        match self {
            Self::Constructor => "constructor",
            Self::MiddleQuarterSpan => "middle-quarter",
        }
    }
}

#[test]
fn loaded_eye_renders_original_chain_in_both_common_modes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(assets.executable_bytes())),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let zero = executable.loader_zero_data(0x00564728, 0xfa004).unwrap();
    // Complete original +D4=4080C0 writes WORD190. Its fresh target is
    // unsigned RNG LOWWORD / 3 + 0x800, so 0x4000 is inside that source range.
    // This guarded fixture never installs a production pose/update default.
    assert_eq!(executable.word(0x0051fbb0 + 0xd4).unwrap(), 0x004080c0);
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(&assets.executable_bytes()[0x80c0..0x81a3])
        ),
        "2c4fa2b41cf4960474794b966e67339faf5364a7910b38d8530af2e0bf40f417"
    );
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        for (near, dispatch) in [(4096, ModelDispatch::Near), (1, ModelDispatch::Far)] {
            let mut constructor_result: Option<(EyeState, ObjectModelPacket, Vec<u8>)> = None;
            for pose in [
                EyePoseFixture::Constructor,
                EyePoseFixture::MiddleQuarterSpan,
            ] {
                let (residency, mut world, _maps, _map_load) = load_first_world_at_tier(tier);
                // report is the actual loader-returned ObjectId map. Class is
                // authenticated independently before using model/placement data.
                let eyes: Vec<_> = world
                    .report
                    .placements
                    .iter()
                    .flatten()
                    .copied()
                    .filter(|&id| world.arena.selector77_actor(id).is_ok())
                    .collect();
                assert_eq!(
                    eyes.len(),
                    1,
                    "actual loaded selector77 identities={eyes:?}"
                );
                let source = eyes[0];
                let actual_position = world
                    .arena
                    .object(source)
                    .unwrap()
                    .base
                    .position_words
                    .unwrap();
                let state = world.arena.selector77_actor(source).unwrap();
                assert_eq!(
                    state.position_192,
                    Some(actual_position),
                    "actual+1C postinit"
                );
                assert_eq!(state.words_184_190, [0; 7]);
                assert_eq!((state.word_1a4, state.word_1a8), (0, 0));
                assert_eq!(
                    state.handle_1b0,
                    world.arena.actor_handles().unwrap().null_handle()
                );
                assert_eq!(
                    (
                        state.word_19c,
                        state.word_19e,
                        state.word_1a0,
                        state.word_1ac
                    ),
                    // Explicit selected production allocation approximation;
                    // original constructors still do not write19C/1A0.
                    (Some(0), None, Some(0), None)
                );
                let original_allocation_carries = (
                    state.word_19c,
                    state.word_19e,
                    state.word_1a0,
                    state.word_1ac,
                );
                // Only this isolated fixture alters WORD190. The Constructor case
                // executes with the actual loaded state and no fixture write.
                if pose == EyePoseFixture::MiddleQuarterSpan {
                    world
                        .arena
                        .supply_selector77_middle_pose_fixture(source, 0x4000)
                        .unwrap();
                }
                let mut expected_pose_words = [0; 7];
                if pose == EyePoseFixture::MiddleQuarterSpan {
                    expected_pose_words[6] = 0x4000;
                }
                assert_eq!(
                    world.arena.selector77_actor(source).unwrap().words_184_190,
                    expected_pose_words
                );
                let captured = entity_submission(&world.arena, source).unwrap();
                assert_eq!(
                    captured.model_slot, 698,
                    "H8 is a witness, not class selection"
                );
                let resources = RenderResources::new(&residency);
                // Original complete entries authenticate the unchanged external
                // child chain plus its separately authored oneeye consumer.
                for (slot, hash) in [
                    (
                        698,
                        "917b907588af6057697e476a8f47f3536fdf846d418158f0990e6aed5592a8b6",
                    ),
                    (
                        699,
                        "737604f08eeb931850c2b3e272f9281b86e0218b9c845c5dbdb9df8f7c45eaa5",
                    ),
                    (
                        700,
                        "caf5090f08552341ed26290f5a0a369da7bb2a0380f2f6f0de0f487936e11f48",
                    ),
                    (
                        701,
                        "745b3c706dda6ae2ec03de45f7c45e953d7f9a8986993fcc5583d92f43eff968",
                    ),
                    (
                        702,
                        "0479dd7da03abc2eb013959ae17ae5607ad620f58b707ed948d9cf5c33e946e6",
                    ),
                ] {
                    let resource = resources
                        .resolve(RenderResourceRequest {
                            kind: RenderResourceKind::Model,
                            argument: slot,
                        })
                        .unwrap();
                    let RenderResourceData::Model(model) = resource.data else {
                        panic!("actual H8 binding");
                    };
                    assert_eq!(model.identity.global_index, slot as u16);
                    assert_eq!(
                        format!(
                            "{:x}",
                            Sha256::digest(
                                &resource.source.bytes[model.span.start..model.span.end]
                            )
                        ),
                        hash
                    );
                }
                let active: Vec<_> = world
                    .arena
                    .list(ObjectList::Flag0Primary)
                    .iter()
                    .map(|n| n.object)
                    .collect();
                for id in active {
                    world.arena.set_flag0(id, false).unwrap();
                }
                world.arena.set_flag0(source, true).unwrap();
                assert_eq!(world.arena.list(ObjectList::Flag0Primary).len(), 1);
                // Supplied eye/fade; actual actor XYZ, basis/model, extent, behavior,
                // raw H13/H14 and constructor RNG state remain retained unchanged.
                let view = SceneView {
                    eye: [
                        captured.position[0],
                        captured.position[1],
                        captured.position[2] - 2048,
                    ],
                    basis: MenuBasis::INITIAL,
                };
                let startup = frontend_startup(
                    &residency,
                    crate::presentation::NativeFrame::original(dimensions),
                )
                .unwrap();
                let mut config = startup.scene;
                config.environment.view = view;
                config.environment.coordinate_shift = 16; // actual wrapped WORD window
                config.environment.projection = ModelProjection {
                    focal: startup.projection.focal,
                    viewport: dimensions.map(|v| v as i32),
                    center: dimensions.map(|v| (v / 2) as i32),
                    fade: [near, 8192],
                };
                config.execution_limits.instructions = 100_000;
                let root_model = resources
                    .resolve(RenderResourceRequest {
                        kind: RenderResourceKind::Model,
                        argument: i32::from(captured.model_slot),
                    })
                    .unwrap();
                let RenderResourceData::Model(root_model) = root_model.data else {
                    unreachable!();
                };
                let transform = ModelContext::new(captured.position, captured.basis)
                    .publish(view, config.environment.coordinate_shift);
                assert_eq!(transform.view_position, [0, 0, 2048]);
                assert_eq!(
                    transform.admission(root_model.header, &config.environment.projection),
                    Some(dispatch)
                );
                let mut context = crate::object_runtime::ObjectDrawContext {
                    view,
                    near,
                    far: 8192,
                    far_color: config.far_color,
                };
                let direction = world.presentation.terrain_light_direction;
                let tint_before = world.presentation.tint_word;
                let rng_before = world.rng;
                let process =
                crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
                let clock = AnimationClock::default();
                let mut draw_process =
                    crate::object_runtime::ObjectDrawProcessState::native_startup(executable)
                        .unwrap();
                draw_process.completion_53b890 = 0; // declared preceding update byte
                let scratch = ProcessModelScratch::native_loader_zero(&zero).unwrap();
                let clear = [17, 23, 31, 255];
                let mut pixels = SoftwareFrame::new(dimensions[0], dimensions[1], clear);
                let mut scene = SoftwareScene::new(SoftwareSceneRequest {
                    model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                        scratch.clone(),
                        (config).environment,
                    ),
                    resources,
                    tables: &tables,
                    frame: &mut pixels,
                    config,
                })
                .unwrap();
                scene.set_terrain(Some(world.terrain.clone()));
                scene.install_terrain_surface_callbacks(world.world_flags, 0);
                scene.set_process_rng(Some(world.rng));
                scene.begin_frame().unwrap();
                let mut observer=EyeObserver {
                actual:SoftwareObjectDrawHost {
                    terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
                    effects: None,
                    scene:&mut scene,animation:&clock,executable,attract_demo:&false,
                    failure_policy:crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
                },source,packets:Vec::new(),
            };
                let grid = crate::world_lighting::WorldLightGrid::default();
                let result=world.arena.run_flag0_object_draw_pass(crate::object_runtime::Flag0ObjectDrawRequest { cone: crate::object_runtime::ObjectDrawCone::Retail,                context:&mut context,process:&mut draw_process,network_flag:&process.network_flag,
                terrain:&mut crate::world_actor_terrain::WorldActorTerrainHost {
                    residency:&residency,source:&world.terrain_source,prepared:&world.terrain,
                },
                lighting:crate::object_runtime::ObjectDrawLighting {
                    grid:&grid,tint_word:&mut world.presentation.tint_word,direction,
                    spacing:world.terrain.sampling().spacing,executable,
                },host:&mut observer,
                failure_policy:crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
            });
                // Same all-return process commit as the consumed world helper.
                world.rng = observer.actual.scene.process_rng().unwrap();
                println!("loaded eye tier={tier} dispatch={dispatch:?} pose={pose:?} result={result:?} packets={:?} rng_before={rng_before:08X} rng_after={:08X}",observer.packets,world.rng);
                assert_eq!(
                    observer.packets.len(),
                    1,
                    "manager must reach genuine common packet"
                );
                let row = &observer.packets[0];
                assert_eq!(row.packet, captured);
                assert_eq!(row.root, ModelRootMode::CommonVisual);
                assert!(
                    !row.exact_unowned2,
                    "new owner must replace exact typed boundary"
                );
                assert!(
                    row.before.captured != row.after.captured
                        || row.before.group_words != row.after.group_words
                        || row.before.group_flags != row.after.group_flags,
                    "require reached real callback state, not cull/count/warning absence: {row:?}"
                );
                assert!(
                    row.error.is_none(),
                    "original eye packet must complete: {row:?}"
                );
                // Original chain completeness is required in this supplied view.
                // Genuine scene/pool/queue errors retain their reached diagnostics.
                assert!(result.is_ok(), "actual reached hard failure: {result:?}");
                let outcome = result.unwrap();
                assert_eq!((outcome.visited, outcome.near_callbacks), (1, 1));
                assert_eq!(world.arena.list(ObjectList::Flag0Primary).len(), 1);
                assert!(world.arena.list(ObjectList::Flag0Secondary).is_empty());
                assert_eq!(draw_process.completion_53b890, 1);
                assert_eq!(world.presentation.tint_word, tint_before);
                assert_eq!(world.rng, rng_before);
                assert_eq!(
                    entity_submission(&world.arena, source).unwrap(),
                    captured,
                    "custom capture74/192 and real140 do not pose the actor"
                );
                let state = world.arena.selector77_actor(source).unwrap();
                assert_eq!(
                    (
                        state.word_19c,
                        state.word_19e,
                        state.word_1a0,
                        state.word_1ac
                    ),
                    original_allocation_carries
                );
                assert_eq!(
                    state.words_184_190, expected_pose_words,
                    "draw must retain supplied pose words"
                );
                assert_eq!(scratch.state().unwrap(), ModelScratchDrawState::Ready);
                assert!(outcome.omissions.is_empty());
                assert_eq!(outcome.common_models, 1);
                assert!(
                    row.commands_after > row.commands_before,
                    "complete visible chain queues primitives"
                );
                // Actual final flush is needed before any pixel conclusion.
                observer.actual.scene(SceneCommand::FlushBatch).unwrap();
                assert!(
                    observer.actual.scene.pixels.stats.fragments > 0,
                    "complete packet needs genuine raster output in this declared view"
                );
                let after = row.after.clone();
                drop(observer);
                drop(scene);
                assert!(
                    pixels.rgba().chunks_exact(4).any(|p| p != clear),
                    "real eye pixels must survive the final flush"
                );
                // Original699 interpolates three Y0..768 rows by callback selector6.
                // 0x4000 is exactly one quarter of its unsigned16 weight domain.
                // Keep the actor packet/basis unchanged and require the real custom
                // capture, behavior group output, and final pixels to distinguish it.
                if let Some((base, base_packet, base_pixels)) = &constructor_result {
                    let base: &EyeState = base;
                    assert_eq!(captured.model_slot, base_packet.model_slot);
                    assert_eq!(captured.position, base_packet.position);
                    assert_eq!(captured.basis, base_packet.basis);
                    assert_eq!(after.height, base.height.wrapping_add(192));
                    let mut expected = base.clone();
                    expected.height = expected.height.wrapping_add(192);
                    let captured_y = &mut expected.captured.as_mut().unwrap()[1];
                    *captured_y = captured_y.wrapping_add(192);
                    if let Some(groups) = &mut expected.group_words {
                        for group in groups {
                            for y in [1, 4] {
                                group[y] = group[y].map(|word| word.wrapping_add(192));
                            }
                        }
                    }
                    assert_eq!(after, expected, "only reached middle-span Y values change");
                    assert_ne!(
                        pixels.rgba(),
                        base_pixels.as_slice(),
                        "complete real raster must distinguish the two declared poses"
                    );
                } else {
                    assert_eq!(pose, EyePoseFixture::Constructor);
                    constructor_result = Some((after, captured, pixels.rgba().to_vec()));
                }
                // Optional, disposable visual evidence; normal tests need no output
                // directory and never overwrite a previous snapshot or retail asset.
                if let Some(directory) = std::env::var_os("INF_EYE_SNAPSHOT_DIR") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let path =
                        directory.join(format!("eye-{tier}-{dispatch:?}-{}.png", pose.label()));
                    let file = std::fs::File::create_new(&path).unwrap();
                    let mut encoder = png::Encoder::new(file, dimensions[0], dimensions[1]);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .unwrap()
                        .write_image_data(pixels.rgba())
                        .unwrap();
                    println!("eye snapshot: {}", path.display());
                }
            }
        }
    }
}
