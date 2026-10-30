use crate::{
    frontend_assets::FrontendAssets,
    menu::MenuBasis,
    model_context::{ModelContext, ModelDispatch, ModelProjection, ModelTransform},
    model_execution::ModelPrimitiveRequest,
    model_lighting::ModelShades,
    model_primitives::{effect::prepare_effect, quad::prepare_quad, *},
    model_values::{ModelValueCallback, ModelValues},
    model_vertices::{
        ModelVertexInput, ModelVertexProjection, ModelVertexProjector, ModelVertices,
    },
    render_resources::{
        RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest,
        RenderResources, RenderSource,
    },
    Installation, ResidencyRequest, ResidentRegistration, ResidentRuntime,
};
use inf_formats::{
    decode_model_program, ByteSpan, FrontendExecutable, ModelEntry, ModelFlags, ModelHeader,
    ModelIdentity, ModelVectorI16, RequestKind,
};
use std::{convert::Infallible, path::Path};

fn tables() -> PrimitiveTables {
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    PrimitiveTables::from_executable(FrontendExecutable::parse(assets.executable_bytes()).unwrap())
        .unwrap()
}
fn state() -> PrimitiveState {
    PrimitiveState {
        mode: 0,
        far_color: 0xa1b2_c3d4,
        effect_color: 0x8765_4321,
        projection: ModelProjection {
            focal: [64, 64],
            viewport: [640, 480],
            center: [320, 240],
            fade: [100, 200],
        },
    }
}
fn point(x: i16, y: i16, depth: u32, clip: u8, fog: Option<u8>) -> ModelVertexProjection {
    let mut p = ModelVertexProjection::default();
    p.write_u32(0, u32::from(x as u16) | (u32::from(y as u16) << 16));
    p.write_u32(4, depth);
    p.bytes[8] = Some(clip);
    p.bytes[9] = fog;
    p
}
struct Projector {
    points: Vec<ModelVertexProjection>,
    cursor: usize,
}
impl ModelVertexProjector for Projector {
    type Error = Infallible;
    fn project(
        &mut self,
        _: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Infallible> {
        *output = self.points[self.cursor];
        self.cursor += 1;
        Ok(output.read_u32(4).unwrap_or(0))
    }
}
struct Fixture {
    model: ModelEntry,
    vertices: ModelVertices,
    shades: ModelShades,
    context: ModelContext,
    registration: ResidentRegistration,
    bytes: Vec<u8>,
}
impl Fixture {
    fn new(words: &[u16], points: Vec<ModelVertexProjection>) -> Self {
        let span = ByteSpan { start: 0, end: 0 };
        let count = points.len() as u16;
        let bytes = words.iter().flat_map(|word| word.to_le_bytes()).collect();
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
                shade_slot_count: 2,
                raw_0e: 0,
                reserved_10: 0,
                vertex_ranges: [0, 0, count, count],
                reserved_1a: 0,
                shade_ranges: [0, 0],
            },
            auxiliary_span: span,
            auxiliary: vec![],
            auxiliary_program: None,
            vertices_span: span,
            vertices: vec![ModelVectorI16 { x: 0, y: 0, z: 0 }; usize::from(count / 2)],
            normals_span: span,
            normals: vec![],
            command_span: span,
            program: decode_model_program(words).unwrap(),
            name: None,
            padding: span,
        };
        let context = ModelContext::new([0; 3], MenuBasis::INITIAL);
        let mut vertices = ModelVertices::new_invocation(count);
        // Paired ranges write both slots. The fixture supplies explicit slot
        // bytes to isolate primitive behavior from already-tested projection.
        let mut projector = Projector { points, cursor: 0 };
        vertices
            .initialize(&model, &context, &mut projector)
            .unwrap();
        Self {
            model,
            vertices,
            shades: ModelShades::new_invocation(2, 0xff80_8080).unwrap(),
            context,
            registration: ResidentRegistration {
                handler: 8,
                global_index: 0,
                level: 47,
                region_index: 0,
                local_index: 0,
                phase: RequestKind::Base,
            },
            bytes,
        }
    }
    fn request(&self, dispatch: ModelDispatch) -> ModelPrimitiveRequest<'_> {
        ModelPrimitiveRequest {
            model: &self.model,
            source: RenderSource {
                registration: &self.registration,
                bytes: &self.bytes,
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
#[derive(Default)]
struct Host<'a> {
    events: Vec<String>,
    commands: Vec<PrimitiveCommand<'a>>,
    texture: Option<RenderResource<'a>>,
    values: Vec<u16>,
    fail_submit: bool,
}
impl ModelValueCallback for Host<'_> {
    type Error = &'static str;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        self.events.push(format!("value:{selector}"));
        values.write_register(0, 99).unwrap();
        if self.values.is_empty() {
            Err("value failure")
        } else {
            Ok(self.values.remove(0))
        }
    }
}
impl<'a> PrimitiveHost<'a> for Host<'a> {
    fn texture(&mut self, argument: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        self.events.push(format!("texture:{argument}"));
        Ok(self.texture)
    }
    fn material(&mut self, _: i32) -> Result<u32, Self::Error> {
        Err("unexpected material")
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.events.push("sorting".into());
        Ok(false)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        Err("unexpected address")
    }
    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.events.push("submit".into());
        if self.fail_submit {
            Err("submission failure")
        } else {
            self.commands.push(command);
            Ok(())
        }
    }
}
fn four() -> Vec<ModelVertexProjection> {
    [
        point(300, 220, 128, 0x12, Some(10)),
        point(340, 220, 128, 0x12, Some(20)),
        point(340, 260, 128, 0x12, Some(30)),
        point(300, 260, 128, 0x12, Some(40)),
    ]
    .into_iter()
    .flat_map(|p| [p, p])
    .collect()
}

