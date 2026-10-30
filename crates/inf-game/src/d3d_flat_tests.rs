use super::*;
use crate::software_frame::FlatVertex;
use inf_formats::MaterialValue;

fn state() -> D3dFlatState {
    D3dFlatState {
        material_shifts: [6, 1, 4],
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}

fn triangle(material: u32, rgb: [[u8; 3]; 3]) -> FlatTriangle {
    FlatTriangle {
        vertices: [
            FlatVertex {
                x: -4.0,
                y: 2.0,
                depth: 64,
            },
            FlatVertex {
                x: 6.0,
                y: 2.0,
                depth: 128,
            },
            FlatVertex {
                x: 0.0,
                y: 8.0,
                depth: 256,
            },
        ],
        mode: 0,
        material: MaterialValue::from_raw(material),
        rgb,
    }
}

#[test]
fn material_uses_live_display_shifts_without_rgb555_bit_replication() {
    let fixture = triangle(0x7fff, [[128; 3]; 3]);
    let result = prepare_flat_triangle(&fixture, state());
    assert_eq!(
        result.vertices.map(|v| v.diffuse),
        [[248, 248, 248, 255]; 3]
    );
    let mut display = state();
    display.material_shifts = [15, 7, 1];
    let result = prepare_flat_triangle(&triangle(0x00fc_8040, [[128; 3]; 3]), display);
    assert_eq!(result.vertices[0].diffuse, [248, 128, 64, 255]);
    // Native variable shifts mask their counts; AL shifts lose upper bits.
    display.material_shifts = [47, 39, 33];
    assert_eq!(
        prepare_flat_triangle(&triangle(0x00fc_8040, [[128; 3]; 3]), display).vertices[0].diffuse,
        [248, 128, 64, 255]
    );
}

#[test]
fn corner_shades_remain_distinct_and_brightness_normalizes_jointly() {
    let fixture = triangle(0x7d_e7, [[128; 3], [255; 3], [0, 128, 255]]);
    let result = prepare_flat_triangle(&fixture, state());
    assert_eq!(result.vertices[0].diffuse, [248, 120, 56, 255]);
    assert_eq!(result.vertices[1].diffuse, [255, 123, 58, 255]);
    assert_eq!(result.vertices[2].diffuse, [0, 120, 112, 255]);
}

#[test]
fn channels_use_nearest_even_after_modulation() {
    let fixture = triangle(0x0421, [[8, 24, 40]; 3]);
    assert_eq!(
        prepare_flat_triangle(&fixture, state()).vertices[0].diffuse,
        [0, 2, 2, 255]
    );
}

#[test]
fn depth_uses_rounded_reciprocal_signed_input_and_unsigned_near_word() {
    let mut fixture = triangle(0, [[0; 3]; 3]);
    let result = prepare_flat_triangle(&fixture, state());
    assert_eq!(
        result.vertices.map(|v| v.rhw),
        [1.0 / 64.0, 1.0 / 128.0, 1.0 / 256.0]
    );
    assert_eq!(result.vertices.map(|v| v.depth), [0.0, 0.5, 0.75]);
    fixture.vertices[0].depth = -128;
    fixture.vertices[1].depth = 3;
    fixture.vertices[2].depth = i32::MAX;
    let mut renderer = state();
    renderer.near_depth = 0xffff_ffff;
    let result = prepare_flat_triangle(&fixture, renderer);
    for (source, output) in fixture.vertices.iter().zip(result.vertices) {
        let reciprocal = (1.0 / f64::from(source.depth)) as f32;
        assert_eq!(output.rhw.to_bits(), reciprocal.to_bits());
        let expected = (1.0 - f64::from(renderer.near_depth) * f64::from(reciprocal)) as f32;
        assert_eq!(output.depth.to_bits(), expected.to_bits());
    }
}

#[test]
fn native_direct3d_vertex_records_match_original_executable() {
    // Selected outputs from 1,024 executions of original 004C12F0 in an
    // isolated x86 emulator, FPCW=027F. External display/COM effects were
    // stubbed; mode20 skipped viewport clipping. These are native output
    // DWORDs, not expected values recomputed through the Rust formulas.
    let fixtures = [
        (
            3907808251,
            [9, 0, 31],
            1024,
            [64, 2147483647, 16777217],
            [[73, 213, 167], [59, 102, 110], [81, 79, 106]],
            40,
            [
                [3245342720, 1015021568, 4278779648],
                [1065353208, 805306368, 4278961664],
                [1065352192, 864026623, 4279212288],
            ],
        ),
        (
            1916932050,
            [6, 1, 4],
            4294967295,
            [64, 16777217, 2147483647],
            [[74, 161, 121], [139, 180, 101], [94, 31, 201]],
            34,
            [
                [3430940672, 1015021568, 851827],
                [3279880191, 864026623, 1376086],
                [3212836864, 805306368, 1194722],
            ],
        ),
        (
            3707055486,
            [6, 1, 4],
            0,
            [-12345, -12345, 2147483647],
            [[6, 145, 38], [106, 168, 1], [49, 145, 222]],
            160,
            [
                [1065353216, 3098140912, 4278477895],
                [1065353216, 3098140912, 4283003906],
                [1065353216, 805306368, 4279582207],
            ],
        ),
        (
            1048646206,
            [6, 1, 4],
            4294967295,
            [-2147483648, 2147483647, 16777217],
            [[0, 154, 72], [88, 92, 179], [0, 4, 157]],
            96,
            [
                [1077936128, 2952790016, 4278232199],
                [3212836864, 805306368, 4279323391],
                [3279880191, 864026623, 4278191359],
            ],
        ),
    ];
    for (material, shifts, near, depths, shades, mode, expected) in fixtures {
        let mut source = triangle(material, shades);
        source.mode = mode;
        for (vertex, (xy, depth)) in source.vertices.iter_mut().zip(
            [[-123.0, 234.0], [32767.0, -32768.0], [1.0, 2.0]]
                .into_iter()
                .zip(depths),
        ) {
            vertex.x = xy[0];
            vertex.y = xy[1];
            vertex.depth = depth;
        }
        let result = prepare_flat_triangle(
            &source,
            D3dFlatState {
                material_shifts: shifts,
                near_depth: near,
                ..state()
            },
        );
        for (index, vertex) in result.vertices.into_iter().enumerate() {
            let [r, g, b, a] = vertex.diffuse;
            let packed = u32::from_be_bytes([a, r, g, b]);
            assert_eq!(
                [vertex.depth.to_bits(), vertex.rhw.to_bits(), packed],
                expected[index]
            );
            assert_eq!(vertex.x, source.vertices[index].x);
            assert_eq!(vertex.y, source.vertices[index].y);
            assert_eq!(vertex.specular, 0xff00_0000);
            assert_eq!(vertex.uv, [0.0; 2]);
        }
    }
}

#[test]
fn zero_depth_keeps_non_finite_native_output() {
    let mut fixture = triangle(0, [[0; 3]; 3]);
    fixture.vertices[0].depth = 0;
    let output = prepare_flat_triangle(&fixture, state()).vertices[0];
    assert_eq!(output.rhw, f32::INFINITY);
    assert_eq!(output.depth, f32::NEG_INFINITY);
    let mut renderer = state();
    renderer.near_depth = 0;
    assert!(prepare_flat_triangle(&fixture, renderer).vertices[0]
        .depth
        .is_nan());
}

#[test]
fn mode_and_depth_management_preserve_native_state_asymmetries() {
    let mut fixture = triangle(0, [[0; 3]; 3]);
    let mut renderer = state();
    renderer.depth_enable = 2;
    renderer.depth_write = 1;
    for manages in [false, true] {
        renderer.manages_depth = manages;
        for additive in [false, true] {
            for bypass in [false, true] {
                fixture.mode = 0x8000_0101 | u32::from(additive) * 2 | u32::from(bypass) * 0x20;
                let output = prepare_flat_triangle(&fixture, renderer);
                assert_eq!(
                    output.policy.depth_enable,
                    (manages && !additive).then_some(2)
                );
                assert_eq!(
                    output.policy.depth_write,
                    manages.then_some(if additive { 0 } else { 1 })
                );
                assert_eq!(
                    output.policy.blend,
                    if additive {
                        D3dFlatBlend::Additive
                    } else {
                        D3dFlatBlend::Copy
                    }
                );
                assert!(!output.policy.fog_enabled);
                assert_eq!(
                    output.clip,
                    if bypass {
                        D3dFlatClip::Bypass
                    } else {
                        D3dFlatClip::NativeViewport
                    }
                );
                assert_eq!(output.mode, fixture.mode);
                assert_eq!(
                    output.vertices[0].diffuse[3],
                    if additive { 0 } else { 255 }
                );
                assert_eq!(output.vertices[0].specular, 0xff00_0000);
                assert_eq!(output.vertices[0].uv, [0.0; 2]);
            }
        }
    }
}

fn e4_packet(far: bool) -> PrimitiveCommand<'static> {
    use crate::model_primitives::PrimitiveCallback::*;
    let callback = if far {
        SharedShadeQuadFar
    } else {
        SharedShadeQuadNear
    };
    let mut command = PrimitiveCommand::new(callback, 0, if far { 4 } else { 3 });
    for i in 0..4 {
        command.write_u32(i * 8, 0x0002_0001 + i as u32);
        command.write_u32(i * 8 + 4, 128 + i as u32);
    }
    command.write_u32(0x20, 0);
    command.write_u32(0x24, 0x7fff);
    command.write_rgb(0x28, [128, 64, 32]);
    command.write_byte(0x2b, 0);
    if far {
        command.write_u32(0x2c, 0xa1b2_c3d4);
        for i in 0..4 {
            command.write_byte(0x30 + i, 10 + i as u8);
        }
    }
    command
}

