//! Focused `prepare_e4` tests: record shape, material error, gates,
//! mirror/cull, fallback owners, far tail. Fixture and host follow
//! `de_tests`; only E4-specific paths (Handler-7 material DWORD,
//! flat sort key, far color/fog) are pinned.

use std::path::Path;

use super::{
    prepare_e4, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveState,
    PrimitiveTables,
};
use crate::{
    frontend_assets::FrontendAssets,
    menu::MenuBasis,
    model_context::{ModelContext, ModelDispatch, ModelProjection, ModelTransform},
    model_execution::ModelPrimitiveRequest,
    model_lighting::ModelShades,
    model_values::{ModelValueCallback, ModelValues},
    model_vertices::{
        ModelVertexInput, ModelVertexProjection, ModelVertexProjector, ModelVertices,
    },
    render_resources::{RenderResource, RenderSource},
    ResidentRegistration,
};
use inf_formats::{
    decode_model_program, ByteSpan, FrontendExecutable, ModelEntry, ModelFlags, ModelHeader,
    ModelIdentity, ModelVectorI16, RequestKind,
};
use std::convert::Infallible;

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
    material: Option<u32>,
    values: Vec<u16>,
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
    fn material(&mut self, argument: i32) -> Result<u32, Self::Error> {
        self.events.push(format!("material:{argument}"));
        self.material.ok_or("material failure")
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
        self.commands.push(command);
        Ok(())
    }
}
/// Quad corners with clip bytes the real table accepts (0x12).
fn quad_points() -> Vec<ModelVertexProjection> {
    [
        point(300, 220, 128, 0x12, Some(10)),
        point(340, 220, 128, 0x12, Some(20)),
        point(340, 260, 128, 0x12, Some(30)),
        point(300, 260, 128, 0x12, Some(40)),
    ]
    .into()
}

