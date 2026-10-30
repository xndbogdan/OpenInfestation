//! Focused `prepare_ground` tests: record shape, resolve order,
//! gates, uninitialized-depth rejection, sprite dimensions, the
//! viewport corner clip, corner math, packet shape. Fixture and host follow
//! `d2_tests`; only 0x1A-specific paths are pinned. Corner, fog
//! and extract fixtures recompute the C semantics with wrapping
//! integers over a synthetic sine table
//! (`word[i] = (3*i+1) & 0xFFFF`).

use std::{convert::Infallible, sync::Arc};

use super::*;
use crate::{
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
use inf_formats::{
    decode_model_program, ByteSpan, ModelEntry, ModelFlags, ModelHeader, ModelIdentity,
    ModelInstruction, ModelInstructionKind, ModelRawCommand, ModelVectorI16, RequestKind,
    SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags, SpriteModeSelectors,
    SpriteVariant,
};

fn synthetic_tables() -> PrimitiveTables {
    PrimitiveTables {
        clip_acceptance: [0; 256],
        sine_table: Arc::new(std::array::from_fn(|i| ((i * 3 + 1) & 0xffff) as u16)),
        quad_u: [0; 4],
        quad_v: [0; 4],
        clip_projection_bits: [0; 2],
        water: crate::terrain_draw::TerrainWaterTables::default(),
    }
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

const REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 47,
    phase: RequestKind::Base,
    handler: 3,
    region_index: 0,
    local_index: 0,
    global_index: 10,
};

fn sprite_entry(width: u16, height: u16) -> SpriteEntry {
    SpriteEntry {
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
            width,
            height,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan::new(0, 2),
            },
        }],
    }
}

fn textured_sprite<'a>(entry: &'a SpriteEntry, argument: i32) -> RenderResource<'a> {
    RenderResource {
        request: RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument,
        },
        source: RenderSource {
            registration: &REGISTRATION,
            bytes: &[],
        },
        data: RenderResourceData::Sprite(entry),
    }
}

fn values() -> ModelValues {
    ModelValues::new_invocation(Arc::new([0; 4096]))
}

fn manual_request<'a>(
    fixture: &'a Fixture,
    instruction: &'a ModelInstruction,
    dispatch: ModelDispatch,
) -> ModelPrimitiveRequest<'a> {
    ModelPrimitiveRequest {
        model: &fixture.model,
        source: RenderSource {
            registration: &fixture.registration,
            bytes: &fixture.bytes,
        },
        instruction,
        context: &fixture.context,
        transform: ModelTransform {
            view_position: [0; 3],
            view_basis: MenuBasis::INITIAL,
        },
        dispatch,
        vertices: &fixture.vertices,
        shades: &fixture.shades,
        state_words: &[],
    }
}