#[test]
fn direct_quad_emits_exact_near_far_payloads_and_no_shade_lookup() {
    let fixture = Fixture::new(&[0xe2, 1805, 65535, 0, 2, 4, 6, 0], four());
    for (dispatch, callback) in [
        (ModelDispatch::Near, PrimitiveCallback::TexturedQuadNear),
        (ModelDispatch::Far, PrimitiveCallback::TexturedQuadFar),
    ] {
        let mut host = Host::default();
        let outcome =
            prepare_quad(&fixture.request(dispatch), state(), &tables(), &mut host).unwrap();
        assert_eq!(outcome.commands, 1);
        assert_eq!(host.events, ["texture:1805", "sorting", "submit"]);
        let command = &host.commands[0];
        assert_eq!(command.callback, callback);
        assert_eq!(command.payload_blocks, 3);
        assert_eq!(command.read_u32(0x20), Some(0));
        assert_eq!(command.read_u32(0x24), Some(0));
        assert_eq!(command.read_u32(0), Some(0x00dc_012c));
        assert_eq!(command.read_u32(0x1c), Some(128));
        if dispatch == ModelDispatch::Far {
            assert_eq!(command.read_u32(0x28), Some(state().far_color));
            assert_eq!(command.read_u32(0x2c), Some(0x281e_140a));
        } else {
            assert!(command.payload[0x28..].iter().all(Option::is_none));
        }
    }
}

#[test]
fn cull_rejection_precedes_unknown_far_fog_and_material() {
    let points = four()
        .into_iter()
        .map(|mut p| {
            p.bytes[9] = None;
            p
        })
        .collect();
    let fixture = Fixture::new(&[0x3e2, 1805, 0, 0, 2, 4, 6, 0], points);
    let mut host = Host::default();
    assert_eq!(
        prepare_quad(
            &fixture.request(ModelDispatch::Far),
            state(),
            &tables(),
            &mut host
        )
        .unwrap()
        .rejected,
        1
    );
    assert!(host.events.is_empty());
}

