//! `prepare_material_clipped` against unchanged 00498060/00498570 receipts:
//! every clip count, facing, gate, material, sort key, packet byte and the
//! reached host boundary.

use std::path::Path;

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    menu::MenuBasis,
    model_context::{ModelContext, ModelProjection, ModelTransform},
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
use serde_json::{json, Value};
use std::convert::Infallible;

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

/// The receipt's material bank, renderer sorting answer and allocator.
struct Host<'a> {
    spec: &'a Value,
    events: Vec<Value>,
    calls: Vec<(&'static str, usize)>,
    commands: Vec<PrimitiveCommand<'a>>,
}

impl Host<'_> {
    /// Records the event, then fails where the receipt's named call failed.
    fn boundary(&mut self, name: &'static str, event: Value) -> Result<(), String> {
        self.events.push(event);
        let count = match self.calls.iter_mut().find(|(call, _)| *call == name) {
            Some((_, count)) => {
                *count += 1;
                *count
            }
            None => {
                self.calls.push((name, 1));
                1
            }
        };
        match self.spec["fail"].as_str() {
            Some(fail) if fail == format!("{name}:{count}") => Err(fail.to_owned()),
            _ => Ok(()),
        }
    }
}

impl ModelValueCallback for Host<'_> {
    type Error = String;

    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
        unreachable!()
    }
}

impl<'a> PrimitiveHost<'a> for Host<'a> {
    fn texture(&mut self, _: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        unreachable!()
    }

    fn material(&mut self, slot: i32) -> Result<u32, Self::Error> {
        let spec = self.spec;
        assert_eq!(slot, spec["material_slot"].as_i64().unwrap() as i32);
        // 00405540-style inclusive lookup; a null entry faults at the deref.
        if slot < 0 || slot > spec["capacity"].as_i64().unwrap() as i32 {
            return Err("null material".into());
        }
        if !spec["lazy"].as_bool().unwrap() {
            return Ok(spec["material"].as_u64().unwrap() as u32);
        }
        self.boundary("lazy", json!(["lazy", slot & 0xffff]))?;
        if spec["null_lazy"].as_bool().unwrap() {
            return Err("null material".into());
        }
        Ok(spec["lazy_material"].as_u64().unwrap() as u32)
    }

    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.boundary("sorting", json!(["sorting"]))?;
        Ok(self.spec["sorting"].as_bool().unwrap())
    }

    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        unreachable!()
    }

    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.boundary("submit", json!(["submit"]))?;
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
    literal_offset: usize,
}

