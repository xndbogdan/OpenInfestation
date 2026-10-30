//! Original-instruction receipts bound 0049A270/0049A930 with count4 and
//! actual DE callers with count3, using
//! supplied source colors/H3/allocator. The separate scratch comparison pins
//! actual 00497250 RGB/UV/geometry; the packet comparison demonstrates that its
//! interpolated RGB does not replace the nominal shared shade.

use super::*;
use crate::{
    menu::MenuBasis,
    model_context::{ModelContext, ModelProjection, ModelTransform},
    model_lighting::{ModelScratchAllocation, ModelShades},
    model_values::{ModelValueCallback, ModelValues},
    model_vertices::{
        ModelVertexInput, ModelVertexProjection, ModelVertexProjector, ModelVertices,
    },
    render_resources::{
        RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderSource,
    },
    ResidentRegistration,
};
use inf_formats::{
    decode_model_program, ByteSpan, ModelEntry, ModelFlags, ModelHeader, ModelIdentity,
    ModelVectorI16, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
    SpriteModeSelectors, SpriteVariant,
};
use serde::Deserialize;
use std::{collections::HashMap, convert::Infallible};

#[derive(Deserialize)]
struct Receipts {
    exe_sha256: String,
    texture_address: u32,
    cases: Vec<Receipt>,
}
#[derive(Deserialize)]
struct Input {
    name: String,
    raw: u16,
    mirror: u16,
    slot: i32,
    shared_ref: u16,
    shared_word: u32,
    scratch_colors: Vec<u32>,
    focal: [i32; 2],
    bounds: [i32; 2],
    center: [i32; 2],
    mode: u32,
    selectors: [u8; 2],
    flags: [u16; 2],
    sorting: bool,
    manages_depth: bool,
    far_color: u32,
    null_texture: bool,
    fail: Option<String>,
    far: bool,
    vertices: Vec<[i64; 5]>,
}
#[derive(Deserialize)]
struct ScratchVertex {
    xy: u32,
    depth: u32,
    clip: u8,
    fog: u8,
    uv: [i32; 2],
    rgb: [u8; 3],
}
#[derive(Deserialize)]
struct NativeCommand {
    key: u32,
    header: [u32; 4],
    blocks: usize,
    payload: Option<Vec<Option<u8>>>,
    texture_offset: Option<usize>,
}
#[derive(Deserialize)]
struct Receipt {
    input: Input,
    events: Vec<String>,
    error: Option<String>,
    scratch: Option<Vec<ScratchVertex>>,
    commands: Vec<NativeCommand>,
}

fn point(vertex: [i64; 5]) -> ModelVertexProjection {
    let [x, y, depth, clip, fog] = vertex;
    let mut result = ModelVertexProjection::default();
    result.write_u32(0, u32::from(x as u16) | (u32::from(y as u16) << 16));
    result.write_u32(4, depth as u32);
    result.bytes[8] = Some(clip as u8);
    result.bytes[9] = Some(fog as u8);
    result
}

struct Projector {
    points: [ModelVertexProjection; 4],
    index: usize,
}
impl ModelVertexProjector for Projector {
    type Error = Infallible;
    fn project(
        &mut self,
        _: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        *output = self.points[self.index];
        self.index += 1;
        Ok(output.read_u32(4).unwrap())
    }
}

