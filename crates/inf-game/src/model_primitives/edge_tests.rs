use std::sync::Arc;

use inf_formats::{
    decode_model_program, ByteSpan, ModelEntry, ModelFlags, ModelHeader, ModelIdentity,
    ModelVectorI16, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
    SpriteModeSelectors, SpriteVariant,
};

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

fn tables() -> PrimitiveTables {
    PrimitiveTables {
        clip_acceptance: [1; 256],
        sine_table: Arc::new([0; 4096]),
        quad_u: [0; 4],
        quad_v: [0; 4],
        clip_projection_bits: [0, 0],
        // This edge-only fixture never submits water.
        water: crate::terrain_draw::TerrainWaterTables::default(),
    }
}

fn state() -> PrimitiveState {
    PrimitiveState {
        mode: 0,
        far_color: 0xa1b2_c3d4,
        effect_color: 0x8765_4321,
        projection: ModelProjection {
            focal: [512, 512],
            viewport: [640, 480],
            center: [320, 240],
            fade: [100, 200],
        },
    }
}

fn point(x: i16, y: i16, depth: u32, clip: u8, fog: Option<u8>) -> ModelVertexProjection {
    let mut projection = ModelVertexProjection::default();
    projection.write_u32(0, u32::from(x as u16) | (u32::from(y as u16) << 16));
    projection.write_u32(4, depth);
    projection.bytes[8] = Some(clip);
    projection.bytes[9] = fog;
    projection
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

fn sprite(width: u16, height: u16) -> SpriteEntry {
    SpriteEntry {
        local_index: 0,
        span: ByteSpan { start: 0, end: 0 },
        selectors: SpriteModeSelectors {
            mode0: 0,
            mode1: 0,
            reserved: 0,
        },
        variants: vec![SpriteVariant {
            physical_index: 0,
            span: ByteSpan { start: 0, end: 0 },
            sprite_id: 0,
            reserved: 0,
            flags: SpriteFlags::new(0),
            color_count: 0,
            atlas_offset_bytes: 0,
            palette_offset: 0,
            width,
            height,
            runtime_slots: [0, 0],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan { start: 0, end: 0 },
            },
        }],
    }
}

struct Host<'a> {
    events: Vec<String>,
    commands: Vec<PrimitiveCommand<'a>>,
    sprite: Option<&'a SpriteEntry>,
    sprite_argument: i32,
    sorting: bool,
    registration: &'a ResidentRegistration,
}

struct Setup {
    sprite: SpriteEntry,
    registration: ResidentRegistration,
}

impl Setup {
    fn new(width: u16, height: u16) -> Self {
        Self {
            sprite: sprite(width, height),
            registration: ResidentRegistration {
                handler: 3,
                global_index: 0,
                level: 47,
                region_index: 0,
                local_index: 0,
                phase: RequestKind::Base,
            },
        }
    }

    fn host(&self, argument: i32, sorting: bool) -> Host<'_> {
        Host {
            events: Vec::new(),
            commands: Vec::new(),
            sprite: Some(&self.sprite),
            sprite_argument: argument,
            sorting,
            registration: &self.registration,
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
        self.events.push(format!("texture:{argument}"));
        if argument == self.sprite_argument {
            Ok(self.sprite.map(|entry| RenderResource {
                request: RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument,
                },
                source: RenderSource {
                    registration: self.registration,
                    bytes: &[],
                },
                data: RenderResourceData::Sprite(entry),
            }))
        } else {
            Ok(None)
        }
    }

    fn material(&mut self, _: i32) -> Result<u32, Self::Error> {
        Err("unexpected material")
    }

    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.events.push("sorting".into());
        Ok(self.sorting)
    }

    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        self.events.push("address".into());
        Ok(crate::model_primitives::TextureSortIdentity::PortableEntryOrdinal(7))
    }

    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.events.push("submit".into());
        self.commands.push(command);
        Ok(())
    }
}