impl Fixture {
    fn new(spec: &Value) -> Self {
        let sources = spec["vertices"].as_array().unwrap();
        let quad = sources.len() == 4;
        let raw = spec["raw"].as_u64().unwrap() as u16;
        let header = [
            raw,
            spec["material_slot"].as_i64().unwrap() as u16,
            spec["shade_slot"].as_u64().unwrap() as u16,
        ];
        let words: Vec<u16> = if quad {
            [&header[..], &[0, 1, 2, 3, 0]].concat()
        } else {
            [&header[..], &[0, 1, 2, 0]].concat()
        };
        // The caller's literal WORDs follow the program in these source bytes.
        let mut bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let literal_offset = bytes.len();
        for index in spec["literal"].as_array().unwrap() {
            bytes.extend((index.as_u64().unwrap() as u16).to_le_bytes());
        }
        let mut points: Vec<_> = sources
            .iter()
            .map(|source| {
                let mut point = ModelVertexProjection::default();
                let x = source[0].as_i64().unwrap() as u16;
                let y = source[1].as_i64().unwrap() as u16;
                point.write_u32(0, u32::from(x) | u32::from(y) << 16);
                point.write_u32(4, source[2].as_i64().unwrap() as u32);
                point.bytes[8] = Some(source[3].as_u64().unwrap() as u8);
                point.bytes[9] = Some(source[4].as_u64().unwrap() as u8);
                point
            })
            .collect();
        // Triangles pad an unreferenced fourth slot to keep raw pairs.
        points.resize(4, ModelVertexProjection::default());
        let span = ByteSpan { start: 0, end: 0 };
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
                vertex_slot_count: 4,
                raw_vertex_count: 2,
                shade_slot_count: 6,
                raw_0e: 0,
                reserved_10: 0,
                vertex_ranges: [0, 0, 4, 4],
                reserved_1a: 0,
                shade_ranges: [0, 0],
            },
            auxiliary_span: span,
            auxiliary: vec![],
            auxiliary_program: None,
            vertices_span: span,
            vertices: vec![ModelVectorI16 { x: 0, y: 0, z: 0 }; 2],
            normals_span: span,
            normals: vec![],
            command_span: span,
            program: decode_model_program(&words).unwrap(),
            name: None,
            padding: span,
        };
        let context = ModelContext::new([0; 3], MenuBasis::INITIAL);
        let mut vertices = ModelVertices::new_invocation(4);
        vertices
            .initialize(&model, &context, &mut Projector { points, cursor: 0 })
            .unwrap();
        let shades: Vec<u32> = spec["shades"]
            .as_array()
            .unwrap()
            .iter()
            .map(|shade| shade.as_u64().unwrap() as u32)
            .collect();
        Self {
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
            bytes,
            literal_offset,
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

fn pair(spec: &Value, key: &str) -> [i32; 2] {
    let values = spec[key].as_array().unwrap();
    [0, 1].map(|axis| values[axis].as_i64().unwrap() as i32)
}

#[test]
fn material_clipping_matches_original_00498060_and_00498570() {
    verify(
        include_str!("../../tests/fixtures/material_clipping_native.json"),
        true,
    );
}

/// D8's and E0's wrappers over the same cases: no shade DWORD.
#[test]
fn flat_material_clipping_matches_original_004976a0_and_00497b30() {
    verify(
        include_str!("../../tests/fixtures/flat_material_clipping_native.json"),
        false,
    );
}

fn verify(fixture: &str, shaded: bool) {
    let receipt: Value = serde_json::from_str(fixture).unwrap();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let (mut packets, mut errors) = (0, 0);
    for spec in receipt["cases"].as_array().unwrap() {
        let name = spec["name"].as_str().unwrap();
        let fixture = Fixture::new(spec);
        let mut host = Host {
            spec,
            events: Vec::new(),
            calls: Vec::new(),
            commands: Vec::new(),
        };
        let state = PrimitiveState {
            mode: spec["mode"].as_u64().unwrap() as u32,
            far_color: spec["far_color"].as_u64().unwrap() as u32,
            effect_color: 0,
            projection: ModelProjection {
                focal: pair(spec, "focal"),
                viewport: pair(spec, "bounds"),
                center: pair(spec, "center"),
                fade: [100, 200],
            },
        };
        let indices = [0, 1, 2, 3];
        let pass = MaterialClipRequest {
            mirror: spec["mirror"].as_u64().unwrap() as u16,
            vertex_indices: if spec["vertices"].as_array().unwrap().len() == 4 {
                SharedShadeVertices::Quad(indices)
            } else {
                SharedShadeVertices::Triangle([0, 1, 2])
            },
            nominal_shade: shaded.then(|| spec["shade_slot"].as_i64().unwrap() as i16),
            literal_shades_offset: fixture.literal_offset,
            material: spec["material_slot"].as_i64().unwrap() as i32,
        };
        let far = spec["far"].as_bool().unwrap();
        let result = prepare_material_clipped(
            &fixture.request(if far {
                ModelDispatch::Far
            } else {
                ModelDispatch::Near
            }),
            pass,
            state,
            &tables,
            &mut host,
        );
        match spec["error"].as_str() {
            None => {
                let outcome = result.unwrap();
                assert_eq!(outcome.commands, host.commands.len(), "{name}");
            }
            Some("division") => {
                assert!(
                    matches!(result, Err(PrimitiveError::Unsupported { .. })),
                    "{name} {result:?}"
                );
                errors += 1;
            }
            Some(expected) => {
                match result {
                    Err(PrimitiveError::Host(actual)) => assert_eq!(actual, expected, "{name}"),
                    other => panic!("{name}: {other:?}"),
                }
                errors += 1;
            }
        }
        assert_eq!(json!(host.events), spec["events"], "{name}");
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
        (receipt["cases"].as_array().unwrap().len(), packets, errors),
        (236, 218, 42)
    );
}