#[test]
fn all_far_fogged_quad_rejects_before_resource_lookup() {
    let points = four()
        .into_iter()
        .map(|mut p| {
            p.bytes[9] = Some(255);
            p
        })
        .collect();
    let fixture = Fixture::new(&[0xe2, 1805, 0, 0, 2, 4, 6, 0], points);
    let mut host = Host::default();
    assert_eq!(
        prepare_quad(
            &fixture.request(ModelDispatch::Far),
            state(),
            &tables(),
            &mut host
        )
        .unwrap()
        .rejected,
        1
    );
    assert!(host.events.is_empty());
}

#[test]
fn clipped_five_vertices_emit_quad_then_triangle_with_native_uv_and_fog_layouts() {
    let mut points = four();
    points[0] = point(0, 0, 0, 0xc0, None);
    let mut fixture = Fixture::new(&[0xe2, 1805, 0, 6, 2, 4, 0, 0], points);
    fixture.bytes.extend_from_slice(&[0; 4]);
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let mut host = Host::default();
        assert_eq!(
            prepare_quad(&fixture.request(dispatch), state(), &tables(), &mut host)
                .unwrap()
                .commands,
            2
        );
        assert_eq!(
            host.events,
            [
                "texture:1805",
                "sorting",
                "submit",
                "texture:1805",
                "sorting",
                "submit"
            ]
        );
        let quad = &host.commands[0];
        let triangle = &host.commands[1];
        assert_eq!((quad.payload_blocks, triangle.payload_blocks), (5, 4));
        assert_eq!(quad.read_u32(4), Some(64));
        assert_eq!(quad.read_u32(12), Some(128));
        assert_eq!(quad.read_u32(0x28), Some(0));
        assert_eq!(quad.read_u32(0x2c), Some(32255));
        assert_eq!(triangle.read_u32(0), quad.read_u32(0));
        assert_eq!(triangle.read_u32(8), quad.read_u32(24));
        if dispatch == ModelDispatch::Far {
            assert_eq!(quad.callback, PrimitiveCallback::TexturedUvQuadFar);
            assert_eq!(triangle.callback, PrimitiveCallback::TexturedUvTriangleFar);
            assert_eq!(quad.read_u32(0x48), Some(state().far_color));
            assert_eq!(quad.read_u32(0x4c), Some(0x1e14_2800));
            assert_eq!(triangle.read_u32(0x38), Some(state().far_color));
            assert_eq!(
                triangle.payload[0x3c..0x40],
                [Some(0), Some(30), Some(0), None]
            );
        } else {
            assert_eq!(quad.callback, PrimitiveCallback::TexturedUvQuadNear);
            assert_eq!(triangle.callback, PrimitiveCallback::TexturedUvTriangleNear);
            assert!(quad.payload[0x48..].iter().all(Option::is_none));
            assert!(triangle.payload[0x38..].iter().all(Option::is_none));
        }
    }
}

#[test]
fn clipped_allocated_unknown_shade_rgb_does_not_block_uv_only_commands() {
    let mut points = four();
    points[2] = point(0, 0, 0, 0xc0, None);
    let mut fixture = Fixture::new(&[0xe2, 1805, 0, 6, 0, 4, 2, 0], points);
    fixture.shades = ModelShades::new_invocation(4, 0x1234_5678).unwrap();
    fixture.bytes.extend_from_slice(&[0; 4]);
    let mut host = Host::default();
    assert_eq!(
        prepare_quad(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        )
        .unwrap()
        .commands,
        2
    );
}

#[test]
fn mixed_quad_requires_proven_scratch_mapping_for_outside_local_shades() {
    let mut points = four();
    points[0] = point(0, 0, 0, 0xc0, None);
    let fixture = Fixture::new(&[0xe2, 1805, 0, 0, 2, 4, 6, 0], points);
    let mut host = Host::default();
    assert!(matches!(
        prepare_quad(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Lighting(_))
    ));
    assert!(host.events.is_empty());
}