#[test]
fn e4_shared_shade_replicates_to_all_corners() {
    let quad = read_shared_shade_quad(&e4_packet(false)).unwrap();
    assert_eq!(quad.mode, 0);
    assert_eq!(quad.material, MaterialValue::from_raw(0x7fff));
    assert_eq!(quad.rgb, [[128, 64, 32]; 4]);
    assert_eq!(quad.fog, None);
    assert_eq!(quad.vertices[3].depth, 131);
}

#[test]
fn e4_far_carries_fog_and_validates_shape() {
    let quad = read_shared_shade_quad(&e4_packet(true)).unwrap();
    assert_eq!(
        quad.fog,
        Some(FlatFog {
            color: 0xa1b2_c3d4,
            fog: [10, 11, 12, 13]
        })
    );
    let mut short = e4_packet(true);
    short.payload.truncate(48);
    short.payload_blocks = 3;
    assert!(read_shared_shade_quad(&short).is_err());
    let mut wrong = e4_packet(false);
    wrong.callback = crate::model_primitives::PrimitiveCallback::FlatQuadNear;
    assert!(read_shared_shade_quad(&wrong).is_err());
}

#[test]
fn e4_prepare_matches_flat_triangle_modulation_uniformly() {
    // Same material/shade as the triangle test's first corner:
    // all four diffuse entries equal that corner's diffuse.
    let quad = read_shared_shade_quad(&e4_packet(false)).unwrap();
    let result = prepare_flat_quad(&quad, state());
    let single = prepare_flat_triangle(&triangle(0x7fff, [[128, 64, 32]; 3]), state());
    assert_eq!(
        result.vertices.map(|v| v.diffuse),
        [single.vertices[0].diffuse; 4]
    );
    assert_eq!(result.triangles, [[1, 2, 3], [1, 3, 0]]);
}

