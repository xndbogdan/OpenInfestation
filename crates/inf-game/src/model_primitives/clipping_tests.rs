use super::*;

fn clip_quad<E>(
    vertices: [NativeProjectedVertex; 4],
    colors: [[u8; 3]; 4],
    projection: ModelProjection,
    tables: &PrimitiveTables,
) -> Result<Vec<ClippedVertex>, PrimitiveError<E>> {
    let raw = vertices.map(|v| {
        let mut p = ModelVertexProjection::default();
        p.write_u32(0, v.xy);
        p.write_u32(4, v.depth);
        p.bytes[8] = Some(v.clip);
        p.bytes[9] = v.fog;
        p
    });
    super::clip_polygon(
        ClipSource::Quad(std::array::from_fn(|i| {
            (
                &raw[i],
                ModelShade {
                    rgb: Some(colors[i]),
                    high_byte: None,
                },
            )
        })),
        projection,
        tables,
        0xe2,
    )
}

#[test]
fn clip40_early_abort_does_not_read_current_or_later_unknown_geometry() {
    let mut early = ModelVertexProjection::default();
    early.bytes[8] = Some(0x40);
    let unknown = ModelVertexProjection::default();
    assert!(super::clip_polygon::<()>(
        ClipSource::Quad([
            (&early, ModelShade::default()),
            (&unknown, ModelShade::default()),
            (&unknown, ModelShade::default()),
            (&early, ModelShade::default())
        ]),
        projection(),
        &tables(),
        0xe2,
    )
    .unwrap()
    .is_empty());
}

fn tables() -> PrimitiveTables {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let executable = std::fs::read(root.join("Infest.exe")).unwrap();
    PrimitiveTables::from_executable(inf_formats::FrontendExecutable::parse(&executable).unwrap())
        .unwrap()
}
fn projection() -> ModelProjection {
    ModelProjection {
        focal: [64, 64],
        viewport: [640, 480],
        center: [320, 240],
        fade: [100, 200],
    }
}
fn front(x: i32, y: i32) -> NativeProjectedVertex {
    NativeProjectedVertex {
        xy: pack_xy(x, y),
        depth: 128,
        clip: 0x12,
        fog: Some(90),
    }
}
fn behind(x: i32, y: i32) -> NativeProjectedVertex {
    NativeProjectedVertex {
        xy: pack_xy(x, y),
        depth: 0,
        clip: 0xc0,
        fog: None,
    }
}

#[test]
fn original_uv_and_projection_constants_are_preserved() {
    let tables = tables();
    assert_eq!(tables.quad_u, [0, 65535, 65535, 0]);
    assert_eq!(tables.quad_v, [0, 0, 65535, 65535]);
    assert_eq!(tables.clip_projection_bits, [0x3c80_0000, 0xbc80_0000]);
}

#[test]
fn triangle_uses_first_three_quad_uv_rows_and_last_to_first_edge() {
    let projections = [front(330, 230), front(350, 230), behind(0, 0)].map(|v| {
        let mut p = ModelVertexProjection::default();
        p.write_u32(0, v.xy);
        p.write_u32(4, v.depth);
        p.bytes[8] = Some(v.clip);
        p.bytes[9] = v.fog;
        p
    });
    let output = clip_polygon::<()>(
        ClipSource::Triangle(std::array::from_fn(|i| {
            (&projections[i], ModelShade::from_word(0x9980_8080))
        })),
        projection(),
        &tables(),
        0xde,
    )
    .unwrap();
    assert_eq!(output.len(), 4);
    assert_eq!(output[0].projected.depth, 64);
    assert_eq!(output[0].uv, [32255, 32255]);
    assert_eq!(output[1].uv, [0, 0]);
    assert_eq!(output[2].uv, [65535, 0]);
    assert_eq!(output[3].uv, [65535, 32255]);
    assert_eq!(
        output.iter().map(|v| v.high_byte).collect::<Vec<_>>(),
        [None, Some(0x99), Some(0x99), None]
    );
}