#[test]
fn far_edge_emits_the_exact_effect_far_packet() {
    // dx=40, dy=0: length 40, unit 41, unit_dy 0, unit_dx 31968.
    // Width scale (variant 8x4): 41*4/((8-4)*2) = 20. Mean 1000 clears
    // the near-camera path. Focal scales: ROUND(512*2/1000) = 1 each.
    // Half-widths guard 0 to 1; perp = (31968*20)>>15 = 19, par = 0.
    // Corners: Xa=80 Ya=201 Xb=160 Yb=201 Xc=158 Yc=199 Xd=82 Yd=199.
    // Retail pairs Ya with Xd and Yd with Xa at +0x00/+0x18.
    let fixture = Fixture::new(
        &[0x07, 5, 2, 0, 1, 0],
        vec![
            point(100, 200, 1000, 0, Some(10)),
            point(140, 200, 1000, 0, Some(20)),
        ],
    );
    let setup = Setup::new(8, 4);
    let mut host = setup.host(5, true);
    let outcome = prepare_edge(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        PrimitiveOutcome {
            commands: 1,
            rejected: 0
        }
    );
    assert_eq!(host.events, ["texture:5", "sorting", "address", "submit"]);
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::EffectQuadFar);
    assert_eq!(command.payload_blocks, 4);
    // Opaque band through the portable ordinal: 0x40000100 + (7 << 8).
    assert_eq!(command.depth, 0x4000_0800);
    assert_eq!(command.read_u32(0x00), Some(0x00c9_0052));
    assert_eq!(command.read_u32(0x04), Some(1000));
    assert_eq!(command.read_u32(0x08), Some(0x00c9_00a0));
    assert_eq!(command.read_u32(0x0c), Some(1000));
    assert_eq!(command.read_u32(0x10), Some(0x00c7_009e));
    assert_eq!(command.read_u32(0x14), Some(1000));
    assert_eq!(command.read_u32(0x18), Some(0x00c7_0050));
    assert_eq!(command.read_u32(0x1c), Some(1000));
    assert_eq!(command.read_u32(0x20), Some(0x100));
    assert_eq!(command.read_u32(0x28), Some(0x8765_4321));
    assert_eq!(command.read_u32(0x2c), Some(0xa1b2_c3d4));
    assert_eq!(command.texture.map(|(offset, _)| offset), Some(0x24));
    for (offset, expected) in [(0x30, 10), (0x31, 20), (0x32, 20), (0x33, 10)] {
        assert_eq!(
            command.payload[offset],
            Some(expected),
            "fog byte {offset:#x}"
        );
    }
}

