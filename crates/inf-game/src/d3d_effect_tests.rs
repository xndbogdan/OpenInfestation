use super::*;
use crate::{
    d3d_viewport::{clip_polygon, D3dViewportState},
    render_resources::{RenderResourceKind, RenderResourceRequest, RenderSource},
    ResidentRegistration,
};
use inf_formats::{
    ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
    SpriteModeSelectors, SpriteVariant,
};
use serde::Deserialize;

const REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 47,
    phase: RequestKind::Base,
    handler: 3,
    region_index: 0,
    local_index: 0,
    global_index: 10,
};

fn sprite(selectors: [u8; 2], flags: u16) -> SpriteEntry {
    SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: selectors[0],
            mode1: selectors[1],
            reserved: 0,
        },
        variants: vec![SpriteVariant {
            physical_index: 0,
            span: ByteSpan::new(0, 0),
            sprite_id: 0,
            reserved: 0,
            flags: SpriteFlags::new(flags),
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
    }
}

fn resource(entry: &SpriteEntry) -> RenderResource<'_> {
    RenderResource {
        request: RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 10,
        },
        source: RenderSource {
            registration: &REGISTRATION,
            bytes: &[],
        },
        data: RenderResourceData::Sprite(entry),
    }
}

fn packet(entry: &SpriteEntry, mode: u32) -> PrimitiveCommand<'_> {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::EffectQuadNear, 0, 3);
    command.write_u32(0x20, mode);
    command.write_texture(0x24, Some(resource(entry)));
    command
}

fn write_inputs(command: &mut PrimitiveCommand<'_>) {
    for (i, (xy, depth)) in [
        ([-123i16, 234i16], 64i32),
        ([32767, -32768], i32::MIN),
        ([1, 2], i32::MAX),
        ([-456, -100], -12345),
    ]
    .into_iter()
    .enumerate()
    {
        command.write_u32(
            i * 8,
            u32::from(xy[0] as u16) | (u32::from(xy[1] as u16) << 16),
        );
        command.write_u32(i * 8 + 4, depth as u32);
    }
    for (i, byte) in [32, 64, 128].into_iter().enumerate() {
        command.write_byte(0x28 + i, byte);
    }
}

fn far_packet(entry: &SpriteEntry, mode: u32) -> PrimitiveCommand<'_> {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::EffectQuadFar, 0, 4);
    command.write_u32(0x20, mode);
    command.write_texture(0x24, Some(resource(entry)));
    command
}

fn write_far_tail(command: &mut PrimitiveCommand<'_>) {
    command.write_u32(0x2c, 0x7c1f);
    for (i, value) in [0, 127, 128, 255].into_iter().enumerate() {
        command.write_byte(0x30 + i, value);
    }
}

/// `prepare_effect_quad` under the packet's own known +20.
fn prepare_known(
    quad: &EffectQuad<'_>,
    renderer: D3dFlatState,
    global_policy: u32,
    secondary_texture: bool,
    uv_bounds: [f32; 4],
) -> EffectQuadSubmission {
    let mode = quad.mode.expect("a known packet word");
    prepare_effect_quad(
        quad,
        mode,
        renderer,
        global_policy,
        secondary_texture,
        uv_bounds,
    )
}