/// Original 0049DBE0 -> 00498B10 -> 00497250, with only queue allocation and
/// renderer prepare intercepted. Each retained case is invariant across 128
/// independent random assignments to the four literal shade DWORD reads.
#[test]
fn mapped_stale_shade_reads_preserve_native_uv_payloads_and_viewport_rejection() {
    #[derive(serde::Deserialize)]
    struct Draw {
        depth: u32,
        callback: u32,
        blocks: usize,
        payload: Vec<u8>,
    }
    #[derive(serde::Deserialize)]
    struct Case {
        geometry: [[i64; 3]; 4],
        draws: Vec<Draw>,
        read_offsets: Vec<[usize; 2]>,
    }
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../tests/fixtures/e2_scratch_native.json")).unwrap();
    assert_eq!(cases.len(), 2);
    for (case_index, case) in cases.into_iter().enumerate() {
        assert_eq!(case.read_offsets, [[40, 4], [904, 4], [7256, 4], [0, 4]]);
        let mut points = vec![point(0, 0, 0, 0xc0, None); 22];
        for (index, [xy, depth, clip]) in [0, 2, 12, 10].into_iter().zip(case.geometry) {
            points[index] = point(xy as i16, (xy >> 16) as i16, depth as u32, clip as u8, None);
        }
        let mut fixture = Fixture::new(
            &[0xe2, 1813, 0, 0, 2, 12, 10, 0xe2, 1814, 0, 2, 4, 14, 12, 0],
            points,
        );
        fixture.shades = ModelShades::in_scratch(
            crate::model_lighting::ModelScratchAllocation::new(0, 22, 2).unwrap(),
            0xab05_060a,
        )
        .unwrap();
        let mut state = state();
        state.projection.focal = [640, 640];
        let mut host = Host::default();
        prepare_quad(
            &fixture.request(ModelDispatch::Near),
            state,
            &tables(),
            &mut host,
        )
        .unwrap();
        assert_eq!(host.commands.len(), case.draws.len(), "case {case_index}");
        for (actual, expected) in host.commands.iter().zip(case.draws) {
            assert_eq!(
                (
                    actual.depth,
                    actual.callback.native_address(),
                    actual.payload_blocks
                ),
                (expected.depth, expected.callback, expected.blocks)
            );
            let texture_offset = if expected.blocks == 4 { 0x1c } else { 0x24 };
            for (offset, byte) in actual.payload.iter().enumerate() {
                if (texture_offset..texture_offset + 4).contains(&offset) {
                    continue;
                }
                if let Some(byte) = byte {
                    assert_eq!(
                        *byte, expected.payload[offset],
                        "case {case_index} byte{offset:#x}"
                    );
                }
            }
        }
    }
}

#[test]
fn effect_lookup_happens_before_clip_rejection_and_callbacks() {
    let fixture = Fixture::new(
        &[0x1b, 0, 1805, 128, 129, 0],
        vec![point(0, 0, 0, 0xc0, None); 2],
    );
    let table = tables();
    let mut values = ModelValues::new_invocation(table.sine_table.clone());
    let mut host = Host::default();
    assert_eq!(
        prepare_effect(
            &fixture.request(ModelDispatch::Near),
            state(),
            &table,
            &mut values,
            &mut host
        )
        .unwrap()
        .rejected,
        1
    );
    assert_eq!(host.events, ["texture:1805"]);
    assert_eq!(values.register(0).unwrap(), None);
}

#[test]
fn effect_callback_order_and_partial_value_mutation_survive_failure() {
    let fixture = Fixture::new(
        &[0x1b, 0, 1805, 128, 129, 0],
        vec![point(320, 240, 128, 0x12, None); 2],
    );
    let table = tables();
    let mut values = ModelValues::new_invocation(table.sine_table.clone());
    let mut host = Host {
        values: vec![8],
        ..Host::default()
    };
    assert!(matches!(
        prepare_effect(
            &fixture.request(ModelDispatch::Near),
            state(),
            &table,
            &mut values,
            &mut host
        ),
        Err(PrimitiveError::Value(_))
    ));
    assert_eq!(host.events, ["texture:1805", "value:0", "value:1"]);
    assert_eq!(values.register(0).unwrap(), Some(99));
}

