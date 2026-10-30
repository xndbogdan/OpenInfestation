//! Focused `prepare_da` tests: record shape (no shade), material,
//! gates, mirror/cull, bounded clipping literals, far tail. Fixture and host
//! follow `de_tests`; only DA-specific paths are pinned.

use std::path::Path;

use super::{
    prepare_da, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveState,
    PrimitiveTables,
};
use crate::{
    frontend_assets::FrontendAssets,
    menu::MenuBasis,
    model_context::{ModelContext, ModelDispatch, ModelProjection, ModelTransform},
    model_execution::ModelPrimitiveRequest,
    model_lighting::{ModelScratchAllocation, ModelShades},
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
            shades: ModelShades::in_scratch(
                ModelScratchAllocation::new(0, count, 2).unwrap(),
                0xff80_8080,
            )
            .unwrap(),
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
/// Triangle corners with clip bytes the real table accepts (0x12).
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
fn rejects_non_da() {
    // 0xDE decodes to Triangle with shared shade but is not DA.
    let fixture = Fixture::new(&[0xde, 1805, 0, 0, 1, 2, 0], tri_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_da(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
    // Per-vertex-lit triangles fail the Shared shape check.
    let fixture = Fixture::new(&[0xd0, 1805, 0, 1, 2, 0, 1, 2, 0], tri_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_da(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
}

#[test]
fn near_submit_writes_shadeless_triangle_record() {
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
    // DA face: material 1805, shade word 7 (unread), slots 0..2.
    let fixture = Fixture::new(&[0xda, 1805, 7, 0, 1, 2, 0], tri_points());
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
    let outcome = prepare_da(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    assert_eq!(host.events[0], "texture:1805");
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::DaTriangleNear);
    assert_eq!(command.payload_blocks, 2);
    assert_eq!(command.payload.len(), 32);
    assert_eq!(command.read_u32(0x00), Some(0x00dc_012c));
    assert_eq!(command.read_u32(0x04), Some(128));
    assert_eq!(command.read_u32(0x10), Some(0x0104_0154));
    assert_eq!(command.read_u32(0x14), Some(128));
    assert_eq!(command.read_u32(0x18), Some(0));
    // Material relocation at +0x1C, like E6/DE.
    assert_eq!(command.texture.map(|(offset, _)| offset), Some(0x1c));
    // No shade: the record ends at the material word.
    assert_eq!(command.read_u32(0x20), None);
}

#[test]
fn null_material_still_sorts_and_queues() {
    let fixture = Fixture::new(&[0xda, 1805, 0, 0, 1, 2, 0], tri_points());
    let mut host = Host::default();
    let outcome = prepare_da(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    assert_eq!(host.events, ["texture:1805", "sorting", "submit"]);
    assert_eq!(host.commands[0].read_u32(0x1c), Some(0));
    assert!(host.commands[0].texture.is_none());
}

#[test]
fn clipped_literals_cross_record_and_precede_abort() {
    let points = vec![
        point(0, 0, 0, 0x40, Some(255)),
        point(340, 220, 128, 0x92, Some(20)),
        point(340, 260, 128, 0x12, Some(30)),
        point(300, 260, 128, 0x12, Some(40)),
    ];
    let mut fixture = Fixture::new(&[0xda, 1805, 0xffff, 0, 1, 2, 0], points);
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let mut host = Host::default();
        assert!(matches!(
            prepare_da(&fixture.request(dispatch), state(), &tables(), &mut host),
            Err(PrimitiveError::Record(14))
        ));
        assert!(host.events.is_empty());
    }
    // Supply the third mandatory literal after the nominal record. Its signed
    // -1 refers to mapped stale scratch, not a declared local shade or zero.
    fixture.bytes.extend_from_slice(&0xffff_u16.to_le_bytes());
    for dispatch in [ModelDispatch::Near, ModelDispatch::Far] {
        let mut host = Host::default();
        let outcome =
            prepare_da(&fixture.request(dispatch), state(), &tables(), &mut host).unwrap();
        assert_eq!((outcome.commands, outcome.rejected), (0, 1));
        assert!(host.events.is_empty());
    }
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
    let fixture = Fixture::new(&[0xda, 1805, 0, 0, 1, 2, 0], tri_points());
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
    let outcome = prepare_da(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::DaTriangleFar);
    assert_eq!(command.payload_blocks, 3);
    assert_eq!(command.payload.len(), 48);
    assert_eq!(command.read_u32(0x20), Some(0xa1b2_c3d4));
    for (corner, fog) in [10u8, 20, 30].into_iter().enumerate() {
        assert_eq!(command.payload[0x24 + corner], Some(fog));
    }
}

#[path = "da_native_tests.rs"]
mod native;
