use std::{path::Path, sync::Arc};

use inf_formats::{
    decode_model_program, ByteSpan, FrontendExecutable, ModelEntry, ModelFlags, ModelHeader,
    ModelIdentity, ModelSlotRef, ModelVectorI16, RequestKind,
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_scene::SceneView,
    menu::MenuBasis,
    model_context::{ModelContext, ModelProjection, ModelTransform},
    model_execution::{
        ModelDrawRequest, ModelEnvironment, ModelExecutionHost, ModelExecutionLimits, ModelExecutor,
    },
    model_lighting::{ModelLighting, ModelShades},
    model_values::{ModelValueCallback, ModelValues},
    model_vertices::{
        ModelVertexInput, ModelVertexProjection, ModelVertexProjector, ModelVertices,
    },
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
        RenderSource,
    },
    Installation, ResidencyRequest, ResidentRegistration, ResidentRuntime,
};

fn tables() -> PrimitiveTables {
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    PrimitiveTables::from_executable(FrontendExecutable::parse(assets.executable_bytes()).unwrap())
        .unwrap()
}

fn state() -> PrimitiveState {
    PrimitiveState {
        mode: 0,
        far_color: 0x1234_5678,
        effect_color: 0x8765_4321,
        projection: ModelProjection {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: [10000, 20000],
        },
    }
}

fn packed([x, y]: [i16; 2]) -> u32 {
    u32::from(x as u16) | (u32::from(y as u16) << 16)
}

struct FixtureProjector {
    points: Vec<[i16; 2]>,
    clips: Vec<u8>,
    omit_xy: bool,
    fog: bool,
    cursor: usize,
}

impl ModelVertexProjector for FixtureProjector {
    type Error = std::convert::Infallible;
    fn project(
        &mut self,
        _: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        let index = self.cursor;
        self.cursor += 1;
        let mut point = self.points[index / 2];
        if index & 1 != 0 {
            point[0] = point[0].wrapping_neg();
        }
        if !self.omit_xy {
            output.write_u32(0, packed(point));
        }
        output.bytes[8] = Some(self.clips[index / 2]);
        if self.fog {
            output.bytes[9] = Some((index / 2 * 10 + 10) as u8);
        }
        Ok((index as u32 / 2 + 1) * 100)
    }
}

struct Fixture {
    model: ModelEntry,
    vertices: ModelVertices,
    shades: ModelShades,
    context: ModelContext,
    registration: ResidentRegistration,
}

impl Fixture {
    fn new(flags: u16, points: &[[i16; 2]]) -> Self {
        Self::with_projection(flags, points, &vec![0x12; points.len()], false, true)
    }

    fn with_projection(
        flags: u16,
        points: &[[i16; 2]],
        clips: &[u8],
        omit_xy: bool,
        fog: bool,
    ) -> Self {
        let textured = flags & 8 != 0;
        let mut words = vec![1, flags, if textured { u16::MAX } else { 91 }];
        let factors: [[u8; 3]; 3] = [[128, 64, 255], [255, 128, 0], [1, 255, 128]];
        for index in 0..points.len() {
            words.extend([index as u16 * 2, 0]);
            if textured {
                words.extend([index as u16 * 12345, u16::MAX - index as u16 * 7777]);
            }
            let [r, g, b] = factors[index % 3];
            words.extend([
                u16::from(r) | (u16::from(g) << 8),
                u16::from(b) | if index + 1 == points.len() { 0x100 } else { 0 },
            ]);
        }
        words.push(0);
        let span = ByteSpan { start: 0, end: 0 };
        let count = points.len() as u16 * 2;
        let model = ModelEntry {
            identity: ModelIdentity {
                local_index: 0,
                global_index: 0,
            },
            span,
            content_span: span,
            header_span: span,
            header: ModelHeader {
                visibility_extent: 0,
                command_word_count: words.len() as u16,
                auxiliary_count: 0,
                flags: ModelFlags::from_raw(0),
                vertex_slot_count: count,
                raw_vertex_count: count / 2,
                shade_slot_count: 4,
                raw_0e: 0,
                reserved_10: 0,
                vertex_ranges: [0, 0, 0, count],
                reserved_1a: 0,
                shade_ranges: [0, 0],
            },
            auxiliary_span: span,
            auxiliary: vec![],
            auxiliary_program: None,
            vertices_span: span,
            vertices: vec![ModelVectorI16 { x: 0, y: 0, z: 0 }; points.len()],
            normals_span: span,
            normals: vec![],
            command_span: span,
            program: decode_model_program(&words).unwrap(),
            name: None,
            padding: span,
        };
        let context = ModelContext::new([0; 3], MenuBasis::INITIAL);
        let mut vertices = ModelVertices::new_invocation(count);
        vertices
            .initialize(
                &model,
                &context,
                &mut FixtureProjector {
                    points: points.to_vec(),
                    clips: clips.to_vec(),
                    omit_xy,
                    fog,
                    cursor: 0,
                },
            )
            .unwrap();
        Self {
            model,
            vertices,
            shades: ModelShades::new_invocation(4, 0xa732_64c8).unwrap(),
            context,
            registration: ResidentRegistration {
                level: 0,
                phase: RequestKind::Base,
                handler: 8,
                region_index: 0,
                local_index: 0,
                global_index: 0,
            },
        }
    }