#[test]
fn sorting_off_keeps_the_raw_vertex_depth() {
    let fixture = Fixture::new(
        &[0x07, 5, 2, 0, 1, 0],
        vec![
            point(100, 200, 1000, 0, Some(10)),
            point(140, 200, 1000, 0, Some(20)),
        ],
    );
    let setup = Setup::new(8, 4);
    let mut host = setup.host(5, false);
    let outcome = prepare_edge(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(outcome.commands, 1);
    assert_eq!(host.events, ["texture:5", "sorting", "submit"]);
    assert_eq!(host.commands[0].depth, 1000);
}

#[test]
fn halved_float_scales_reach_admitted_corners() {
    // Scales ROUND(512*32767/500) = 33553 exceed 0x7FFE, so one halving
    // pass leaves 16776. Without the loop the truncated i16 factors
    // (-31983) would emit a different packet; the pinned corners below
    // only match the halved path. dx=853, dy=8: length 853, unit 854,
    // unit_dy 306, unit_dx 32729, width scale (854*4)/8 = 427.
    // neg_half -156, half_v 16756, perp 426, par 3; every X corner
    // clears the +-0x3FF window while Y corners take the 0x20/8 bands.
    let fixture = Fixture::new(
        &[0x07, 7, 32767, 0, 1, 0],
        vec![
            point(-413, 200, 500, 0, Some(10)),
            point(440, 208, 500, 0, Some(20)),
        ],
    );
    let setup = Setup::new(8, 4);
    let mut host = setup.host(7, true);
    let outcome = prepare_edge(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(outcome.commands, 1);
    let command = &host.commands[0];
    assert_eq!(command.read_u32(0x00), Some(0x4239_fc1d));
    assert_eq!(command.read_u32(0x08), Some(0x4247_02c6));
    assert_eq!(command.read_u32(0x10), Some(0xbf5f_03fe));
    assert_eq!(command.read_u32(0x18), Some(0xbf51_fd55));
}

#[test]
fn clip_and_fog_skips_precede_every_host_lookup() {
    for points in [
        vec![
            point(100, 200, 1000, 0x40, Some(10)),
            point(140, 200, 1000, 0, Some(20)),
        ],
        vec![
            point(100, 200, 1000, 0, Some(255)),
            point(140, 200, 1000, 0, Some(255)),
        ],
    ] {
        let fixture = Fixture::new(&[0x07, 5, 2, 0, 1, 0], points);
        let setup = Setup::new(8, 4);
        let mut host = setup.host(5, true);
        let outcome = prepare_edge(
            &fixture.request(ModelDispatch::Far),
            state(),
            &tables(),
            &mut host,
        )
        .unwrap();
        assert_eq!(
            outcome,
            PrimitiveOutcome {
                commands: 0,
                rejected: 1
            }
        );
        assert!(host.events.is_empty());
        assert!(host.commands.is_empty());
    }
}

#[test]
fn clip_table_rejection_counts_without_a_packet() {
    let fixture = Fixture::new(
        &[0x07, 5, 2, 0, 1, 0],
        vec![
            point(100, 200, 1000, 0, Some(10)),
            point(140, 200, 1000, 0, Some(20)),
        ],
    );
    let denied = PrimitiveTables {
        clip_acceptance: [0; 256],
        ..tables()
    };
    let setup = Setup::new(8, 4);
    let mut host = setup.host(5, true);
    let outcome = prepare_edge(
        &fixture.request(ModelDispatch::Far),
        state(),
        &denied,
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        PrimitiveOutcome {
            commands: 0,
            rejected: 1
        }
    );
    assert!(host.commands.is_empty());
}

#[test]
fn near_edge_emits_the_exact_effect_near_packet() {
    // Same geometry as the far happy path: corners Xa=80 Ya=201
    // Xb=160 Yb=201 Xc=158 Yc=199 Xd=82 Yd=199, paired Ya-with-Xd
    // and Yd-with-Xa like the far table. Three blocks, no far tail;
    // the existing effect-near reader consumes this shape directly.
    let fixture = Fixture::new(
        &[0x07, 5, 2, 0, 1, 0],
        vec![
            point(100, 200, 1000, 0, Some(10)),
            point(140, 200, 1000, 0, Some(20)),
        ],
    );
    let setup = Setup::new(8, 4);
    let mut host = setup.host(5, false);
    let outcome = prepare_edge(
        &fixture.request(ModelDispatch::Near),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        PrimitiveOutcome {
            commands: 1,
            rejected: 0
        }
    );
    assert_eq!(host.events, ["texture:5", "sorting", "submit"]);
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::EffectQuadNear);
    assert_eq!(command.payload_blocks, 3);
    assert_eq!(command.payload.len(), 48);
    assert_eq!(command.depth, 1000);
    assert_eq!(command.read_u32(0x00), Some(0x00c9_0052));
    assert_eq!(command.read_u32(0x04), Some(1000));
    assert_eq!(command.read_u32(0x08), Some(0x00c9_00a0));
    assert_eq!(command.read_u32(0x0c), Some(1000));
    assert_eq!(command.read_u32(0x10), Some(0x00c7_009e));
    assert_eq!(command.read_u32(0x14), Some(1000));
    assert_eq!(command.read_u32(0x18), Some(0x00c7_0050));
    assert_eq!(command.read_u32(0x1c), Some(1000));
    assert_eq!(command.read_u32(0x20), Some(0x100));
    assert_eq!(command.read_u32(0x28), Some(0x8765_4321));
    assert_eq!(command.texture.map(|(offset, _)| offset), Some(0x24));
    assert!(command.payload[0x2c..].iter().all(Option::is_none));
}

#[test]
fn fault_paths_fail_closed_without_packets() {
    // Negative material, null material, near-camera mean depth, and a
    // zero width divisor all fault retail; the port reports each.
    let cases: &[(&[u16], Vec<ModelVertexProjection>, u16, u16)] = &[
        (
            &[0x07, 0xffff, 2, 0, 1, 0],
            vec![
                point(100, 200, 1000, 0, Some(10)),
                point(140, 200, 1000, 0, Some(20)),
            ],
            8,
            4,
        ),
        (
            &[0x07, 6, 2, 0, 1, 0],
            vec![
                point(100, 200, 1000, 0, Some(10)),
                point(140, 200, 1000, 0, Some(20)),
            ],
            8,
            4,
        ),
        (
            &[0x07, 5, 2, 0, 1, 0],
            vec![
                point(100, 200, 32, 0, Some(10)),
                point(140, 200, 32, 0, Some(20)),
            ],
            8,
            4,
        ),
        (
            &[0x07, 5, 2, 0, 1, 0],
            vec![
                point(100, 200, 1000, 0, Some(10)),
                point(140, 200, 1000, 0, Some(20)),
            ],
            8,
            8,
        ),
    ];
    for (words, points, width, height) in cases {
        let fixture = Fixture::new(words, points.clone());
        let setup = Setup::new(*width, *height);
        let mut host = setup.host(5, true);
        assert!(
            prepare_edge(
                &fixture.request(ModelDispatch::Far),
                state(),
                &tables(),
                &mut host
            )
            .is_err(),
            "words={words:04x?}"
        );
        assert!(host.commands.is_empty());
    }
}

fn oracle_state() -> PrimitiveState {
    PrimitiveState {
        mode: 0,
        far_color: 0x5242,
        effect_color: 0x0048_606c,
        projection: ModelProjection {
            focal: [512, 512],
            viewport: [1024, 768],
            center: [512, 384],
            fade: [100, 200],
        },
    }
}

fn oracle_points(first: bool) -> Vec<ModelVertexProjection> {
    let mut points = vec![point(0, 0, 0, 0, Some(0)); 30];
    if first {
        points[4] = point(0x18e, 0x153, 0x1fe8, 0x12, Some(0xbf));
        points[28] = point(0x18f, 0x165, 0x1feb, 0x12, Some(0xbf));
    } else {
        points[4] = point(0x1ed, 0x15c, 0x266c, 0x12, Some(0xf4));
        points[28] = point(0x1ed, 0x16b, 0x2671, 0x12, Some(0xf5));
    }
    points
}

#[test]
fn retail_first_firing_reproduces_its_packet_bytes() {
    // Edge07PacketProbe bundle 20260928-114208-948-Edge07PacketProbe:
    // first frame-1 07-FAR firing with its entry inputs; every packet
    // word below is the recorded retail bytes (only the material
    // pointer at +0x24 and the sort key are port-side: relocation
    // out-of-band, raw depth without sorting).
    let fixture = Fixture::new(&[0x07, 0x114, 8, 4, 28, 0], oracle_points(true));
    let setup = Setup::new(0x80, 0x40);
    let mut host = setup.host(0x114, false);
    let outcome = prepare_edge(
        &fixture.request(ModelDispatch::Far),
        oracle_state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        PrimitiveOutcome {
            commands: 1,
            rejected: 0
        }
    );
    assert_eq!(host.events, ["texture:276", "sorting", "submit"]);
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::EffectQuadFar);
    assert_eq!(command.payload_blocks, 4);
    assert_eq!(command.depth, 0x1fe8);
    for (offset, expected) in [
        (0x00, 0x014c_018f),
        (0x04, 0x1fe8),
        (0x08, 0x016e_0190),
        (0x0c, 0x1feb),
        (0x10, 0x016c_018e),
        (0x14, 0x1feb),
        (0x18, 0x014a_018d),
        (0x1c, 0x1fe8),
        (0x20, 0x100),
        (0x28, 0x0048_606c),
        (0x2c, 0x5242),
    ] {
        assert_eq!(command.read_u32(offset), Some(expected), "word {offset:#x}");
    }
    assert_eq!(command.texture.map(|(offset, _)| offset), Some(0x24));
    assert!(command.payload[0x24..0x28].iter().all(Option::is_none));
    for (offset, expected) in [(0x30, 0xbf), (0x31, 0xbf), (0x32, 0xbf), (0x33, 0xbf)] {
        assert_eq!(
            command.payload[offset],
            Some(expected),
            "fog byte {offset:#x}"
        );
    }
}