fn renderer() -> D3dFlatState {
    D3dFlatState {
        material_shifts: [u32::MAX; 3], // Near effect RGB does not read display shifts.
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}

#[test]
fn selector_ff_suppresses_before_effect_color_and_vertices() {
    let entry = sprite([255, 0], 0x18);
    for mode in [0, 0x20, 0x100, 0x4000, u32::MAX - 1] {
        assert!(read_effect_quad(&packet(&entry, mode)).unwrap().is_none());
    }
    assert!(matches!(
        read_effect_quad(&packet(&entry, 1)),
        Err(EffectPixelError::Unknown { offset: 0, .. })
    ));
    let entry = sprite([0, 255], 0);
    assert!(read_effect_quad(&packet(&entry, u32::MAX))
        .unwrap()
        .is_none());
    let entry = sprite([0, 2], 0);
    assert!(matches!(
        read_effect_quad(&packet(&entry, 1)),
        Err(EffectPixelError::Selector(2))
    ));
}

#[test]
fn packet_consumes_only_three_effect_bytes_and_signed_vertex_fields() {
    let entry = sprite([0, 0], 8);
    let mut command = packet(&entry, 0x4020);
    write_inputs(&mut command);
    assert!(command.payload[0x2b..].iter().all(Option::is_none));
    let quad = read_effect_quad(&command).unwrap().unwrap();
    assert_eq!(quad.effect_rgb, [32, 64, 128]);
    assert_eq!(
        quad.vertices.map(|v| v.depth),
        [64, i32::MIN, i32::MAX, -12345]
    );
    assert_eq!(quad.vertices[1].x, 32767.);
    assert_eq!(quad.vertices[1].y, -32768.);
    let prepared = prepare_known(&quad, renderer(), 8, false, [0., 0., 1., 1.]).primary;
    assert_eq!(
        prepared.vertices.map(|v| v.diffuse),
        [[64, 128, 255, 128]; 4]
    );
    command.payload[0x2b..].fill(Some(0xff));
    assert_eq!(
        prepare_known(
            &read_effect_quad(&command).unwrap().unwrap(),
            renderer(),
            8,
            false,
            [0., 0., 1., 1.]
        )
        .primary,
        prepared
    );
}

#[test]
fn packet_rejects_unknown_inputs_wrong_family_extent_and_resource() {
    let entry = sprite([0, 0], 0);
    let mut command = packet(&entry, 0);
    write_inputs(&mut command);
    for offset in (0..0x20).chain(0x28..0x2b) {
        let mut missing = command.clone();
        missing.payload[offset] = None;
        assert!(matches!(
            read_effect_quad(&missing),
            Err(EffectPixelError::Unknown { .. })
        ));
    }
    for callback in [
        PrimitiveCallback::EffectQuadFar,
        PrimitiveCallback::TexturedQuadNear,
    ] {
        let mut wrong = command.clone();
        wrong.callback = callback;
        assert!(matches!(
            read_effect_quad(&wrong),
            Err(EffectPixelError::Packet)
        ));
    }
    for length in [47, 49] {
        let mut wrong = command.clone();
        wrong.payload.resize(length, None);
        assert!(matches!(
            read_effect_quad(&wrong),
            Err(EffectPixelError::Packet)
        ));
    }
    let mut wrong = command.clone();
    wrong.payload_blocks = 4;
    assert!(matches!(
        read_effect_quad(&wrong),
        Err(EffectPixelError::Packet)
    ));
    for texture in [
        None,
        Some((0x28, resource(&entry))),
        Some((
            0x24,
            RenderResource {
                data: RenderResourceData::Style(0),
                ..resource(&entry)
            },
        )),
    ] {
        let mut wrong = command.clone();
        wrong.texture = texture;
        assert!(matches!(
            read_effect_quad(&wrong),
            Err(EffectPixelError::Texture)
        ));
    }
}

#[test]
fn unknown_mode_reads_only_when_both_selectors_choose_one_descriptor() {
    for selectors in [[0, 0], [255, 255]] {
        let entry = sprite(selectors, 0x14);
        let mut command = packet(&entry, 0);
        write_inputs(&mut command);
        command.payload[0x22] = None;
        let quad = read_effect_quad(&command).unwrap();
        assert_eq!(quad.is_none(), selectors[0] == 255);
        if let Some(quad) = quad {
            assert_eq!(quad.mode, None);
            assert_eq!(quad.sprite_mode, SpriteRenderMode::Mode0);
        }
    }
    for selectors in [[0, 1], [255, 0], [0, 255]] {
        let entry = sprite(selectors, 0x14);
        let mut command = packet(&entry, 0);
        write_inputs(&mut command);
        command.payload[0x20] = None;
        assert!(matches!(
            read_effect_quad(&command),
            Err(EffectPixelError::Unknown {
                field: "mode",
                offset: 0x20
            })
        ));
    }
}

/// The world flash's screen quad: corners on the inclusive display bounds,
/// one depth (0x40), packet+20 left to the allocation's carry.
fn screen_quad(entry: &SpriteEntry, corners: [[i16; 2]; 4]) -> EffectQuad<'_> {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::EffectQuadNear, 0, 3);
    for (i, [x, y]) in corners.into_iter().enumerate() {
        command.write_u32(i * 8, u32::from(x as u16) | (u32::from(y as u16) << 16));
        command.write_u32(i * 8 + 4, 0x40);
    }
    command.write_texture(0x24, Some(resource(entry)));
    command.write_rgb(0x28, [128; 3]);
    read_effect_quad(&command).unwrap().unwrap()
}