    fn request(&self, dispatch: ModelDispatch) -> ModelPrimitiveRequest<'_> {
        ModelPrimitiveRequest {
            model: &self.model,
            source: RenderSource {
                registration: &self.registration,
                bytes: &[],
            },
            instruction: &self.model.program.instructions[0],
            context: &self.context,
            transform: ModelTransform {
                view_position: [0; 3],
                view_basis: MenuBasis::INITIAL,
            },
            dispatch,
            vertices: &self.vertices,
            shades: &self.shades,
            state_words: &[],
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Texture(i32),
    Material(i32),
    Sorting,
    Address,
    Submit(PrimitiveCallback),
}

struct Host<'a> {
    events: Vec<Event>,
    commands: Vec<PrimitiveCommand<'a>>,
    sorting: bool,
    fail_lookup: bool,
    fail_submit: Option<usize>,
    resources: Option<RenderResources<'a>>,
}

impl<'a> Host<'a> {
    fn fixture() -> Self {
        Self {
            events: vec![],
            commands: vec![],
            sorting: false,
            fail_lookup: false,
            fail_submit: None,
            resources: None,
        }
    }
}

impl ModelValueCallback for Host<'_> {
    type Error = &'static str;
    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
        Err("unexpected packed callback")
    }
}

impl<'a> PrimitiveHost<'a> for Host<'a> {
    fn texture(&mut self, argument: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        self.events.push(Event::Texture(argument));
        if self.fail_lookup {
            return Err("lookup failed");
        }
        if let Some(resources) = self.resources {
            return resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument,
                })
                .map(Some)
                .map_err(|_| "texture absent");
        }
        if argument == -1 {
            Ok(None)
        } else {
            Err("unexpected texture")
        }
    }
    fn material(&mut self, argument: i32) -> Result<u32, Self::Error> {
        self.events.push(Event::Material(argument));
        if self.fail_lookup {
            return Err("lookup failed");
        }
        if argument == 91 {
            Ok(0x4433_2211)
        } else {
            Err("unexpected material")
        }
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.events.push(Event::Sorting);
        Ok(self.sorting)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        self.events.push(Event::Address);
        Err("no runtime texture allocation in this recording fixture")
    }
    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.events.push(Event::Submit(command.callback));
        if self.fail_submit == Some(self.commands.len() + 1) {
            return Err("queue unavailable");
        }
        self.commands.push(command);
        Ok(())
    }
}

#[test]
fn textured_near_command_preserves_uv_rgb_unknown_bytes_and_native_null_texture() {
    let fixture = Fixture::new(0x0a, &[[0, 0], [10, 0], [0, 10]]);
    let mut host = Host::fixture();
    let result = prepare_fan(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        result,
        PrimitiveOutcome {
            commands: 1,
            rejected: 0
        }
    );
    assert_eq!(
        host.events,
        [
            Event::Texture(-1),
            Event::Sorting,
            Event::Submit(PrimitiveCallback::TexturedTriangleNear)
        ]
    );
    let command = &host.commands[0];
    assert_eq!((command.payload_blocks, command.depth), (5, 100));
    for (offset, value) in [
        (0, packed([0, 0])),
        (4, 100),
        (8, packed([10, 0])),
        (12, 200),
        (16, packed([0, 10])),
        (20, 300),
        (24, 0),
        (28, 0),
        (44, 0),
        (48, 65535),
        (52, 12345),
        (56, 57758),
        (60, 24690),
        (64, 49981),
    ] {
        assert_eq!(command.read_u32(offset), Some(value));
    }
    for (index, rgb) in [[200, 50, 99], [255, 100, 0], [1, 199, 50]]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            command.payload[0x20 + index * 4..0x23 + index * 4],
            rgb.map(Some)
        );
        assert_eq!(command.payload[0x23 + index * 4], None);
    }
    assert!(command.payload[68..].iter().all(Option::is_none));
    assert_eq!(command.header_words, [0; 3]);
    assert!(command.texture.is_none());
}

