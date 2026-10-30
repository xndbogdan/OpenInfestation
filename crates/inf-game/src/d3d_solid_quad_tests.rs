use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    mode: u32,
    near: u32,
    manages: bool,
    depth_enable: u32,
    depth_write: u32,
    shifts: [u32; 3],
    color: u32,
    far_color: Option<u32>,
    fog: Option<[u8; 4]>,
    depths: [i32; 4],
    coords: [[i16; 2]; 4],
    vertices: [[u32; 8]; 4],
    draws: [[[u32; 8]; 3]; 2],
    states: Vec<[u32; 2]>,
}

fn bits(v: D3dTlVertex) -> [u32; 8] {
    let [r, g, b, a] = v.diffuse;
    [
        v.x.to_bits(),
        v.y.to_bits(),
        v.depth.to_bits(),
        v.rhw.to_bits(),
        u32::from_be_bytes([a, r, g, b]),
        v.specular,
        v.uv[0].to_bits(),
        v.uv[1].to_bits(),
    ]
}

fn verify(cases: &[Case]) {
    for case in cases {
        let quad = SolidQuad {
            vertices: std::array::from_fn(|i| FlatVertex {
                x: case.coords[i][0].into(),
                y: case.coords[i][1].into(),
                depth: case.depths[i],
            }),
            mode: case.mode,
            color: if let Some(far_color) = case.far_color {
                SolidQuadColor::Fog {
                    material: case.color,
                    far_color,
                    amount: case.fog.unwrap(),
                }
            } else {
                SolidQuadColor::Material(case.color)
            },
        };
        let state = D3dFlatState {
            material_shifts: case.shifts,
            near_depth: case.near,
            manages_depth: case.manages,
            depth_enable: case.depth_enable,
            depth_write: case.depth_write,
        };
        let result = prepare_solid_quad(&quad, state);
        let vertices = result.vertices.map(bits);
        assert_eq!(vertices, case.vertices);
        assert_eq!(result.triangles.map(|t| t.map(|i| vertices[i])), case.draws);
        let state = |id| case.states.iter().rev().find(|v| v[0] == id).map(|v| v[1]);
        assert_eq!(result.policy.depth_enable, state(7));
        assert_eq!(result.policy.depth_write, state(14));
        assert_eq!(Some(u32::from(result.policy.fog_enabled)), state(28));
        assert_eq!(state(19), Some(2));
        assert_eq!(
            state(20),
            Some(if result.policy.blend == D3dFlatBlend::Additive {
                2
            } else {
                1
            })
        );
    }
}

#[test]
fn original_solid_quad_tl_records_and_state_receipts_match() {
    // Eight selected original 004D1BB0 executions from512 cases, x87 027F.
    // Platform/COM effects intercepted; mode20 bypassed viewport clipping.
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../tests/fixtures/d3d_solid_quad_native.json")).unwrap();
    assert_eq!(cases.len(), 8);
    verify(&cases);
}

#[test]
fn original_far_solid_quad_cpu_fog_tl_records_and_state_receipts_match() {
    // Sixteen unmodified 004D2170 executions, x87CW027F. Display/COM
    // returns supplied; mode20 bypasses the independently owned clipper.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/d3d_solid_quad_far_native.json"
    ))
    .unwrap();
    let cases: Vec<Case> = serde_json::from_value(fixture["cases"].clone()).unwrap();
    assert_eq!(cases.len(), 16);
    verify(&cases);
}