#[test]
fn far_flat_triangle_matches_256_original_executions() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/d3d_flat_triangle_far_native.json"
    ))
    .unwrap();
    let word = |value: &serde_json::Value| value.as_i64().unwrap() as u32;
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 256);
    for (index, case) in cases.iter().enumerate() {
        let mut source = triangle(word(&case["material"]), [[0; 3]; 3]);
        source.mode = word(&case["mode"]);
        for corner in 0..3 {
            source.rgb[corner] =
                std::array::from_fn(|channel| word(&case["shades"][corner][channel]) as u8);
            source.vertices[corner] = FlatVertex {
                x: [-123.0, 32767.0, 1.0][corner],
                y: [234.0, -32768.0, 2.0][corner],
                depth: case["depths"][corner].as_i64().unwrap() as i32,
            };
        }
        let fog = FlatFog {
            color: word(&case["far"]),
            fog: std::array::from_fn(|corner| word(&case["fog"][corner]) as u8),
        };
        let manages = case["manages"] == 1;
        let result = prepare_flat_triangle_far(
            &source,
            fog,
            D3dFlatState {
                material_shifts: std::array::from_fn(|i| word(&case["shifts"][i])),
                near_depth: word(&case["near"]),
                manages_depth: manages,
                depth_enable: 2,
                depth_write: 7,
            },
        );
        for (corner, vertex) in result.vertices.into_iter().enumerate() {
            let [r, g, b, a] = vertex.diffuse;
            let actual = [
                vertex.x.to_bits(),
                vertex.y.to_bits(),
                vertex.depth.to_bits(),
                vertex.rhw.to_bits(),
                u32::from_be_bytes([a, r, g, b]),
                vertex.specular,
                vertex.uv[0].to_bits(),
                vertex.uv[1].to_bits(),
            ];
            let expected: Vec<u32> = case["vertices"][corner]
                .as_array()
                .unwrap()
                .iter()
                .map(word)
                .collect();
            assert_eq!(actual.to_vec(), expected, "case {index} corner {corner}");
        }
        // The native SetRenderState sequence equals the near policy's writes.
        let additive = source.mode & 2 != 0;
        let mut states = Vec::new();
        if let Some(value) = result.policy.depth_enable {
            states.push([7, value]);
        }
        if let Some(value) = result.policy.depth_write {
            states.push([14, value]);
        }
        states.push([19, 2]);
        states.push([20, if additive { 2 } else { 1 }]);
        if additive {
            states.push([28, 0]);
        }
        states.push([28, 0]);
        let native: Vec<[u32; 2]> = case["states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| [word(&pair[0]), word(&pair[1])])
            .collect();
        assert_eq!(states, native, "case {index} states");
        assert!(!result.policy.fog_enabled);
        assert_eq!(result.clip, D3dFlatClip::Bypass);
    }
}