#[test]
fn all_four_callback_layouts_keep_exact_allocations_and_far_fog_bytes() {
    for (flags, dispatch, callback, blocks, far_offset) in [
        (
            0,
            ModelDispatch::Near,
            PrimitiveCallback::FlatTriangleNear,
            3,
            None,
        ),
        (
            0,
            ModelDispatch::Far,
            PrimitiveCallback::FlatTriangleFar,
            4,
            Some(44),
        ),
        (
            8,
            ModelDispatch::Near,
            PrimitiveCallback::TexturedTriangleNear,
            5,
            None,
        ),
        (
            8,
            ModelDispatch::Far,
            PrimitiveCallback::TexturedTriangleFar,
            5,
            Some(68),
        ),
    ] {
        let fixture = Fixture::new(flags, &[[0, 0], [10, 0], [0, 10]]);
        let mut host = Host::fixture();
        prepare_fan(&fixture.request(dispatch), state(), &tables(), &mut host).unwrap();
        let command = &host.commands[0];
        assert_eq!(
            (command.callback, command.payload_blocks),
            (callback, blocks)
        );
        if flags == 0 {
            assert_eq!(command.read_u32(28), Some(0x4433_2211));
        }
        let written_end = if let Some(offset) = far_offset {
            assert_eq!(command.read_u32(offset), Some(0x1234_5678));
            assert_eq!(
                command.payload[offset + 4..offset + 7],
                [Some(10), Some(20), Some(30)]
            );
            offset + 7
        } else if flags == 0 {
            44
        } else {
            68
        };
        assert!(command.payload[written_end..].iter().all(Option::is_none));
    }
}

#[test]
fn first_triangle_and_each_triangle_culling_have_distinct_termination_rules() {
    let points = [[0, 0], [10, 0], [0, 10], [10, 10], [20, 30]];
    for (flags, commands, rejected) in [(2, 3, 0), (4, 2, 1), (6, 1, 1)] {
        let fixture = Fixture::new(flags, &points);
        let mut host = Host::fixture();
        let result = prepare_fan(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host,
        )
        .unwrap();
        assert_eq!(result, PrimitiveOutcome { commands, rejected });
        assert_eq!(
            host.events
                .iter()
                .filter(|event| **event == Event::Sorting)
                .count(),
            commands
        );
    }
    let fixture = Fixture::new(3, &points); // Opposite first-face gate fails.
    let mut host = Host::fixture();
    assert_eq!(
        prepare_fan(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        )
        .unwrap(),
        PrimitiveOutcome {
            commands: 0,
            rejected: 1
        }
    );
    assert_eq!(host.events, [Event::Material(91)]);
}

#[test]
fn mirror_pass_precedes_regular_pass_and_uses_opposite_winding_gate() {
    let fixture = Fixture::new(0x12, &[[0, 0], [10, 0], [0, 10]]);
    let mut host = Host::fixture();
    let outcome = prepare_fan(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        PrimitiveOutcome {
            commands: 2,
            rejected: 0
        }
    );
    assert_eq!(host.commands[0].read_u32(8), Some(packed([-10, 0])));
    assert_eq!(host.commands[1].read_u32(8), Some(packed([10, 0])));
    assert_eq!(
        host.events
            .iter()
            .filter(|event| matches!(event, Event::Material(_)))
            .count(),
        1
    );
}

