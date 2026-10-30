//! Actual loaded Flag0 packets through the production model backend.

use super::*;
use crate::model_execution::ModelRootMode;
use crate::object_runtime::{
    Flag0ObjectDrawHost, ObjectDrawCallbackBoundary, ObjectDrawServiceError,
    ObjectModelDrawSubmission, ObjectModelPacket,
};
use crate::software_scene::{SoftwareObjectDrawHost, SoftwareSceneError};

#[derive(Debug)]
struct ActualPacketObservation {
    packet: ObjectModelPacket,
    root: ModelRootMode,
    actor_model_word: Option<u16>,
    // Some identifies the actual typed class60 source, independently of H8.
    before_cache: Option<(u8, Option<[u16; 3]>)>,
    after_cache: Option<(u8, Option<[u16; 3]>)>,
    // Outer Some identifies the genuine typed selector30 source; inner None
    // is its constructor-unwritten16C row, independently of model/slot.
    before_position16c: Option<Option<[u16; 3]>>,
    after_position16c: Option<Option<[u16; 3]>>,
    exact_pre_owner_unowned_8: bool,
    exact_pre_owner_unowned_9: bool,
    returned_error: Option<String>,
}

struct ActualPacketObserver<'a, 'f, 'r> {
    actual: SoftwareObjectDrawHost<'a, 'f, 'r>,
    packets: Vec<ActualPacketObservation>,
}

impl Flag0ObjectDrawHost for ActualPacketObserver<'_, '_, '_> {
    type Error = SoftwareSceneError;

    fn scene(&mut self, command: crate::frontend_scene::SceneCommand) -> Result<(), Self::Error> {
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
        let packet = request.packet;
        let root = request.root;
        let actor_model_word = request
            .arena
            .actor(packet.source)
            .ok()
            .map(|actor| actor.rotating.visual.model_word);
        let before_cache = request
            .arena
            .selector60_actor(packet.source)
            .ok()
            .map(|state| (state.byte_184, state.position_198));
        let before_position16c = request
            .arena
            .selector30_actor(packet.source)
            .ok()
            .map(|state| state.position_16c);
        // The explicit reborrow ends at the real backend's return. The same
        // arena/source/context/root/geometry are forwarded without replacement.
        let result = self.actual.model(ObjectModelDrawSubmission {
            arena: &mut *request.arena,
            packet,
            root,
            context: request.context,
        });
        let after_cache = request
            .arena
            .selector60_actor(packet.source)
            .ok()
            .map(|state| (state.byte_184, state.position_198));
        let after_position16c = request
            .arena
            .selector30_actor(packet.source)
            .ok()
            .map(|state| state.position_16c);
        let exact_pre_owner_unowned_8 = matches!(
            &result,
            Err(ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::CustomVertex {
                    object, selector: 8,
                    reason: crate::object_runtime::ObjectCustomVertexError::Unowned {
                        object: callback_source, selector: 8,
                    },
                },
            )) if *object == packet.source && *callback_source == packet.source
        );
        let exact_pre_owner_unowned_9 = matches!(
            &result,
            Err(ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::CustomVertex {
                    object,
                    selector: 9,
                    reason: crate::object_runtime::ObjectCustomVertexError::Unowned {
                        object: callback_source,
                        selector: 9,
                    },
                },
            )) if *object == packet.source && *callback_source == packet.source
        );
        self.packets.push(ActualPacketObservation {
            packet,
            root,
            actor_model_word,
            before_cache,
            after_cache,
            before_position16c,
            after_position16c,
            exact_pre_owner_unowned_8,
            exact_pre_owner_unowned_9,
            returned_error: result.as_ref().err().map(|error| format!("{error:?}")),
        });
        result
    }
}

// Find the actual typed-class mutation before checking model/root witnesses.
// No arena slot or H8 argument selects the callback under test.
fn assert_post_owner_source_join(observer: &ActualPacketObserver<'_, '_, '_>) {
    let reaches: Vec<_> = observer
        .packets
        .iter()
        .filter(|row| {
            row.before_cache == Some((0, None)) && matches!(row.after_cache, Some((1, Some(_))))
        })
        .collect();
    assert!(!reaches.is_empty(), "actual packets={:?}", observer.packets);
    for row in reaches {
        assert_eq!(row.actor_model_word, Some(390), "{row:?}");
        assert_eq!(row.packet.model_slot, 390, "{row:?}");
        assert_eq!(row.root, ModelRootMode::ManagerLate, "{row:?}");
        assert!(!row.exact_pre_owner_unowned_9, "{row:?}");
        // This supplied manager view completes the actual packet; other poses
        // and later branch choices remain separate from this witness.
        assert!(row.returned_error.is_none(), "{row:?}");
    }
}

