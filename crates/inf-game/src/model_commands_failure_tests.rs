use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_scene::{ModelCallbackPolicy, SceneModelSubmission, SceneView},
    menu::MenuBasis,
    model_execution::{
        ModelDrawRequest, ModelEnvironment, ModelExecutionError, ModelExecutionLimits,
        ModelExecutor,
    },
    model_lighting::ModelLighting,
    model_primitives::PrimitiveCallback,
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::FrontendExecutable;
use std::{path::Path, sync::Arc};

struct Services {
    source: SceneModelSubmission,
    queries: usize,
    fail_at: Option<usize>,
    sorting: bool,
    address: Option<u32>,
    pending: bool,
}
impl ModelValueCallback for Services {
    type Error = &'static str;
    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        self.source
            .callback_value(selector)
            .ok_or("pure virtual callback")
    }
}
impl<'a> ModelCommandServices<'a> for Services {
    fn prepare_draw(
        &mut self,
        queue: &mut DrawQueue<PrimitiveCommand<'a>>,
    ) -> Result<bool, Self::Error> {
        self.queries += 1;
        if self.pending {
            self.pending = false;
            // A recognizable fixture for the native virtual call's previously
            // pending work; this is not a manufactured model primitive.
            let mut command = PrimitiveCommand::new(PrimitiveCallback::FlatTriangleNear, 0x1234, 3);
            command.write_u32(0, 0xcafe_babe);
            queue
                .submit(DrawSubmission {
                    depth: command.depth,
                    payload_blocks: 3,
                    receiver_offset: 0,
                    command,
                })
                .map_err(|_| "pending allocation")?;
        }
        if self.fail_at == Some(self.queries) {
            Err("renderer unavailable")
        } else {
            Ok(self.sorting)
        }
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        self.address
            .map(crate::model_primitives::TextureSortIdentity::NativeEntryAddress)
            .ok_or("texture allocation identity unavailable")
    }
}

fn setup() -> (ResidentRuntime, PrimitiveTables) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    (runtime, tables)
}
fn scene<'a>(
    scratch: &'a crate::model_scratch::ProcessModelScratch,
) -> (ModelDrawRequest<'a>, PrimitiveState, Services) {
    let projection = crate::model_context::ModelProjection {
        focal: [320, 240],
        viewport: [640, 480],
        center: [320, 240],
        fade: [1_000_000, 2_000_000],
    };
    let request = ModelDrawRequest {
        scratch,
        root_mode: crate::model_execution::ModelRootMode::CommonVisual,
        model_argument: 751,
        context: ModelContext::new([0, 0, 100_000], MenuBasis::INITIAL),
        surface: crate::model_vertices::ModelSurfaceBinding::Default,
        environment: ModelEnvironment {
            projection,
            view: SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            },
            coordinate_shift: 0,
            fog_factor: 16,
            lighting: ModelLighting {
                direction: [0.0, 0.0, 1.0],
                directional_word: 0xaad2ff,
                ambient: [10, 6, 5],
                ambient_word: 0x05060a,
            },
        },
    };
    let state = PrimitiveState {
        mode: 0,
        far_color: 0x1020_3040,
        effect_color: 0xaabb_ccdd,
        projection,
    };
    let services = Services {
        source: SceneModelSubmission {
            model_slot: 751,
            position: request.context.position,
            basis: request.context.basis,
            animation: AnimationClock::default(),
            primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
            failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
            surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
            policy: ModelCallbackPolicy::Frontend,
        },
        queries: 0,
        fail_at: None,
        sorting: false,
        address: None,
        pending: false,
    };
    (request, state, services)
}
fn executor<'a>(runtime: &'a ResidentRuntime, tables: &PrimitiveTables) -> ModelExecutor<'a> {
    ModelExecutor::new(
        RenderResources::new(runtime),
        Arc::clone(&tables.sine_table),
        ModelExecutionLimits {
            instructions: 100_000,
            invocation_depth: 16,
        },
    )
}