#[test]
fn unknown_mode_settles_only_where_every_word_draws_the_quad_alike() {
    let entry = sprite([0, 0], 0x14);
    let screen = [[0, 0], [640, 0], [640, 480], [0, 480]];
    let bounds = [0, 0, 640, 480];
    let white = crate::d3d_texture::test_texture([2, 2], |_, _| [255; 4]);
    let quad = screen_quad(&entry, screen);
    assert_eq!(settle_effect_mode(&quad, &white, bounds), Ok(0x4120));
    let unknown = Err(EffectPixelError::Unknown {
        field: "mode",
        offset: 0x20,
    });
    // A known word is itself, whatever the quad.
    let known = EffectQuad {
        mode: Some(7),
        ..quad
    };
    let two = crate::d3d_texture::test_texture([2, 1], |x, _| [255 * x as u8; 4]);
    assert_eq!(settle_effect_mode(&known, &two, [0; 4]), Ok(7));
    // Two texel colors: the diagonal and perspective move coordinates.
    assert_eq!(settle_effect_mode(&quad, &two, bounds), unknown);
    // A corner past the bounds: 004BCCE0 cuts it unless bit 20 skips it.
    for bounds in [[0, 0, 639, 480], [0, 0, 640, 479], [1, 0, 640, 480]] {
        assert_eq!(settle_effect_mode(&quad, &white, bounds), unknown);
    }
    // Depths that differ, or one that is not positive.
    for depths in [[0x40, 0x40, 0x41, 0x40], [0; 4], [-0x40; 4]] {
        let mut changed = quad;
        for (vertex, depth) in changed.vertices.iter_mut().zip(depths) {
            vertex.depth = depth;
        }
        assert_eq!(settle_effect_mode(&changed, &white, bounds), unknown);
    }
    // A bow tie: the two diagonals cover different areas.
    let crossed = screen_quad(&entry, [[0, 0], [640, 480], [640, 0], [0, 480]]);
    assert_eq!(settle_effect_mode(&crossed, &white, bounds), unknown);
    // Far quads keep the word: 004D9840's reads are not settled here.
    let far = EffectQuad {
        shading: EffectQuadShading::Far {
            color: 0,
            fog: [0; 4],
        },
        ..quad
    };
    assert_eq!(settle_effect_mode(&far, &white, bounds), unknown);
    // Either winding of a convex quad settles.
    let reversed = screen_quad(&entry, [[0, 480], [640, 480], [640, 0], [0, 0]]);
    assert_eq!(settle_effect_mode(&reversed, &white, bounds), Ok(0x4120));
}

#[test]
fn effect_color_normalizes_whole_rgb_and_preserves_spill_rounding() {
    assert_eq!(effect_rgb([0, 0, 0]), [0, 0, 0]);
    assert_eq!(effect_rgb([64; 3]), [128; 3]);
    assert_eq!(effect_rgb([128; 3]), [255; 3]);
    assert_eq!(effect_rgb([255, 128, 64]), [255, 128, 64]);
    // Peak green scales red to an exact half; red is not spilled to f32.
    assert_eq!(effect_rgb([1, 170, 85]), [2, 255, 128]);
}

