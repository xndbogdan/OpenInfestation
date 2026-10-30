//! Original-instruction receipts bound 0049B070/0049B6F0 with count4 and
//! supplied source colors/H3/allocator. The separate scratch comparison pins
//! actual 00497250 RGB/UV/geometry; the packet comparison pins per-corner RGB and retained high bytes.
//! Intersection high bytes are unwritten, not fabricated zeroes.

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
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SourceColorPhase {
    Ambient,
    Supplied,
}
#[derive(Deserialize)]
struct Input {
    source_phase: SourceColorPhase,
    name: String,
    raw: u16,
    mirror: u16,
    slot: i32,
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
    high: Option<u8>,
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
        let words = if input.raw as u8 == 0xd2 {
            vec![input.raw, input.slot as u16, 0, 1, 2, 0, 1, 0, 0]
        } else {
            vec![
                input.raw,
                input.slot as u16,
                input.mirror,
                1 ^ input.mirror,
                2 ^ input.mirror,
                3 ^ input.mirror,
                input.mirror,
                1 ^ input.mirror,
                input.mirror,
                1 ^ input.mirror,
                0,
            ]
        };
        let mut bytes: Vec<_> = words.iter().copied().flat_map(u16::to_le_bytes).collect();
        bytes.extend_from_slice(&[0, 0, 0, 0]);
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
                point(input.vertices.get(i).copied().unwrap_or([0; 5]))
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
                input.scratch_colors[0],
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
fn receipts() -> Receipts {
    serde_json::from_str(include_str!("../../tests/fixtures/d6_clipping_native.json")).unwrap()
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
    let receipts = receipts();
    assert_eq!(
        receipts.exe_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(receipts.cases.len(), 100);
    let tables = tables();
    for receipt in &receipts.cases {
        let input = &receipt.input;
        let points: Vec<_> = input.vertices.iter().copied().map(point).collect();
        let scratch = clip_polygon::<&'static str>(
            ClipSource::Quad(std::array::from_fn(|i| {
                (
                    &points[i],
                    crate::model_lighting::ModelShade::from_word(input.scratch_colors[i]),
                )
            })),
            state(input).projection,
            &tables,
            0xd6,
        );
        if receipt.error.as_deref() == Some("division") {
            assert!(
                matches!(
                    scratch,
                    Err(PrimitiveError::Unsupported { opcode: 0xd6, .. })
                ),
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
                assert_eq!(actual.high_byte, expected.high, "{} high", input.name);
                assert_eq!(actual.rgb, Some(expected.rgb), "{} RGB", input.name);
            }
        }
        if input.source_phase == SourceColorPhase::Supplied {
            continue; // Separate native copied-color/emit phase below.
        }
        let fixture = Fixture::new(input);
        let sprite = sprite(input);
        let mut host = host(input, &sprite, receipts.texture_address);
        let result = prepare_per_corner_clipped(
            &fixture.request(input.far),
            PerCornerClipPass {
                mirror: input.mirror,
                vertex_indices: PerCornerVertices::Quad([0, 1, 2, 3]),
            },
            state(input),
            &tables,
            &mut host,
        );
        assert_eq!(
            result.is_err(),
            receipt.error.is_some(),
            "{}: {result:?}",
            input.name
        );
        match receipt.error.as_deref() {
            Some("division") => assert!(matches!(
                result,
                Err(PrimitiveError::Unsupported { opcode: 0xd6, .. })
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

#[test]
fn supplied_copied_colors_match_original_clip_and_per_corner_packet_phase() {
    let receipts = receipts();
    let tables = tables();
    let mut count = 0;
    for receipt in receipts
        .cases
        .iter()
        .filter(|case| case.input.source_phase == SourceColorPhase::Supplied)
    {
        let input = &receipt.input;
        let points: Vec<_> = input.vertices.iter().copied().map(point).collect();
        let clipped = clip_polygon::<&'static str>(
            ClipSource::Quad(std::array::from_fn(|i| {
                (
                    &points[i],
                    crate::model_lighting::ModelShade::from_word(input.scratch_colors[i]),
                )
            })),
            state(input).projection,
            &tables,
            0xd6,
        )
        .unwrap();
        let fixture = Fixture::new(input);
        let sprite = sprite(input);
        let mut host = host(input, &sprite, receipts.texture_address);
        let request = fixture.request(input.far);
        let emitter = PerCornerEmission {
            request: &request,
            state: state(input),
        };
        let mut outcome = PrimitiveOutcome::default();
        if clipped.len() == 5 {
            emitter
                .emit(&clipped[..4], &tables, &mut host, &mut outcome)
                .unwrap();
            emitter
                .emit(
                    &[clipped[0], clipped[3], clipped[4]],
                    &tables,
                    &mut host,
                    &mut outcome,
                )
                .unwrap();
        } else {
            emitter
                .emit(&clipped, &tables, &mut host, &mut outcome)
                .unwrap();
        }
        assert_eq!(host.events, receipt.events, "{} host order", input.name);
        assert_eq!(
            host.attempted.len(),
            receipt.commands.len(),
            "{} packet count",
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
                actual.payload,
                *expected.payload.as_ref().unwrap(),
                "{} copied/interpolated colors",
                input.name
            );
        }
        count += 1;
    }
    assert_eq!(count, 6);
}

#[test]
fn mapped_unknown_per_corner_shades_queue_without_fabricating_rgb_or_high() {
    let receipts = receipts();
    let input = &receipts
        .cases
        .iter()
        .find(|case| case.input.name == "0-mask7-mirror0")
        .unwrap()
        .input;
    let tables = tables();
    let mut fixture = Fixture::new(input);
    for word in 6..10 {
        fixture.bytes[word * 2..word * 2 + 2].copy_from_slice(&2_u16.to_le_bytes());
    }
    let words: Vec<_> = fixture.bytes[..22]
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .collect();
    fixture.model.program = decode_model_program(&words).unwrap();
    let sprite = sprite(input);
    for far in [false, true] {
        let mut host = host(input, &sprite, receipts.texture_address);
        let outcome = prepare_per_corner_clipped(
            &fixture.request(far),
            PerCornerClipPass {
                mirror: 0,
                vertex_indices: PerCornerVertices::Quad([0, 1, 2, 3]),
            },
            state(input),
            &tables,
            &mut host,
        )
        .unwrap();
        assert_eq!(outcome.commands, 2);
        for command in &host.attempted {
            let corners = match command.callback {
                PrimitiveCallback::TexturedTriangleNear
                | PrimitiveCallback::TexturedTriangleFar => 3,
                PrimitiveCallback::TexturedShadeUvQuadNear
                | PrimitiveCallback::TexturedShadeUvQuadFar => 4,
                other => panic!("unexpected callback {other:?}"),
            };
            let color_offset = corners * 8 + 8;
            assert_eq!(
                command.payload[color_offset..color_offset + corners * 4],
                vec![None; corners * 4]
            );
        }
    }
}

#[path = "d2_clipping_tests.rs"]
mod d2;
