use super::*;

fn renderer() -> D3dFlatState {
    D3dFlatState {
        material_shifts: [6, 1, 4],
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}
fn pipeline() -> D3dRasterState {
    D3dRasterState {
        viewport: [0, 0, 8, 8],
        depth_enabled: true,
        depth_write: true,
        depth_function: 4,
        alpha_blend_enabled: true,
        shade_mode: 2,
        cull_mode: 1,
    }
}
fn triangle() -> FlatTriangle {
    FlatTriangle {
        vertices: [(0., 0., 128), (4., 0., 128), (0., 4., 128)].map(|(x, y, depth)| FlatVertex {
            x,
            y,
            depth,
        }),
        mode: 0x20,
        material: MaterialValue::from_raw(0x7fff),
        rgb: [[128; 3]; 3],
    }
}

#[test]
fn gouraud_and_transformed_depth_are_interpolated_at_integer_centers() {
    let mut t = triangle();
    t.vertices[0].depth = 64;
    t.vertices[1].depth = 128;
    t.vertices[2].depth = 256;
    t.rgb = [[128, 0, 0], [0, 128, 0], [0, 0, 128]];
    let mut f = SoftwareFrame::new(8, 8, [0; 4]);
    assert_eq!(
        f.fill_flat_triangle(&t, renderer(), &mut pipeline())
            .unwrap(),
        10
    );
    assert_eq!(f.pixel_rgba(0, 0), Some([248, 0, 0, 255]));
    assert_eq!(f.pixel_rgba(1, 1), Some([124, 62, 62, 255]));
    assert_eq!(f.pixel_depth(1, 1), Some(0.3125));
    assert_eq!(f.pixel_rgba(3, 1), Some([0; 4])); // excluded hypotenuse
}

#[test]
fn adjacent_additive_triangles_cover_each_pixel_once_in_either_winding() {
    for reverse in [false, true] {
        let mut first = triangle();
        first.mode |= 2;
        first.material = MaterialValue::from_raw(0x0421);
        let mut second = first;
        second.vertices = [
            first.vertices[1],
            FlatVertex {
                x: 4.,
                y: 4.,
                depth: 128,
            },
            first.vertices[2],
        ];
        if reverse {
            first.vertices.swap(1, 2);
            second.vertices.swap(1, 2);
        }
        let mut f = SoftwareFrame::new(6, 6, [0, 0, 0, 255]);
        let mut state = pipeline();
        let a = f
            .fill_flat_triangle(&first, renderer(), &mut state)
            .unwrap();
        let b = f
            .fill_flat_triangle(&second, renderer(), &mut state)
            .unwrap();
        assert_eq!(a + b, 16);
        for y in 0..6 {
            for x in 0..6 {
                assert_eq!(
                    f.pixel_rgba(x, y),
                    Some(if x < 4 && y < 4 {
                        [8, 8, 8, 255]
                    } else {
                        [0, 0, 0, 255]
                    })
                );
                assert_eq!(f.pixel_depth(x, y), Some(1.));
            }
        }
    }
}

#[test]
fn additive_preserves_depth_and_inherited_enable_then_opaque_restores_writes() {
    let mut f = SoftwareFrame::new(8, 8, [10, 20, 30, 255]);
    let mut state = pipeline();
    let mut t = triangle();
    t.mode |= 2;
    t.material = MaterialValue::from_raw(0x0421);
    f.fill_flat_triangle(&t, renderer(), &mut state).unwrap();
    assert_eq!(f.pixel_rgba(1, 1), Some([18, 28, 38, 255]));
    assert_eq!(f.pixel_depth(1, 1), Some(1.));
    assert!(!state.depth_write);
    state.depth_enabled = false;
    f.fill_flat_triangle(&t, renderer(), &mut state).unwrap();
    assert!(!state.depth_enabled);
    t.mode &= !2;
    f.fill_flat_triangle(&t, renderer(), &mut state).unwrap();
    assert!(state.depth_enabled && state.depth_write);
    assert_eq!(f.pixel_depth(1, 1), Some(0.5));
    assert_eq!(f.pixel_rgba(1, 1), Some([8, 8, 8, 255]));
}

#[test]
fn unmanaged_depth_and_disabled_alpha_blending_remain_live_inputs() {
    let mut f = SoftwareFrame::new(8, 8, [50; 4]);
    let mut r = renderer();
    r.manages_depth = false;
    let mut state = pipeline();
    state.alpha_blend_enabled = false;
    let mut t = triangle();
    t.mode |= 2;
    t.material = MaterialValue::from_raw(0x0421);
    f.fill_flat_triangle(&t, r, &mut state).unwrap();
    assert!(state.depth_write);
    assert_eq!(f.pixel_depth(1, 1), Some(0.5));
    assert_eq!(f.pixel_rgba(1, 1), Some([8, 8, 8, 0]));
}

#[test]
fn native_clip_identity_is_inclusive_and_crossing_draws_a_clipped_fan() {
    let mut t = triangle();
    t.mode = 0;
    let mut f = SoftwareFrame::new(8, 8, [3; 4]);
    let mut state = pipeline();
    state.viewport = [0, 0, 4, 4];
    assert_eq!(
        f.fill_flat_triangle(&t, renderer(), &mut state).unwrap(),
        10
    );
    let before = f.rgba.clone();
    t.vertices[0].x = -1.;
    t.mode = 2;
    assert!(f.fill_flat_triangle(&t, renderer(), &mut state).unwrap() > 0);
    assert_ne!(f.rgba, before);
    assert!(!state.depth_write);
    t.mode |= 0x20;
    assert!(f.fill_flat_triangle(&t, renderer(), &mut state).unwrap() > 0);
}

#[test]
fn clipped_additive_fans_keep_shared_edges_single_covered() {
    for reverse in [false, true] {
        let mut first = triangle();
        first.mode = 2;
        first.material = MaterialValue::from_raw(0x0421);
        first.vertices =
            [(-2., -2.), (6., -2.), (-2., 6.)].map(|(x, y)| FlatVertex { x, y, depth: 128 });
        let mut second = first;
        second.vertices = [
            first.vertices[1],
            FlatVertex {
                x: 6.,
                y: 6.,
                depth: 128,
            },
            first.vertices[2],
        ];
        if reverse {
            first.vertices.swap(1, 2);
            second.vertices.swap(1, 2);
        }
        let mut f = SoftwareFrame::new(6, 6, [0, 0, 0, 255]);
        let mut state = pipeline();
        state.viewport = [1, 1, 4, 4];
        let a = f
            .fill_flat_triangle(&first, renderer(), &mut state)
            .unwrap();
        let b = f
            .fill_flat_triangle(&second, renderer(), &mut state)
            .unwrap();
        assert_eq!(a + b, 9);
        for y in 0..6 {
            for x in 0..6 {
                assert_eq!(
                    f.pixel_rgba(x, y),
                    Some(if (1..4).contains(&x) && (1..4).contains(&y) {
                        [8, 8, 8, 255]
                    } else {
                        [0, 0, 0, 255]
                    })
                );
                assert_eq!(f.pixel_depth(x, y), Some(1.));
            }
        }
    }
}

#[test]
fn a_clipped_triangle_fan_uses_rounded_intersection_colors() {
    let mut t = triangle();
    t.mode = 0;
    t.material = MaterialValue::from_raw(0x0421);
    t.vertices = [(-2., 0.), (2., 0.), (2., 4.)].map(|(x, y)| FlatVertex { x, y, depth: 128 });
    // Base channel 8 with shades 16/32 produces TL colors 1/2. Clipping
    // at x=0 rounds the +0.5 delta to zero; rasterizing the original
    // triangle and merely restricting its pixel loop would produce 2.
    t.rgb = [[16; 3], [32; 3], [32; 3]];
    let mut f = SoftwareFrame::new(6, 6, [0; 4]);
    f.fill_flat_triangle(&t, renderer(), &mut pipeline())
        .unwrap();
    assert_eq!(f.pixel_rgba(0, 0), Some([1, 1, 1, 255]));
    assert_eq!(f.pixel_depth(0, 0), Some(0.5));
}

#[test]
fn zero_depth_and_unsupported_device_policies_do_not_write_pixels() {
    let mut f = SoftwareFrame::new(8, 8, [3; 4]);
    let mut t = triangle();
    t.vertices[0].depth = 0;
    assert_eq!(
        f.fill_flat_triangle(&t, renderer(), &mut pipeline()),
        Err(SoftwarePixelError::NonFinite)
    );
    t = triangle();
    for (field, value) in [("ZFUNC", 2), ("SHADEMODE", 1), ("CULLMODE", 2)] {
        let mut state = pipeline();
        match field {
            "ZFUNC" => state.depth_function = value,
            "SHADEMODE" => state.shade_mode = value,
            _ => state.cull_mode = value,
        }
        assert_eq!(
            f.fill_flat_triangle(&t, renderer(), &mut state),
            Err(SoftwarePixelError::State { field, value })
        );
    }
    let mut r = renderer();
    r.depth_enable = 2;
    assert_eq!(
        f.fill_flat_triangle(&t, r, &mut pipeline()),
        Err(SoftwarePixelError::State {
            field: "ZENABLE",
            value: 2
        })
    );
    assert!(f.rgba.chunks_exact(4).all(|v| v == [3; 4]));
}

#[test]
fn headless_shade_and_shared_edge_smoke_artifact() {
    let mut f = SoftwareFrame::new(256, 128, [16, 20, 30, 255]);
    let mut t = triangle();
    t.vertices = [(8., 8.), (120., 8.), (8., 120.)].map(|(x, y)| FlatVertex { x, y, depth: 128 });
    t.rgb = [[128, 0, 0], [0, 128, 0], [0, 0, 128]];
    let mut state = pipeline();
    f.fill_flat_triangle(&t, renderer(), &mut state).unwrap();
    t.vertices =
        [(136., 8.), (248., 8.), (136., 120.)].map(|(x, y)| FlatVertex { x, y, depth: 128 });
    t.mode |= 2;
    t.rgb = [[64; 3]; 3];
    t.material = MaterialValue::from_raw(0x4210);
    f.fill_flat_triangle(&t, renderer(), &mut state).unwrap();
    t.vertices = [
        t.vertices[1],
        FlatVertex {
            x: 248.,
            y: 120.,
            depth: 128,
        },
        t.vertices[2],
    ];
    f.fill_flat_triangle(&t, renderer(), &mut state).unwrap();
    assert_eq!(f.pixel_rgba(180, 40), f.pixel_rgba(220, 80));
    if let Some(path) = std::env::var_os("INF_PIXEL_SMOKE_PPM") {
        let mut image = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
        image.extend(f.rgba.chunks_exact(4).flat_map(|c| c[..3].iter().copied()));
        std::fs::write(path, image).unwrap();
    }
}