#[test]
fn far_selector_precedes_tail_and_tail_bytes_are_required_without_padding() {
    let entry = sprite([255, 0], 0);
    assert!(read_effect_quad(&far_packet(&entry, 0)).unwrap().is_none());
    let entry = sprite([0, 0], 0);
    let mut command = far_packet(&entry, 0);
    write_inputs(&mut command);
    write_far_tail(&mut command);
    assert_eq!(command.payload[0x2b], None);
    assert!(command.payload[0x34..].iter().all(Option::is_none));
    let quad = read_effect_quad(&command).unwrap().unwrap();
    assert!(matches!(
        quad.shading,
        EffectQuadShading::Far {
            color: 0x7c1f,
            fog: [0, 127, 128, 255]
        }
    ));
    for offset in 0x2c..0x34 {
        let mut missing = command.clone();
        missing.payload[offset] = None;
        assert!(matches!(
            read_effect_quad(&missing),
            Err(EffectPixelError::Unknown { .. })
        ));
    }
    let state = D3dFlatState {
        material_shifts: [6, 1, 4],
        ..renderer()
    };
    let expected = prepare_known(&quad, state, 8, false, [0., 0., 1., 1.]).primary;
    command.write_byte(0x2b, 0xff);
    command.payload[0x34..].fill(Some(0xff));
    assert_eq!(
        prepare_known(
            &read_effect_quad(&command).unwrap().unwrap(),
            state,
            8,
            false,
            [0., 0., 1., 1.]
        )
        .primary,
        expected,
    );
}

#[test]
fn far_flags_select_rgb_attenuation_or_specular_vertex_fog() {
    // 004D9982..004D9A45 branches on descriptor flags/global08. The two
    // Far paths intentionally do not substitute the Near color policy.
    for (flags, global_policy, blend, alpha, vertex_fog) in [
        (0, 0, D3dTextureBlend::Copy, 255, true),
        (0, 8, D3dTextureBlend::SourceAlpha, 255, true),
        (8, 0, D3dTextureBlend::SourceAlpha, 128, true),
        (0x10, 0, D3dTextureBlend::Additive, 255, false),
        (0x18, 8, D3dTextureBlend::Additive, 255, false),
        (0x20, 0, D3dTextureBlend::InverseSourceColor, 255, false),
        (0x28, 8, D3dTextureBlend::InverseSourceColor, 255, false),
        (0x30, 8, D3dTextureBlend::Additive, 255, false),
    ] {
        let entry = sprite([0, 0], flags);
        let mut command = far_packet(&entry, 0x4100);
        write_inputs(&mut command);
        write_far_tail(&mut command);
        let state = D3dFlatState {
            material_shifts: [6, 1, 4],
            ..renderer()
        };
        let result = prepare_known(
            &read_effect_quad(&command).unwrap().unwrap(),
            state,
            global_policy,
            false,
            [0., 0., 1., 1.],
        )
        .primary;
        assert_eq!(result.policy.blend, blend, "flags {flags:04x}");
        assert_eq!(result.policy.depth_enable, Some(1));
        assert_eq!(
            result.policy.depth_write,
            Some(if flags & 0x39 != 0 { 0 } else { 1 })
        );
        assert!(!result.policy.perspective);
        assert_eq!(result.triangles, [[0, 1, 2], [0, 2, 3]]);
        if vertex_fog {
            assert_eq!(
                result.policy.fog,
                D3dTextureFog::Vertex {
                    color: [255, 0, 255]
                }
            );
            assert_eq!(
                result.vertices.map(|v| v.diffuse),
                [[64, 128, 255, alpha]; 4]
            );
            assert_eq!(
                result.vertices.map(|v| v.specular),
                [0xff00_0000, 0x8000_0000, 0x7f00_0000, 0]
            );
        } else {
            assert_eq!(result.policy.fog, D3dTextureFog::Disabled);
            assert_eq!(
                result.vertices.map(|v| v.diffuse),
                [
                    [64, 128, 255, 255],
                    [32, 64, 129, 255],
                    [32, 64, 127, 255],
                    [0, 0, 0, 255],
                ]
            );
            assert_eq!(result.vertices.map(|v| v.specular), [0xff00_0000; 4]);
        }
        assert_eq!(
            prepare_known(
                &read_effect_quad(&command).unwrap().unwrap(),
                D3dFlatState {
                    manages_depth: false,
                    ..state
                },
                global_policy,
                false,
                [0.; 4]
            )
            .primary
            .policy
            .depth_enable,
            None,
        );
    }
}

