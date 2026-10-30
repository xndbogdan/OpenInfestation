//! `prepare_d8` and `prepare_e0` against the unchanged 0049BE10/0049C020 and
//! 0049D110/0049D380 receipts: mirror, cull, clip and Far-fog gates, resident
//! and lazy materials, sorting keys, the clipped fallbacks' packets and every
//! packet byte.

use std::path::Path;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    menu::MenuBasis,
    model_context::{ModelContext, ModelProjection, ModelTransform},
    model_lighting::ModelShades,
    model_primitives::e0::prepare_e0,
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
use serde_json::{json, Value};
use std::convert::Infallible;

fn number(spec: &Value, key: &str, fallback: u64) -> u64 {
    spec[key].as_u64().unwrap_or(fallback)
}

fn words(spec: &Value, key: &str, fallback: &[u16]) -> Vec<u16> {
    spec[key]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_u64().unwrap() as u16).collect())
        .unwrap_or_else(|| fallback.to_vec())
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

/// The receipt's material table: resident or lazily loaded entries.
struct Host<'a> {
    spec: &'a Value,
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
    fn texture(&mut self, _: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        unreachable!()
    }

    fn material(&mut self, slot: i32) -> Result<u32, Self::Error> {
        assert_eq!(slot, number(self.spec, "material_slot", 10) as i32);
        if self.spec["lazy"].as_bool().unwrap_or(false) {
            self.events.push(json!(["lazy", slot]));
            Ok(number(self.spec, "lazy_material", 0x0bad_f00d) as u32)
        } else {
            Ok(number(self.spec, "material", 0x7a5b_c3d4) as u32)
        }
    }

    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.events.push(json!(["sorting"]));
        Ok(self.spec["sorting"].as_bool().unwrap_or(false))
    }

    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        unreachable!()
    }

    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.events.push(json!(["submit"]));
        self.commands.push(command);
        Ok(())
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
    fn new(spec: &Value) -> Self {
        let raw = number(spec, "raw", 0) as u16;
        let quad = raw as u8 == 0xe0;
        let slots = words(spec, "slots", if quad { &[0, 1, 2, 3] } else { &[0, 1, 2] });
        let tail = words(spec, "tail", if quad { &[3, 2, 1] } else { &[1, 0] });
        let mut record = vec![
            raw,
            number(spec, "material_slot", 10) as u16,
            number(spec, "shade_slot", 0) as u16,
        ];
        record.extend(&slots);
        let program: Vec<u16> = record.iter().copied().chain([0]).collect();
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
                command_word_count: program.len() as u16,
                auxiliary_count: 0,
                flags: ModelFlags::from_raw(0),
                vertex_slot_count: count,
                raw_vertex_count: count / 2,
                shade_slot_count: 4,
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
            program: decode_model_program(&program).unwrap(),
            name: None,
            padding: span,
        };
        let context = ModelContext::new([0; 3], MenuBasis::INITIAL);
        let mut vertices = ModelVertices::new_invocation(count);
        vertices
            .initialize(&model, &context, &mut Projector { points, cursor: 0 })
            .unwrap();
        let shades: Vec<u32> = spec["shades"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        // The clipped fallback's caller reads literal shade WORDs from the
        // record's last vertex on: the receipt's `tail` words follow it in the
        // source bytes, where the program has its terminator.
        Self {
            bytes: record
                .iter()
                .chain(&tail)
                .flat_map(|word| word.to_le_bytes())
                .collect(),
            model,
            vertices,
            shades: ModelShades::from_words_for_tests(&shades),
            context,
            registration: ResidentRegistration {
                handler: 8,
                global_index: 0,
                level: 47,
                region_index: 0,
                local_index: 0,
                phase: RequestKind::Base,
            },
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

#[test]
fn flat_material_faces_match_original_gates_materials_sorting_and_packets() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/flat_material_native.json"
    ))
    .unwrap();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let (mut packets, mut clipped) = (0, 0);
    for spec in receipt["cases"].as_array().unwrap() {
        let name = spec["name"].as_str().unwrap();
        let fixture = Fixture::new(spec);
        let mut host = Host {
            spec,
            events: Vec::new(),
            commands: Vec::new(),
        };
        let state = PrimitiveState {
            mode: number(spec, "mode", 0) as u32,
            far_color: number(spec, "far_color", 0x1234_5678) as u32,
            effect_color: 0,
            projection: ModelProjection {
                focal: [64, 64],
                viewport: [640, 480],
                center: [320, 240],
                fade: [100, 200],
            },
        };
        let request = fixture.request(if spec["far"].as_bool().unwrap() {
            ModelDispatch::Far
        } else {
            ModelDispatch::Near
        });
        let result = if number(spec, "raw", 0) as u8 == 0xe0 {
            prepare_e0(&request, state, &tables, &mut host)
        } else {
            prepare_d8(&request, state, &tables, &mut host)
        };
        // The receipt also marks each call into the clipped fallback, which
        // the port runs as `prepare_material_clipped` without a shade.
        let mut expected: Vec<Value> = spec["events"].as_array().unwrap().to_vec();
        let recorded = expected.len();
        expected.retain(|event| event[0] != "clipped");
        if expected.len() != recorded {
            clipped += 1;
        }
        result.unwrap();
        assert_eq!(host.events, expected, "{name}");
        let natives = spec["packets"].as_array().unwrap();
        assert_eq!(host.commands.len(), natives.len(), "{name}");
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
            assert_eq!(json!(actual.payload), native["payload"], "{name}");
            packets += 1;
        }
    }
    assert_eq!(
        (receipt["cases"].as_array().unwrap().len(), packets, clipped),
        (400, 392, 16)
    );
}