#[test]
#[ignore = "requires INF_SOLID_QUAD_ORACLE pointing to the local512-case PE output"]
fn full_local_solid_quad_native_oracle() {
    let cases: Vec<Case> = serde_json::from_slice(
        &std::fs::read(std::env::var("INF_SOLID_QUAD_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(cases.len(), 512);
    verify(&cases);
}

#[test]
fn exact_star_packet_ignores_unused_tail_and_preserves_full_signed_depth() {
    let mut command = PrimitiveCommand::new(PrimitiveCallback::FlatQuadNear, 0, 3);
    for (i, [x, y]) in [[1i16, 2i16], [2, 2], [2, 3], [1, 3]]
        .into_iter()
        .enumerate()
    {
        command.write_u32(i * 8, u32::from(x as u16) | (u32::from(y as u16) << 16));
        command.write_u32(i * 8 + 4, 0x7fffffff);
    }
    command.write_u32(0x20, 0);
    command.write_u32(0x24, 0x7fff);
    let quad = read_solid_quad(&command).unwrap();
    assert_eq!(quad.vertices.map(|v| v.depth), [i32::MAX; 4]);
    let state = D3dFlatState {
        material_shifts: [6, 1, 4],
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    };
    let prepared = prepare_solid_quad(&quad, state);
    assert_eq!(prepared.vertices.map(|v| v.diffuse), [[255; 4]; 4]);
    for offset in 0..0x28 {
        let mut missing = command.clone();
        missing.payload[offset] = None;
        assert!(matches!(
            read_solid_quad(&missing),
            Err(SoftwarePixelError::Unknown { .. })
        ));
    }
    command.payload[0x28..].fill(Some(0x12));
    assert_eq!(
        prepare_solid_quad(&read_solid_quad(&command).unwrap(), state),
        prepared
    );
    let mut frame = crate::software_frame::SoftwareFrame::new(4, 4, [0; 4]);
    let mut pipeline = crate::software_frame::D3dRasterState {
        viewport: [0, 0, 3, 3],
        depth_enabled: true,
        depth_write: true,
        depth_function: 4,
        alpha_blend_enabled: true,
        shade_mode: 2,
        cull_mode: 1,
    };
    assert_eq!(
        frame
            .fill_flat_submission(&prepared, 64, &mut pipeline)
            .unwrap(),
        1
    );
    assert_eq!(frame.pixel_rgba(1, 2), Some([255; 4]));
    assert_eq!(frame.pixel_rgba(2, 2), Some([0; 4]));
    // Native TL conversion rounds 1 - 64/i32::MAX to exactly 1.0f32;
    // the authored stars pass the inherited LESSEQUAL depth comparison.
    assert_eq!(frame.pixel_depth(1, 2), Some(1.));
    frame.clear_depth();
    assert_eq!(frame.pixel_depth(1, 2), Some(1.));
    assert_eq!(frame.pixel_rgba(1, 2), Some([255; 4]));
}

#[derive(Deserialize)]
struct TriangleCase {
    far: bool,
    mode: u32,
    material: u32,
    shifts: [u32; 3],
    near: u32,
    manages: u8,
    depths: [i32; 3],
    fan: [[u32; 8]; 3],
    states: Vec<[u32; 2]>,
    far_color: Option<u32>,
    fog: Option<[u8; 3]>,
}

#[test]
fn original_solid_triangle_tl_records_and_state_receipts_match() {
    // 512 unmodified 004BFAB0/004BFE80 executions (check_solid_triangle_
    // consumers.py), x87 CW 027F, renderer +3C/+40 = 2/7. Display/COM
    // effects intercepted; mode 20 bypasses viewport clipping, so the drawn
    // fan is the prepared vertices.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/d3d_solid_triangle_native.json"
    ))
    .unwrap();
    let cases: Vec<TriangleCase> = serde_json::from_value(fixture["cases"].clone()).unwrap();
    assert_eq!(cases.len(), 512);
    let coords = [[-123i16, 234], [32767, -32768], [1, 2]];
    for case in &cases {
        let triangle = SolidTriangle {
            vertices: std::array::from_fn(|i| FlatVertex {
                x: coords[i][0].into(),
                y: coords[i][1].into(),
                depth: case.depths[i],
            }),
            mode: case.mode,
            color: if case.far {
                SolidColor::Fog {
                    material: case.material,
                    far_color: case.far_color.unwrap(),
                    amount: case.fog.unwrap(),
                }
            } else {
                SolidColor::Material(case.material)
            },
        };
        let state = D3dFlatState {
            material_shifts: case.shifts,
            near_depth: case.near,
            manages_depth: case.manages != 0,
            depth_enable: 2,
            depth_write: 7,
        };
        let result = prepare_solid_triangle(&triangle, state);
        assert_eq!(result.vertices.map(bits), case.fan);
        assert_eq!(result.triangles, [[0, 1, 2]]);
        let state = |id| case.states.iter().rev().find(|v| v[0] == id).map(|v| v[1]);
        assert_eq!(result.policy.depth_enable, state(7));
        assert_eq!(result.policy.depth_write, state(14));
        assert_eq!(Some(u32::from(result.policy.fog_enabled)), state(28));
        assert_eq!(state(19), Some(2));
        assert_eq!(
            state(20),
            Some(if result.policy.blend == D3dFlatBlend::Additive {
                2
            } else {
                1
            })
        );
    }
}

#[test]
fn a_solid_triangle_packet_reads_its_short_layout() {
    let mut near = PrimitiveCommand::new(PrimitiveCallback::SolidTriangleNear, 0, 2);
    let mut far = PrimitiveCommand::new(PrimitiveCallback::SolidTriangleFar, 0, 3);
    for command in [&mut near, &mut far] {
        for i in 0..3 {
            command.write_u32(i * 8, (i as u32 + 1) | (2 << 16));
            command.write_u32(i * 8 + 4, 100 + i as u32);
        }
        command.write_u32(0x18, 0x22);
        command.write_u32(0x1c, 0x7fff);
    }
    let triangle = read_solid_triangle(&near).unwrap();
    assert_eq!(triangle.mode, 0x22);
    assert!(matches!(triangle.color, SolidColor::Material(0x7fff)));
    assert_eq!(triangle.vertices.map(|v| v.depth), [100, 101, 102]);
    // Far needs its color and three fog bytes; the block's last byte is
    // padding.
    assert!(read_solid_triangle(&far).is_err());
    far.write_u32(0x20, 0x1234);
    for i in 0..3 {
        far.write_byte(0x24 + i, 10 * i as u8);
    }
    let triangle = read_solid_triangle(&far).unwrap();
    assert!(matches!(
        triangle.color,
        SolidColor::Fog {
            material: 0x7fff,
            far_color: 0x1234,
            amount: [0, 10, 20]
        }
    ));
    assert!(read_solid_quad(&near).is_err());
}
