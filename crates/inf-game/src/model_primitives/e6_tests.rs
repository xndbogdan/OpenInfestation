//! Focused `prepare_e6` tests: mirror/cull/gate/material/fallback/record shape.
//! Fixture and host follow `quad_effect_tests` (explicit slot bytes isolate
//! primitive behavior from projection); only E6-specific paths are pinned.

use std::path::Path;

use super::{
    prepare_e6, PrimitiveCallback, PrimitiveCommand, PrimitiveError, PrimitiveHost, PrimitiveState,
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
            // byte land at record+0x28..0x2B in that order.
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
/// Quad corners with clip bytes the real table accepts (0x12) and
/// distinct fog bytes; XY/depth land verbatim in the record.
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
fn rejects_non_quad_and_non_e6() {
    // 0xE2 decodes to Quad but is not this producer's opcode.
    let fixture = Fixture::new(&[0xe2, 1805, 0, 0, 1, 2, 3, 0], quad_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_e6(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
    // 0x16 is not a face at all.
    let fixture = Fixture::new(&[0x16, 8, 193, 0, 0], quad_points());
    let mut host = Host::default();
    assert!(matches!(
        prepare_e6(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
}

#[test]
fn near_submit_writes_effect_record_with_shade_color() {
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
    // E6 face: material 1805, shared shade 0, slots 0..3. Words are
    // [opcode, material, shade, v0, v1, v2, v3].
    let fixture = Fixture::new(&[0xe6, 1805, 0, 0, 1, 2, 3, 0], quad_points());
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
    let outcome = prepare_e6(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    assert_eq!(host.events[0], "texture:1805");
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::EffectQuadNear);
    assert_eq!(command.payload_blocks, 3);
    // Corners land verbatim (XY packed, depth after).
    assert_eq!(command.read_u32(0x00), Some(0x00dc_012c));
    assert_eq!(command.read_u32(0x04), Some(128));
    assert_eq!(command.read_u32(0x18), Some(0x0104_012c));
    assert_eq!(command.read_u32(0x1c), Some(128));
    //49E398 stores mode plain, without introducing a packet policy bit.
    assert_eq!(command.read_u32(0x20), Some(0));
    // Shared shade 0 resolves through the ambient word: rgb plus
    // high byte land at +0x28..0x2B in that order.
    assert_eq!(command.read_u32(0x28), Some(0xff80_8080));
}

#[test]
fn far_submit_carries_far_color_and_fog_bytes() {
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
    let fixture = Fixture::new(&[0xe6, 1805, 0, 0, 1, 2, 3, 0], quad_points());
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
    let outcome = prepare_e6(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::EffectQuadFar);
    assert_eq!(command.read_u32(0x28), Some(0xff80_8080));
    // Far color plus per-corner fog bytes follow the shade DWORD.
    assert_eq!(command.read_u32(0x2c), Some(state().far_color));
    assert_eq!(command.payload[0x30], Some(10));
    assert_eq!(command.payload[0x31], Some(20));
    assert_eq!(command.payload[0x32], Some(30));
    assert_eq!(command.payload[0x33], Some(40));
}

#[test]
fn mirror_bit_runs_both_passes_with_xored_slots() {
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
    // Bit 10 set: passes for mirror 1 then 0, with slots XORed.
    let fixture = Fixture::new(&[0x04e6, 1805, 0, 0, 1, 2, 3, 0], quad_points());
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
    let outcome = prepare_e6(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (2, 0));
    // Pass 1 uses slots 1..0^1: first corner is slot 1's XY.
    assert_eq!(host.commands[0].read_u32(0x00), Some(0x00dc_0154));
    // Pass 0 uses slots as authored: first corner is slot 0's XY.
    assert_eq!(host.commands[1].read_u32(0x00), Some(0x00dc_012c));
}

#[test]
fn clipped_fallback_delegates_to_required_native_owner() {
    // Clip bytes OR to 0xC0 with no common 0x40 bit: the native path
    // calls shared 0049A270. The nominal shade is NOT its literal four
    // scratch color inputs: this bounded fixture has no trailing source words.
    let points = [
        point(300, 220, 128, 0x40, Some(10)),
        point(340, 220, 128, 0x80, Some(20)),
        point(340, 260, 128, 0x00, Some(30)),
        point(300, 260, 128, 0x00, Some(40)),
    ]
    .into();
    let mut fixture = Fixture::new(&[0xe6, 1805, 0, 0, 1, 2, 3, 0], points);
    // Literal vertex3 WORD is a mapped scratch read before the missing words.
    fixture.shades = ModelShades::in_scratch(
        crate::model_lighting::ModelScratchAllocation::new(0, 4, 2).unwrap(),
        0xff80_8080,
    )
    .unwrap();
    let mut host = Host::default();
    assert!(matches!(
        prepare_e6(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
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
    // 0x01e6 sets bit 8 (cull enable); winding bit 9 clear with a
    // negative area fails the native area test.
    let fixture = Fixture::new(&[0x01e6, 1805, 0, 0, 1, 2, 3, 0], points);
    let mut host = Host::default();
    let outcome = prepare_e6(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (0, 1));
}

mod direct_native {
    use super::*;
    use crate::{
        model_lighting::{ModelLighting, ModelScratchAllocation},
        model_primitives::TextureSortIdentity,
        render_resources::{RenderResourceKind, RenderResourceRequest},
    };
    use inf_formats::{
        SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags, SpriteModeSelectors,
        SpriteVariant,
    };
    use serde_json::{json, Value};

    fn u32_field(spec: &Value, key: &str, fallback: u32) -> u32 {
        spec[key].as_u64().map(|v| v as u32).unwrap_or(fallback)
    }
    fn sprite(spec: &Value) -> SpriteEntry {
        let selectors = spec["selectors"]
            .as_array()
            .map(|a| [a[0].as_u64().unwrap() as u8, a[1].as_u64().unwrap() as u8])
            .unwrap_or([0, 1]);
        let flags = spec["texture_flags"]
            .as_array()
            .map(|a| [a[0].as_u64().unwrap() as u16, a[1].as_u64().unwrap() as u16])
            .unwrap_or([0, 8]);
        SpriteEntry {
            local_index: 0,
            span: ByteSpan::new(0, 0),
            selectors: SpriteModeSelectors {
                mode0: selectors[0],
                mode1: selectors[1],
                reserved: 0,
            },
            variants: (0..2)
                .map(|i| SpriteVariant {
                    physical_index: i,
                    span: ByteSpan::new(0, 0),
                    sprite_id: 0,
                    reserved: 0,
                    flags: SpriteFlags::new(flags[usize::from(i)]),
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
                })
                .collect(),
        }
    }
    const TEXTURE_REGISTRATION: ResidentRegistration = ResidentRegistration {
        level: 0,
        phase: RequestKind::Base,
        handler: 3,
        region_index: 0,
        local_index: 0,
        global_index: 10,
    };
    struct DirectHost<'a> {
        spec: &'a Value,
        entry: &'a SpriteEntry,
        events: Vec<Value>,
        commands: Vec<PrimitiveCommand<'a>>,
    }
    impl DirectHost<'_> {
        fn boundary(&self, name: &str) -> Result<(), &'static str> {
            if self.spec["fail"].as_str() == Some(name) {
                Err("required host failed")
            } else {
                Ok(())
            }
        }
    }
    impl ModelValueCallback for DirectHost<'_> {
        type Error = &'static str;
        fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
            unreachable!()
        }
    }
    impl<'a> PrimitiveHost<'a> for DirectHost<'a> {
        fn texture(&mut self, slot: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
            self.events.push(json!(["texture", slot]));
            self.boundary("texture")?;
            if slot < 0
                || slot as u32 > u32_field(self.spec, "capacity", 10)
                || self.spec["null_texture"].as_bool().unwrap_or(false)
            {
                return Ok(None);
            }
            Ok(Some(RenderResource {
                request: RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: slot,
                },
                source: RenderSource {
                    registration: &TEXTURE_REGISTRATION,
                    bytes: &[],
                },
                data: RenderResourceData::Sprite(self.entry),
            }))
        }
        fn material(&mut self, _: i32) -> Result<u32, Self::Error> {
            unreachable!()
        }
        fn material_sorting(&mut self) -> Result<bool, Self::Error> {
            self.events.push(json!(["sorting"]));
            self.boundary("sorting")?;
            Ok(self.spec["sorting"].as_bool().unwrap_or(false))
        }
        fn texture_sort_identity(
            &mut self,
            _: RenderResource<'a>,
        ) -> Result<TextureSortIdentity, Self::Error> {
            let selector = if u32_field(self.spec, "mode", 0) & 1 == 0 {
                self.entry.selectors.mode0
            } else {
                self.entry.selectors.mode1
            };
            self.events.push(json!(["identity", selector]));
            self.boundary("identity")?;
            Ok(TextureSortIdentity::NativeEntryAddress(0x72b000))
        }
        fn submit(&mut self, c: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
            self.events.push(json!(["submit"]));
            if self.spec["fail"].as_str() == Some("submit")
                && self.commands.len() == u32_field(self.spec, "fail_after", 0) as usize
            {
                return Err("required host failed");
            }
            self.commands.push(c);
            Ok(())
        }
    }
    fn make_fixture(spec: &Value) -> Fixture {
        let raw = u32_field(spec, "raw", 0xe6) as u16;
        let slots = spec["slots"]
            .as_array()
            .map(|a| std::array::from_fn::<_, 4, _>(|i| a[i].as_u64().unwrap() as u16))
            .unwrap_or([0, 1, 2, 3]);
        let points = spec["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let mut v = ModelVertexProjection::default();
                v.write_u32(0, p[0].as_u64().unwrap() as u32);
                v.write_u32(4, p[1].as_i64().unwrap() as u32);
                v.bytes[8] = Some(p[2].as_u64().unwrap() as u8);
                v.bytes[9] = Some(p[3].as_u64().unwrap() as u8);
                v
            })
            .collect();
        let mut f = Fixture::new(
            &[
                raw,
                spec["texture_slot"].as_i64().unwrap_or(10) as u16,
                u32_field(spec, "shade_slot", 0) as u16,
                slots[0],
                slots[1],
                slots[2],
                slots[3],
                0,
            ],
            points,
        );
        if u32_field(spec, "shade_slot", 0) == 2 {
            f.shades = ModelShades::new_invocation(4, 0xff808080).unwrap();
            if spec["name"].as_str().unwrap().ends_with("computed-shade") {
                // Real owned RGB writer, with explicit normal/light inputs.
                // Positive normal2 gives[23,41,75], mirrored normal3[7,9,11];
                // neither writes its high byte.
                let normal_start = f.bytes.len();
                f.bytes
                    .extend([4096_i16, 0, 0].into_iter().flat_map(i16::to_le_bytes));
                f.model.span = ByteSpan::new(0, f.bytes.len());
                f.model.content_span = f.model.span;
                f.model.normals_span = ByteSpan::new(normal_start, normal_start + 6);
                f.model.header.reserved_1a = 2;
                f.model.header.shade_ranges = [2, 4];
                f.shades
                    .initialize_normals(
                        &f.model,
                        &f.bytes,
                        &MenuBasis::INITIAL,
                        ModelLighting {
                            direction: [1., 0., 0.],
                            directional_word: 0x00402010,
                            ambient: [7, 9, 11],
                            ambient_word: 0xff808080,
                        },
                    )
                    .unwrap();
            }
        }
        f
    }

    #[test]
    fn direct_e6_matches_original_null_sorting_mirror_fog_and_packet_receipts() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/e6_native.json")).unwrap();
        let tables = tables();
        let mut count = 0;
        for spec in fixture["cases"].as_array().unwrap() {
            let f = make_fixture(spec);
            let entry = sprite(spec);
            let mut host = DirectHost {
                spec,
                entry: &entry,
                events: Vec::new(),
                commands: Vec::new(),
            };
            let mut state = state();
            state.mode = u32_field(spec, "mode", 0);
            state.far_color = u32_field(spec, "far_color", 0x12345678);
            let result = prepare_e6(
                &f.request(if spec["far"].as_bool().unwrap() {
                    ModelDispatch::Far
                } else {
                    ModelDispatch::Near
                }),
                state,
                &tables,
                &mut host,
            );
            assert_eq!(
                result.is_err(),
                !spec["error"].is_null(),
                "{} {result:?}",
                spec["name"]
            );
            assert_eq!(
                host.events,
                spec["events"].as_array().unwrap().to_vec(),
                "{}",
                spec["name"]
            );
            let packets = spec["packets"].as_array().unwrap();
            assert_eq!(host.commands.len(), packets.len(), "{}", spec["name"]);
            for (actual, expected) in host.commands.iter().zip(packets) {
                assert_eq!(
                    actual.callback.native_address(),
                    expected["callback"].as_u64().unwrap() as u32,
                    "{}",
                    spec["name"]
                );
                assert_eq!(
                    actual.depth,
                    expected["depth"].as_u64().unwrap() as u32,
                    "{}",
                    spec["name"]
                );
                assert_eq!(
                    json!(actual.header_words),
                    expected["header_words"],
                    "{}",
                    spec["name"]
                );
                assert_eq!(
                    actual.payload_blocks,
                    expected["blocks"].as_u64().unwrap() as usize,
                    "{}",
                    spec["name"]
                );
                assert_eq!(
                    json!(actual.payload),
                    expected["payload"],
                    "{}",
                    spec["name"]
                );
                assert_eq!(
                    actual.texture.is_some(),
                    expected["texture"].as_bool().unwrap(),
                    "{}",
                    spec["name"]
                );
            }
            count += 1;
        }
        assert_eq!(count, 152);
    }

    #[test]
    fn direct_e6_preserves_mapped_negative_shade_unknown_bytes() {
        let mut f = Fixture::new(&[0xe6, 0xffff, 0xffff, 0, 1, 2, 3, 0], quad_points());
        f.shades =
            ModelShades::in_scratch(ModelScratchAllocation::new(0, 4, 2).unwrap(), 0xff808080)
                .unwrap();
        let mut host = Host::default();
        let result = prepare_e6(
            &f.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host,
        )
        .unwrap();
        assert_eq!(result.commands, 1);
        assert_eq!(host.events, ["texture:-1", "sorting", "submit"]);
        let packet = &host.commands[0];
        assert_eq!(packet.read_u32(0x24), Some(0));
        assert_eq!(&packet.payload[0x28..0x2c], &[None; 4]);
        // An isolated invocation has no evidence for memory before its base.
        f.shades = ModelShades::new_invocation(2, 0xff808080).unwrap();
        assert!(matches!(
            prepare_e6(
                &f.request(ModelDispatch::Near),
                state(),
                &tables(),
                &mut Host::default()
            ),
            Err(PrimitiveError::Lighting(_))
        ));
    }
}