#[test]
fn retail_effect_sprite_uses_first_dimensions_and_retains_full_color_and_far_fog() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let resource = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1805,
        })
        .unwrap();
    let RenderResourceData::Sprite(sprite) = resource.data else {
        unreachable!()
    };
    assert!(!sprite.variants.is_empty());
    let fixture = Fixture::new(
        &[0x1b, 0, 1805, 128, 129, 0],
        vec![point(320, 240, 256, 0x12, Some(99)); 2],
    );
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let table = tables();
        let mut values = ModelValues::new_invocation(table.sine_table.clone());
        let mut host = Host {
            texture: Some(resource),
            values: vec![16, 0],
            ..Host::default()
        };
        assert_eq!(
            prepare_effect(
                &fixture.request(dispatch),
                state(),
                &table,
                &mut values,
                &mut host
            )
            .unwrap()
            .commands,
            1
        );
        assert_eq!(
            host.events,
            ["texture:1805", "value:0", "value:1", "sorting", "submit"]
        );
        let command = &host.commands[0];
        assert_eq!(command.read_u32(0x28), Some(0x8765_4321));
        assert_eq!(command.texture.unwrap().0, 0x24);
        if dispatch == ModelDispatch::Far {
            assert_eq!(command.callback, PrimitiveCallback::EffectQuadFar);
            assert_eq!(command.read_u32(0x30), Some(0x6363_6363));
            assert!(command.payload[0x34..].iter().all(Option::is_none));
        } else {
            assert_eq!(command.callback, PrimitiveCallback::EffectQuadNear);
            assert!(command.payload[0x2c..].iter().all(Option::is_none));
        }
    }
}