#[test]
fn renderer_failure_retains_preceding_packets_and_stops_both_owners() {
    let (runtime, tables) = setup();
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let (request, state, mut services) = scene(&scratch);
    services.fail_at = Some(2);
    let mut queue = DrawQueue::new(4096).unwrap();
    let mut backend = ModelCommandBackend::new(
        RenderResources::new(&runtime),
        &tables,
        state,
        &mut queue,
        services,
    );
    let mut executor = executor(&runtime, &tables);
    assert!(matches!(
        executor.draw(request, &mut backend),
        Err(ModelExecutionError::Host(ModelCommandError::Service(
            "renderer unavailable"
        )))
    ));
    assert_eq!(backend.queue.state().command_count, 1);
    assert_eq!(backend.services.queries, 2);
    assert!(backend.stopped() && executor.stopped());
    assert!(matches!(
        backend.material_sorting(),
        Err(ModelCommandError::Stopped)
    ));
    assert!(matches!(
        executor.draw(request, &mut backend),
        Err(ModelExecutionError::Stopped)
    ));
    let mut callbacks = vec![];
    backend
        .queue
        .flush(|_, command| {
            callbacks.push(command.callback);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(callbacks, [PrimitiveCallback::TexturedQuadNear]);
}

#[test]
fn arena_exhaustion_keeps_native_0301_for_frontend_recovery() {
    let (runtime, tables) = setup();
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let (request, state, services) = scene(&scratch);
    let mut queue = DrawQueue::new(96).unwrap(); // One3-block primitive +32-byte root entry.
    let mut backend = ModelCommandBackend::new(
        RenderResources::new(&runtime),
        &tables,
        state,
        &mut queue,
        services,
    );
    let error = executor(&runtime, &tables)
        .draw(request, &mut backend)
        .unwrap_err();
    let ModelExecutionError::Host(ModelCommandError::Queue(fault)) = error else {
        panic!("unexpected error: {error}")
    };
    assert_eq!(fault.native_error_code(), Some(0x301));
    assert_eq!(backend.queue.state().command_count, 1);
    assert!(backend.queue.state().stopped && backend.stopped());
}

#[test]
fn group_preparation_publishes_pending_work_before_child_group_and_discards_policy_return() {
    let (runtime, tables) = setup();
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let (_, state, mut services) = scene(&scratch);
    services.pending = true;
    services.sorting = true;
    let mut queue = DrawQueue::new(1024).unwrap();
    let mut backend = ModelCommandBackend::new(
        RenderResources::new(&runtime),
        &tables,
        state,
        &mut queue,
        services,
    );
    backend.begin_depth_group(0x1234, false).unwrap();
    for depth in [1, 99] {
        let mut command = PrimitiveCommand::new(PrimitiveCallback::FlatTriangleNear, depth, 3);
        command.write_u32(0, depth);
        backend.submit(command).unwrap();
    }
    backend.end_depth_group().unwrap();
    let mut observed = vec![];
    backend
        .queue
        .flush(|_, command| {
            observed.push(command.read_u32(0).unwrap());
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(observed, [0xcafe_babe, 1, 99]); // Parent equal-key FIFO, unsorted child order.
    assert_eq!(backend.services.queries, 1);
}

#[test]
fn material_pointer_sorting_requires_its_allocation_owner() {
    let (runtime, tables) = setup();
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let (_, state, mut services) = scene(&scratch);
    services.sorting = true;
    let mut queue = DrawQueue::new(1024).unwrap();
    let mut backend = ModelCommandBackend::new(
        RenderResources::new(&runtime),
        &tables,
        state,
        &mut queue,
        services,
    );
    let texture = backend.texture(1805).unwrap().unwrap();
    assert!(matches!(
        backend.texture_sort_identity(texture),
        Err(ModelCommandError::Service(
            "texture allocation identity unavailable"
        ))
    ));
    assert!(backend.stopped());
    assert_eq!(backend.queue.state().command_count, 0);
}

#[test]
fn negative_texture_is_null_but_negative_material_never_becomes_a_color() {
    let (runtime, tables) = setup();
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let (_, state, services) = scene(&scratch);
    let mut queue = DrawQueue::new(1024).unwrap();
    let mut backend = ModelCommandBackend::new(
        RenderResources::new(&runtime),
        &tables,
        state,
        &mut queue,
        services,
    );
    assert!(backend.texture(0x1234_ffff).unwrap().is_none());
    assert!(backend.texture(i32::from(i16::MAX)).unwrap().is_none());
    assert!(!backend.stopped());
    assert!(matches!(
        backend.material(-1),
        Err(ModelCommandError::Resource(
            RenderResourceError::NegativeIndex { .. }
        ))
    ));
    assert!(backend.stopped());
}

#[test]
fn native_one_past_texture_pool_remains_an_error() {
    let (runtime, tables) = setup();
    let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
    let (_, state, services) = scene(&scratch);
    let resources = RenderResources::new(&runtime);
    let capacity = resources.pool_capacity(RenderResourceKind::Sprite) as i32;
    let mut queue = DrawQueue::new(1024).unwrap();
    let mut backend = ModelCommandBackend::new(resources, &tables, state, &mut queue, services);
    assert!(backend.texture(capacity + 1).unwrap().is_none());
    assert!(matches!(
        backend.texture(capacity),
        Err(ModelCommandError::Resource(
            RenderResourceError::NotResident { .. }
        ))
    ));
    assert!(backend.stopped());
}