#[test]
fn rejects_non_quad_and_non_e4() {
    // 0xE6 decodes to Quad but is not E4.
    let fixture = Fixture::new(&[0xe6, 1805, 0, 0, 1, 2, 3, 0], quad_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_e4(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
    // 0xDE decodes to Triangle, not Quad.
    let fixture = Fixture::new(&[0xde, 1805, 0, 0, 1, 2, 0], quad_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_e4(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
}

#[test]
fn near_submit_writes_flat_shade_quad_record() {
    // E4 quad: material 1805, shared shade 0, slots 0..3. Words are
    // [opcode, material, shade, v0, v1, v2, v3].
    let fixture = Fixture::new(&[0xe4, 1805, 0, 0, 1, 2, 3, 0], quad_points());
    let mut host = Host {
        material: Some(0x1234_5678),
        ..Default::default()
    };
    let outcome = prepare_e4(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    // Material resolves before the sort gate, then the submit.
    assert_eq!(host.events, ["material:1805", "sorting", "submit"]);
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::SharedShadeQuadNear);
    assert_eq!(command.payload_blocks, 3);
    assert_eq!(command.read_u32(0x00), Some(0x00dc_012c));
    assert_eq!(command.read_u32(0x04), Some(128));
    assert_eq!(command.read_u32(0x10), Some(0x0104_0154));
    assert_eq!(command.read_u32(0x18), Some(0x0104_012c));
    assert_eq!(command.read_u32(0x1c), Some(128));
    // Mode stored plain; material is the raw Handler-7 DWORD.
    assert_eq!(command.read_u32(0x20), Some(0));
    assert_eq!(command.read_u32(0x24), Some(0x1234_5678));
    // Shared shade 0 resolves through ambient; high byte unwritten.
    assert_eq!(command.read_u32(0x28), Some(0xff80_8080));
}

#[test]
fn unresolvable_material_fails_closed() {
    // Retail dereferences the material pointer unconditionally (no
    // null skip like E6): the host error propagates per feature.
    let fixture = Fixture::new(&[0xe4, 1805, 0, 0, 1, 2, 3, 0], quad_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_e4(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Host(_))
    ));
    assert!(host.commands.is_empty());
}

#[test]
fn clipped_quads_run_the_shared_material_wrapper() {
    // Vertex 0 lies behind the near plane in view XY, so E4 calls
    // 00498060 / 00498570. Its five clipped corners queue the quad 0/1/2/3
    // and the triangle 0/3/4 (see the material clipping receipts).
    for (dispatch, quad, triangle) in [
        (
            ModelDispatch::Near,
            PrimitiveCallback::SharedShadeQuadNear,
            PrimitiveCallback::DcTriangleNear,
        ),
        (
            ModelDispatch::Far,
            PrimitiveCallback::SharedShadeQuadFar,
            PrimitiveCallback::DcTriangleFar,
        ),
    ] {
        let points = [
            point(-5, 5, 0, 0xc0, Some(255)),
            point(340, 220, 128, 0x12, Some(60)),
            point(340, 260, 128, 0x12, Some(103)),
            point(300, 260, 128, 0x12, Some(146)),
        ]
        .into();
        let mut fixture = Fixture::new(&[0xe4, 1805, 0, 0, 1, 2, 3, 0], points);
        // The literal WORDs at record +0A run past this one-instruction source.
        fixture
            .bytes
            .extend([5u16, 6].iter().flat_map(|word| word.to_le_bytes()));
        fixture.shades = ModelShades::from_words_for_tests(&[0xff80_8080; 8]);
        let mut host = Host {
            material: Some(0x1234_5678),
            ..Default::default()
        };
        let outcome =
            prepare_e4(&fixture.request(dispatch), state(), &tables(), &mut host).unwrap();
        assert_eq!((outcome.commands, outcome.rejected), (2, 0));
        let callbacks: Vec<_> = host
            .commands
            .iter()
            .map(|command| command.callback)
            .collect();
        assert_eq!(callbacks, [quad, triangle]);
    }
}

#[test]
fn clipped_quads_without_a_projected_behind_corner_queue_nothing() {
    // A 0x40 corner without 0x80 makes 00497250 abandon the polygon, after
    // the caller's literal shade reads, which fail closed past the source.
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let points = [
            point(300, 220, 128, 0x40, Some(10)),
            point(340, 220, 128, 0x80, Some(20)),
            point(340, 260, 128, 0x00, Some(30)),
            point(300, 260, 128, 0x00, Some(40)),
        ]
        .into();
        let mut fixture = Fixture::new(&[0xe4, 1805, 0, 0, 1, 2, 3, 0], points);
        fixture.shades = ModelShades::from_words_for_tests(&[0xff80_8080; 8]);
        let mut host = Host {
            material: Some(0),
            ..Default::default()
        };
        assert!(matches!(
            prepare_e4(&fixture.request(dispatch), state(), &tables(), &mut host),
            Err(PrimitiveError::Record(16))
        ));
        fixture
            .bytes
            .extend([5u16, 6].iter().flat_map(|word| word.to_le_bytes()));
        let outcome =
            prepare_e4(&fixture.request(dispatch), state(), &tables(), &mut host).unwrap();
        assert_eq!((outcome.commands, outcome.rejected), (0, 1));
        assert!(host.commands.is_empty());
    }
}

#[test]
fn far_submit_appends_far_color_and_fog() {
    let fixture = Fixture::new(&[0xe4, 1805, 0, 0, 1, 2, 3, 0], quad_points());
    let mut host = Host {
        material: Some(0x1234_5678),
        ..Default::default()
    };
    let outcome = prepare_e4(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::SharedShadeQuadFar);
    assert_eq!(command.payload_blocks, 4);
    assert_eq!(command.read_u32(0x24), Some(0x1234_5678));
    assert_eq!(command.read_u32(0x2c), Some(0xa1b2_c3d4));
    for (corner, fog) in [10u8, 20, 30, 40].into_iter().enumerate() {
        assert_eq!(command.payload[0x30 + corner], Some(fog));
    }
}

#[test]
fn all_fogged_far_skips() {
    let points = [
        point(300, 220, 128, 0x12, Some(255)),
        point(340, 220, 128, 0x12, Some(255)),
        point(340, 260, 128, 0x12, Some(255)),
        point(300, 260, 128, 0x12, Some(255)),
    ]
    .into();
    let fixture = Fixture::new(&[0xe4, 1805, 0, 0, 1, 2, 3, 0], points);
    let mut host = Host {
        material: Some(0),
        ..Default::default()
    };
    let outcome = prepare_e4(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (0, 1));
}
