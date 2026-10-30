//! Focused `prepare_de` tests: mirror/cull/gate/material/fallback/record shape.
//! Fixture and host follow `quad_effect_tests` (explicit slot bytes isolate
//! primitive behavior from projection); only DE-specific paths are pinned.

use std::path::Path;

use super::{
    prepare_de, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveState,
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
    render_resources::{RenderResource, RenderResourceData, RenderSource},
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
            // Slots 0 and 1 share the ambient word: rgb bytes plus high
            // byte land at record+0x20..0x23 in that order.
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
/// Triangle corners with clip bytes the real table accepts (0x12).
/// Four slots back three face vertices (mirrored pairs exist for the
/// mirror-bit pass); XY/depth land verbatim in the record.
fn tri_points() -> Vec<ModelVertexProjection> {
    [
        point(300, 220, 128, 0x12, Some(10)),
        point(340, 220, 128, 0x12, Some(20)),
        point(340, 260, 128, 0x12, Some(30)),
        point(300, 260, 128, 0x12, Some(40)),
    ]
    .into()
}

#[test]
fn rejects_non_triangle_and_non_de() {
    // 0xE6 decodes to Quad, not Triangle.
    let fixture = Fixture::new(&[0xe6, 1805, 0, 0, 1, 2, 3, 0], tri_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_de(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
    // 0x1C is not a face at all.
    let fixture = Fixture::new(&[0x1c, 0, 0, 0], tri_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_de(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
}

#[test]
fn near_submit_writes_shared_shade_triangle_record() {
    use crate::render_resources::{RenderResourceKind, RenderResourceRequest};
    use inf_formats::{
        SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags, SpriteModeSelectors,
        SpriteVariant,
    };
    const REGISTRATION: ResidentRegistration = ResidentRegistration {
        level: 47,
        phase: RequestKind::Base,
        handler: 3,
        region_index: 0,
        local_index: 0,
        global_index: 10,
    };
    let entry = SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: 0,
            mode1: 0,
            reserved: 0,
        },
        variants: vec![SpriteVariant {
            physical_index: 0,
            span: ByteSpan::new(0, 0),
            sprite_id: 0,
            reserved: 0,
            flags: SpriteFlags::new(0),
            color_count: 0,
            atlas_offset_bytes: 0,
            palette_offset: 0,
            width: 1,
            height: 1,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan::new(0, 2),
            },
        }],
    };
    // DE face: material 1805, shared shade 0, slots 0..2. Words are
    // [opcode, material, shade, v0, v1, v2].
    let fixture = Fixture::new(&[0xde, 1805, 0, 0, 1, 2, 0], tri_points());
    let mut host = Host {
        texture: Some(RenderResource {
            request: RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: 1805,
            },
            source: RenderSource {
                registration: &REGISTRATION,
                bytes: &[],
            },
            data: RenderResourceData::Sprite(&entry),
        }),
        ..Default::default()
    };
    let outcome = prepare_de(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    assert_eq!(host.events[0], "texture:1805");
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::SharedShadeTriangleNear);
    assert_eq!(command.payload_blocks, 3);
    // Corners land verbatim (XY packed, depth after).
    assert_eq!(command.read_u32(0x00), Some(0x00dc_012c));
    assert_eq!(command.read_u32(0x04), Some(128));
    assert_eq!(command.read_u32(0x10), Some(0x0104_0154));
    assert_eq!(command.read_u32(0x14), Some(128));
    // Mode stored plain; shared shade 0 resolves through ambient.
    assert_eq!(command.read_u32(0x18), Some(0));
    assert_eq!(command.read_u32(0x20), Some(0xff80_8080));
}

#[test]
fn null_material_queries_sorting_and_queues_a_native_null_packet() {
    let fixture = Fixture::new(&[0xde, 1805, 0, 0, 1, 2, 0], tri_points());
    let mut host = Host::default();
    let outcome = prepare_de(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    assert_eq!(host.commands[0].read_u32(0x1c), Some(0));
    assert!(host.commands[0].texture.is_none());
}

#[test]
fn clipped_literal_reads_precede_an_early_geometry_abort() {
    // Clip bytes OR to 0xC0 with no common 0x40 bit: the native path
    // calls 0049A270. Its third literal shade WORD lies beyond this source.
    // Require it before the clipper may abandon40/no80 geometry.
    let points = [
        point(300, 220, 128, 0x40, Some(10)),
        point(340, 220, 128, 0x80, Some(20)),
        point(340, 260, 128, 0x00, Some(30)),
        point(300, 260, 128, 0x00, Some(40)),
    ]
    .into();
    let mut fixture = Fixture::new(&[0xde, 1805, 0, 0, 1, 2, 0], points);
    fixture.shades = ModelShades::in_scratch(
        crate::model_lighting::ModelScratchAllocation::new(0, 4, 2).unwrap(),
        0xff80_8080,
    )
    .unwrap();
    let mut host = Host::default();
    let result = prepare_de(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    );
    assert!(
        matches!(result, Err(PrimitiveError::Record(14))),
        "{result:?}"
    );
    assert!(host.events.is_empty());
    fixture.bytes.extend_from_slice(&0_u16.to_le_bytes());
    assert_eq!(
        prepare_de(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        )
        .unwrap()
        .commands,
        0
    );
    assert!(host.events.is_empty());
}

#[test]
fn signed_mapped_nominal_shade_retains_unknown_bytes_after_sorting() {
    let mut fixture = Fixture::new(&[0xde, 0xffff, 0xffff, 0, 1, 2, 0], tri_points());
    fixture.shades = ModelShades::in_scratch(
        crate::model_lighting::ModelScratchAllocation::new(0, 4, 2).unwrap(),
        0xff80_8080,
    )
    .unwrap();
    let mut host = Host::default();
    assert_eq!(
        prepare_de(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        )
        .unwrap()
        .commands,
        1
    );
    assert_eq!(host.events, ["texture:-1", "sorting", "submit"]);
    assert_eq!(host.commands[0].payload[0x20..0x24], [None; 4]);
    fixture.shades = ModelShades::new_invocation(2, 0xff80_8080).unwrap();
    let mut isolated_host = Host::default();
    assert!(matches!(
        prepare_de(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut isolated_host
        ),
        Err(PrimitiveError::Lighting(_))
    ));
    assert_eq!(isolated_host.events, ["texture:-1", "sorting"]);
}

#[test]
fn area_cull_rejects_without_fallback() {
    // mode&0x100 with negative signed area fails the cull; clip bytes
    // of zero match no fallback pattern, so the triangle is skipped.
    let points = [
        point(300, 220, 128, 0x00, Some(10)),
        point(300, 260, 128, 0x00, Some(20)),
        point(340, 260, 128, 0x00, Some(30)),
        point(340, 220, 128, 0x00, Some(40)),
    ]
    .into();
    // 0x01de sets bit 8 (cull enable); winding bit 9 clear with a
    // negative area fails the native area test.
    let fixture = Fixture::new(&[0x01de, 1805, 0, 0, 1, 2, 0], points);
    let mut host = Host::default();
    let outcome = prepare_de(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (0, 1));
}

#[path = "de_native_tests.rs"]
mod direct_native;
