use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_display::frontend_startup,
    frontend_scene::FrontendSceneHost,
    model_execution::ModelExecutionError,
    model_primitives::PrimitiveTables,
    model_values::{ModelValueError, ModelValueFault},
    render_resources::RenderResources,
    scene_resources::{ResidentSceneHost, SceneResourceError},
    software_frame::SoftwareFrame,
    software_scene::{SoftwareScene, SoftwareSceneError, SoftwareSceneRequest},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};
use inf_formats::FrontendExecutable;
use sha2::{Digest, Sha256};
use std::path::Path;

fn executable() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("required retail Infest.exe")
}

fn inputs() -> (LoadingModelTemplate, LoadingProcessState, MenuBasisMath) {
    let bytes = executable();
    let template = LoadingModelTemplate::native_startup(&bytes).unwrap();
    let state = LoadingProcessState::native_startup(&bytes).unwrap();
    let assets = FrontendAssets::from_executable(bytes).unwrap();
    (
        template,
        state,
        MenuBasisMath::new(assets.sine_table().values),
    )
}

#[derive(Clone, Debug)]
enum Event {
    Scene(SceneCommand),
    Width,
    Height,
    Projection(LoadingModelProjection),
}

struct Host {
    events: Vec<Event>,
    width: i32,
    heights: [i32; 2],
    height_reads: usize,
    fail: Option<usize>,
}

impl Host {
    fn supplied(width: i32, heights: [i32; 2]) -> Self {
        Self {
            events: Vec::new(),
            width,
            heights,
            height_reads: 0,
            fail: None,
        }
    }
    fn reached(&mut self, event: Event) -> Result<(), usize> {
        let index = self.events.len();
        self.events.push(event);
        if self.fail == Some(index) {
            Err(index)
        } else {
            Ok(())
        }
    }
    fn model(&self) -> SceneModelSubmission {
        let Event::Scene(SceneCommand::Model(model)) = self.events.last().unwrap() else {
            panic!("model submission")
        };
        *model
    }
}

impl LoadingModelHost for Host {
    type Error = usize;
    fn submit_loading_scene(&mut self, command: SceneCommand) -> Result<(), usize> {
        self.reached(Event::Scene(command))
    }
    fn surface_width(&mut self) -> Result<i32, usize> {
        self.reached(Event::Width)?;
        Ok(self.width)
    }
    fn surface_height(&mut self) -> Result<i32, usize> {
        self.reached(Event::Height)?;
        let value = self.heights[self.height_reads];
        self.height_reads += 1;
        Ok(value)
    }
    fn publish_loading_projection(
        &mut self,
        projection: LoadingModelProjection,
    ) -> Result<(), usize> {
        self.reached(Event::Projection(projection))
    }
}

#[test]
fn startup_preserves_raw_parser_boundary_and_checks_required_template_operands() {
    let bytes = executable();
    let template = LoadingModelTemplate::native_startup(&bytes).unwrap();
    assert_eq!(
        template.view(),
        SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL
        }
    );
    assert_eq!(template.model_basis, MenuBasis::INITIAL);
    assert_eq!(template.fade(), SceneFade::new([0x10_0000; 2]));
    assert_eq!((template.far_color(), template.mode()), (0, 0));
    // Virtual-zero constructor backing must not become authored file bytes.
    assert!(FrontendExecutable::parse(&bytes)
        .unwrap()
        .mapped(0x0054_74ec, 12)
        .is_err());

    let pe = word_at(&bytes, 0x3c).unwrap() as usize;
    let first = pe + 24 + half_at(&bytes, pe + 20).unwrap() as usize;
    let count = half_at(&bytes, pe + 6).unwrap() as usize;
    let data = (0..count)
        .map(|i| first + i * 40)
        .find(|&at| bytes.get(at..at + 8) == Some(b".data\0\0\0".as_slice()))
        .unwrap();
    let mut changed = bytes.clone();
    changed[data + 8..data + 12].copy_from_slice(&0x19000u32.to_le_bytes());
    assert_eq!(
        LoadingModelTemplate::native_startup(&changed),
        Err(LoadingModelStartupError::UnsupportedLoaderProfile)
    );
    for address in [0x0043_9637, 0x0043_9767, 0x0043_9939, 0x0043_994b] {
        let mut changed = bytes.clone();
        changed[(address - 0x0040_0000) as usize] ^= 1;
        assert_eq!(
            LoadingModelTemplate::native_startup(&changed),
            Err(LoadingModelStartupError::Initializer { address })
        );
    }
    let mut changed = bytes.clone();
    changed[0x120494..0x120498].copy_from_slice(&0x0043_a270u32.to_le_bytes());
    assert_eq!(
        LoadingModelTemplate::native_startup(&changed),
        Err(LoadingModelStartupError::Initializer {
            address: 0x0052_0494
        })
    );
}