struct Fixture {
    model: ModelEntry,
    context: ModelContext,
    vertices: ModelVertices,
    shades: ModelShades,
    bytes: Vec<u8>,
}
impl Fixture {
    fn new(input: &Input) -> Self {
        let triangle = input.raw as u8 == 0xde;
        let mut words = vec![
            input.raw,
            input.slot as u16,
            input.shared_ref,
            input.mirror,
            1 ^ input.mirror,
            2 ^ input.mirror,
        ];
        if !triangle {
            words.push(3 ^ input.mirror);
        }
        words.push(0);
        let mut bytes: Vec<_> = words.iter().copied().flat_map(u16::to_le_bytes).collect();
        if triangle {
            // Original DE reads last vertex WORD plus two following WORDs.
            // Parser framing still stops at the original terminator; these
            // byte reads belong to the producer's bounded source arena.
            bytes.resize(16, 0);
            bytes[12..14].copy_from_slice(&4_u16.to_le_bytes());
            bytes[14..16].copy_from_slice(&5_u16.to_le_bytes());
        } else {
            bytes.extend_from_slice(&[0, 0, 0, 0]);
        }
        let span = ByteSpan::new(0, 0);
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
                shade_slot_count: 8,
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
        let mut projector = Projector {
            points: std::array::from_fn(|i| {
                point(*input.vertices.get(i).unwrap_or(&input.vertices[0]))
            }),
            index: 0,
        };
        vertices
            .initialize(&model, &context, &mut projector)
            .unwrap();
        Self {
            model,
            context,
            vertices,
            shades: ModelShades::in_scratch(
                ModelScratchAllocation::new(0, 4, 8).unwrap(),
                input.shared_word,
            )
            .unwrap(),
            bytes,
        }
    }
    fn request(&self, far: bool) -> ModelPrimitiveRequest<'_> {
        ModelPrimitiveRequest {
            model: &self.model,
            source: RenderSource {
                registration: &MODEL_REGISTRATION,
                bytes: &self.bytes,
            },
            instruction: &self.model.program.instructions[0],
            context: &self.context,
            transform: ModelTransform {
                view_position: [0; 3],
                view_basis: MenuBasis::INITIAL,
            },
            dispatch: if far {
                ModelDispatch::Far
            } else {
                ModelDispatch::Near
            },
            vertices: &self.vertices,
            shades: &self.shades,
            state_words: &[],
        }
    }
}
const MODEL_REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 47,
    phase: RequestKind::Base,
    handler: 8,
    region_index: 0,
    local_index: 0,
    global_index: 0,
};
const TEXTURE_REGISTRATION: ResidentRegistration = ResidentRegistration {
    handler: 3,
    global_index: 3,
    ..MODEL_REGISTRATION
};

fn state(input: &Input) -> PrimitiveState {
    PrimitiveState {
        mode: input.mode,
        far_color: input.far_color,
        effect_color: 0,
        projection: ModelProjection {
            focal: input.focal,
            viewport: input.bounds,
            center: input.center,
            fade: [100, 200],
        },
    }
}
fn sprite(input: &Input) -> SpriteEntry {
    SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: input.selectors[0],
            mode1: input.selectors[1],
            reserved: 0,
        },
        variants: input
            .flags
            .into_iter()
            .enumerate()
            .map(|(index, flags)| SpriteVariant {
                physical_index: index as u8,
                span: ByteSpan::new(0, 0),
                sprite_id: 0,
                reserved: 0,
                flags: SpriteFlags::new(flags),
                color_count: 0,
                atlas_offset_bytes: 0,
                palette_offset: 0,
                width: 2,
                height: 2,
                runtime_slots: [0; 2],
                atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
                atlas_record_index: 0,
                encoding: SpriteEncoding::DirectRgb555 {
                    pixels: ByteSpan::new(0, 0),
                },
            })
            .collect(),
    }
}
struct Host<'a> {
    input: &'a Input,
    texture: Option<RenderResource<'a>>,
    texture_address: u32,
    events: Vec<String>,
    calls: HashMap<&'static str, usize>,
    attempted: Vec<PrimitiveCommand<'a>>,
    completed: usize,
}
impl Host<'_> {
    fn boundary(&mut self, name: &'static str) -> Result<(), &'static str> {
        self.events.push(name.into());
        let count = self.calls.entry(name).or_default();
        *count += 1;
        if self.input.fail.as_deref() == Some(format!("{name}:{count}").as_str()) {
            Err("supplied required host failure")
        } else {
            Ok(())
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
        assert_eq!(i32::from(argument as i16), self.input.slot);
        self.boundary("texture")?;
        Ok(self.texture)
    }
    fn material(&mut self, _: i32) -> Result<u32, Self::Error> {
        Err("unexpected H7")
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.boundary("sort")?;
        Ok(self.input.sorting && self.input.manages_depth)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<super::super::TextureSortIdentity, Self::Error> {
        self.boundary("identity")?;
        Ok(super::super::TextureSortIdentity::NativeEntryAddress(
            self.texture_address,
        ))
    }
    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.attempted.push(command);
        self.boundary("submit")?;
        self.completed += 1;
        Ok(())
    }
}
fn host<'a>(input: &'a Input, entry: &'a SpriteEntry, address: u32) -> Host<'a> {
    Host {
        input,
        texture: (!input.null_texture).then_some(RenderResource {
            request: RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: input.slot,
            },
            source: RenderSource {
                registration: &TEXTURE_REGISTRATION,
                bytes: &[],
            },
            data: RenderResourceData::Sprite(entry),
        }),
        texture_address: address,
        events: vec![],
        calls: HashMap::new(),
        attempted: vec![],
        completed: 0,
    }
}
fn de_receipts() -> Receipts {
    serde_json::from_str(include_str!("../../tests/fixtures/de_clipping_native.json")).unwrap()
}
fn receipts() -> Receipts {
    serde_json::from_str(include_str!("../../tests/fixtures/e6_clipping_native.json")).unwrap()
}
fn tables() -> PrimitiveTables {
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    PrimitiveTables::from_executable(inf_formats::FrontendExecutable::parse(&exe).unwrap()).unwrap()
}

