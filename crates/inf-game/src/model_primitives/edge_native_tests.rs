//! `prepare_edge` against the unchanged 00495CA0/004961A0 receipts: clip-40
//! and far double-FF skips, resident and lazy sprites, scales, the corner
//! classification and X window, sorting bands and every packet byte.

use std::path::Path;

use inf_formats::{
    decode_model_program, ByteSpan, FrontendExecutable, ModelEntry, ModelFlags, ModelHeader,
    ModelIdentity, ModelVectorI16, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry,
    SpriteFlags, SpriteModeSelectors, SpriteVariant,
};
use serde_json::{json, Value};

use super::*;
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
    render_resources::{
        RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderSource,
    },
    ResidentRegistration,
};

fn number(spec: &Value, key: &str, fallback: i64) -> i64 {
    spec[key].as_i64().unwrap_or(fallback)
}

fn pair(spec: &Value, key: &str, fallback: [i64; 2]) -> [i64; 2] {
    spec[key]
        .as_array()
        .map(|values| [values[0].as_i64().unwrap(), values[1].as_i64().unwrap()])
        .unwrap_or(fallback)
}

struct Projector {
    points: Vec<ModelVertexProjection>,
    cursor: usize,
}

impl ModelVertexProjector for Projector {
    type Error = std::convert::Infallible;

    fn project(
        &mut self,
        _: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        *output = self.points[self.cursor];
        self.cursor += 1;
        Ok(output.read_u32(4).unwrap_or(0))
    }
}

/// The receipt's sprite table: one resident or lazily loaded entry at the
/// receipt's native address, which also keys opaque sorting.
struct Host<'a> {
    spec: &'a Value,
    sprite: &'a SpriteEntry,
    registration: &'a ResidentRegistration,
    events: Vec<Value>,
    commands: Vec<PrimitiveCommand<'a>>,
}

impl ModelValueCallback for Host<'_> {
    type Error = &'static str;

    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
        unreachable!()
    }
}

impl<'a> PrimitiveHost<'a> for Host<'a> {
    fn texture(&mut self, argument: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        assert_eq!(i64::from(argument), number(self.spec, "material", 5));
        self.events.push(json!(["texture"]));
        if self.spec["lazy"].as_bool().unwrap_or(false) {
            self.events.push(json!(["lazy", argument]));
        }
        Ok(Some(RenderResource {
            request: RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument,
            },
            source: RenderSource {
                registration: self.registration,
                bytes: &[],
            },
            data: RenderResourceData::Sprite(self.sprite),
        }))
    }

    fn material(&mut self, _: i32) -> Result<u32, Self::Error> {
        unreachable!()
    }

    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.events.push(json!(["sorting"]));
        Ok(self.spec["sorting"].as_bool().unwrap_or(false))
    }

    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        Ok(
            crate::model_primitives::TextureSortIdentity::NativeEntryAddress(
                self.spec["sprite_address"].as_u64().unwrap() as u32,
            ),
        )
    }

    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.events.push(json!(["submit"]));
        self.commands.push(command);
        Ok(())
    }
}

/// Selectors at entry +00/+01; each variant's flags and dimensions as the
/// receipt wrote them at variant +04 and +10/+12.
fn sprite(spec: &Value) -> SpriteEntry {
    let [mode0, mode1] = pair(spec, "selectors", [0, 0]);
    let [width, height] = pair(spec, "dimensions", [8, 4]);
    let flags = pair(spec, "variant_flags", [0, 0]);
    SpriteEntry {
        local_index: 0,
        span: ByteSpan { start: 0, end: 0 },
        selectors: SpriteModeSelectors {
            mode0: mode0 as u8,
            mode1: mode1 as u8,
            reserved: 0,
        },
        variants: flags
            .iter()
            .map(|flags| SpriteVariant {
                physical_index: 0,
                span: ByteSpan { start: 0, end: 0 },
                sprite_id: 0,
                reserved: 0,
                flags: SpriteFlags::new(*flags as u16),
                color_count: 0,
                atlas_offset_bytes: 0,
                palette_offset: 0,
                width: width as u16,
                height: height as u16,
                runtime_slots: [0, 0],
                atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
                atlas_record_index: 0,
                encoding: SpriteEncoding::DirectRgb555 {
                    pixels: ByteSpan { start: 0, end: 0 },
                },
            })
            .collect(),
    }
}