#[test]
fn fan_vertex_references_above_signed_word_range_remain_unsigned() {
    let mut fixture = Fixture::new(0x0a, &[[0, 0], [10, 0], [0, 10]]);
    let count = 32774;
    fixture.model.header.vertex_slot_count = count;
    fixture.model.header.raw_vertex_count = count / 2;
    fixture.model.header.vertex_ranges = [0, 0, 0, count];
    fixture
        .model
        .vertices
        .resize(usize::from(count / 2), ModelVectorI16 { x: 0, y: 0, z: 0 });
    let mut points = vec![[0, 0]; usize::from(count / 2)];
    points[16384..].copy_from_slice(&[[0, 0], [10, 0], [0, 10]]);
    fixture.vertices = ModelVertices::new_invocation(count);
    fixture
        .vertices
        .initialize(
            &fixture.model,
            &fixture.context,
            &mut FixtureProjector {
                points,
                clips: vec![0x12; usize::from(count / 2)],
                omit_xy: false,
                fog: false,
                cursor: 0,
            },
        )
        .unwrap();
    let ModelInstructionKind::TriangleFan(fan) = &mut fixture.model.program.instructions[0].kind
    else {
        panic!()
    };
    for (record, index) in fan.vertices.iter_mut().zip([32768, 32770, 32772]) {
        record.vertex = ModelSlotRef::from_raw(index);
    }
    let mut host = Host::fixture();
    let outcome = prepare_fan(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        PrimitiveOutcome {
            commands: 1,
            rejected: 0
        }
    );
    assert_eq!(host.commands[0].depth, 1_638_500);
    assert_eq!(host.commands[0].read_u32(8), Some(packed([10, 0])));
}

#[test]
fn cull_arithmetic_uses_signed_words_wrapping_products_and_strict_positive() {
    assert!(positive_area([[0, 0], [10, 0], [0, 10]].map(packed)));
    assert!(!positive_area([[0, 0], [0, 10], [10, 0]].map(packed)));
    assert!(!positive_area([[0, 0], [1, 1], [2, 2]].map(packed)));
    // Mathematical determinant 65535² is positive, native lowDWORD negative.
    assert!(!positive_area(
        [[-32768, -32768], [32767, -32768], [-32768, 32767]].map(packed)
    ));
}

#[test]
fn clip_lookup_rejects_without_reading_xy_and_keeps_crossing_triangles_unclipped() {
    let fixture = Fixture::with_projection(8, &[[0, 0], [10, 0], [0, 10]], &[0x09; 3], true, false);
    let mut host = Host::fixture();
    assert_eq!(
        prepare_fan(
            &fixture.request(ModelDispatch::Far),
            state(),
            &tables(),
            &mut host
        )
        .unwrap(),
        PrimitiveOutcome {
            commands: 0,
            rejected: 1
        }
    );
    assert_eq!(host.events, [Event::Texture(-1)]);
    let points = [[-20, 0], [700, 0], [100, 100]];
    let fixture = Fixture::with_projection(8, &points, &[0x11, 0x14, 0x12], false, true);
    let mut host = Host::fixture();
    prepare_fan(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(host.commands.len(), 1);
    for (index, point) in points.into_iter().enumerate() {
        assert_eq!(host.commands[0].read_u32(index * 8), Some(packed(point)));
    }
}

#[test]
fn material_and_shade_reads_precede_culling_and_failure_keeps_prior_commands() {
    let mut fixture = Fixture::new(3, &[[0, 0], [10, 0], [0, 10]]);
    let ModelInstructionKind::TriangleFan(fan) = &mut fixture.model.program.instructions[0].kind
    else {
        panic!()
    };
    fan.vertices[2].lighting_ref = ModelSlotRef::from_raw(2);
    let mut host = Host::fixture();
    assert!(matches!(
        prepare_fan(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Unknown {
            field: "shade RGB",
            index: 2
        })
    ));
    assert_eq!(host.events, [Event::Material(91)]);
    host = Host::fixture();
    host.fail_lookup = true;
    assert!(matches!(
        prepare_fan(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Host("lookup failed"))
    ));
    let fixture = Fixture::new(8, &[[0, 0], [10, 0], [0, 10], [10, 10]]);
    let mut host = Host::fixture();
    host.fail_submit = Some(2);
    assert!(matches!(
        prepare_fan(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Host("queue unavailable"))
    ));
    assert_eq!(host.commands.len(), 1);
    assert_eq!(host.events.len(), 5);
}

#[test]
fn far_command_requires_actual_fog_and_sort_query_still_occurs_for_null_texture() {
    let fixture =
        Fixture::with_projection(8, &[[0, 0], [10, 0], [0, 10]], &[0x12; 3], false, false);
    let mut host = Host::fixture();
    host.sorting = true;
    assert!(matches!(
        prepare_fan(
            &fixture.request(ModelDispatch::Far),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Unknown {
            field: "vertex fog byte",
            index: 0
        })
    ));
    assert_eq!(host.events, [Event::Texture(-1), Event::Sorting]);
    assert!(host.commands.is_empty());
}

struct CorpusHost<'a> {
    primitive: Host<'a>,
    state: PrimitiveState,
    tables: PrimitiveTables,
    prepared: PrimitiveOutcome,
    fans: usize,
    other_recorded: usize,
    groups: usize,
    publications: Vec<(ModelContext, ModelTransform)>,
    selections: Vec<ModelDispatch>,
}