#[test]
fn both_wrappers_match_original_clipper_packets_and_required_failure_prefixes() {
    let all_receipts = [receipts(), de_receipts()];
    assert_eq!(all_receipts[0].cases.len(), 94);
    assert_eq!(all_receipts[1].cases.len(), 96);
    let tables = tables();
    for receipts in &all_receipts {
        assert_eq!(
            receipts.exe_sha256,
            "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
        );
        for receipt in &receipts.cases {
            let input = &receipt.input;
            let points: Vec<_> = input.vertices.iter().copied().map(point).collect();
            let source = if input.raw as u8 == 0xde {
                ClipSource::Triangle(std::array::from_fn(|i| {
                    (&points[i], ModelShade::from_word(input.scratch_colors[i]))
                }))
            } else {
                ClipSource::Quad(std::array::from_fn(|i| {
                    (&points[i], ModelShade::from_word(input.scratch_colors[i]))
                }))
            };
            let scratch = clip_polygon::<&'static str>(
                source,
                state(input).projection,
                &tables,
                input.raw as u8,
            );
            if receipt.error.as_deref() == Some("division") {
                assert!(
                    matches!(scratch, Err(PrimitiveError::Unsupported { .. })),
                    "{}",
                    input.name
                );
            } else {
                let actual = scratch.unwrap();
                let expected = receipt.scratch.as_ref().unwrap();
                assert_eq!(actual.len(), expected.len(), "{} scratch count", input.name);
                for (actual, expected) in actual.iter().zip(expected) {
                    assert_eq!(actual.projected.xy, expected.xy, "{} XY", input.name);
                    assert_eq!(
                        actual.projected.depth, expected.depth,
                        "{} depth",
                        input.name
                    );
                    assert_eq!(actual.projected.clip, expected.clip, "{} clip", input.name);
                    assert_eq!(
                        actual.projected.fog,
                        Some(expected.fog),
                        "{} fog",
                        input.name
                    );
                    assert_eq!(actual.uv, expected.uv, "{} UV", input.name);
                    assert_eq!(actual.rgb, Some(expected.rgb), "{} RGB", input.name);
                }
            }
            let fixture = Fixture::new(input);
            let sprite = sprite(input);
            let mut host = host(input, &sprite, receipts.texture_address);
            let result = if input.raw as u8 == 0xde {
                super::super::de::prepare_de(
                    &fixture.request(input.far),
                    state(input),
                    &tables,
                    &mut host,
                )
            } else {
                prepare_shared_shade_clipped(
                    &fixture.request(input.far),
                    SharedShadeClipRequest {
                        mirror: input.mirror,
                        vertex_indices: SharedShadeVertices::Quad([0, 1, 2, 3]),
                        nominal_shade: input.shared_ref as i16,
                        literal_shades_offset: 12,
                        material: input.slot,
                    },
                    state(input),
                    &tables,
                    &mut host,
                )
            };
            assert_eq!(
                result.is_err(),
                receipt.error.is_some(),
                "{}: {result:?}",
                input.name
            );
            match receipt.error.as_deref() {
                Some("division") => assert!(matches!(
                    result,
                    Err(PrimitiveError::Unsupported { opcode, .. }) if opcode == input.raw as u8
                )),
                Some(_) => assert!(matches!(result, Err(PrimitiveError::Host(_)))),
                None => assert_eq!(result.unwrap().commands, host.completed, "{}", input.name),
            }
            assert_eq!(host.events, receipt.events, "{} host order", input.name);
            assert_eq!(
                host.attempted.len(),
                receipt.commands.len(),
                "{} allocations",
                input.name
            );
            assert_eq!(
                host.completed,
                receipt
                    .commands
                    .iter()
                    .filter(|command| command.payload.is_some())
                    .count(),
                "{} completed prefix",
                input.name
            );
            for (actual, expected) in host.attempted.iter().zip(&receipt.commands) {
                assert_eq!(
                    actual.callback.native_address(),
                    expected.header[0],
                    "{} callback",
                    input.name
                );
                assert_eq!(
                    actual.header_words,
                    expected.header[1..],
                    "{} header",
                    input.name
                );
                assert_eq!(actual.depth, expected.key, "{} sort", input.name);
                assert_eq!(
                    actual.payload_blocks, expected.blocks,
                    "{} reservation",
                    input.name
                );
                if let Some(payload) = &expected.payload {
                    assert_eq!(actual.payload, *payload, "{} packet", input.name);
                    assert_eq!(
                        actual.texture.map(|(offset, _)| offset),
                        expected.texture_offset,
                        "{} relocation",
                        input.name
                    );
                }
            }
        }
    }
}