#[test]
fn real_table_background_orders_light_fresh_dimensions_projection_fade_view_model() {
    let (template, mut state, math) = inputs();
    state.age_micros = 200;
    state.selector = 0x4321;
    state.pulse = 0xbeef_1234;
    let before = state.clone();
    let mut host = Host::supplied(641, [480, 640]);
    draw_loading_model_40e7cb(
        LoadingModelRequest {
            state: &state,
            template: &template,
            focal: [513, 777],
            math: &math,
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(host.events.len(), 9);
    assert!(matches!(
        host.events[0],
        Event::Scene(SceneCommand::DirectionalLight {
            rgb: [128, 128, 128],
            ..
        })
    ));
    assert!(matches!(
        host.events[1],
        Event::Scene(SceneCommand::AmbientLight([32, 32, 32]))
    ));
    assert!(matches!(host.events[2], Event::Width));
    assert!(matches!(host.events[3], Event::Height));
    assert!(matches!(host.events[4], Event::Height));
    assert!(matches!(
        host.events[5],
        Event::Projection(LoadingModelProjection {
            focal: [513, 777],
            viewport: [641, 480],
            center: [320, 200]
        })
    ));
    assert!(
        matches!(host.events[6], Event::Scene(SceneCommand::PublishFade(value))
        if value == template.fade())
    );
    assert!(
        matches!(host.events[7], Event::Scene(SceneCommand::PublishView(value))
        if value == template.view())
    );
    let model = host.model();
    assert_eq!(model.model_slot, 117);
    assert_eq!(model.position, [0, 0, 500]);
    assert_eq!(
        model.surface_table,
        ModelSurfaceTableOrigin::TemplateDefault
    );
    assert_eq!(
        model.primitive_context,
        ModelPrimitiveContext::Caller {
            far_color: 0,
            mode: 0
        }
    );
    assert_eq!(
        model.policy,
        ModelCallbackPolicy::Loading {
            age_div_200: 1,
            selector: 0x4321,
            pulse: 0x1234
        }
    );
    // age/32=6 masks only its low two bits at491543/578: sine index1,
    // cosine index4094. The authored sine WORD12 is not angle-zero identity.
    // Derive the stored identity-row coefficients directly from those samples
    // and the original FMUL m32 scale, without calling the rotation under test.
    let bytes = executable();
    let scale = f64::from(f32::from_bits(word_at(&bytes, 0x128ce0).unwrap()));
    let sine = (f64::from(half_at(&bytes, 0x13df02).unwrap()) * scale) as f32;
    let cosine = (f64::from(half_at(&bytes, 0x13df00 + 2 * 4094).unwrap()) * scale) as f32;
    let mut expected = MenuBasis::INITIAL;
    expected.words[0] = 1;
    expected.words[1] = cosine.to_bits();
    expected.words[3] = (-sine).to_bits();
    expected.words[7] = sine.to_bits();
    expected.words[9] = cosine.to_bits();
    assert_eq!(model.basis, expected);
    assert_eq!(state, before);
}

#[test]
fn callback_consumes_captured_signed_age_word_and_distinct_selector_pulse() {
    let (template, mut state, math) = inputs();
    state.age_micros = (-201i32) as u32;
    state.selector = 0x8765;
    state.pulse = 0xfeed_ba98;
    let mut host = Host::supplied(640, [480; 2]);
    draw_loading_model_40e7cb(
        LoadingModelRequest {
            state: &state,
            template: &template,
            focal: [512; 2],
            math: &math,
        },
        &mut host,
    )
    .unwrap();
    let mut model = host.model();
    assert_eq!(
        model.failure_policy,
        ModelFailurePolicy::LoadingModel117StackHistoryApproximation
    );
    // This unrelated clock must not enter selector0 or selector3.
    model.animation.advance(4_000_000);
    let cases = [
        (0, 0xffff),
        (1, 0x8765),
        (2, 0xba98),
        (3, 0xff9c),
        (4, 0),
        (5, 0),
        (6, 1),
        (7, 0),
        (8, 0),
        (0xffff_ffff, 0),
    ];
    for (selector, expected) in cases {
        assert_eq!(
            model.callback_value(selector),
            Some(expected),
            "selector {selector:#x}"
        );
    }
    // Full quotient high bits are discarded before the *100 low-WORD result.
    model.policy = ModelCallbackPolicy::Loading {
        age_div_200: 0x1234_8001,
        selector: 2,
        pulse: 3,
    };
    assert_eq!(model.callback_value(0), Some(0x8001));
    assert_eq!(model.callback_value(3), Some(100));
}

#[test]
fn signed_wrapping_center_and_age_are_independent_native_operands() {
    let (template, mut state, math) = inputs();
    let cases = [(-3, -1, -1), (640, i32::MIN, 0), (641, i32::MAX, -1)];
    for (width, fresh_height, expected_y) in cases {
        state.age_micros = 0x8000_0000;
        let mut host = Host::supplied(width, [99, fresh_height]);
        draw_loading_model_40e7cb(
            LoadingModelRequest {
                state: &state,
                template: &template,
                focal: [17, -19],
                math: &math,
            },
            &mut host,
        )
        .unwrap();
        assert!(
            matches!(host.events[5], Event::Projection(LoadingModelProjection {
            focal: [17, -19], viewport: [actual_width, 99], center: [x, y]
        }) if actual_width == width && x == width / 2 && y == expected_y)
        );
        let model = host.model();
        assert_eq!(
            model.policy,
            ModelCallbackPolicy::Loading {
                age_div_200: -10_737_418,
                selector: state.selector,
                pulse: state.pulse as u16
            }
        );
        let mut expected = MenuBasis::INITIAL;
        math.rotate_in_plane(&mut expected, -67_108_864, RotationPlane::Xz);
        assert_eq!(model.basis, expected);
    }
}

#[test]
fn retained_template_inputs_are_not_replaced_with_renderer_view_or_clock_defaults() {
    let (mut template, state, math) = inputs();
    // Declared complete supplied current-template domain; this test does not
    // assert that a retail writer produced these values.
    template.view.eye = [100, -200, 300];
    template.view.basis.words[0] = 9;
    template.fade = SceneFade::new([1000, 9000]);
    template.far_color = 0xdead_beef;
    template.mode = 1;
    let before = template;
    let mut host = Host::supplied(320, [240; 2]);
    draw_loading_model_40e7cb(
        LoadingModelRequest {
            state: &state,
            template: &template,
            focal: [123, 456],
            math: &math,
        },
        &mut host,
    )
    .unwrap();
    assert!(
        matches!(host.events[6], Event::Scene(SceneCommand::PublishFade(fade))
        if fade == before.fade)
    );
    assert!(
        matches!(host.events[7], Event::Scene(SceneCommand::PublishView(view))
        if view == before.view)
    );
    assert_eq!(
        host.model().primitive_context,
        ModelPrimitiveContext::Caller {
            far_color: 0xdead_beef,
            mode: 1
        }
    );
    assert_eq!(template, before);
}

#[test]
fn required_errors_preserve_each_reached_prefix_and_all_borrowed_scalar_custody() {
    let (template, state, math) = inputs();
    let before = state.clone();
    for fail in 0..9 {
        let mut host = Host::supplied(640, [480; 2]);
        host.fail = Some(fail);
        assert_eq!(
            draw_loading_model_40e7cb(
                LoadingModelRequest {
                    state: &state,
                    template: &template,
                    focal: [512; 2],
                    math: &math,
                },
                &mut host
            ),
            Err(fail)
        );
        assert_eq!(host.events.len(), fail + 1);
        // No hidden flush, begin, coordinate shift, reset or retry. The
        // enclosing host retains actual queued commands/pixels on model error.
        assert!(host.events.iter().all(|event| !matches!(
            event,
            Event::Scene(
                SceneCommand::FlushBatch
                    | SceneCommand::BeginBatch
                    | SceneCommand::CoordinateShift(_)
                    | SceneCommand::ResetAfterFirstModel
            )
        )));
        assert_eq!(state, before);
    }
}

#[test]
fn retail_model_queues_and_flushes_pixels_with_invocation_local_primitive_context() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("required retail frontend executable");
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    assert!(runtime.resource(8, 117).is_none());
    // The first real world-loading activation loads parent1 before leaf48.
    // Its model117 remains the current source during ordinary mode0 drawing;
    // requesting frontend47 again would unload parent1 and lose that source.
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    assert_eq!(runtime.resource(8, 117).unwrap().registration.level, 1);
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let template = LoadingModelTemplate::native_startup(assets.executable_bytes()).unwrap();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let mut state = LoadingProcessState::native_startup(assets.executable_bytes()).unwrap();
    // Declared current callback operands exercise a visible pulse. These are
    // supplied scalar-domain inputs, not captured values or a writer claim.
    state.age_micros = 800_000;
    state.selector = 1;
    state.pulse = 0x8000;
    let before = state.clone();
    let resources = RenderResources::new(&runtime);
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            (startup.scene).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut frame,
        config: startup.scene,
    })
    .unwrap();
    scene.primitive_state.mode = 0xa5a5_0001;
    scene.primitive_state.far_color = 0xdead_beef;
    let inherited = (scene.primitive_state.mode, scene.primitive_state.far_color);
    // The real scene owns this supplied current RNG word if H8 reaches jitter.
    scene.set_process_rng(Some(0x1357_9bdf));
    {
        let mut host = PixelHost {
            scene: &mut scene,
            resources,
            policy_override: None,
        };
        host.submit_loading_scene(SceneCommand::BeginBatch).unwrap();
        draw_loading_model_40e7cb(
            LoadingModelRequest {
                state: &state,
                template: &template,
                focal: startup.projection.focal,
                math: &math,
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(host.scene.stats.models, 1);
        assert!(host.scene.stats.model_commands > 0);
        assert!(host.scene.queue.state().command_count > 0);
        assert_eq!(host.scene.pixels.stats.fragments, 0);
        host.submit_loading_scene(SceneCommand::FlushBatch).unwrap();
    }
    assert!(scene.queue.state().last_flush_completed > 0);
    assert!(scene.pixels.stats.fragments > 0);
    assert_eq!(scene.pixels.stats.unsupported, 0);
    assert_eq!(
        (scene.primitive_state.mode, scene.primitive_state.far_color),
        inherited
    );
    assert_eq!(state, before);
    let rgba = scene.pixels.frame.rgba();
    assert!(rgba
        .chunks_exact(4)
        .any(|pixel| pixel != startup.clear_rgba));
    // Optional diagnostic output is an unversioned test artifact. It does not
    // establish original-game frame or complete primitive acceptance.
    if let Some(path) = std::env::var_os("INF_LOADING_MODEL_OUTPUT") {
        let mut encoder = png::Encoder::new(
            std::io::BufWriter::new(std::fs::File::create(path).unwrap()),
            640,
            480,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(rgba).unwrap();
        writer.finish().unwrap();
    }
}

#[test]
fn retail_selector_two_preserves_unknown_register_prefix_without_a_seed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("required retail frontend executable");
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    assert_eq!(runtime.resource(8, 117).unwrap().registration.level, 1);
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let template = LoadingModelTemplate::native_startup(assets.executable_bytes()).unwrap();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let resources = RenderResources::new(&runtime);
    // Original H8/117 selector2 reaches119, then131 at command+154 whenever
    // pulse!=FFFF. Child131 queues four edges, then opcode03+40 reads unknown
    // inherited register3. The age operand does not guard this first read.
    for age in [0, 800_000] {
        for (policy, nested) in [
            (ModelFailurePolicy::StopScene, false),
            (
                ModelFailurePolicy::UninitializedValuePrefixApproximation,
                false,
            ),
            (
                ModelFailurePolicy::UninitializedValuePrefixApproximation,
                true,
            ),
        ] {
            let mut state = LoadingProcessState::native_startup(assets.executable_bytes()).unwrap();
            state.age_micros = age;
            state.selector = 2;
            state.pulse = 0;
            let before = state.clone();
            let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
            let mut scene = SoftwareScene::new(SoftwareSceneRequest {
                model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                    crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                    (startup.scene).environment,
                ),
                resources,
                tables: &tables,
                frame: &mut frame,
                config: startup.scene,
            })
            .unwrap();
            scene.primitive_state.mode = 0xa5a5_0001;
            scene.primitive_state.far_color = 0xdead_beef;
            let inherited = (scene.primitive_state.mode, scene.primitive_state.far_color);
            let rng = Some(0x1357_9bdf);
            scene.set_process_rng(rng);
            let mut host = PixelHost {
                scene: &mut scene,
                resources,
                policy_override: Some(policy),
            };
            host.submit_loading_scene(SceneCommand::BeginBatch).unwrap();
            if nested {
                // A genuine supplied outer queue group excludes this model
                // from the approximation's unchanged implicit-root domain.
                host.scene
                    .queue
                    .begin_group(17, true, |_| Ok::<(), std::convert::Infallible>(()))
                    .unwrap();
            }
            let result = draw_loading_model_40e7cb(
                LoadingModelRequest {
                    state: &state,
                    template: &template,
                    focal: startup.projection.focal,
                    math: &math,
                },
                &mut host,
            );
            assert_eq!(host.scene.stats.models, 0);
            assert_eq!(host.scene.stats.model_commands, 4);
            let queue_nodes = 4 + usize::from(nested);
            assert_eq!(host.scene.queue.state().command_count, queue_nodes);
            assert_eq!(host.scene.queue.has_only_implicit_root(), !nested);
            assert_eq!(
                host.scene.queue.state().open_groups,
                1 + usize::from(nested)
            );
            assert_eq!(host.scene.process_rng(), rng);
            assert_eq!(
                (
                    host.scene.primitive_state.mode,
                    host.scene.primitive_state.far_color
                ),
                inherited
            );
            assert_eq!(state, before);
            assert_eq!(host.scene.pixels.stats.fragments, 0);
            match (policy, nested) {
                (ModelFailurePolicy::StopScene, _)
                | (ModelFailurePolicy::UninitializedValuePrefixApproximation, true) => {
                    assert!(matches!(
                        result,
                        Err(SceneResourceError::Backend(SoftwareSceneError::Model(
                            ModelExecutionError::Value(ModelValueError::Fault(
                                ModelValueFault::UninitializedRegister { index: 3 }
                            ))
                        )))
                    ));
                    assert_eq!(host.scene.stats.model_value_prefix_skips, 0);
                    assert!(host.submit_loading_scene(SceneCommand::FlushBatch).is_err());
                    assert_eq!(host.scene.queue.state().command_count, queue_nodes);
                    assert_eq!(host.scene.pixels.stats.fragments, 0);
                }
                (ModelFailurePolicy::UninitializedValuePrefixApproximation, false) => {
                    result.unwrap();
                    assert_eq!(host.scene.stats.model_value_prefix_skips, 1);
                    host.submit_loading_scene(SceneCommand::FlushBatch).unwrap();
                    assert_eq!(host.scene.queue.state().last_flush_completed, 4);
                    assert!(host.scene.pixels.stats.fragments > 0);
                    assert_eq!(host.scene.pixels.stats.unsupported, 0);
                    assert!(host.scene.queue.is_fully_replayed());
                    assert_eq!(host.scene.process_rng(), rng);
                }
                (
                    ModelFailurePolicy::LoadingModel117StackHistoryApproximation
                    | ModelFailurePolicy::ObjectStackHistoryApproximations
                    | ModelFailurePolicy::ShopPreviewStackHistoryApproximation
                    | ModelFailurePolicy::BriefingPreviewMapFrameBases,
                    _,
                ) => {
                    unreachable!("the loop draws no seeded policy")
                }
            }
        }
    }
}