/// The five-vertex straddle above driven through the concrete backend into the
/// deferred queue: resident material lookup, one sorting query per emission,
/// native UV payloads with borrowed texture relocations, signed queue order
/// and repeat-flush replay. This synthetic straddle supplies initialized shades
/// separately from the mapped, unknown arena bytes read by retail E2 records.
#[test]
fn clipped_straddle_through_backend_and_queue_preserves_uv_layouts_order_and_replay() {
    use crate::{
        draw_queue::DrawQueue,
        model_commands::{ModelCommandBackend, ModelCommandServices},
        model_execution::ModelExecutionHost,
    };

    struct Services {
        prepares: usize,
    }
    impl ModelValueCallback for Services {
        type Error = &'static str;
        fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
            Err("the quad path performs no packed-value callbacks")
        }
    }
    impl<'a> ModelCommandServices<'a> for Services {
        fn prepare_draw(
            &mut self,
            _: &mut DrawQueue<PrimitiveCommand<'a>>,
        ) -> Result<bool, Self::Error> {
            self.prepares += 1;
            // Explicit fixture renderer policy: no material batching, matching
            // the menu-path backend tests. Not a production graphics backend.
            Ok(false)
        }
        fn texture_sort_identity(
            &mut self,
            _: RenderResource<'a>,
        ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
            Err("unbatched fixture performs no pointer-based sorting")
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Queued {
        callback: PrimitiveCallback,
        depth: u32,
        blocks: usize,
        offset: u32,
        payload: Vec<Option<u8>>,
        texture: Option<usize>,
    }
    fn read(payload: &[Option<u8>], offset: usize) -> Option<u32> {
        let bytes: Option<Vec<u8>> = payload[offset..offset + 4].iter().copied().collect();
        bytes.map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let resources = RenderResources::new(&runtime);
    let table = tables();
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let mut points = four();
        points[0] = point(0, 0, 0, 0xc0, None);
        let mut fixture = Fixture::new(&[0xe2, 1805, 0, 6, 2, 4, 0, 0], points);
        fixture.bytes.extend_from_slice(&[0; 4]);
        let request = fixture.request(dispatch);
        let mut queue = DrawQueue::new(4096).unwrap();
        let mut backend = ModelCommandBackend::new(
            resources,
            &table,
            state(),
            &mut queue,
            Services { prepares: 0 },
        );
        backend
            .publish_transform(&request.context, request.transform)
            .unwrap();
        backend.select_projection(request.dispatch).unwrap();
        let mut values = ModelValues::new_invocation(table.sine_table.clone());
        backend.primitive(request, &mut values).unwrap();
        assert_eq!(
            (backend.outcome().commands, backend.outcome().rejected),
            (2, 0)
        );
        assert_eq!(backend.services.prepares, 2);
        assert!(!backend.stopped());
        assert_eq!(backend.queue.state().command_count, 2);
        let mut snapshots = Vec::new();
        let flushed = backend
            .queue
            .flush(|invocation, command: &mut PrimitiveCommand<'_>| {
                assert_eq!(command.payload.len(), command.payload_blocks * 16);
                assert_eq!(command.header_words, [0; 3]);
                let texture = command.texture.map(|(offset, texture)| {
                    assert!(command.payload[offset..offset + 4]
                        .iter()
                        .all(Option::is_none));
                    assert!(matches!(texture.data, RenderResourceData::Sprite(_)));
                    let resident = resources.resolve(texture.request).unwrap();
                    assert!(std::ptr::eq(
                        texture.source.registration,
                        resident.source.registration
                    ));
                    assert!(std::ptr::eq(texture.source.bytes, resident.source.bytes));
                    offset
                });
                snapshots.push(Queued {
                    callback: command.callback,
                    depth: command.depth,
                    blocks: command.payload_blocks,
                    offset: invocation.command_offset,
                    payload: command.payload.clone(),
                    texture,
                });
                Ok::<_, Infallible>(())
            })
            .unwrap();
        assert_eq!(flushed, snapshots.len());
        assert_eq!(snapshots.len(), 2);
        let (quad, triangle) = (&snapshots[0], &snapshots[1]);
        // Equal sort depths keep allocation order: quad before triangle.
        assert_eq!((quad.depth, triangle.depth), (64, 64));
        assert!(quad.offset < triangle.offset);
        assert_eq!((quad.blocks, triangle.blocks), (5, 4));
        assert_eq!(quad.texture, Some(0x24));
        assert_eq!(triangle.texture, Some(0x1c));
        assert_eq!(read(&quad.payload, 4), Some(64));
        assert_eq!(read(&quad.payload, 12), Some(128));
        assert_eq!(read(&quad.payload, 0x28), Some(0));
        assert_eq!(read(&quad.payload, 0x2c), Some(32255));
        assert_eq!(read(&triangle.payload, 0), read(&quad.payload, 0));
        assert_eq!(read(&triangle.payload, 8), read(&quad.payload, 24));
        if dispatch == ModelDispatch::Far {
            assert_eq!(quad.callback, PrimitiveCallback::TexturedUvQuadFar);
            assert_eq!(triangle.callback, PrimitiveCallback::TexturedUvTriangleFar);
            assert_eq!(read(&quad.payload, 0x48), Some(state().far_color));
            assert_eq!(read(&quad.payload, 0x4c), Some(0x1e14_2800));
            assert_eq!(read(&triangle.payload, 0x38), Some(state().far_color));
            assert_eq!(
                triangle.payload[0x3c..0x40],
                [Some(0), Some(30), Some(0), None]
            );
        } else {
            assert_eq!(quad.callback, PrimitiveCallback::TexturedUvQuadNear);
            assert_eq!(triangle.callback, PrimitiveCallback::TexturedUvTriangleNear);
            assert!(quad.payload[0x48..].iter().all(Option::is_none));
            assert!(triangle.payload[0x38..].iter().all(Option::is_none));
        }
        let replayed = backend
            .queue
            .flush(|invocation, command: &mut PrimitiveCommand<'_>| {
                snapshots.push(Queued {
                    callback: command.callback,
                    depth: command.depth,
                    blocks: command.payload_blocks,
                    offset: invocation.command_offset,
                    payload: command.payload.clone(),
                    texture: command.texture.map(|(offset, _)| offset),
                });
                Ok::<_, Infallible>(())
            })
            .unwrap();
        assert_eq!(replayed, 2);
        assert_eq!(snapshots[2..], snapshots[..2]);
    }
}