#[test]
fn retail_second_firing_reproduces_its_packet_bytes() {
    // Second firing from the same probe: a vertical edge (dx=0) with
    // distinct per-vertex fog, pinning the fog-byte pairing too.
    let fixture = Fixture::new(&[0x07, 0x114, 8, 4, 28, 0], oracle_points(false));
    let setup = Setup::new(0x80, 0x40);
    let mut host = setup.host(0x114, false);
    let outcome = prepare_edge(
        &fixture.request(ModelDispatch::Far),
        oracle_state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        PrimitiveOutcome {
            commands: 1,
            rejected: 0
        }
    );
    let command = &host.commands[0];
    assert_eq!(command.callback, PrimitiveCallback::EffectQuadFar);
    assert_eq!(command.depth, 0x266c);
    for (offset, expected) in [
        (0x00, 0x0156_01ee),
        (0x04, 0x266c),
        (0x08, 0x0173_01ee),
        (0x0c, 0x2671),
        (0x10, 0x0171_01ec),
        (0x14, 0x2671),
        (0x18, 0x0154_01ec),
        (0x1c, 0x266c),
        (0x20, 0x100),
        (0x28, 0x0048_606c),
        (0x2c, 0x5242),
    ] {
        assert_eq!(command.read_u32(offset), Some(expected), "word {offset:#x}");
    }
    for (offset, expected) in [(0x30, 0xf4), (0x31, 0xf5), (0x32, 0xf5), (0x33, 0xf4)] {
        assert_eq!(
            command.payload[offset],
            Some(expected),
            "fog byte {offset:#x}"
        );
    }
}

