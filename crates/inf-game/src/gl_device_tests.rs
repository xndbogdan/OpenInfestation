use super::*;
use crate::{d3d_textured::D3dTextureFog, frame_device::DeviceTexturing};

fn tl(x: f32, y: f32, rhw: f32, uv: [f32; 2], specular: u32) -> D3dTlVertex {
    D3dTlVertex {
        x,
        y,
        depth: 0.25,
        rhw,
        diffuse: [10, 20, 30, 40],
        specular,
        uv,
    }
}

fn texturing(texture: &CpuTexture, perspective: bool, fog: D3dTextureFog) -> DeviceTexturing<'_> {
    DeviceTexturing {
        texture,
        sampling: crate::software_frame::D3dTextureSampling {
            min_filter: 1,
            mag_filter: 1,
            texture_blend: 4,
            address: 1,
            color_key_enabled: false,
            alpha_test_enabled: false,
        },
        perspective,
        fog,
    }
}

#[test]
fn device_vertices_keep_the_tl_position_depth_and_diffuse() {
    let vertex = device_vertex(&tl(3.5, -2.0, 0.5, [0.25, 0.75], 0x8000_0000), None);
    assert_eq!(vertex.position, [3.5, -2.0, 0.25]);
    assert_eq!(vertex.diffuse, [10, 20, 30, 40]);
    // Flat fans neither sample nor fog: q = 1 and full visibility.
    assert_eq!(vertex.texture, [0.0, 0.0, 1.0]);
    assert_eq!(vertex.fog, [0, 0, 0, 255]);
}

#[test]
fn perspective_uv_rides_with_reciprocal_depth_and_affine_uv_does_not() {
    let texture = crate::d3d_texture::test_texture([2, 2], |_, _| [1, 2, 3, 255]);
    let vertex = tl(0.0, 0.0, 0.5, [0.25, 0.75], 0xff00_0000);
    let perspective = device_vertex(
        &vertex,
        Some(&texturing(&texture, true, D3dTextureFog::Disabled)),
    );
    assert_eq!(perspective.texture, [0.125, 0.375, 0.5]);
    let affine = device_vertex(
        &vertex,
        Some(&texturing(&texture, false, D3dTextureFog::Disabled)),
    );
    assert_eq!(affine.texture, [0.25, 0.75, 1.0]);
}

#[test]
fn vertex_fog_carries_the_colour_and_specular_alpha_visibility() {
    let texture = crate::d3d_texture::test_texture([2, 2], |_, _| [1, 2, 3, 255]);
    let fog = D3dTextureFog::Vertex {
        color: [90, 80, 70],
    };
    let vertex = device_vertex(
        &tl(0.0, 0.0, 1.0, [0.0; 2], 0x4012_3456),
        Some(&texturing(&texture, true, fog)),
    );
    assert_eq!(vertex.fog, [90, 80, 70, 0x40]);
    let clear = device_vertex(
        &tl(0.0, 0.0, 1.0, [0.0; 2], 0x4012_3456),
        Some(&texturing(&texture, true, D3dTextureFog::Disabled)),
    );
    assert_eq!(clear.fog, [0, 0, 0, 255]);
}

#[test]
fn blends_use_the_cpu_raster_factors() {
    assert_eq!(blend_factors(D3dTextureBlend::Copy), None);
    assert_eq!(
        blend_factors(D3dTextureBlend::Additive),
        Some((gl::ONE, gl::ONE))
    );
    assert_eq!(
        blend_factors(D3dTextureBlend::InverseSourceColor),
        Some((gl::ZERO, gl::ONE_MINUS_SRC_COLOR))
    );
    assert_eq!(
        blend_factors(D3dTextureBlend::SourceAlpha),
        Some((gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA))
    );
}

#[test]
fn native_point_linear_wrap_and_clamp_select_gl_sampling() {
    let texture = crate::d3d_texture::test_texture([2, 2], |_, _| [1, 2, 3, 255]);
    let mut sampling = texturing(&texture, true, D3dTextureFog::Disabled).sampling;
    assert_eq!(filter(sampling), gl::NEAREST);
    assert_eq!(address(sampling), gl::REPEAT);
    sampling.min_filter = 2;
    sampling.mag_filter = 2;
    sampling.address = 3;
    assert_eq!(filter(sampling), gl::LINEAR);
    assert_eq!(address(sampling), gl::CLAMP_TO_EDGE);
}

#[test]
fn vertex_layout_matches_the_attribute_pointers() {
    assert_eq!(std::mem::size_of::<DeviceVertex>(), 32);
    assert_eq!(std::mem::offset_of!(DeviceVertex, position), 0);
    assert_eq!(std::mem::offset_of!(DeviceVertex, texture), 12);
    assert_eq!(std::mem::offset_of!(DeviceVertex, diffuse), 24);
    assert_eq!(std::mem::offset_of!(DeviceVertex, fog), 28);
}