#[test]
fn loaded_flag0_custom_capture_source_joins() {
    let (residency, mut world, _maps, _map_load) = load_first_world();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let mut config = frontend_startup(
        &residency,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap()
    .scene;
    // A supplied broad manager view; neither source nor model is selected.
    // This phase fixture does not claim a complete production world frame.
    let mut context = crate::object_runtime::ObjectDrawContext {
        view: crate::frontend_scene::SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        near: 1_000_000,
        far: 2_000_000,
        far_color: 0,
    };
    config.environment.view = context.view;
    config.environment.projection.fade = [context.near, context.far];
    config.environment.coordinate_shift = 0;
    config.execution_limits.instructions = 100_000;
    let direction = config.environment.lighting.direction;
    let mut tint_word = config.environment.lighting.ambient_word;
    let mut frame = SoftwareFrame::new(640, 480, [0; 4]);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (config).environment,
        ),
        resources: RenderResources::new(&residency),
        tables: &tables,
        frame: &mut frame,
        config,
    })
    .unwrap();
    scene.set_terrain(Some(world.terrain.clone()));
    scene.install_terrain_surface_callbacks(world.world_flags, 0);
    scene.set_process_rng(Some(world.rng));
    let animation = AnimationClock::default();
    let mut observer = ActualPacketObserver {
        actual: SoftwareObjectDrawHost {
            terrain: crate::software_scene::ModelTerrainRead::RendererEpoch,
            effects: None,
            scene: &mut scene, animation: &animation, executable, attract_demo: &false,
            failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
        }, packets: Vec::new(),
    };
    let mut terrain = crate::world_actor_terrain::WorldActorTerrainHost {
        residency: &residency,
        source: &world.terrain_source,
        prepared: &world.terrain,
    };
    let grid = crate::world_lighting::WorldLightGrid::default();
    let mut process =
        crate::object_runtime::ObjectDrawProcessState::native_startup(executable).unwrap();
    let result = world.arena.run_flag0_object_draw_pass(crate::object_runtime::Flag0ObjectDrawRequest { cone: crate::object_runtime::ObjectDrawCone::Retail,        context: &mut context, process: &mut process, network_flag: &0, terrain: &mut terrain,
        lighting: crate::object_runtime::ObjectDrawLighting {
            grid: &grid, tint_word: &mut tint_word, direction, spacing: world.terrain.sampling().spacing, executable,
        }, host: &mut observer,
        failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
    });
    println!(
        "Flag0 completed={}; class60 packets={:?}",
        result.is_ok(),
        observer
            .packets
            .iter()
            .filter(|r| r.before_cache.is_some())
            .collect::<Vec<_>>()
    );
    assert!(result.is_ok(), "result={result:?}");
    assert_post_owner_source_join(&observer);
    let selector30_reaches: Vec<_> = observer
        .packets
        .iter()
        .filter(|row| {
            row.before_position16c == Some(None) && matches!(row.after_position16c, Some(Some(_)))
        })
        .collect();
    println!("actual selector30 capture packets={selector30_reaches:?}");
    assert!(
        !selector30_reaches.is_empty(),
        "actual packets={:?}",
        observer.packets
    );
    for row in selector30_reaches {
        // Select the actual typed source mutation first; model/root are only
        // witnesses of the original reached packet, never admission policy.
        assert_eq!(row.actor_model_word, Some(62), "{row:?}");
        assert_eq!(row.packet.model_slot, 62, "{row:?}");
        assert_eq!(row.root, ModelRootMode::CommonVisual, "{row:?}");
        assert!(!row.exact_pre_owner_unowned_8, "{row:?}");
        // This declared manager view completes the original62->63 packet.
        assert!(row.returned_error.is_none(), "{row:?}");
    }
}