#[test]
fn retail_selector_two_starts_register_three_from_infest01_stack_history() {
    // LoadingModel117StackHistoryApproximation: child 131's opcode 03 at +40
    // reads the root's register 3, which Infest01's post-intro loading
    // screen held at 6EBB in 64 of 68 reads. Seeded, the whole model draws.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("required retail frontend executable");
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let template = LoadingModelTemplate::native_startup(assets.executable_bytes()).unwrap();
    let math = MenuBasisMath::new(assets.sine_table().values);
    let resources = RenderResources::new(&runtime);
    for age in [0, 800_000] {
        let mut state = LoadingProcessState::native_startup(assets.executable_bytes()).unwrap();
        state.age_micros = age;
        state.selector = 2;
        state.pulse = 0;
        let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        scene.set_process_rng(Some(0x1357_9bdf));
        let mut host = PixelHost {
            scene: &mut scene,
            resources,
            policy_override: None,
        };
        host.submit_loading_scene(SceneCommand::BeginBatch).unwrap();
        draw_loading_model_40e7cb(
            LoadingModelRequest {
                state: &state,
                template: &template,
                focal: startup.projection.focal,
                math: &math,
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(host.scene.stats.models, 1, "age {age}");
        assert_eq!(host.scene.stats.model_value_prefix_skips, 0);
        assert!(host.scene.stats.model_commands > 4);
        host.submit_loading_scene(SceneCommand::FlushBatch).unwrap();
        assert!(host.scene.queue.is_fully_replayed());
        assert!(host.scene.pixels.stats.fragments > 0);
        assert_eq!(host.scene.pixels.stats.unsupported, 0);
    }
}

struct PixelHost<'scene, 'frame, 'resources> {
    scene: &'scene mut SoftwareScene<'frame, 'resources>,
    resources: RenderResources<'resources>,
    policy_override: Option<ModelFailurePolicy>,
}

impl LoadingModelHost for PixelHost<'_, '_, '_> {
    type Error = SceneResourceError<SoftwareSceneError>;
    fn submit_loading_scene(&mut self, mut command: SceneCommand) -> Result<(), Self::Error> {
        if let SceneCommand::Model(model) = &mut command {
            if let Some(policy) = self.policy_override {
                model.failure_policy = policy;
            }
        }
        ResidentSceneHost {
            resources: self.resources,
            backend: self.scene,
        }
        .submit(command)
    }
    fn surface_width(&mut self) -> Result<i32, Self::Error> {
        Ok(self.scene.pixels.frame.width() as i32)
    }
    fn surface_height(&mut self) -> Result<i32, Self::Error> {
        Ok(self.scene.pixels.frame.height() as i32)
    }
    fn publish_loading_projection(
        &mut self,
        projection: LoadingModelProjection,
    ) -> Result<(), Self::Error> {
        self.scene.environment.projection.focal = projection.focal;
        self.scene.environment.projection.viewport = projection.viewport;
        self.scene.environment.projection.center = projection.center;
        self.scene.primitive_state.projection.focal = projection.focal;
        self.scene.primitive_state.projection.viewport = projection.viewport;
        self.scene.primitive_state.projection.center = projection.center;
        Ok(())
    }
}