#[test]
fn far_color_uses_actual_display_shifts_and_secondary_follows_primary() {
    assert_eq!(far_rgb(0x7fff, [6, 1, 4]), [255; 3]);
    assert_eq!(far_rgb(0x3fe0, [7, 2, 4]), [57, 255, 0]);
    // Native shifts operate on DWORDs and mask CL, including oversized values.
    assert_eq!(far_rgb(0x7fff, [38, 33, 36]), far_rgb(0x7fff, [6, 1, 4]));
    let entry = sprite([0, 0], 0x200);
    let mut command = far_packet(&entry, 0);
    write_inputs(&mut command);
    write_far_tail(&mut command);
    let quad = read_effect_quad(&command).unwrap().unwrap();
    let enabled = prepare_known(&quad, renderer(), 8, true, [0.; 4]);
    let disabled = prepare_known(&quad, renderer(), 8, false, [0.; 4]);
    assert_eq!(
        enabled.continuation,
        EffectQuadContinuation::SecondaryPassRequired
    );
    assert_eq!(disabled.continuation, EffectQuadContinuation::Complete);
    assert_eq!(enabled.primary, disabled.primary);
}

#[test]
fn secondary_continuation_preserves_zero_depth_for_primary_pixel_admission() {
    let entry = sprite([0, 0], 0x200);
    let mut command = packet(&entry, 0);
    write_inputs(&mut command);
    command.write_u32(4, 0);
    let quad = read_effect_quad(&command).unwrap().unwrap();
    let submission = prepare_known(&quad, renderer(), 8, true, [0.; 4]);
    assert_eq!(
        submission.continuation,
        EffectQuadContinuation::SecondaryPassRequired
    );
    assert!(submission.primary.vertices[0].rhw.is_infinite());
    assert!(submission.primary.vertices[0].depth.is_infinite());
}

#[test]
fn secondary_gate_requires_both_inputs_without_changing_primary_policy() {
    for far in [false, true] {
        for flags in [0, 8, 0x10, 0x20, 0x200, 0x208, 0x210, 0x220, 0x238] {
            let entry = sprite([0, 0], flags);
            let mut command = if far {
                far_packet(&entry, 0x4100)
            } else {
                packet(&entry, 0x4100)
            };
            write_inputs(&mut command);
            if far {
                write_far_tail(&mut command);
            }
            let quad = read_effect_quad(&command).unwrap().unwrap();
            let enabled = prepare_known(&quad, renderer(), 8, true, [0., 0., 1., 1.]);
            let disabled = prepare_known(&quad, renderer(), 8, false, [0., 0., 1., 1.]);
            assert_eq!(enabled.primary, disabled.primary);
            assert_eq!(
                enabled.continuation == EffectQuadContinuation::SecondaryPassRequired,
                flags & 0x200 != 0
            );
            assert_eq!(disabled.continuation, EffectQuadContinuation::Complete);
        }
    }
}

#[derive(Deserialize)]
struct NativeCase {
    flags: u16,
    mode: u32,
    policy: u32,
    manages: bool,
    near: u32,
    depth_enable: u32,
    depth_write: u32,
    depths: [i32; 4],
    coords: [[i16; 2]; 4],
    uv_bounds: [f32; 4],
    effect_color: u32,
    viewport: [i16; 4],
    vertices: [[u32; 8]; 4],
    draws: Vec<Vec<[u32; 8]>>,
    states: Vec<[u32; 2]>,
}

fn vertex_bits(vertex: D3dTlVertex) -> [u32; 8] {
    let [r, g, b, a] = vertex.diffuse;
    [
        vertex.x.to_bits(),
        vertex.y.to_bits(),
        vertex.depth.to_bits(),
        vertex.rhw.to_bits(),
        u32::from_be_bytes([a, r, g, b]),
        vertex.specular,
        vertex.uv[0].to_bits(),
        vertex.uv[1].to_bits(),
    ]
}