#[test]
fn rejects_non_1a_shapes() {
    // 0x1B decodes to Opaque with the same arity but is not 0x1A.
    let fixture = Fixture::new(
        &[0x1b, 10, 4, 25, 0, 0],
        vec![point(0, 0, 256, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut host = Host::default();
    let mut eval = values();
    assert!(matches!(
        prepare_ground(
            &fixture.request(ModelDispatch::Near),
            state(),
            &synthetic_tables(),
            &mut eval,
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
    // Two argument words fail closed (retail reads four).
    let short = ModelInstruction {
        span: ByteSpan { start: 0, end: 0 },
        raw_command: 0x001a,
        kind: ModelInstructionKind::Opaque(ModelRawCommand {
            arguments: vec![10, 4],
        }),
    };
    let fixture = Fixture::new(
        &[0x1a, 10, 4, 25, 0, 0],
        vec![point(0, 0, 256, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut host = Host::default();
    let mut eval = values();
    assert!(matches!(
        prepare_ground(
            &manual_request(&fixture, &short, ModelDispatch::Near),
            state(),
            &synthetic_tables(),
            &mut eval,
            &mut host
        ),
        Err(PrimitiveError::Record(_))
    ));
}

#[test]
fn corner_clip_uses_the_published_viewport() {
    // 004A1F7E/004A1FC4 compare each corner with the published viewport
    // 0055F234/38. Only the all-inside code 2|0x10 is accepted here, so
    // narrowing the viewport below the right corners rejects the quad.
    let entry = sprite_entry(64, 64);
    let mut tables = synthetic_tables();
    tables.clip_acceptance[0x12] = 1;
    for (viewport, expected) in [([640, 480], (1, 0)), ([300, 480], (0, 1))] {
        let fixture = Fixture::new(
            &[0x1a, 0, 585, 2, 0, 0],
            vec![point(320, 240, 256, 0, None), point(0, 0, 256, 0, None)],
        );
        let mut st = state();
        st.projection.viewport = viewport;
        let mut host = passing_host(&entry, 585, vec![512, 0]);
        let mut eval = values();
        let outcome = prepare_ground(
            &fixture.request(ModelDispatch::Near),
            st,
            &tables,
            &mut eval,
            &mut host,
        )
        .unwrap();
        assert_eq!(
            (outcome.commands, outcome.rejected),
            expected,
            "{viewport:?}"
        );
    }
}

fn passing_host<'a>(entry: &'a SpriteEntry, argument: i32, evals: Vec<u16>) -> Host<'a> {
    Host {
        texture: Some(textured_sprite(entry, argument)),
        values: evals,
        ..Default::default()
    }
}

fn bare_request<'a>(
    fixture: &'a Fixture,
    instruction: &'a ModelInstruction,
    vertices: &'a ModelVertices,
    dispatch: ModelDispatch,
) -> ModelPrimitiveRequest<'a> {
    let mut base = manual_request(fixture, instruction, dispatch);
    base.vertices = vertices;
    base
}

#[test]
fn resolve_precedes_vertex_reads_and_gates_skip() {
    let entry = sprite_entry(64, 64);
    // Lookup runs before the vertex reference: the texture event
    // leads even when the dest slot was never projected.
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 2, 0, 0],
        vec![point(0, 0, 256, 0, None), point(0, 0, 256, 0, None)],
    );
    let bare = ModelVertices::new_invocation(1);
    let mut host = passing_host(&entry, 585, vec![100, 200]);
    let mut eval = values();
    let outcome = prepare_ground(
        &bare_request(
            &fixture,
            &fixture.model.program.instructions[0],
            &bare,
            ModelDispatch::Near,
        ),
        state(),
        &synthetic_tables(),
        &mut eval,
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (0, 1));
    assert_eq!(host.events, ["texture:585"]);
    // Clip-40 skips with the lookup already consumed, before evals.
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 2, 0, 0],
        vec![point(1, 2, 256, 0x40, None), point(0, 0, 256, 0, None)],
    );
    let mut host = passing_host(&entry, 585, vec![100, 200]);
    let mut eval = values();
    let outcome = prepare_ground(
        &fixture.request(ModelDispatch::Near),
        state(),
        &synthetic_tables(),
        &mut eval,
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (0, 1));
    assert_eq!(host.events, ["texture:585"]);
    // Evaluated size/angle run in record order once admitted.
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 0x81, 0x82, 0],
        vec![point(1, 2, 63, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut host = passing_host(&entry, 585, vec![100, 200]);
    let mut eval = values();
    let _ = prepare_ground(
        &fixture.request(ModelDispatch::Near),
        state(),
        &synthetic_tables(),
        &mut eval,
        &mut host,
    );
    assert_eq!(host.events, ["texture:585", "value:1", "value:2"]);
}

#[test]
fn depth_below_64_and_missing_inputs_fail_closed() {
    let entry = sprite_entry(64, 64);
    // Depth below 64 with a clear clip byte: the native extent
    // words are uninitialized, so reject the inconsistent state.
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 2, 0, 0],
        vec![point(1, 2, 63, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut host = passing_host(&entry, 585, vec![100, 200]);
    let mut eval = values();
    assert!(matches!(
        prepare_ground(
            &fixture.request(ModelDispatch::Near),
            state(),
            &synthetic_tables(),
            &mut eval,
            &mut host
        ),
        Err(PrimitiveError::Unsupported { opcode: 0x1a, .. })
    ));
    // Failed packed evals propagate as values errors.
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 0x81, 0x82, 0],
        vec![point(1, 2, 256, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut host = passing_host(&entry, 585, vec![]);
    let mut eval = values();
    assert!(matches!(
        prepare_ground(
            &fixture.request(ModelDispatch::Near),
            state(),
            &synthetic_tables(),
            &mut eval,
            &mut host
        ),
        Err(PrimitiveError::Value(_))
    ));
    // Null sprite faults retail.
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 2, 0, 0],
        vec![point(1, 2, 256, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut host = Host {
        values: vec![100, 200],
        ..Default::default()
    };
    let mut eval = values();
    assert!(matches!(
        prepare_ground(
            &fixture.request(ModelDispatch::Near),
            state(),
            &synthetic_tables(),
            &mut eval,
            &mut host
        ),
        Err(PrimitiveError::Unsupported { opcode: 0x1a, .. })
    ));
    // Zero dimensions would divide by zero in the native idiv.
    let flat = sprite_entry(0, 0);
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 2, 0, 0],
        vec![point(1, 2, 256, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut host = passing_host(&flat, 585, vec![100, 200]);
    let mut eval = values();
    assert!(matches!(
        prepare_ground(
            &fixture.request(ModelDispatch::Near),
            state(),
            &synthetic_tables(),
            &mut eval,
            &mut host
        ),
        Err(PrimitiveError::Unsupported { opcode: 0x1a, .. })
    ));
}

#[test]
fn corner_gate_uses_the_acceptance_table() {
    // A zeroed table rejects regardless of corners.
    let entry = sprite_entry(64, 64);
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 2, 0, 0],
        vec![point(320, 240, 256, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut host = passing_host(&entry, 585, vec![512, 0]);
    let mut eval = values();
    let outcome = prepare_ground(
        &fixture.request(ModelDispatch::Near),
        state(),
        &synthetic_tables(),
        &mut eval,
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (0, 1));
    // An all-accepting table submits.
    let fixture = Fixture::new(
        &[0x1a, 0, 585, 2, 0, 0],
        vec![point(320, 240, 256, 0, None), point(0, 0, 256, 0, None)],
    );
    let mut tables = synthetic_tables();
    tables.clip_acceptance = [1; 256];
    let mut host = passing_host(&entry, 585, vec![512, 0]);
    let mut eval = values();
    let outcome = prepare_ground(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables,
        &mut eval,
        &mut host,
    )
    .unwrap();
    assert_eq!((outcome.commands, outcome.rejected), (1, 0));
}

#[test]
fn corners_match_c_semantics_vectors() {
    // Recomputed from the C pipeline with wrapping integers over
    // the synthetic sine table in this file's header.
    for (center, depth, evals, dims, focal, expected) in [
        (
            (320i16, 240i16),
            256u32,
            [512u16, 0u16],
            (64u16, 32u16),
            [64, 64],
            [(253i16, 207i16), (387, 207), (387, 273), (253, 273)],
        ),
        (
            (100, 200),
            300u32,
            [7, 0x4000],
            (16, 16),
            [512, 512],
            [(107, 195), (105, 207), (93, 205), (95, 193)],
        ),
        (
            (-1000, 2000),
            100000u32,
            [300, 0xc000],
            (0xff, 0x10),
            [512, 512],
            [(-1001, 2001), (-1001, 1999), (-999, 1999), (-999, 2001)],
        ),
    ] {
        let (cx, cy) = center;
        let entry = sprite_entry(dims.0, dims.1);
        let fixture = Fixture::new(
            &[0x1a, 0, 585, 0x81, 0x82, 0],
            vec![point(cx, cy, depth, 0, None), point(0, 0, 256, 0, None)],
        );
        let mut tables = synthetic_tables();
        tables.clip_acceptance = [1; 256];
        let mut st = state();
        st.projection.focal = focal;
        let mut host = passing_host(&entry, 585, evals.to_vec());
        let mut eval = values();
        let outcome = prepare_ground(
            &fixture.request(ModelDispatch::Near),
            st,
            &tables,
            &mut eval,
            &mut host,
        )
        .unwrap();
        assert_eq!((outcome.commands, outcome.rejected), (1, 0));
        let command = &host.commands[0];
        let got: Vec<(i16, i16)> = (0..4)
            .map(|corner| {
                let xy = command.read_u32(corner * 8).unwrap();
                (xy as i16, (xy >> 16) as i16)
            })
            .collect();
        assert_eq!(got, expected);
        // One depth for all four corners, mode plain, texture bound.
        for corner in 0..4 {
            assert_eq!(command.read_u32(corner * 8 + 4), Some(depth));
        }
        assert_eq!(command.read_u32(0x20), Some(0));
        assert_eq!(command.callback, PrimitiveCallback::TexturedQuadNear);
    }
}

#[path = "ground_native_tests.rs"]
mod native;