#[test]
fn mapped_unknown_nominal_shade_is_queued_without_zero_filling() {
    let receipts = receipts();
    let mut input = receipts
        .cases
        .into_iter()
        .find(|case| case.input.name == "0-mask7-mirror0")
        .unwrap()
        .input;
    input.shared_ref = 2; // Mapped but uninitialized, unlike ambient0/1.
    let tables = tables();
    for far in [false, true] {
        let fixture = Fixture::new(&input);
        let sprite = sprite(&input);
        let mut host = host(&input, &sprite, receipts.texture_address);
        let result = prepare_shared_shade_clipped(
            &fixture.request(far),
            SharedShadeClipRequest {
                mirror: 0,
                vertex_indices: SharedShadeVertices::Quad([0, 1, 2, 3]),
                nominal_shade: input.shared_ref as i16,
                literal_shades_offset: 12,
                material: input.slot,
            },
            state(&input),
            &tables,
            &mut host,
        )
        .unwrap();
        assert_eq!(result.commands, 2);
        for command in &host.attempted {
            let shade_offset = match command.callback {
                PrimitiveCallback::SharedShadeUvQuadNear
                | PrimitiveCallback::SharedShadeUvQuadFar => 0x28,
                PrimitiveCallback::SharedShadeUvTriangleNear
                | PrimitiveCallback::SharedShadeUvTriangleFar => 0x20,
                other => panic!("unexpected clipped callback {other:?}"),
            };
            assert_eq!(command.payload[shade_offset..shade_offset + 4], [None; 4]);
            assert_eq!(command.header_words, [0; 3]);
            assert!(command.texture.is_some());
        }
    }
}

#[test]
fn literal_source_reads_precede_clip_abort_and_do_not_substitute_nominal_shade() {
    let receipts = receipts();
    let input = &receipts
        .cases
        .iter()
        .find(|case| case.input.name == "0-abort40-no80")
        .unwrap()
        .input;
    let mut fixture = Fixture::new(input);
    // The missing fourth literal WORD must fail even though clipper would
    // return0 without any material or allocation operation.
    fixture.bytes.truncate(18);
    let sprite = sprite(input);
    let mut host = host(input, &sprite, receipts.texture_address);
    assert!(matches!(
        prepare_shared_shade_clipped(
            &fixture.request(false),
            SharedShadeClipRequest {
                mirror: 0,
                vertex_indices: SharedShadeVertices::Quad([0, 1, 2, 3]),
                nominal_shade: input.shared_ref as i16,
                literal_shades_offset: 12,
                material: input.slot,
            },
            state(input),
            &tables(),
            &mut host
        ),
        Err(PrimitiveError::Record(18))
    ));
    assert!(host.events.is_empty());
    // A negative reference that still lies in the mapped native arena is
    // unknown, not an invalid declared shade index; it must not block abort.
    fixture.bytes.extend_from_slice(&0xffff_u16.to_le_bytes());
    assert_eq!(
        prepare_shared_shade_clipped(
            &fixture.request(false),
            SharedShadeClipRequest {
                mirror: 0,
                vertex_indices: SharedShadeVertices::Quad([0, 1, 2, 3]),
                nominal_shade: input.shared_ref as i16,
                literal_shades_offset: 12,
                material: input.slot,
            },
            state(input),
            &tables(),
            &mut host
        )
        .unwrap()
        .commands,
        0
    );
    assert!(host.events.is_empty());
}