#[test]
fn near_edges_emit_whatever_their_fog_bytes_hold() {
    // 00495CA0 reads no fog byte: absent, mixed or double-FF stale
    // bytes all emit the near packet, which carries no fog.
    for points in [
        vec![
            point(100, 200, 1000, 0, None),
            point(140, 200, 1000, 0, None),
        ],
        vec![
            point(100, 200, 1000, 0, Some(255)),
            point(140, 200, 1000, 0, None),
        ],
        vec![
            point(100, 200, 1000, 0, Some(255)),
            point(140, 200, 1000, 0, Some(255)),
        ],
    ] {
        let fixture = Fixture::new(&[0x07, 5, 2, 0, 1, 0], points);
        let setup = Setup::new(8, 4);
        let mut host = setup.host(5, false);
        let outcome = prepare_edge(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host,
        )
        .unwrap();
        assert_eq!(
            outcome,
            PrimitiveOutcome {
                commands: 1,
                rejected: 0
            }
        );
        let command = &host.commands[0];
        assert_eq!(command.callback, PrimitiveCallback::EffectQuadNear);
        assert_eq!(command.read_u32(0x00), Some(0x00c9_0052));
        assert_eq!(command.read_u32(0x18), Some(0x00c7_0050));
    }
}

#[test]
fn far_missing_fog_fails_closed_at_packet_fill() {
    // Far packets carry fog bytes, so absent ones fail closed after
    // admission and sorting (event order pins the failure point).
    let fixture = Fixture::new(
        &[0x07, 5, 2, 0, 1, 0],
        vec![
            point(100, 200, 1000, 0, None),
            point(140, 200, 1000, 0, None),
        ],
    );
    let setup = Setup::new(8, 4);
    let mut host = setup.host(5, true);
    assert!(
        matches!(
            prepare_edge(
                &fixture.request(ModelDispatch::Far),
                state(),
                &tables(),
                &mut host
            ),
            Err(PrimitiveError::Unknown { .. })
        ),
        "far packet needs real fog bytes"
    );
    assert!(host.commands.is_empty());
    assert_eq!(host.events, ["texture:5", "sorting", "address"]);
}

#[test]
fn overlong_edge_wraps_its_unit_and_still_rejects() {
    // dx=40000: length 40000 truncates the native unit to negative.
    // The divisions run without fault and the +-0x3FF window rejects
    // the sprawl, so this pins fail-closed behavior, not pixels.
    let fixture = Fixture::new(
        &[0x07, 5, 2, 0, 1, 0],
        vec![
            point(-20000, 200, 1000, 0, Some(10)),
            point(20000, 200, 1000, 0, Some(20)),
        ],
    );
    let setup = Setup::new(8, 4);
    let mut host = setup.host(5, true);
    let outcome = prepare_edge(
        &fixture.request(ModelDispatch::Far),
        state(),
        &tables(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        PrimitiveOutcome {
            commands: 0,
            rejected: 1
        }
    );
    assert_eq!(host.events, ["texture:5"]);
}

#[test]
fn a_widescreen_frame_moves_the_windows_right_bound_out_by_its_extra_width() {
    assert_eq!(super::edge_right_bound([640, 480]), 0x3ff);
    assert_eq!(super::edge_right_bound([1024, 768]), 0x3ff);
    assert_eq!(super::edge_right_bound([1280, 1024]), 0x3ff);
    assert_eq!(super::edge_right_bound([1365, 768]), 0x554);
    assert_eq!(super::edge_right_bound([853, 480]), 0x3ff + 213);
    // An edge across native X 1100..1140, inside a 1365-wide frame's right
    // quarter: retail's 1024x768 frame rejects it, the widescreen frame
    // draws it.
    let fixture = Fixture::new(
        &[0x07, 5, 2, 0, 1, 0],
        vec![
            point(1100, 200, 1000, 0, Some(10)),
            point(1140, 200, 1000, 0, Some(20)),
        ],
    );
    let setup = Setup::new(8, 4);
    for (viewport, commands) in [([1024, 768], 0), ([1365, 768], 1)] {
        let mut wide = state();
        wide.projection.viewport = viewport;
        wide.projection.center = [viewport[0] / 2, viewport[1] / 2];
        let mut host = setup.host(5, false);
        let outcome = prepare_edge(
            &fixture.request(ModelDispatch::Near),
            wide,
            &tables(),
            &mut host,
        )
        .unwrap();
        assert_eq!(
            outcome,
            PrimitiveOutcome {
                commands,
                rejected: 1 - commands
            },
            "{viewport:?}"
        );
    }
}