#[test]
fn flat_quads_match_512_original_executions() {
    // Original 004D2950 (near) and 004D3260 (far) draw two three-vertex
    // fans; mode 20 bypassed clipping, so the drawn TL DWORDs are prepared.
    let receipt: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/d3d_flat_quad_native.json")).unwrap();
    let word = |value: &serde_json::Value| value.as_i64().unwrap() as u32;
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 512);
    for (index, case) in cases.iter().enumerate() {
        let shade = std::array::from_fn(|channel| word(&case["shade"][channel]) as u8);
        let quad = FlatQuad {
            vertices: std::array::from_fn(|corner| FlatVertex {
                x: [-123.0, 32767.0, 1.0, -7.0][corner],
                y: [234.0, -32768.0, 2.0, -9.0][corner],
                depth: case["depths"][corner].as_i64().unwrap() as i32,
            }),
            mode: word(&case["mode"]),
            material: MaterialValue::from_raw(word(&case["material"])),
            rgb: [shade; 4],
            fog: case["far"].as_bool().unwrap().then(|| FlatFog {
                color: word(&case["far_color"]),
                fog: std::array::from_fn(|corner| word(&case["fog"][corner]) as u8),
            }),
        };
        let manages = case["manages"] == 1;
        let result = prepare_flat_quad(
            &quad,
            D3dFlatState {
                material_shifts: std::array::from_fn(|i| word(&case["shifts"][i])),
                near_depth: word(&case["near"]),
                manages_depth: manages,
                depth_enable: 2,
                depth_write: 7,
            },
        );
        for (fan, corners) in result.triangles.iter().enumerate() {
            for (position, corner) in corners.iter().enumerate() {
                let vertex = result.vertices[*corner];
                let [r, g, b, a] = vertex.diffuse;
                let actual = [
                    vertex.x.to_bits(),
                    vertex.y.to_bits(),
                    vertex.depth.to_bits(),
                    vertex.rhw.to_bits(),
                    u32::from_be_bytes([a, r, g, b]),
                    vertex.specular,
                    vertex.uv[0].to_bits(),
                    vertex.uv[1].to_bits(),
                ];
                let expected: Vec<u32> = case["fans"][fan][position]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(word)
                    .collect();
                assert_eq!(
                    actual.to_vec(),
                    expected,
                    "case {index} fan {fan} {position}"
                );
            }
        }
        // The native SetRenderState sequence equals the flat policy's writes.
        let additive = quad.mode & 2 != 0;
        let mut states = Vec::new();
        if let Some(value) = result.policy.depth_enable {
            states.push([7, value]);
        }
        if let Some(value) = result.policy.depth_write {
            states.push([14, value]);
        }
        states.push([19, 2]);
        states.push([20, if additive { 2 } else { 1 }]);
        if additive {
            states.push([28, 0]);
        }
        states.push([28, 0]);
        let native: Vec<[u32; 2]> = case["states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| [word(&pair[0]), word(&pair[1])])
            .collect();
        assert_eq!(states, native, "case {index} states");
        assert!(!result.policy.fog_enabled);
        assert_eq!(result.clip, D3dFlatClip::Bypass);
    }
}