#[test]
fn retained_unknown_high_and_intersection_unwritten_high_do_not_become_zero() {
    let mut front = ModelVertexProjection::default();
    front.write_u32(0, pack_xy(330, 230));
    front.write_u32(4, 128);
    front.bytes[8] = Some(0x12);
    front.bytes[9] = Some(90);
    let mut behind = front;
    behind.write_u32(0, 0);
    behind.write_u32(4, 0);
    behind.bytes[8] = Some(0xc0);
    let output = clip_polygon::<()>(
        ClipSource::Triangle([
            (
                &front,
                ModelShade {
                    rgb: Some([128; 3]),
                    high_byte: None,
                },
            ),
            (&front, ModelShade::from_word(0x7180_8080)),
            (&behind, ModelShade::from_word(0x4280_8080)),
        ]),
        projection(),
        &tables(),
        0xde,
    )
    .unwrap();
    assert_eq!(
        output.iter().map(|v| v.high_byte).collect::<Vec<_>>(),
        [None, None, Some(0x71), None]
    );
    assert!(output.iter().all(|v| v.rgb == Some([128; 3])));
}

#[test]
fn one_front_corner_becomes_triangle_with_intersections_first() {
    let result = clip_quad::<()>(
        [front(330, 230), behind(0, 0), behind(0, 0), behind(0, 0)],
        [[128; 3]; 4],
        projection(),
        &tables(),
    )
    .unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(
        result.iter().map(|v| v.projected.depth).collect::<Vec<_>>(),
        [64, 128, 64]
    );
    assert_eq!(result[0].projected.xy, pack_xy(330, 230));
    assert_eq!(result[0].uv, [0, 32255]);
    assert_eq!(result[2].uv, [32255, 0]);
    assert_eq!(result[0].projected.fog, Some(0));
    assert_eq!(result[1].projected.fog, Some(90));
    assert_eq!(result[0].rgb, Some([128; 3]));
}

#[test]
fn counts_retain_native_three_four_five_and_unemitted_six() {
    for (mask, count) in [(0, 0), (1, 3), (3, 4), (7, 5), (15, 4), (5, 6)] {
        let vertices = std::array::from_fn(|i| {
            if mask & (1 << i) != 0 {
                front(330, 230)
            } else {
                behind(0, 0)
            }
        });
        assert_eq!(
            clip_quad::<()>(vertices, [[0; 3]; 4], projection(), &tables())
                .unwrap()
                .len(),
            count
        );
    }
}

#[test]
fn shade_bytes_are_interpolated_even_for_uv_only_output() {
    let result = clip_quad::<()>(
        [front(330, 230), behind(0, 0), behind(0, 0), behind(0, 0)],
        [[200, 50, 0], [0, 100, 255], [0; 3], [0, 100, 255]],
        projection(),
        &tables(),
    )
    .unwrap();
    assert_eq!(result[0].rgb, Some([101, 74, 125]));
}

#[test]
fn forbidden_clip_class_abandons_polygon_and_division_fault_stays_explicit() {
    let mut vertex = behind(0, 0);
    vertex.clip = 0x40;
    assert!(
        clip_quad::<()>([vertex; 4], [[0; 3]; 4], projection(), &tables())
            .unwrap()
            .is_empty()
    );
    let mut bad = projection();
    bad.focal[0] = 0;
    assert!(matches!(
        clip_quad::<()>(
            [front(330, 230), behind(0, 0), behind(0, 0), behind(0, 0)],
            [[0; 3]; 4],
            bad,
            &tables()
        ),
        Err(PrimitiveError::Unsupported { .. })
    ));
}

#[test]
fn q31_division_saturates_native_zero_zero_and_signed_limits() {
    assert_eq!(divide_q31(0, 0), i32::MAX);
    assert_eq!(divide_q31(1, 2), 0x4000_0000);
    assert_eq!(divide_q31(-1, 2), -0x4000_0000);
    assert_eq!(divide_q31(i32::MIN, i32::MAX), -i32::MAX);
    assert_eq!(divide_q31(i32::MIN, i32::MIN), i32::MAX);
}
