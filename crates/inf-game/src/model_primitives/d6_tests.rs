//! Focused `prepare_d6` tests: per-vertex-shade quad record shape,
//! material-without-skip, gates, mirror/cull, clipped dispatch, far
//! tail. Fixture and host follow `de_tests`; only D6-specific paths
//! are pinned.

use std::path::Path;

use super::{
    prepare_d6, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveState,
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
fn rejects_non_d6() {
    // 0xE4 decodes to Quad with a shared shade but is not D6.
    let fixture = Fixture::new(&[0xe4, 1805, 0, 0, 1, 2, 3, 0], quad_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_d6(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
    // Per-vertex-lit 0xD4 quads fail the opcode check, not the shape.
    let fixture = Fixture::new(&[0xd4, 1805, 0, 1, 2, 3, 0, 1, 0, 1, 0], quad_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_d6(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
}

#[test]
fn near_submit_writes_per_vertex_shade_record() {
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
            width: 64,
            height: 1,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan::new(0, 2),
            },
        }],
    };
    // D6 face: material 1805, slots 0..3, per-vertex shades 0/1/0/1.
    // Words are [opcode, material, v0..v3, s0..s3, terminator].
    let fixture = Fixture::new(&[0xd6, 1805, 0, 1, 2, 3, 0, 1, 0, 1, 0], quad_points());
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
    let outcome = prepare_d6(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    assert_eq!(host.events[0], "texture:1805");
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::D6QuadNear);
    assert_eq!(command.callback.native_address(), 0x0044_ea60);
    assert_eq!(command.payload_blocks, 4);
    // Corners land verbatim (XY packed, depth after).
    assert_eq!(command.read_u32(0x00), Some(0x00dc_012c));
    assert_eq!(command.read_u32(0x04), Some(128));
    assert_eq!(command.read_u32(0x08), Some(0x00dc_0154));
    assert_eq!(command.read_u32(0x0c), Some(128));
    assert_eq!(command.read_u32(0x10), Some(0x0104_0154));
    assert_eq!(command.read_u32(0x14), Some(128));
    assert_eq!(command.read_u32(0x18), Some(0x0104_012c));
    assert_eq!(command.read_u32(0x1c), Some(128));
    // Mode stored plain; per-vertex shades resolve through ambient.
    assert_eq!(command.read_u32(0x20), Some(0));
    assert_eq!(command.read_u32(0x28), Some(0xff80_8080));
    assert_eq!(command.read_u32(0x2c), Some(0xff80_8080));
    assert_eq!(command.read_u32(0x30), Some(0xff80_8080));
    assert_eq!(command.read_u32(0x34), Some(0xff80_8080));
}

#[test]
fn null_material_still_submits() {
    // Native null H3 still sorts and queues its packet. The reached pixel
    // consumer owns required resource validation.
    let fixture = Fixture::new(&[0xd6, 1805, 0, 1, 2, 3, 0, 1, 0, 1, 0], quad_points());
    let mut host = Host::default();
    let outcome = prepare_d6(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    assert_eq!(host.commands.len(), 1);
}

#[test]
fn far_submit_appends_far_color_and_fog() {
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
            width: 64,
            height: 1,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan::new(0, 2),
            },
        }],
    };
    let fixture = Fixture::new(&[0xd6, 1805, 0, 1, 2, 3, 0, 1, 0, 1, 0], quad_points());
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
    let outcome = prepare_d6(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::D6QuadFar);
    assert_eq!(command.callback.native_address(), 0x0044_ea70);
    assert_eq!(command.payload_blocks, 4);
    assert_eq!(command.read_u32(0x38), Some(0xa1b2_c3d4));
    for (corner, fog) in [10u8, 20, 30, 40].into_iter().enumerate() {
        assert_eq!(command.payload[0x3c + corner], Some(fog));
    }
}

#[test]
fn mirror_bit_submits_both_passes() {
    // 0x04d6 sets bit 10 (mirror): passes 1 then 0 both submit.
    let fixture = Fixture::new(&[0x04d6, 1805, 0, 1, 2, 3, 0, 1, 0, 1, 0], quad_points());
    let mut host = Host::default();
    host.texture = None;
    let outcome = prepare_d6(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (2, 0));
}

#[test]
fn clipped_fallback_delegates_and_retains_native_abort() {
    // ORC0/AND40 clear reaches B070/B6F0. A behind40 without80 then
    // causes the actual clipper to abandon the polygon before H3/sorting.
    let points = [
        point(300, 220, 128, 0x40, Some(10)),
        point(340, 220, 128, 0x80, Some(20)),
        point(340, 260, 128, 0x00, Some(30)),
        point(300, 260, 128, 0x00, Some(40)),
    ]
    .into();
    let fixture = Fixture::new(&[0xd6, 1805, 0, 1, 2, 3, 0, 1, 0, 1, 0], points);
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let mut host = Host::default();
        let outcome =
            prepare_d6(&fixture.request(dispatch), state(), &tables(), &mut host).unwrap();
        assert_eq!((outcome.commands, outcome.rejected), (0, 1));
        assert!(host.events.is_empty());
    }
}

#[test]
fn area_cull_rejects_without_fallback() {
    // mode&0x100 with negative signed area fails the cull; clip bytes
    // of zero match no fallback pattern, so the quad is skipped.
    let points = [
        point(300, 220, 128, 0x00, Some(10)),
        point(300, 260, 128, 0x00, Some(20)),
        point(340, 260, 128, 0x00, Some(30)),
        point(340, 220, 128, 0x00, Some(40)),
    ]
    .into();
    // 0x01d6 sets bit 8 (cull enable); winding bit 9 clear with a
    // negative area fails the native area test.
    let fixture = Fixture::new(&[0x01d6, 1805, 0, 1, 2, 3, 0, 1, 0, 1, 0], points);
    let mut host = Host::default();
    let outcome = prepare_d6(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (0, 1));
}

#[path = "d6_direct_tests.rs"]
mod direct_native;