fn sprite_registration() -> ResidentRegistration {
    ResidentRegistration {
        handler: 3,
        global_index: 0,
        level: 47,
        region_index: 0,
        local_index: 0,
        phase: RequestKind::Base,
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
    fn new(spec: &Value) -> Self {
        let [v0, v1] = pair(spec, "vertices", [0, 1]);
        let words = [
            0x07,
            number(spec, "material", 5) as u16,
            number(spec, "parameter", 2) as u16,
            v0 as u16,
            v1 as u16,
            0,
        ];
        let points: Vec<_> = spec["points"]
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
        let span = ByteSpan { start: 0, end: 0 };
        let count = points.len() as u16;
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
            program: decode_model_program(&words).unwrap(),
            name: None,
            padding: span,
        };
        let context = ModelContext::new([0; 3], MenuBasis::INITIAL);
        let mut vertices = ModelVertices::new_invocation(count);
        vertices
            .initialize(&model, &context, &mut Projector { points, cursor: 0 })
            .unwrap();
        Self {
            model,
            vertices,
            shades: ModelShades::new_invocation(2, 0xff80_8080).unwrap(),
            context,
            registration: sprite_registration(),
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

#[test]
fn edges_match_original_gates_scales_sorting_and_packets() {
    let receipt: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/edge_native.json")).unwrap();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut packets = 0;
    for spec in receipt["cases"].as_array().unwrap() {
        let name = spec["name"].as_str().unwrap();
        let fixture = Fixture::new(spec);
        let entry = sprite(spec);
        let registration = sprite_registration();
        let mut host = Host {
            spec,
            sprite: &entry,
            registration: &registration,
            events: Vec::new(),
            commands: Vec::new(),
        };
        let focal = pair(spec, "focal", [512, 512]).map(|v| v as i32);
        let viewport = pair(spec, "viewport", [1024, 768]).map(|v| v as i32);
        let state = PrimitiveState {
            mode: number(spec, "mode", 0) as u32,
            far_color: number(spec, "far_color", 0xa1b2_c3d4) as u32,
            effect_color: number(spec, "effect_color", 0x8765_4321) as u32,
            projection: ModelProjection {
                focal,
                viewport,
                center: viewport.map(|v| v / 2),
                fade: [100, 200],
            },
        };
        let far = spec["far"].as_bool().unwrap();
        let outcome = prepare_edge(
            &fixture.request(if far {
                ModelDispatch::Far
            } else {
                ModelDispatch::Near
            }),
            state,
            &tables,
            &mut host,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(&host.events, spec["events"].as_array().unwrap(), "{name}");
        let natives = spec["packets"].as_array().unwrap();
        assert_eq!(
            (outcome.commands, outcome.rejected),
            (natives.len(), 1 - natives.len()),
            "{name}"
        );
        for (actual, native) in host.commands.iter().zip(natives) {
            assert_eq!(
                u64::from(actual.callback.native_address()),
                native["callback"].as_u64().unwrap(),
                "{name}"
            );
            assert_eq!(
                u64::from(actual.depth),
                native["depth"].as_u64().unwrap(),
                "{name}"
            );
            assert_eq!(actual.header_words, [0; 3], "{name}");
            assert_eq!(
                actual.payload_blocks as u64,
                native["blocks"].as_u64().unwrap(),
                "{name}"
            );
            // Native stores the sprite entry pointer at +24; the port keeps
            // the same resource as a relocation there.
            assert_eq!(actual.texture.as_ref().map(|(at, _)| *at), Some(0x24));
            let mut expected = native["payload"].as_array().unwrap().clone();
            let pointer: Vec<_> = expected[0x24..0x28].iter().map(|b| b.as_u64()).collect();
            let address = spec["sprite_address"].as_u64().unwrap();
            assert_eq!(
                pointer,
                (0..4)
                    .map(|i| Some((address >> (8 * i)) & 0xff))
                    .collect::<Vec<_>>(),
                "{name}"
            );
            expected[0x24..0x28].fill(Value::Null);
            assert_eq!(json!(actual.payload), Value::Array(expected), "{name}");
            packets += 1;
        }
    }
    assert_eq!(
        (receipt["cases"].as_array().unwrap().len(), packets),
        (72, 55)
    );
}