fn verify_native_cases(cases: &[NativeCase]) {
    let entry = sprite([0, 0], 0);
    for (index, case) in cases.iter().enumerate() {
        let quad = EffectQuad {
            vertices: std::array::from_fn(|i| FlatVertex {
                x: f32::from(case.coords[i][0]),
                y: f32::from(case.coords[i][1]),
                depth: case.depths[i],
            }),
            mode: Some(case.mode),
            sprite_mode: sprite_mode(case.mode),
            resource: resource(&entry),
            flags: case.flags,
            effect_rgb: std::array::from_fn(|i| (case.effect_color >> (i * 8)) as u8),
            shading: EffectQuadShading::Near,
        };
        let result = prepare_known(
            &quad,
            D3dFlatState {
                near_depth: case.near,
                manages_depth: case.manages,
                depth_enable: case.depth_enable,
                depth_write: case.depth_write,
                ..renderer()
            },
            case.policy,
            false,
            case.uv_bounds,
        )
        .primary;
        assert_eq!(
            result.vertices.map(vertex_bits),
            case.vertices,
            "native TL records case {index}"
        );
        let mut draws = Vec::new();
        for indices in result.triangles {
            let triangle = indices.map(|i| result.vertices[i]);
            let polygon = if case.mode & 0x20 != 0 {
                triangle.to_vec()
            } else {
                clip_polygon(
                    &triangle,
                    D3dViewportState {
                        bounds: case.viewport,
                        near_depth: case.near,
                        mode: case.mode,
                    },
                )
                .unwrap()
            };
            if polygon.len() >= 3 {
                draws.push(polygon.into_iter().map(vertex_bits).collect::<Vec<_>>());
            }
        }
        assert_eq!(
            draws, case.draws,
            "native ordered/clipped draws case {index}"
        );
        let state = |id| {
            case.states
                .iter()
                .rev()
                .find(|pair| pair[0] == id)
                .map(|pair| pair[1])
        };
        assert_eq!(
            result.policy.depth_enable,
            state(7),
            "native ZENABLE case {index}"
        );
        assert_eq!(
            result.policy.depth_write,
            state(14),
            "native ZWRITEENABLE case {index}"
        );
        assert_eq!(
            Some(u32::from(result.policy.perspective)),
            state(4),
            "native TEXTUREPERSPECTIVE case {index}"
        );
        assert_eq!(
            Some(u32::from(result.policy.fog.enabled())),
            state(28),
            "native FOGENABLE case {index}"
        );
        let factors = match result.policy.blend {
            D3dTextureBlend::Copy => (2, 1),
            D3dTextureBlend::Additive => (2, 2),
            D3dTextureBlend::InverseSourceColor => (1, 4),
            D3dTextureBlend::SourceAlpha => (5, 6),
        };
        assert_eq!(
            (Some(factors.0), Some(factors.1)),
            (state(19), state(20)),
            "native blend factors case {index}"
        );
    }
}

#[test]
fn native_tl_colors_flags_and_clipped_draws_match_original_executable() {
    // Original 004D78A0 with x87 FPCW=027F and intercepted COM/display/upload
    // effects. 12 bypass cases and 20 real original clipping cases; descriptor
    // flags vary, renderer+44 is null, and allocation bounds are supplied.
    let cases: Vec<NativeCase> =
        serde_json::from_str(include_str!("../tests/fixtures/d3d_effect_native.json")).unwrap();
    assert_eq!(cases.len(), 32);
    verify_native_cases(&cases);
}

#[test]
#[ignore = "requires INF_EFFECT_QUAD_ORACLE pointing to the local 1024-case native oracle JSON"]
fn full_local_native_oracle_matches_rust_submission_and_clipping() {
    let path =
        std::env::var("INF_EFFECT_QUAD_ORACLE").expect("explicit local oracle path required");
    let cases: Vec<NativeCase> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 1024);
    verify_native_cases(&cases);
}

#[derive(Deserialize)]
struct NativePrefixFixture {
    cases: Vec<NativePrefixCase>,
}

#[derive(Deserialize)]
struct NativePrefixCase {
    callback: String,
    flags: u16,
    mode: u32,
    policy: u32,
    manages: bool,
    near: u32,
    depth_enable: u32,
    depth_write: u32,
    coords: [[i16; 2]; 4],
    depths: [i32; 4],
    uv_bounds: [f32; 4],
    effect_color: u32,
    far_color: u32,
    fog: [u8; 4],
    secondary_enabled: bool,
    secondary_reached: bool,
    selector_ff: bool,
    viewport: [i16; 4],
    vertices: Vec<[u32; 8]>,
    draws: Vec<Vec<[u32; 8]>>,
    states: Vec<[u32; 2]>,
    events: Vec<String>,
}

