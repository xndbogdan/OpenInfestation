//! Actual DA/E2 caller receipts, supplied native geometry/shade/H3/allocator.
//! Independent scratch comparisons use the receipt's known copied DWORDs;
//! production caller packets also pass with mapped unknown scratch RGB, since
//! these emitters consume only geometry/UV. Allocation faults are terminal
//! supplied hosts; no current partial-payload fault equivalence is claimed.

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
    source_count: usize,
    tail_refs: Vec<i16>,
    #[serde(default)]
    unknown_geometry: bool,
    slot: i32,
    shared_ref: u16,
    shared_word: u32,
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
    passes: Vec<NativePass>,
    commands: Vec<NativeCommand>,
}

#[derive(Deserialize)]
struct NativePass {
    mirror: u16,
    literal_refs: Vec<i16>,
    copied_colors: Vec<u32>,
    scratch: Option<Vec<ScratchVertex>>,
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
        Ok(output.read_u32(4).unwrap_or(0))
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
        let triangle = input.source_count == 3;
        let mut words = vec![input.raw, input.slot as u16, input.shared_ref, 0, 1, 2];
        if !triangle {
            words.push(3);
        }
        words.push(0);
        let mut bytes: Vec<_> = words.iter().copied().flat_map(u16::to_le_bytes).collect();
        let literal_start = if triangle { 10 } else { 12 };
        bytes.resize(literal_start + input.source_count * 2, 0);
        for (i, reference) in input.tail_refs.iter().enumerate() {
            let offset = literal_start + (i + 1) * 2;
            bytes[offset..offset + 2].copy_from_slice(&reference.to_le_bytes());
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
        if input.unknown_geometry {
            for i in [0, input.source_count - 1] {
                projector.points[i].bytes[..8].fill(None);
            }
        }
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
fn receipts() -> Receipts {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/uv_only_clipping_native.json"
    ))
    .unwrap()
}
fn tables() -> PrimitiveTables {
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    PrimitiveTables::from_executable(inf_formats::FrontendExecutable::parse(&exe).unwrap()).unwrap()
}
fn prepare_caller<'a>(
    input: &Input,
    fixture: &Fixture,
    tables: &PrimitiveTables,
    host: &mut Host<'a>,
) -> Result<PrimitiveOutcome, PrimitiveError<&'static str>> {
    if input.source_count == 3 {
        super::super::da::prepare_da(&fixture.request(input.far), state(input), tables, host)
    } else {
        super::super::quad::prepare_quad(&fixture.request(input.far), state(input), tables, host)
    }
}

#[test]
fn actual_da_e2_callers_match_original_uv_only_packets_and_failure_prefixes() {
    let receipts = receipts();
    assert_eq!(receipts.cases.len(), 182);
    assert_eq!(
        receipts.exe_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let tables = tables();
    for receipt in &receipts.cases {
        let input = &receipt.input;
        let points: Vec<_> = input.vertices.iter().copied().map(point).collect();
        for pass in &receipt.passes {
            assert_eq!(
                pass.literal_refs,
                [
                    vec![(input.source_count - 1) as i16],
                    input.tail_refs.clone()
                ]
                .concat()
            );
            let vertex = |i: usize| {
                (
                    &points[i ^ usize::from(pass.mirror)],
                    ModelShade::from_word(pass.copied_colors[i]),
                )
            };
            let source = if input.source_count == 3 {
                ClipSource::Triangle(std::array::from_fn(vertex))
            } else {
                ClipSource::Quad(std::array::from_fn(vertex))
            };
            let scratch = clip_polygon::<&'static str>(
                source,
                state(input).projection,
                &tables,
                input.raw as u8,
            );
            if let Some(expected) = &pass.scratch {
                let actual = scratch.unwrap();
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
                    assert_eq!(
                        actual.rgb,
                        Some(expected.rgb),
                        "{} supplied RGB",
                        input.name
                    );
                }
            } else {
                assert_eq!(receipt.error.as_deref(), Some("division"));
                assert!(
                    matches!(scratch, Err(PrimitiveError::Unsupported { .. })),
                    "{}",
                    input.name
                );
            }
        }
        let fixture = Fixture::new(input);
        let sprite = sprite(input);
        let mut host = host(input, &sprite, receipts.texture_address);
        let result = prepare_caller(input, &fixture, &tables, &mut host);
        assert_eq!(
            result.is_err(),
            receipt.error.is_some(),
            "{}: {result:?}",
            input.name
        );
        match receipt.error.as_deref() {
            Some("division") => assert!(
                matches!(result, Err(PrimitiveError::Unsupported { opcode, .. }) if opcode == input.raw as u8)
            ),
            Some(_) => assert!(matches!(result, Err(PrimitiveError::Host(_)))),
            None => assert_eq!(result.unwrap().commands, host.completed, "{}", input.name),
        }
        assert_eq!(host.events, receipt.events, "{} host order", input.name);
        assert_eq!(
            host.attempted.len(),
            receipt.commands.len(),
            "{} reservations",
            input.name
        );
        assert_eq!(
            host.completed,
            receipt
                .commands
                .iter()
                .filter(|c| c.payload.is_some())
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
fn callers_read_all_literal_words_before_lazy_unknown_geometry_abort() {
    let native_receipts = receipts();
    for count in [3, 4] {
        let mut input = receipts()
            .cases
            .into_iter()
            .find(|r| {
                r.input.source_count == count
                    && r.input.raw & 0xff00 == 0
                    && !r.input.far
                    && r.input.fail.is_none()
            })
            .unwrap()
            .input;
        input.vertices = vec![
            [0, 0, 0, 0x40, 255],
            [340, 220, 128, 0x92, 10],
            [340, 260, 128, 0x92, 20],
            [0, 0, 0, 0x40, 255],
        ];
        input.vertices[count - 1] = [0, 0, 0, 0x40, 255];
        input.unknown_geometry = true;
        input.tail_refs.fill(-1);
        let mut fixture = Fixture::new(&input);
        let entry = sprite(&input);
        let offset = if count == 3 { 14 } else { 18 };
        fixture.bytes.truncate(offset);
        let mut host = host(&input, &entry, native_receipts.texture_address);
        assert!(
            matches!(prepare_caller(&input, &fixture, &tables(), &mut host), Err(PrimitiveError::Record(at)) if at == offset)
        );
        assert!(host.events.is_empty());
        fixture.bytes.extend_from_slice(&(-1_i16).to_le_bytes());
        let result = prepare_caller(&input, &fixture, &tables(), &mut host).unwrap();
        assert_eq!((result.commands, result.rejected), (0, 1));
        assert!(host.events.is_empty());
    }
}