impl ModelValueCallback for CorpusHost<'_> {
    type Error = &'static str;
    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        Ok(AnimationClock::default().callback_value(selector))
    }
}

impl ModelExecutionHost for CorpusHost<'_> {
    fn publish_transform(
        &mut self,
        context: &ModelContext,
        transform: ModelTransform,
    ) -> Result<(), Self::Error> {
        self.publications.push((*context, transform));
        Ok(())
    }
    fn select_projection(&mut self, dispatch: ModelDispatch) -> Result<(), Self::Error> {
        self.selections.push(dispatch);
        Ok(())
    }
    fn primitive(
        &mut self,
        request: ModelPrimitiveRequest<'_>,
        _: &mut ModelValues,
    ) -> Result<(), Self::Error> {
        if request.instruction.raw_command as u8 == 1 {
            self.fans += 1;
            let outcome = prepare_fan(&request, self.state, &self.tables, &mut self.primitive)
                .map_err(|_| "fan preparation failed")?;
            self.prepared.commands += outcome.commands;
            self.prepared.rejected += outcome.rejected;
        } else if matches!(request.instruction.raw_command as u8, 0xe2 | 0x1b) {
            self.other_recorded += 1;
        } else {
            return Err("unexpected non-fan primitive");
        }
        Ok(())
    }
    fn begin_depth_group(&mut self, _: i32, sorting: bool) -> Result<(), Self::Error> {
        assert!(!sorting);
        self.groups += 1;
        Ok(())
    }
    fn end_depth_group(&mut self) -> Result<(), Self::Error> {
        assert_eq!(self.groups, 1);
        self.groups -= 1;
        Ok(())
    }
}

#[test]
fn all_six_retail_fan_models_resolve_textures_and_prepare_actual_triangles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let resources = RenderResources::new(&runtime);
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let mut host = CorpusHost {
            primitive: Host {
                resources: Some(resources),
                ..Host::fixture()
            },
            state: state(),
            tables: tables(),
            prepared: PrimitiveOutcome::default(),
            fans: 0,
            other_recorded: 0,
            groups: 0,
            publications: vec![],
            selections: vec![],
        };
        let scratch = crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture();
        let mut executor = ModelExecutor::new(
            resources,
            Arc::new(assets.sine_table().values),
            ModelExecutionLimits {
                instructions: 100_000,
                invocation_depth: 16,
            },
        );
        let projection = ModelProjection {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: if dispatch == ModelDispatch::Near {
                [1_000_000, 2_000_000]
            } else {
                [0, 2_000_000]
            },
        };
        let environment = ModelEnvironment {
            view: SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            },
            projection,
            coordinate_shift: 0,
            fog_factor: 8,
            lighting: ModelLighting {
                direction: [0.0, 0.0, 1.0],
                directional_word: 0x00aa_d2ff,
                ambient: [10, 6, 5],
                ambient_word: 0xa705_060a,
            },
        };
        for id in [747, 748, 749, 754, 755, 756] {
            executor
                .draw(
                    ModelDrawRequest {
                        scratch: &scratch,
                        root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                        model_argument: id,
                        context: ModelContext::new([0, 0, 100_000], MenuBasis::INITIAL),
                        environment,
                        surface: crate::model_vertices::ModelSurfaceBinding::Default,
                    },
                    &mut host,
                )
                .unwrap();
        }
        assert_eq!(host.fans, 3168);
        assert_eq!(host.other_recorded, 16);
        assert_eq!(host.groups, 0);
        assert_eq!(host.publications.len(), 18);
        assert_eq!(host.selections.len(), 14);
        assert_eq!(
            host.prepared,
            PrimitiveOutcome {
                commands: 864,
                rejected: 2304
            }
        );
        assert_eq!(host.primitive.commands.len(), host.prepared.commands);
        for command in host.primitive.commands {
            assert_eq!(
                command.callback,
                if dispatch == ModelDispatch::Near {
                    PrimitiveCallback::TexturedTriangleNear
                } else {
                    PrimitiveCallback::TexturedTriangleFar
                }
            );
            let (offset, texture) = command
                .texture
                .expect("resident original texture relocation");
            assert_eq!(offset, 0x1c);
            assert!(matches!(texture.data, RenderResourceData::Sprite(_)));
            assert_eq!(command.payload[0x23], None);
            assert!(command.read_u32(0x2c).is_some());
        }
    }
}