#[test]
fn original_near_far_effects_preserve_primary_before_secondary_setup() {
    // Original consumer conversion, viewport clipping and COM fan arguments.
    // The authenticated checker hosts external upload/display/error/device
    // boundaries and stops before required secondary setup. No GPU pixels or
    // execution of these supplied inputs in a retail scene is established.
    let fixture: NativePrefixFixture = serde_json::from_str(include_str!(
        "../tests/fixtures/effect_secondary_prefix_native.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 40);
    for (index, case) in fixture.cases.iter().enumerate() {
        let far = match case.callback.as_str() {
            "near" => false,
            "far" => true,
            _ => panic!("native effect family"),
        };
        let entry = sprite(if case.selector_ff { [255; 2] } else { [0; 2] }, case.flags);
        let mut command = if far {
            far_packet(&entry, case.mode)
        } else {
            packet(&entry, case.mode)
        };
        if case.selector_ff {
            assert!(read_effect_quad(&command).unwrap().is_none());
            assert!(
                case.vertices.is_empty()
                    && case.draws.is_empty()
                    && case.states.is_empty()
                    && case.events.is_empty()
            );
            continue;
        }
        for i in 0..4 {
            command.write_u32(
                i * 8,
                u32::from(case.coords[i][0] as u16) | (u32::from(case.coords[i][1] as u16) << 16),
            );
            command.write_u32(i * 8 + 4, case.depths[i] as u32);
        }
        command.write_u32(0x28, case.effect_color);
        if far {
            command.write_u32(0x2c, case.far_color);
            for (i, byte) in case.fog.into_iter().enumerate() {
                command.write_byte(0x30 + i, byte);
            }
        }
        let result = prepare_known(
            &read_effect_quad(&command).unwrap().unwrap(),
            D3dFlatState {
                material_shifts: [6, 1, 4],
                near_depth: case.near,
                manages_depth: case.manages,
                depth_enable: case.depth_enable,
                depth_write: case.depth_write,
            },
            case.policy,
            case.secondary_enabled,
            case.uv_bounds,
        );
        assert_eq!(
            result.continuation == EffectQuadContinuation::SecondaryPassRequired,
            case.secondary_reached,
            "continuation case{index}"
        );
        let primary = result.primary;
        assert_eq!(
            primary.vertices.map(vertex_bits).as_slice(),
            case.vertices.as_slice(),
            "TL case{index}"
        );
        let mut draws = Vec::new();
        for indices in primary.triangles {
            let triangle = indices.map(|i| primary.vertices[i]);
            let polygon = if case.mode & 0x20 != 0 {
                triangle.to_vec()
            } else {
                clip_polygon(
                    &triangle,
                    D3dViewportState {
                        bounds: case.viewport,
                        near_depth: primary.near_depth,
                        mode: primary.mode,
                    },
                )
                .unwrap()
            };
            if polygon.len() >= 3 {
                draws.push(polygon.into_iter().map(vertex_bits).collect::<Vec<_>>());
            }
        }
        assert_eq!(draws, case.draws, "primary fans case{index}");
        let mut states = Vec::new();
        if far {
            let [r, g, b] = far_rgb(case.far_color, [6, 1, 4]);
            states.push([34, u32::from_be_bytes([0, r, g, b])]);
        }
        if case.manages {
            states.extend([
                [7, primary.policy.depth_enable.unwrap()],
                [14, primary.policy.depth_write.unwrap()],
            ]);
        } else {
            assert_eq!(
                (primary.policy.depth_enable, primary.policy.depth_write),
                (None, None)
            );
        }
        states.push([4, u32::from(primary.policy.perspective)]);
        let blend = match primary.policy.blend {
            D3dTextureBlend::Copy => [2, 1],
            D3dTextureBlend::Additive => [2, 2],
            D3dTextureBlend::InverseSourceColor => [1, 4],
            D3dTextureBlend::SourceAlpha => [5, 6],
        };
        states.extend([[19, blend[0]], [20, blend[1]]]);
        if case.flags & 0x30 != 0 {
            states.extend([[28, 0], [28, 0]]);
        } else {
            states.push([28, u32::from(primary.policy.fog.enabled())]);
        }
        assert_eq!(
            states, case.states,
            "ordered primary device states case{index}"
        );
        let mut events = vec!["converted", "primary_draw", "primary_draw"];
        if case.secondary_reached {
            events.push("secondary_required");
        }
        assert_eq!(
            events, case.events,
            "reached primary/secondary order case{index}"
        );
    }
}