#[test]
fn complete_original_body_guards_and_loading_callback_table_are_authentic() {
    let bytes = executable();
    let spans = [
        (
            0x0040_e6b0usize,
            0x0040_ed4dusize,
            "a9e087ff6e4f2d39b0f752e66442521abd8168ff9f997f44a6de544492cc8a89",
        ),
        (
            0x0040_d6a0,
            0x0040_d7af,
            "89cfb37d18878148bae7fcc7dbf0456cd5abd80517c28a42bf2dbde929426ea0",
        ),
        (
            0x0040_e7cb,
            0x0040_e970,
            "f8acd0241f3c1b89d8670a70349c068d41af6583d2878a76c9e29fd89e4c6dc4",
        ),
        (
            0x0040_e0d0,
            0x0040_e112,
            "1e4c12ebfd514dd056663aaa994d4830ac092a5edb8cb3ad1c7835185b45ce62",
        ),
        (
            0x0043_9630,
            0x0043_9743,
            "65d325d6e3c975b4a765ebaa6d07ea081a1bb2d000d393bea1c605c88c340c6b",
        ),
        (
            0x0043_9760,
            0x0043_9873,
            "02ee5e7ce0761658b8ea859396497a7e55fe35f06b16da129b7931b4c62f04be",
        ),
        (
            0x0043_9880,
            0x0043_9981,
            "1a2acdc748ebaa314b0c4006d3aa2f13cac0096ce5b4df12d3c0e6f08258b1c0",
        ),
        (
            0x0043_a150,
            0x0043_a26f,
            "0ff902edab34845c60c64a655cf6b5deeb91dfdf595339c39746f0278111d3b9",
        ),
        (
            0x0040_d1b0,
            0x0040_d1c9,
            "92782247fa786f1cbc0e78e43c76ba006a4daac83765e2ad52f6c71d41b9cea3",
        ),
        (
            0x0043_b4a0,
            0x0043_b5ca,
            "02860a116fb37b406d24262635a16bc4d86e5d147aa983c149520a7c1ceff6bc",
        ),
        (
            0x0043_b5d0,
            0x0043_b67a,
            "ae6fdb6e179b9aac9d835962f20d7ea9c79eb3afc0e954057f03ef156bb94a40",
        ),
        (
            0x0049_fea0,
            0x0049_feeb,
            "13a2f81229236a8da3a9c0446f38a45d1dd37b069053f2a1693132d01c6ff67f",
        ),
        (
            0x0049_fef0,
            0x004a_00e0,
            "3999652a8b38bc97f0041cf803ec2e533d94831eabe8121c9caa13ecb84e7141",
        ),
        (
            0x004a_00e0,
            0x004a_0239,
            "18ce68e4b32e26da6882e6866c0c2bb7412e12ab2faf49940a5ff55bfd025a02",
        ),
    ];
    for (begin, end, expected) in spans {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[begin - 0x0040_0000..end - 0x0040_0000])
            ),
            expected
        );
    }
    let source = FrontendExecutable::parse(&bytes).unwrap();
    // The canonical parser already maps file-backed .rdata as well as .data;
    // this vtable must retain its original physical source, not BSS backing.
    assert_eq!(
        source.mapped(0x0052_0494, 8).unwrap().file_span,
        ByteSpan::new(0x120494, 0x12049c)
    );
    assert_eq!(source.word(0x0052_0494).unwrap(), 0x0040_e0d0);
    assert_eq!(source.word(0x0052_0498).unwrap(), 0x0043_a2d0);
    let expected = [
        0x0040_e0e0u32,
        0x0040_e0f7,
        0x0040_e0fe,
        0x0040_e0e7,
        0x0040_e10c,
        0x0040_e10c,
        0x0040_e105,
        0x0040_e10c,
    ];
    for (index, address) in expected.into_iter().enumerate() {
        assert_eq!(word_at(&bytes, 0xe114 + 4 * index).unwrap(), address);
    }
}
