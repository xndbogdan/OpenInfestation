//! The OpenGL and Vulkan devices against the CPU reference on identical
//! submissions. A device is not a pixel claim: the checks bound its
//! differences to coverage ties on shared edges and sub-step colour
//! rounding.

use super::*;
use crate::{
    d3d_flat::D3dFlatPolicy,
    d3d_texture::{test_texture, CpuTexture},
    d3d_textured::{D3dTextureFog, D3dTexturePolicy},
    d3d_viewport::D3dTlVertex,
    frame_device::Device,
    gl_device::GlDevice,
    vk_device::VkDevice,
};

const SIZE: [u32; 2] = [64, 48];
const CLEAR: [u8; 4] = [16, 24, 64, 255];

/// Device first, so it releases its objects while its window is current.
struct Target {
    device: Device,
    _window: sdl2::video::Window,
    _sdl: sdl2::Sdl,
}

fn target() -> Target {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let attributes = video.gl_attr();
    attributes.set_context_profile(sdl2::video::GLProfile::Compatibility);
    attributes.set_context_version(2, 1);
    attributes.set_double_buffer(true);
    let window = video
        .window("Infestation device check", SIZE[0], SIZE[1])
        .hidden()
        .opengl()
        .build()
        .unwrap();
    Target {
        device: Device::OpenGl(GlDevice::new(&window).unwrap()),
        _window: window,
        _sdl: sdl,
    }
}

fn vulkan_target() -> Target {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("Infestation Vulkan check", SIZE[0], SIZE[1])
        .hidden()
        .vulkan()
        .build()
        .unwrap();
    Target {
        device: Device::Vulkan(VkDevice::new(&window, true).unwrap()),
        _window: window,
        _sdl: sdl,
    }
}

fn pipeline() -> D3dRasterState {
    D3dRasterState {
        viewport: [0, 0, SIZE[0] as i16 - 1, SIZE[1] as i16 - 1],
        depth_enabled: true,
        depth_write: true,
        depth_function: 4,
        alpha_blend_enabled: true,
        shade_mode: 2,
        cull_mode: 1,
    }
}

fn vertex(x: f32, y: f32, depth: f32, rhw: f32, diffuse: [u8; 4], uv: [f32; 2]) -> D3dTlVertex {
    D3dTlVertex {
        x,
        y,
        depth,
        rhw,
        diffuse,
        specular: 0xff00_0000,
        uv,
    }
}

fn flat(vertices: [D3dTlVertex; 3], blend: D3dFlatBlend) -> D3dFlatSubmission {
    D3dFlatSubmission {
        vertices,
        triangles: [[0, 1, 2]],
        policy: D3dFlatPolicy {
            depth_enable: None,
            depth_write: None,
            blend,
            fog_enabled: false,
        },
        clip: D3dFlatClip::NativeViewport,
        mode: 0,
    }
}

fn quad(
    corners: [(f32, f32, f32); 4],
    uv: [[f32; 2]; 4],
    blend: D3dTextureBlend,
    fog: D3dTextureFog,
    perspective: bool,
) -> D3dTexturedSubmission {
    D3dTexturedSubmission {
        vertices: std::array::from_fn(|i| {
            let (x, y, rhw) = corners[i];
            let mut v = vertex(x, y, 0.5 - rhw / 4.0, rhw, [255, 230, 200, 160], uv[i]);
            // Visibility falls across the quad for the fog check.
            v.specular = (255 - 60 * i as u32) << 24;
            v
        }),
        triangles: [[0, 1, 2], [0, 2, 3]],
        policy: D3dTexturePolicy {
            depth_enable: None,
            depth_write: None,
            blend,
            perspective,
            fog,
        },
        mode: if perspective { 0 } else { 0x100 },
        near_depth: 64,
    }
}

fn checker() -> CpuTexture {
    test_texture([8, 8], |x, y| {
        if (x + y) % 2 == 0 {
            [240, 40, 30, 255]
        } else {
            [20, 200, 90, 96]
        }
    })
}

/// Draw the same sequence into a CPU frame and a device frame.
fn render_both(device: &Device, draw: impl Fn(&mut SoftwareFrame)) -> (Vec<u8>, Vec<u8>) {
    let mut cpu = SoftwareFrame::new(SIZE[0], SIZE[1], CLEAR);
    let mut gpu = SoftwareFrame::with_device(device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
    draw(&mut cpu);
    draw(&mut gpu);
    (cpu.rgba().to_vec(), gpu.read_rgba().into_owned())
}

/// One traced frame, read back as a presented frame ends: traces left in one
/// submission share its frame's buffers, the last one's view and random
/// numbers.
fn trace_frame(frame: &mut SoftwareFrame, feed: &crate::scene_feed::SceneFeed) {
    assert!(frame.trace_world(feed, &[]));
    frame.read_rgba();
}

/// Count pixels whose largest channel difference exceeds `tolerance`.
fn differing(cpu: &[u8], gpu: &[u8], tolerance: u8) -> usize {
    cpu.chunks_exact(4)
        .zip(gpu.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > tolerance))
        .count()
}

#[test]
#[ignore = "opens an OpenGL context; run explicitly"]
fn device_clears_and_replaces_surfaces_exactly() {
    check_clears_and_replaces_surfaces_exactly(target());
}

#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_clears_and_replaces_surfaces_exactly() {
    check_clears_and_replaces_surfaces_exactly(vulkan_target());
}

fn check_clears_and_replaces_surfaces_exactly(target: Target) {
    let (cpu, gpu) = render_both(&target.device, |frame| {
        frame.clear_color([1, 2, 3, 4]);
    });
    assert_eq!(cpu, gpu);
    let surface: Vec<u8> = (0..SIZE[0] * SIZE[1] * 4)
        .map(|i| (i * 7 % 251) as u8)
        .collect();
    let (cpu, gpu) = render_both(&target.device, |frame| {
        assert!(frame.replace_surface(&surface));
    });
    assert_eq!(cpu, gpu);
    let (cpu, gpu) = render_both(&target.device, |frame| {
        assert!(frame.fit_surface(5, 3, &surface[..5 * 3 * 4]));
    });
    assert_eq!(cpu, gpu);
}

#[test]
#[ignore = "opens an OpenGL context; run explicitly"]
fn device_flat_fans_match_the_cpu_reference() {
    check_flat_fans_match_the_cpu_reference(target());
}

#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_flat_fans_match_the_cpu_reference() {
    check_flat_fans_match_the_cpu_reference(vulkan_target());
}

fn check_flat_fans_match_the_cpu_reference(target: Target) {
    let (cpu, gpu) = render_both(&target.device, |frame| {
        let mut pipeline = pipeline();
        // Gouraud base, a nearer additive overlay, and a farther triangle
        // the depth test hides where they overlap.
        for (submission, label) in [
            (
                flat(
                    [
                        vertex(2.0, 3.0, 0.5, 1.0, [250, 10, 10, 255], [0.0; 2]),
                        vertex(60.0, 6.0, 0.5, 1.0, [10, 250, 10, 255], [0.0; 2]),
                        vertex(20.0, 45.0, 0.5, 1.0, [10, 10, 250, 255], [0.0; 2]),
                    ],
                    D3dFlatBlend::Copy,
                ),
                "base",
            ),
            (
                flat(
                    [
                        vertex(30.0, 1.0, 0.25, 1.0, [40, 40, 40, 0], [0.0; 2]),
                        vertex(63.0, 40.0, 0.25, 1.0, [90, 10, 60, 0], [0.0; 2]),
                        vertex(10.0, 30.0, 0.25, 1.0, [10, 90, 30, 0], [0.0; 2]),
                    ],
                    D3dFlatBlend::Additive,
                ),
                "additive",
            ),
            (
                flat(
                    [
                        vertex(0.0, 0.0, 0.75, 1.0, [200, 200, 0, 255], [0.0; 2]),
                        vertex(40.0, 0.0, 0.75, 1.0, [200, 200, 0, 255], [0.0; 2]),
                        vertex(0.0, 47.0, 0.75, 1.0, [200, 200, 0, 255], [0.0; 2]),
                    ],
                    D3dFlatBlend::Copy,
                ),
                "behind",
            ),
        ] {
            frame
                .fill_flat_submission(&submission, 64, &mut pipeline)
                .unwrap_or_else(|error| panic!("{label}: {error}"));
        }
    });
    let drawn = cpu.chunks_exact(4).filter(|pixel| **pixel != CLEAR).count();
    assert!(drawn > 1500, "the fans cover only {drawn} pixels");
    // Interiors agree within one interpolation step. On the development
    // GPU coverage matched exactly; the slack covers a driver breaking
    // shared-edge ties the other way.
    let count = differing(&cpu, &gpu, 3);
    assert!(count <= 8, "{count} pixels differ");
}

#[test]
#[ignore = "opens an OpenGL context; run explicitly"]
fn device_textured_fans_match_the_cpu_reference() {
    check_textured_fans_match_the_cpu_reference(target());
}

#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_textured_fans_match_the_cpu_reference() {
    check_textured_fans_match_the_cpu_reference(vulkan_target());
}

fn check_textured_fans_match_the_cpu_reference(target: Target) {
    let texture = checker();
    let corners = [
        (3.0, 2.0, 1.0),
        (61.0, 5.0, 0.25),
        (58.0, 44.0, 0.25),
        (6.0, 46.0, 1.0),
    ];
    // The eight uploaded texels, between their duplicated borders: the
    // surface wraps at 256, so a native quad stays inside its upload.
    let uv = [[1.0, 1.0], [9.0, 1.0], [9.0, 9.0], [1.0, 9.0]].map(|[u, v]| [u / 256., v / 256.]);
    for (min_filter, address, blend, fog, perspective) in [
        (1, 1, D3dTextureBlend::Copy, D3dTextureFog::Disabled, true),
        (1, 1, D3dTextureBlend::Copy, D3dTextureFog::Disabled, false),
        (
            2,
            3,
            D3dTextureBlend::SourceAlpha,
            D3dTextureFog::Vertex {
                color: [100, 110, 120],
            },
            true,
        ),
        (
            1,
            3,
            D3dTextureBlend::Additive,
            D3dTextureFog::Disabled,
            true,
        ),
        (
            2,
            1,
            D3dTextureBlend::InverseSourceColor,
            D3dTextureFog::Disabled,
            true,
        ),
    ] {
        let mut sampling = tests::texture_state().sampling;
        sampling.min_filter = min_filter;
        sampling.mag_filter = min_filter;
        sampling.address = address;
        let submission = quad(corners, uv, blend, fog, perspective);
        let (cpu, gpu) = render_both(&target.device, |frame| {
            frame
                .fill_textured_submission(&submission, &texture, &mut pipeline(), sampling)
                .unwrap();
        });
        let label = format!("{min_filter}/{address}/{blend:?}/{fog:?}/{perspective}");
        let drawn = cpu.chunks_exact(4).filter(|pixel| **pixel != CLEAR).count();
        assert!(drawn > 1500, "{label}: the quad covers only {drawn} pixels");
        // Modulation rounds one level apart. On the development GPU no
        // pixel differed by more than 1 (point) or 3 (bilinear); the slack
        // covers texel edges within float rounding of a pixel centre.
        let tolerance = if min_filter == 2 { 3 } else { 1 };
        let count = differing(&cpu, &gpu, tolerance);
        assert!(count <= 8, "{label}: {count} pixels differ");
    }
}

/// Native pixel `(x, y)` of a frame drawn at `target`: the target pixel
/// under the native pixel's centre.
fn downscaled(gpu: &[u8], target: [u32; 2]) -> Vec<u8> {
    let mut native = Vec::with_capacity(gpu.len());
    for y in 0..SIZE[1] {
        let ty = (((y as f32 + 0.5) * target[1] as f32 / SIZE[1] as f32) as u32).min(target[1] - 1);
        for x in 0..SIZE[0] {
            let tx =
                (((x as f32 + 0.5) * target[0] as f32 / SIZE[0] as f32) as u32).min(target[0] - 1);
            let offset = ((ty * target[0] + tx) * 4) as usize;
            native.extend_from_slice(&gpu[offset..offset + 4]);
        }
    }
    native
}

/// Pixels whose eight neighbours all equal them in the CPU frame: away from
/// every edge, where coverage cannot depend on the sampling position.
fn interior_differences(cpu: &[u8], gpu: &[u8], tolerance: u8) -> usize {
    let pixel = |image: &[u8], x: i32, y: i32| {
        let offset = ((y as u32 * SIZE[0] + x as u32) * 4) as usize;
        <[u8; 4]>::try_from(&image[offset..offset + 4]).unwrap()
    };
    let mut count = 0;
    for y in 1..SIZE[1] as i32 - 1 {
        for x in 1..SIZE[0] as i32 - 1 {
            let centre = pixel(cpu, x, y);
            let uniform = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                .all(|(dx, dy)| pixel(cpu, x + dx, y + dy) == centre);
            let device = pixel(gpu, x, y);
            if uniform
                && centre
                    .iter()
                    .zip(device)
                    .any(|(a, b)| a.abs_diff(b) > tolerance)
            {
                count += 1;
            }
        }
    }
    count
}

#[test]
#[ignore = "opens an OpenGL context; run explicitly"]
fn device_draws_native_submissions_above_native_size() {
    check_draws_native_submissions_above_native_size(target());
}

#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_draws_native_submissions_above_native_size() {
    check_draws_native_submissions_above_native_size(vulkan_target());
}

fn check_draws_native_submissions_above_native_size(target: Target) {
    let texture = checker();
    let corners = [
        (3.0, 2.0, 1.0),
        (61.0, 5.0, 0.25),
        (58.0, 44.0, 0.25),
        (6.0, 46.0, 1.0),
    ];
    let uv = [[1.0, 1.0], [9.0, 1.0], [9.0, 9.0], [1.0, 9.0]].map(|[u, v]| [u / 256., v / 256.]);
    let draw = |frame: &mut SoftwareFrame| {
        let mut pipeline = pipeline();
        frame
            .fill_flat_submission(
                &flat(
                    [
                        vertex(2.0, 3.0, 0.5, 1.0, [250, 10, 10, 255], [0.0; 2]),
                        vertex(60.0, 6.0, 0.5, 1.0, [10, 250, 10, 255], [0.0; 2]),
                        vertex(20.0, 45.0, 0.5, 1.0, [10, 10, 250, 255], [0.0; 2]),
                    ],
                    D3dFlatBlend::Copy,
                ),
                64,
                &mut pipeline,
            )
            .unwrap();
        frame
            .fill_textured_submission(
                &quad(
                    corners,
                    uv,
                    D3dTextureBlend::SourceAlpha,
                    D3dTextureFog::Disabled,
                    true,
                ),
                &texture,
                &mut pipeline,
                tests::texture_state().sampling,
            )
            .unwrap();
    };
    let mut cpu = SoftwareFrame::new(SIZE[0], SIZE[1], CLEAR);
    draw(&mut cpu);
    let cpu = cpu.rgba().to_vec();
    let cpu_drawn = cpu.chunks_exact(4).filter(|pixel| **pixel != CLEAR).count();
    for scaled in [[96, 72], [128, 96], [128, 72]] {
        let mut gpu =
            SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], scaled, CLEAR).unwrap();
        // Layout, clipping and validation stay native.
        assert_eq!([gpu.width(), gpu.height()], SIZE);
        assert_eq!(gpu.read_size(), scaled);
        draw(&mut gpu);
        let pixels = gpu.read_rgba().into_owned();
        assert_eq!(pixels.len(), (scaled[0] * scaled[1] * 4) as usize);
        // The same coverage at the larger density.
        let area = f64::from(scaled[0] * scaled[1]) / f64::from(SIZE[0] * SIZE[1]);
        let gpu_drawn = pixels
            .chunks_exact(4)
            .filter(|pixel| **pixel != CLEAR)
            .count();
        let ratio = gpu_drawn as f64 / area / cpu_drawn as f64;
        assert!(
            (0.97..1.03).contains(&ratio),
            "{scaled:?}: coverage ratio {ratio}"
        );
        // Away from edges the colours are the native ones.
        let count = interior_differences(&cpu, &downscaled(&pixels, scaled), 3);
        assert!(count <= 4, "{scaled:?}: {count} interior pixels differ");
    }
}

/// The swapchain path a shown window takes: a hidden window given a surface
/// all the same, frames presented into the output with black bars and
/// swapped to the swapchain several times, and the output read back.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_presents_through_a_swapchain() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("Infestation Vulkan swapchain check", 96, 64)
        .hidden()
        .vulkan()
        .build()
        .unwrap();
    let device = Device::Vulkan(VkDevice::new(&window, false).unwrap());
    let output = device.drawable_size(&window);
    let destination = crate::presentation::PresentationRect {
        x: 16,
        y: 8,
        width: SIZE[0],
        height: SIZE[1],
    };
    for frame_index in 0..4_u8 {
        let mut frame = SoftwareFrame::with_device(&device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
        frame.clear_color([200, frame_index * 10, 40, 255]);
        assert!(frame.present_device(output, destination));
        let pixels = device.read_output(output);
        let at = |x: u32, y: u32| {
            let offset = ((y * output[0] + x) * 4) as usize;
            pixels[offset..offset + 4].to_vec()
        };
        assert_eq!(at(0, 0), [0, 0, 0, 255], "a black bar");
        assert_eq!(at(20, 10), [200, frame_index * 10, 40, 255], "the frame");
        drop(frame);
        device.swap(&window);
    }
}

/// The tracer on a synthetic scene: a quad ten units ahead of the eye,
/// facing it and lit head-on, against the sky; its pixels take the quad's
/// colour lit by the sun, the rest the sky. Facing both the sun and the
/// eye, the quad also mirrors the sun's highlight straight at the eye: its
/// colour less the 4% a dielectric reflects, plus that highlight, white
/// (about 0.028 in linear light in every channel: green 10 and blue 20
/// come up alike, red stays).
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_traces_a_synthetic_scene() {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneSun, SceneSurface, SceneVertex};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    let mut feed = SceneFeed::default();
    feed.begin(SceneCamera {
        eye: [0.0; 3],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        focal: [32.0, 32.0],
        centre: [31.5, 23.5],
        native: SIZE,
        near: 1.0,
        far: 1000.0,
    });
    feed.sun = SceneSun {
        direction: [0.0, 0.0, -1.0],
        color: [1.0, 1.0, 1.0],
        ambient: [0.0; 3],
    };
    feed.fog.start = 900.0;
    feed.fog.end = 1000.0;
    feed.sky = [0.0, 0.0, 1.0];
    let corner = |x: f32, y: f32| SceneVertex {
        position: [x, y, 10.0],
        normal: [0.0; 3],
        uv: [0.0; 2],
        color: [200, 10, 20, 255],
        shade: [0; 4],
        fog: [0; 4],
        motion: [0.0; 3],
    };
    feed.polygon(
        &[
            corner(-2.0, -2.0),
            corner(2.0, -2.0),
            corner(2.0, 2.0),
            corner(-2.0, 2.0),
        ],
        None,
        SceneSurface::Opaque,
        None,
        None,
        false,
    );
    let mut frame =
        SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
    assert!(frame.trace_world(&feed, &[]));
    let pixels = frame.read_rgba().into_owned();
    let at = |x: usize, y: usize| pixels[(y * SIZE[0] as usize + x) * 4..][..4].to_vec();
    assert_eq!(at(32, 24), [200, 49, 52, 255], "the lit quad");
    // Where the tracer meets nothing, the raster's sky (here the frame's
    // clear colour) stays.
    assert_eq!(at(0, 0), CLEAR, "the sky");
}

/// A headlamp's light falls off with the square of the distance in linear
/// light: a grey wall square to its beam, lit by it alone, takes about 3.9
/// times the light 1,000 units out that it takes 2,000 out (the falloff's
/// 3.82, eased near the lamp, and its window). Decoded whole as a display
/// tone, as before 2026-10-29, it took 21 times: a pool blown out near the
/// bumper and dark not far beyond.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_headlamp_falls_off_with_the_square_of_the_distance() {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneLamp, SceneSun, SceneSurface};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    let lit = |distance: f32| {
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera {
            eye: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [32.0, 32.0],
            centre: [31.5, 23.5],
            native: SIZE,
            near: 1.0,
            far: 5000.0,
        });
        feed.sun = SceneSun {
            direction: [0.0, 1.0, 0.0],
            color: [0.0; 3],
            ambient: [0.0; 3],
        };
        feed.fog.start = 4900.0;
        feed.fog.end = 5000.0;
        feed.sky = [0.0; 3];
        feed.lamps = vec![SceneLamp {
            axis: [0.0, 0.0, 1.0],
            across: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            color: [1.0; 3],
            ..Default::default()
        }];
        let half = 0.4 * distance;
        let corner = |x: f32, y: f32| crate::scene_feed::SceneVertex {
            position: [x * half, y * half, distance],
            normal: [0.0; 3],
            uv: [0.0; 2],
            color: [100, 100, 100, 255],
            shade: [0; 4],
            fog: [0; 4],
            motion: [0.0; 3],
        };
        feed.polygon(
            &[
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ],
            None,
            SceneSurface::Opaque,
            None,
            None,
            false,
        );
        let mut frame =
            SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
        for _ in 0..8 {
            trace_frame(&mut frame, &feed);
        }
        let pixels = frame.read_rgba().into_owned();
        let at = (24 * SIZE[0] as usize + 32) * 4;
        pixels[at..at + 3]
            .iter()
            .map(|&channel| (f32::from(channel) / 255.0).powf(2.2))
            .sum::<f32>()
            / 3.0
    };
    let [near, far] = [lit(1000.0), lit(2000.0)];
    assert!(far > 0.01, "the far wall is lit: {far}");
    let ratio = near / far;
    assert!((3.4..4.5).contains(&ratio), "{near} against {far}: {ratio}");
}

/// The remastered camera's eye under the water: the view runs through the
/// water as a medium. A white panel 2,000 units off loses more of its red
/// than of its green to the water (its hue 0.3, 0.5, 0.55: red goes first),
/// and the water scatters its own colour over it; a view down that meets
/// nothing is the water's colour, not the sky behind; and with the eye
/// above, the same panel stays white.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_water_round_an_eye_under_it() {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneSun, SceneSurface, SceneVertex};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    let scene = |eye_under: bool| {
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera {
            eye: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [32.0, 32.0],
            centre: [31.5, 23.5],
            native: SIZE,
            near: 1.0,
            far: 8000.0,
        });
        // A sun behind the eye and above, lighting the panel's face.
        feed.sun = SceneSun {
            direction: [0.0, 0.6, -0.8],
            color: [1.0, 1.0, 1.0],
            ambient: [0.25; 3],
        };
        feed.fog.start = 7000.0;
        feed.fog.end = 8000.0;
        feed.sky = [0.0, 0.0, 1.0];
        // The water's plane 2,000 over the eye, the whole panel under it;
        // with the eye above the water, far below both.
        feed.water_level = Some(if eye_under { 2000.0 } else { -5000.0 });
        feed.eye_under = eye_under;
        let corner = |x: f32, y: f32| SceneVertex {
            position: [x, y, 2000.0],
            normal: [0.0; 3],
            uv: [0.0; 2],
            color: [230, 230, 230, 255],
            shade: [0; 4],
            fog: [0; 4],
            motion: [0.0; 3],
        };
        feed.polygon(
            &[
                corner(-600.0, -600.0),
                corner(600.0, -600.0),
                corner(600.0, 600.0),
                corner(-600.0, 600.0),
            ],
            None,
            SceneSurface::Opaque,
            None,
            None,
            false,
        );
        feed
    };
    let shot = |eye_under: bool| {
        let mut frame =
            SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
        let feed = scene(eye_under);
        for _ in 0..8 {
            assert!(frame.trace_world(&feed, &[]));
            frame.read_rgba();
        }
        frame.read_rgba().into_owned()
    };
    let at = |pixels: &[u8], x: usize, y: usize| -> [f32; 3] {
        let at = (y * SIZE[0] as usize + x) * 4;
        std::array::from_fn(|channel| f32::from(pixels[at + channel]))
    };
    let above = shot(false);
    let under = shot(true);
    let [r, g, _] = at(&above, 32, 24);
    assert!(
        r > 40.0 && (r - g).abs() <= 2.0,
        "the panel above the water: {r}, {g}"
    );
    let [r, g, b] = at(&under, 32, 24);
    assert!(
        r < 0.8 * g && b > r,
        "the panel through the water: {r}, {g}, {b}"
    );
    // A view down past the panel meets nothing: the water's colour.
    let water = at(&under, 32, 47);
    assert_ne!(
        water.map(|channel| channel as u8),
        [CLEAR[0], CLEAR[1], CLEAR[2]],
        "the water, not the sky"
    );
    assert!(
        water[1] > water[0] && water[1] > 4.0,
        "the water's colour: {water:?}"
    );
    assert_eq!(
        &above[(47 * SIZE[0] as usize + 32) * 4..][..4],
        CLEAR,
        "above the water the sky stays"
    );
}

/// The models outside the view (world_scene's outside the view) shadow and
/// light what the eye sees, each kind's after the view's own triangles,
/// and are not seen themselves. The sun before the eye and above lights a
/// floor and leaves a wall facing the eye in shade. A panel outside the
/// view over the floor, a masked sprite keyed transparent on its left half
/// (a masked sprite of the view's own beside the view comes first),
/// shadows the floor under its right half only. A blue panel outside the
/// view just before the eye leaves the eye its view of the wall. A green
/// panel outside the view behind the eye, in the sun, throws its colour
/// onto the shaded wall.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_models_outside_the_view_light_it_unseen() {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneSun, SceneSurface, SceneVertex};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    let keyed = test_texture([2, 1], |x, _| {
        if x == 0 {
            [255, 255, 255, 0]
        } else {
            [80, 80, 240, 255]
        }
    });
    let scene = |green: bool| {
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera {
            eye: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [32.0, 32.0],
            centre: [31.5, 23.5],
            native: SIZE,
            near: 1.0,
            far: 1000.0,
        });
        let half = std::f32::consts::FRAC_1_SQRT_2;
        feed.sun = SceneSun {
            direction: [0.0, half, half],
            color: [1.0, 1.0, 1.0],
            ambient: [0.0; 3],
        };
        feed.fog.start = 900.0;
        feed.fog.end = 1000.0;
        feed.sky = [0.0; 3];
        feed.textures.push((keyed.content_key(), -1));
        let mut quad = |corners: [[f32; 3]; 4],
                        uv: [[f32; 2]; 4],
                        color: [u8; 4],
                        texture: Option<u32>,
                        cutout: bool,
                        outside: bool| {
            let first = feed.triangles.len();
            let corners: [SceneVertex; 4] = std::array::from_fn(|corner| SceneVertex {
                position: corners[corner],
                uv: uv[corner],
                color,
                ..Default::default()
            });
            feed.polygon(&corners, texture, SceneSurface::Opaque, None, None, cutout);
            for triangle in &mut feed.triangles[first..] {
                triangle.culled = outside;
            }
        };
        let flat = [[0.0; 2]; 4];
        let white = [220, 220, 220, 255];
        // The view's: the floor, the wall, and a masked sprite beside the view.
        quad(
            [
                [-12.0, -3.0, 1.0],
                [-12.0, -3.0, 30.0],
                [12.0, -3.0, 30.0],
                [12.0, -3.0, 1.0],
            ],
            flat,
            white,
            None,
            false,
            false,
        );
        quad(
            [
                [-30.0, -3.0, 30.0],
                [-30.0, 10.0, 30.0],
                [30.0, 10.0, 30.0],
                [30.0, -3.0, 30.0],
            ],
            flat,
            white,
            None,
            false,
            false,
        );
        quad(
            [
                [40.0, -3.0, 20.0],
                [40.0, 0.0, 20.0],
                [44.0, 0.0, 20.0],
                [44.0, -3.0, 20.0],
            ],
            flat,
            white,
            None,
            true,
            false,
        );
        // Outside the view: the keyed panel over the floor, the blue one
        // before the eye and the green one behind it.
        quad(
            [
                [-4.0, -1.0, 6.0],
                [-4.0, -1.0, 12.0],
                [4.0, -1.0, 12.0],
                [4.0, -1.0, 6.0],
            ],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            [255; 4],
            Some(0),
            true,
            true,
        );
        quad(
            [
                [-1.0, -0.5, 2.0],
                [-1.0, 0.5, 2.0],
                [1.0, 0.5, 2.0],
                [1.0, -0.5, 2.0],
            ],
            flat,
            [20, 20, 250, 255],
            None,
            false,
            true,
        );
        if green {
            quad(
                [
                    [-20.0, -3.0, -4.0],
                    [-20.0, 10.0, -4.0],
                    [20.0, 10.0, -4.0],
                    [20.0, -3.0, -4.0],
                ],
                flat,
                [20, 220, 20, 255],
                None,
                false,
                true,
            );
        }
        feed
    };
    let traced = |feed: &SceneFeed| {
        let mut frame =
            SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
        for _ in 0..32 {
            assert!(frame.trace_world(feed, &[&keyed]));
        }
        frame.read_rgba().into_owned()
    };
    let at = |pixels: &[u8], x: usize, y: usize| -> [f32; 3] {
        let at = (y * SIZE[0] as usize + x) * 4;
        std::array::from_fn(|c| (f32::from(pixels[at + c]) / 255.0).powf(2.2))
    };
    let luminance = |rgb: [f32; 3]| (rgb[0] + rgb[1] + rgb[2]) / 3.0;
    let plain = traced(&scene(false));
    // The floor at depth 7, two to the left (under the keyed half's shadow
    // the sun's light passes) and two to the right (under its solid half).
    let [lit, shaded] = [22, 41].map(|x| luminance(at(&plain, x, 37)));
    assert!(lit > 0.05, "the floor in the sun: {lit}");
    assert!(
        shaded < 0.5 * lit,
        "the solid half's shadow: {shaded} of {lit}"
    );
    // The view's middle shows the wall, not the blue panel before it.
    let middle = at(&plain, 32, 22);
    assert!(
        middle[2] < middle[0] * 1.5 + 0.01,
        "the wall, not the panel: {middle:?}"
    );
    // The wall's shade, without and with the green panel behind the eye.
    let tint = |pixels: &[u8]| {
        let (mut green, mut red) = (0.0, 0.0);
        for y in 14..21 {
            for x in 12..52 {
                let rgb = at(pixels, x, y);
                green += rgb[1];
                red += rgb[0];
            }
        }
        green / red.max(1.0e-6)
    };
    let greened = traced(&scene(true));
    let [before, after] = [tint(&plain), tint(&greened)];
    assert!(
        after > 1.2 * before,
        "the wall takes the green panel's light: {before} to {after}"
    );
}

/// The bounce light follows a change within frames, not over its long
/// history (the denoiser's anti-lag): a wall the sun never reaches takes
/// only the light its sunlit floor throws back; with the sun put out, the
/// wall's light falls to a fifth (0.18) within eight frames, where a
/// 32-frame mean would still hold four fifths of it. A sparse light: about
/// one bounce ray in five from the wall meets the floor, so the anti-lag
/// knows the light went out by every sample round the wall staying dark,
/// not by those few agreeing.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_bounce_light_follows_a_change_within_frames() {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneSun, SceneSurface, SceneVertex};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    let mut feed = SceneFeed::default();
    feed.begin(SceneCamera {
        eye: [0.0; 3],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        focal: [32.0, 32.0],
        centre: [31.5, 23.5],
        native: SIZE,
        near: 1.0,
        far: 1000.0,
    });
    // The sun straight overhead: the wall, facing the eye, takes none.
    feed.sun = SceneSun {
        direction: [0.0, 1.0, 0.0],
        color: [1.0, 1.0, 1.0],
        ambient: [0.0; 3],
    };
    feed.fog.start = 900.0;
    feed.fog.end = 1000.0;
    feed.sky = [0.0, 0.0, 0.0];
    let corner = |position: [f32; 3]| SceneVertex {
        position,
        normal: [0.0; 3],
        uv: [0.0; 2],
        color: [220, 220, 220, 255],
        shade: [0; 4],
        fog: [0; 4],
        motion: [0.0; 3],
    };
    // The floor, under the eye to the wall's foot, and the wall.
    for quad in [
        [
            [-12.0, -3.0, 1.0],
            [-12.0, -3.0, 10.0],
            [12.0, -3.0, 10.0],
            [12.0, -3.0, 1.0],
        ],
        [
            [-12.0, -3.0, 10.0],
            [-12.0, 6.0, 10.0],
            [12.0, 6.0, 10.0],
            [12.0, -3.0, 10.0],
        ],
    ] {
        feed.polygon(
            &quad.map(corner),
            None,
            SceneSurface::Opaque,
            None,
            None,
            false,
        );
    }
    let mut frame =
        SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
    // The wall's middle, a little above the eye, in linear light.
    let wall = |frame: &mut SoftwareFrame| {
        let pixels = frame.read_rgba().into_owned();
        let at = (16 * SIZE[0] as usize + 32) * 4;
        pixels[at..at + 3]
            .iter()
            .map(|&channel| (f32::from(channel) / 255.0).powf(2.2))
            .sum::<f32>()
            / 3.0
    };
    for _ in 0..48 {
        trace_frame(&mut frame, &feed);
    }
    let lit = wall(&mut frame);
    assert!(lit > 0.01, "the wall takes the floor's light: {lit}");
    feed.sun.color = [0.0; 3];
    for _ in 0..8 {
        trace_frame(&mut frame, &feed);
    }
    let dark = wall(&mut frame);
    assert!(dark < 0.35 * lit, "{dark} of {lit} after eight frames");
}

/// A surface just come into view starts from the traced samples round it
/// on its surface, not from one alone (the denoiser's history fix): the
/// wall a panel hid, lit only by its sunlit floor, comes out on its first
/// frame in view nearly as even as once settled: over eight reveals, its
/// spread 1.25 times the settled one's (0.0127 against 0.0102; one reveal
/// alone ran from 1.03 to 1.94 times as the random numbers fell), where on
/// 2026-10-28 the samples beside it alone gave 1.57 times, one reveal.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_newly_seen_surface_starts_quiet() {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneSun, SceneSurface, SceneVertex};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    let corner = |position: [f32; 3]| SceneVertex {
        position,
        normal: [0.0; 3],
        uv: [0.0; 2],
        color: [220, 220, 220, 255],
        shade: [0; 4],
        fog: [0; 4],
        motion: [0.0; 3],
    };
    let scene = |panel: bool| {
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera {
            eye: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [32.0, 32.0],
            centre: [31.5, 23.5],
            native: SIZE,
            near: 1.0,
            far: 1000.0,
        });
        feed.sun = SceneSun {
            direction: [0.0, 1.0, 0.0],
            color: [1.0, 1.0, 1.0],
            ambient: [0.0; 3],
        };
        feed.fog.start = 900.0;
        feed.fog.end = 1000.0;
        feed.sky = [0.0; 3];
        let mut quads = vec![
            [
                [-12.0, -3.0, 1.0],
                [-12.0, -3.0, 10.0],
                [12.0, -3.0, 10.0],
                [12.0, -3.0, 1.0],
            ],
            [
                [-12.0, -3.0, 10.0],
                [-12.0, 6.0, 10.0],
                [12.0, 6.0, 10.0],
                [12.0, -3.0, 10.0],
            ],
        ];
        if panel {
            // Before the wall's right half, out of the sun's way.
            quads.push([
                [0.0, -1.0, 6.0],
                [0.0, 4.0, 6.0],
                [6.0, 4.0, 6.0],
                [6.0, -1.0, 6.0],
            ]);
        }
        for quad in quads {
            feed.polygon(
                &quad.map(corner),
                None,
                SceneSurface::Opaque,
                None,
                None,
                false,
            );
        }
        feed
    };
    let mut frame =
        SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
    // The wall where the panel stood, in linear light: its mean and spread.
    let revealed = |frame: &mut SoftwareFrame| {
        let pixels = frame.read_rgba().into_owned();
        let values: Vec<f32> = (8..16)
            .flat_map(|y| (38..50).map(move |x| (y, x)))
            .map(|(y, x)| {
                let at = (y * SIZE[0] as usize + x) * 4;
                pixels[at..at + 3]
                    .iter()
                    .map(|&channel| (f32::from(channel) / 255.0).powf(2.2))
                    .sum::<f32>()
                    / 3.0
            })
            .collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let spread =
            (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt();
        (mean, spread)
    };
    // Eight reveals, each a frame later in the random numbers' turn than the
    // last: the means of their first frames and of their settled ones.
    let (hidden, open) = (scene(true), scene(false));
    let [mut first_mean, mut first_spread, mut settled_mean, mut settled_spread] = [0.0_f32; 4];
    for reveal in 0..8 {
        for _ in 0..40 + reveal {
            trace_frame(&mut frame, &hidden);
        }
        assert!(frame.trace_world(&open, &[]));
        let (mean, spread) = revealed(&mut frame);
        first_mean += mean / 8.0;
        first_spread += spread / 8.0;
        for _ in 0..60 {
            trace_frame(&mut frame, &open);
        }
        let (mean, spread) = revealed(&mut frame);
        settled_mean += mean / 8.0;
        settled_spread += spread / 8.0;
    }
    assert!(settled_mean > 0.01, "{settled_mean}");
    assert!(
        (first_mean - settled_mean).abs() < 0.4 * settled_mean,
        "first frame {first_mean} against settled {settled_mean}"
    );
    assert!(
        first_spread < 1.4 * settled_spread,
        "first frame spread {first_spread} against settled {settled_spread}"
    );
}

/// The emissive light list (the full path tracing plan's stage 5): a small
/// unlit panel over a floor in the dark lights the floor under it. Sampled
/// as a light (next-event estimation, weighed against the bounce rays that
/// meet it), the floor's light lies within 3% of where it settles from the
/// first frame (2.0 to 2.5% over the first four), and settles within 4% of
/// the light the panel gives (its form factor; 3.1% under). Met only by the
/// bounce rays (as a `Shown` face, which stays out of the list), it settles
/// as near (0.4% under; 5% under over 512 frames), and its first four
/// frames lie further off where it settles, 6 to 9%. Until the denoiser's
/// anti-lag weighed the frames between rare hits by how often its samples
/// meet the light (the full path tracing plan's stage 6), it took them for
/// the light going out and settled at 71% of the panel's light (65% over 64
/// frames). (Every frame read back, 2026-10-29; the first figures, 95 to
/// 140% off and under half, came from runs of traces between read-backs,
/// which replay the last one's view and random numbers.)
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_emitter_sampling_settles_a_lit_patch_sooner() {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneSun, SceneSurface, SceneVertex};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    // A larger frame than the others', so the denoiser's filter spans little
    // of the light's fall-off.
    const WIDE: [u32; 2] = [256, 192];
    // The panel: two units square, six over the floor (y -3), six to eight
    // ahead (left, right, near, far).
    const PANEL: [f32; 4] = [-1.0, 1.0, 6.0, 8.0];
    const PANEL_Y: f32 = 3.0;
    let corner = |position: [f32; 3], grey: u8| SceneVertex {
        position,
        normal: [0.0; 3],
        uv: [0.0; 2],
        color: [grey, grey, grey, 255],
        shade: [0; 4],
        fog: [0; 4],
        motion: [0.0; 3],
    };
    let scene = |panel: SceneSurface| {
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera {
            eye: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [128.0, 128.0],
            centre: [127.5, 95.5],
            native: WIDE,
            near: 1.0,
            far: 1000.0,
        });
        // No sun, no sky: the panel alone lights the floor.
        feed.sun = SceneSun {
            direction: [0.0, 1.0, 0.0],
            color: [0.0; 3],
            ambient: [0.0; 3],
        };
        feed.fog.start = 900.0;
        feed.fog.end = 1000.0;
        feed.sky = [0.0; 3];
        let floor = [
            [-12.0, -3.0, 1.0],
            [-12.0, -3.0, 30.0],
            [12.0, -3.0, 30.0],
            [12.0, -3.0, 1.0],
        ];
        feed.polygon(
            &floor.map(|position| corner(position, 220)),
            None,
            SceneSurface::Opaque,
            None,
            None,
            false,
        );
        let [left, right, near, far] = PANEL;
        let lit = [
            [left, PANEL_Y, near],
            [left, PANEL_Y, far],
            [right, PANEL_Y, far],
            [right, PANEL_Y, near],
        ];
        feed.polygon(
            &lit.map(|position| corner(position, 255)),
            None,
            panel,
            None,
            None,
            false,
        );
        feed
    };
    // The floor under the panel, in linear light: rows 146 to 159 (the
    // floor at z = 384 / (row - 95.5), 6.1 to 7.6 ahead), a unit across.
    let rows = 146..160;
    let columns = 118..138;
    let patch = |frame: &mut SoftwareFrame| -> Vec<f32> {
        let pixels = frame.read_rgba().into_owned();
        let decode = |byte: u8| {
            let c = f32::from(byte) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        rows.clone()
            .flat_map(|y| columns.clone().map(move |x| (y, x)))
            .map(|(y, x)| {
                let at = (y * WIDE[0] as usize + x) * 4;
                pixels[at..at + 3].iter().map(|&c| decode(c)).sum::<f32>() / 3.0
            })
            .collect()
    };
    // The light the panel gives a floor point (radiance 1 times the form
    // factor, the panel's cosine-weighted share of the floor's sky) where the
    // bounce's rays leave it, half a unit over it (pt_trace.comp's path
    // pass), times the floor's colour: the mean over the patch.
    let expected = {
        let steps = 48;
        let [left, right, near, far] = PANEL;
        let (dx, dz) = ((right - left) / steps as f32, (far - near) / steps as f32);
        let albedo = ((220.0_f32 / 255.0 + 0.055) / 1.055).powf(2.4);
        let mut sum = 0.0;
        let mut count = 0.0;
        for y in rows.clone() {
            for x in columns.clone() {
                let z = 384.0 / (y as f32 - 95.5);
                let point = [(x as f32 - 127.5) / 128.0 * z, -2.5, z];
                let mut form = 0.0;
                for i in 0..steps {
                    for j in 0..steps {
                        let to = [
                            left + dx * (i as f32 + 0.5) - point[0],
                            PANEL_Y - point[1],
                            near + dz * (j as f32 + 0.5) - point[2],
                        ];
                        let d2 = to.iter().map(|v| v * v).sum::<f32>();
                        let cosine = to[1] / d2.sqrt();
                        form += cosine * cosine / d2 * dx * dz;
                    }
                }
                sum += albedo * form / std::f32::consts::PI;
                count += 1.0;
            }
        }
        sum / count
    };
    // Each way: the patch over its first four frames, and settled (the
    // mean of 64 frames after 120: a sparse light's eight frames hold too
    // few of its hits to say where it settles).
    let run = |panel: SceneSurface| {
        let feed = scene(panel);
        let mut frame =
            SoftwareFrame::with_device(&target.device, WIDE[0], WIDE[1], WIDE, CLEAR).unwrap();
        let mut early = Vec::new();
        for _ in 0..4 {
            assert!(frame.trace_world(&feed, &[]));
            early.push(patch(&mut frame));
        }
        for _ in 4..120 {
            trace_frame(&mut frame, &feed);
        }
        let mut settled = vec![0.0_f32; early[0].len()];
        for _ in 0..64 {
            assert!(frame.trace_world(&feed, &[]));
            for (sum, value) in settled.iter_mut().zip(patch(&mut frame)) {
                *sum += value / 64.0;
            }
        }
        (early, settled)
    };
    // How far a frame lies from where it settles, against the light there.
    let error = |frame: &[f32], settled: &[f32]| {
        let mean = settled.iter().sum::<f32>() / settled.len() as f32;
        (frame
            .iter()
            .zip(settled)
            .map(|(value, truth)| (value - truth).powi(2))
            .sum::<f32>()
            / frame.len() as f32)
            .sqrt()
            / mean
    };
    let mean = |values: &[f32]| values.iter().sum::<f32>() / values.len() as f32;
    let (sampled_early, sampled) = run(SceneSurface::Emissive);
    let (found_early, found) = run(SceneSurface::Shown);
    let (sampled_mean, found_mean) = (mean(&sampled), mean(&found));
    for (way, settled) in [("sampled", sampled_mean), ("found", found_mean)] {
        assert!(
            (settled - expected).abs() < 0.08 * expected,
            "settled light {settled} {way}, {expected} the panel gives"
        );
    }
    let early = |frames: &[Vec<f32>], settled: &[f32]| -> Vec<f32> {
        frames.iter().map(|frame| error(frame, settled)).collect()
    };
    let (with, without) = (early(&sampled_early, &sampled), early(&found_early, &found));
    assert!(
        with.iter().all(|&e| e < 0.035),
        "sampled, the first frames {with:?} from where it settles"
    );
    assert!(
        mean(&with) < 0.75 * mean(&without),
        "the first frames {with:?} from settled sampled against {without:?} found"
    );
}

/// The glossy tests' frame: a larger one than the others', so the
/// reflection spans many pixels.
const GLOSSY_SIZE: [u32; 2] = [256, 192];

/// The glossy tests' scene: over a metal floor in the dark (the default
/// tuning's smoothest metal, half metal, roughness 0.3), a lit panel
/// standing fourteen ahead facing the eye, `panel_x` aside and moving by
/// `panel_motion` (where it was in the last frame, less where it is), seen
/// from an eye `eye_x` aside.
fn glossy_scene(eye_x: f32, panel_x: f32, panel_motion: f32) -> crate::scene_feed::SceneFeed {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneSun, SceneSurface, SceneVertex};
    const FLOOR: u16 = 1;
    // The feed's points are the eye's offsets.
    let corner = |position: [f32; 3], grey: u8, motion: f32| SceneVertex {
        position: [position[0] - eye_x, position[1], position[2]],
        normal: [0.0; 3],
        uv: [0.0; 2],
        color: [grey, grey, grey, 255],
        shade: [0; 4],
        fog: [0; 4],
        motion: [motion, 0.0, 0.0],
    };
    let mut feed = SceneFeed::default();
    feed.begin(SceneCamera {
        eye: [f64::from(eye_x), 0.0, 0.0],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        focal: [128.0, 128.0],
        centre: [127.5, 95.5],
        native: GLOSSY_SIZE,
        near: 1.0,
        far: 1000.0,
    });
    feed.model_names.insert(FLOOR, "turbo".into());
    // No sun, no sky: the panel alone lights the floor.
    feed.sun = SceneSun {
        direction: [0.0, 1.0, 0.0],
        color: [0.0; 3],
        ambient: [0.0; 3],
    };
    feed.fog.start = 900.0;
    feed.fog.end = 1000.0;
    feed.sky = [0.0; 3];
    let floor = [
        [-40.0, -3.0, 1.0],
        [-40.0, -3.0, 40.0],
        [40.0, -3.0, 40.0],
        [40.0, -3.0, 1.0],
    ];
    feed.polygon(
        &floor.map(|position| corner(position, 200, 0.0)),
        None,
        SceneSurface::Opaque,
        Some(FLOOR),
        None,
        false,
    );
    let panel = [
        [panel_x - 1.5, -1.0, 14.0],
        [panel_x - 1.5, 3.0, 14.0],
        [panel_x + 1.5, 3.0, 14.0],
        [panel_x + 1.5, -1.0, 14.0],
    ];
    feed.polygon(
        &panel.map(|position| corner(position, 255, panel_motion)),
        None,
        SceneSurface::Emissive,
        None,
        None,
        false,
    );
    feed
}

/// The panel's reflection in a glossy test's frame, traced and read back
/// as a presented frame ends (trace_frame): its rows (the panel's image
/// under the floor seen at 4.7 to 8.4 ahead), each column's mean in linear
/// light.
fn glossy_profile(frame: &mut SoftwareFrame, feed: &crate::scene_feed::SceneFeed) -> Vec<f32> {
    assert!(frame.trace_world(feed, &[]));
    let pixels = frame.read_rgba().into_owned();
    let decode = |byte: u8| {
        let c = f32::from(byte) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let rows = 142..178;
    (0..GLOSSY_SIZE[0] as usize)
        .map(|x| {
            rows.clone()
                .map(|y| {
                    let at = (y * GLOSSY_SIZE[0] as usize + x) * 4;
                    pixels[at..at + 3].iter().map(|&c| decode(c)).sum::<f32>() / 3.0
                })
                .sum::<f32>()
                / rows.len() as f32
        })
        .collect()
}

fn profiles_apart(a: &[f32], b: &[f32]) -> f32 {
    (a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f32>() / a.len() as f32).sqrt()
}

fn glossy_frame(target: &Target) -> SoftwareFrame {
    SoftwareFrame::with_device(
        &target.device,
        GLOSSY_SIZE[0],
        GLOSSY_SIZE[1],
        GLOSSY_SIZE,
        CLEAR,
    )
    .unwrap()
}

/// Where a glossy test's reflection settles: the mean profile of eight
/// frames after 90, and how far the furthest of the eight lies from it.
fn glossy_settled(target: &Target, eye_x: f32, panel_x: f32) -> (Vec<f32>, f32) {
    let feed = glossy_scene(eye_x, panel_x, 0.0);
    let mut frame = glossy_frame(target);
    for _ in 0..90 {
        glossy_profile(&mut frame, &feed);
    }
    let singles: Vec<Vec<f32>> = (0..8).map(|_| glossy_profile(&mut frame, &feed)).collect();
    let mean: Vec<f32> = (0..GLOSSY_SIZE[0] as usize)
        .map(|x| singles.iter().map(|one| one[x]).sum::<f32>() / 8.0)
        .collect();
    let flicker = singles
        .iter()
        .map(|one| profiles_apart(one, &mean))
        .fold(0.0, f32::max);
    (mean, flicker)
}

/// A metal's glossy reflection follows a camera move within frames (the
/// full path tracing plan's stage 6: the glossy bounce's history found where
/// its reflected image lay) and holds still while the camera does: over a
/// metal floor in the dark, a lit panel's reflection, settled with the eye at
/// one place, comes out near where it settles with the eye two aside four
/// frames after gliding there over eight, its column profile under a
/// quarter as far from there as the glide moved it (0.12; followed by the
/// floor's own motion, the reflection's history came from where another
/// part of it lay and trailed it, 0.44); settled, a frame lies within a
/// twentieth of that from the frames' mean (0.037).
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_glossy_reflection_follows_a_camera_move() {
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    const STEP: f32 = 2.0;
    let ((before, still), (after, _)) = (
        glossy_settled(&target, 0.0, 0.0),
        glossy_settled(&target, STEP, 0.0),
    );
    let moved = profiles_apart(&before, &after);
    let peak = after.iter().copied().fold(0.0, f32::max);
    assert!(
        moved > 0.1 * peak,
        "the reflection moves: {moved} of {peak}"
    );
    assert!(
        still < 0.05 * moved,
        "settled, a frame within a twentieth of the step: {still} of {moved}"
    );
    // Settled at the first eye, gliding to the second over eight frames,
    // then four frames there.
    let mut frame = glossy_frame(&target);
    let first = glossy_scene(0.0, 0.0, 0.0);
    for _ in 0..90 {
        glossy_profile(&mut frame, &first);
    }
    for k in 1..8 {
        glossy_profile(&mut frame, &glossy_scene(STEP * k as f32 / 8.0, 0.0, 0.0));
    }
    let second = glossy_scene(STEP, 0.0, 0.0);
    let followed: Vec<f32> = (0..4)
        .map(|_| profiles_apart(&glossy_profile(&mut frame, &second), &after) / moved)
        .collect();
    assert!(
        followed[3] < 0.25,
        "four frames on, {:.3} as far from where it settles as it moved (by frame: {followed:.3?})",
        followed[3]
    );
}

/// A moving model's reflection in a still metal follows it within frames
/// with the eye still too (the glossy history shortens where its rays reach
/// another distance than its light came from, the eye moving or not): the
/// lit panel over the metal floor, settled at one place, glides two aside
/// over eight frames; four frames after, the reflection's column profile
/// lies 0.21 as far from where it settles there as the glide moved it
/// (0.24 with the reach checked only as the eye moves; the luminance
/// anti-lag held it that near already, the diffuse light under the panel
/// following within frames too).
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_glossy_reflection_follows_a_moving_model() {
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    const STEP: f32 = 2.0;
    let ((before, _), (after, _)) = (
        glossy_settled(&target, 0.0, 0.0),
        glossy_settled(&target, 0.0, STEP),
    );
    let moved = profiles_apart(&before, &after);
    let peak = after.iter().copied().fold(0.0, f32::max);
    assert!(
        moved > 0.1 * peak,
        "the reflection moves: {moved} of {peak}"
    );
    let mut frame = glossy_frame(&target);
    let first = glossy_scene(0.0, 0.0, 0.0);
    for _ in 0..90 {
        glossy_profile(&mut frame, &first);
    }
    for k in 1..=8 {
        glossy_profile(
            &mut frame,
            &glossy_scene(0.0, STEP * k as f32 / 8.0, -STEP / 8.0),
        );
    }
    let second = glossy_scene(0.0, STEP, 0.0);
    let followed: Vec<f32> = (0..4)
        .map(|_| profiles_apart(&glossy_profile(&mut frame, &second), &after) / moved)
        .collect();
    assert!(
        followed[3] < 0.22,
        "four frames on, {:.3} as far from where it settles as it moved (by frame: {followed:.3?})",
        followed[3]
    );
}

/// Under infrared vision the traced world is heat in grey: a mutant, warm
/// by its name, comes out near white over the cold surroundings once the
/// camera's gain spans the scene, and the camera leaves no colour.
/// Without traced rays the player's anti-aliasing still applies: FXAA and
/// SMAA blend the world's hard edge, and the HUD's draw after the world's
/// comes out sharp and unmoved, under TAA's jitter too. Supersampling draws
/// the whole frame twice as large on each side and reads it back averaged
/// down, the world's edge and the HUD's both blended.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_anti_aliases_an_untraced_world() {
    use crate::modern_settings::{AntiAliasing, ModernSettings, UpscaleQuality, Upscaler};
    use crate::scene_feed::SceneCamera;
    const WHITE: [u8; 4] = [255; 4];
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    let camera = SceneCamera {
        eye: [0.0; 3],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        focal: [32.0, 32.0],
        centre: [31.5, 23.5],
        native: SIZE,
        near: 1.0,
        far: 1000.0,
    };
    let draw = |anti_aliasing: AntiAliasing| {
        device.set_modern(ModernSettings {
            ray_tracing: false,
            upscaler: Upscaler::Off,
            quality: UpscaleQuality::Quality,
            anti_aliasing,
            ..ModernSettings::default()
        });
        let mut frame =
            SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
        let mut pipeline = pipeline();
        frame.begin_raster_world(&camera, Some(0.02), None, None, false);
        let world = flat(
            [
                vertex(2.0, 2.0, 0.5, 1.0, WHITE, [0.0; 2]),
                vertex(62.0, 10.0, 0.5, 1.0, WHITE, [0.0; 2]),
                vertex(10.0, 46.0, 0.5, 1.0, WHITE, [0.0; 2]),
            ],
            D3dFlatBlend::Copy,
        );
        frame
            .fill_flat_submission(&world, 64, &mut pipeline)
            .unwrap();
        frame.finish_raster_world();
        let hud = flat(
            [
                vertex(50.0, 38.0, 0.0, 1.0, WHITE, [0.0; 2]),
                vertex(63.0, 38.0, 0.0, 1.0, WHITE, [0.0; 2]),
                vertex(63.0, 47.0, 0.0, 1.0, WHITE, [0.0; 2]),
            ],
            D3dFlatBlend::Copy,
        );
        frame.fill_flat_submission(&hud, 64, &mut pipeline).unwrap();
        frame.read_rgba().into_owned()
    };
    let plain = draw(AntiAliasing::Off);
    let fxaa = draw(AntiAliasing::Fxaa);
    let smaa = draw(AntiAliasing::Smaa);
    let taa = draw(AntiAliasing::Taa);
    let ssaa = draw(AntiAliasing::Ssaa4x);
    // Pixels neither the clear nor the fill, in the world's part or the HUD's.
    let blended = |pixels: &[u8], hud: bool| {
        pixels
            .chunks_exact(4)
            .enumerate()
            .filter(|(index, pixel)| {
                let [x, y] = [index % SIZE[0] as usize, index / SIZE[0] as usize];
                (x >= 48 && y >= 36) == hud && pixel[..3] != CLEAR[..3] && pixel[..3] != WHITE[..3]
            })
            .count()
    };
    assert_eq!(
        blended(&plain, false),
        0,
        "a hard edge without anti-aliasing"
    );
    for (pixels, name) in [(&fxaa, "FXAA"), (&smaa, "SMAA"), (&ssaa, "SSAA 4x")] {
        let edge = blended(pixels, false);
        assert!(edge > 20, "{name} blends the world's edge: {edge} pixels");
    }
    assert!(blended(&ssaa, true) > 0, "supersampling blends the HUD too");
    for (pixels, name) in [(&fxaa, "FXAA"), (&smaa, "SMAA"), (&taa, "TAA")] {
        assert_eq!(blended(pixels, true), 0, "{name} leaves the HUD sharp");
        let hud = |pixels: &[u8]| {
            pixels
                .chunks_exact(4)
                .enumerate()
                .filter(|(index, _)| {
                    index % SIZE[0] as usize >= 48 && index / SIZE[0] as usize >= 36
                })
                .map(|(_, pixel)| pixel.to_vec())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            hud(pixels),
            hud(&plain),
            "{name} leaves the HUD where it was"
        );
    }
}

/// Without traced rays a metal face mirrors the sky ([`crate::raster_metal`]):
/// a white metal triangle turned up toward a blue painted sky takes the
/// sky's blue above its sprite's split (its material there metal, matte
/// below), and stays retail's white to the pixel below it, as does a matte
/// triangle beside it.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_raster_metal_mirrors_the_sky() {
    use crate::modern_settings::ModernSettings;
    use crate::raster_metal::{MetalCorner, MetalFace};
    use crate::scene_feed::{SceneCamera, SceneFeed};
    const WHITE: [u8; 4] = [255; 4];
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    device.set_modern(ModernSettings {
        ray_tracing: false,
        ..ModernSettings::default()
    });
    let camera = SceneCamera {
        eye: [0.0; 3],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        focal: [32.0, 32.0],
        centre: [31.5, 23.5],
        native: SIZE,
        near: 1.0,
        far: 1000.0,
    };
    let mut feed = SceneFeed::default();
    feed.begin(camera);
    feed.sky = [0.0, 0.0, 1.0];
    // Two triangles 10 units ahead, the metal one on the left.
    let triangle = |left: f32| {
        [(left, 4.0), (left + 26.0, 4.0), (left, 44.0)]
            .map(|(x, y)| vertex(x, y, 0.9, 0.1, WHITE, [0.0; 2]))
    };
    let metal = triangle(2.0);
    let normal = [0.0, 0.6, -0.8];
    // Matte below half its sprite's height, metal above it; its V runs
    // from 0 along the top edge to 1 at the bottom corner.
    let face = MetalFace {
        material: [0, 255, 255, 13],
        split: 32768,
        bare: false,
        corners: metal
            .iter()
            .map(|corner| MetalCorner {
                view: [
                    (corner.x - 31.5) * 10.0 / 32.0,
                    (23.5 - corner.y) * 10.0 / 32.0,
                    10.0,
                ],
                normal,
                albedo: [1.0; 3],
                v: (corner.y - 4.0) / 40.0,
            })
            .collect(),
        passes: 1,
    };
    let mut frame =
        SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
    let mut pipeline = pipeline();
    frame.begin_raster_world(&camera, None, None, Some(&feed), false);
    if !frame.reflects_metal() {
        // No scalar block layout: no frame block for the metal to read.
        return;
    }
    frame.set_draw_metal(Some(std::rc::Rc::new(face)));
    frame
        .fill_flat_submission(&flat(metal, D3dFlatBlend::Copy), 64, &mut pipeline)
        .unwrap();
    frame.set_draw_metal(None);
    frame
        .fill_flat_submission(&flat(triangle(34.0), D3dFlatBlend::Copy), 64, &mut pipeline)
        .unwrap();
    frame.finish_raster_world();
    let pixels = frame.read_rgba().into_owned();
    let at = |x: usize, y: usize| pixels[(y * SIZE[0] as usize + x) * 4..][..4].to_vec();
    let mirrored = at(8, 10);
    assert!(
        mirrored[2] > 200 && mirrored[0] < 60 && mirrored[1] < 60,
        "the metal takes the sky: {mirrored:?}"
    );
    assert_eq!(at(3, 38), WHITE, "below the split the face is matte");
    assert_eq!(at(40, 10), WHITE, "the matte face stays retail's");
}

/// Without traced rays the water takes the tracer's lamps and splashes
/// (`vk_mirror.comp`, `pt_splash.glsl`). Looking straight down at deep water
/// 100 units under the eye, the sky beside it: a dim lamp 40 over the
/// middle of the water, aimed down, lights it in the water's colour and
/// glints on its ripples; a splash there rings the water and foams it,
/// moving the glints and taking the lamp's light; beside the water nothing
/// changes.
#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_raster_water_takes_splashes_and_lamps() {
    use crate::modern_settings::{AntiAliasing, ModernSettings};
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneLamp, SceneSplash};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    // No anti-aliasing to blend the water's edge into the bank.
    device.set_modern(ModernSettings {
        ray_tracing: false,
        anti_aliasing: AntiAliasing::Off,
        ..ModernSettings::default()
    });
    // Down, the screen's top toward +Z.
    let camera = SceneCamera {
        eye: [0.0; 3],
        basis: [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]],
        focal: [32.0, 32.0],
        centre: [31.5, 23.5],
        native: SIZE,
        near: 1.0,
        far: 8000.0,
    };
    // Deep water 100 under the eye on the screen's left, as retail lays it.
    let water = {
        let corner = |x: f32, y: f32| vertex(x, y, 0.99, 0.01, [30, 60, 90, 255], [0.0; 2]);
        [
            [corner(0.0, 0.0), corner(48.0, 0.0), corner(48.0, 48.0)],
            [corner(0.0, 0.0), corner(48.0, 48.0), corner(0.0, 48.0)],
        ]
    };
    let shot = |lamp: bool, splash: bool| {
        let mut light = SceneFeed::default();
        light.begin(camera);
        light.water_level = Some(-100.0);
        if splash {
            light.splashes.push(SceneSplash {
                position: [0.0, -100.0, 0.0],
                age: 0.05,
                scale: 0.5,
            });
        }
        let mut frame =
            SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
        let mut pipeline = pipeline();
        frame.begin_raster_world(&camera, Some(0.02), None, Some(&light), false);
        if !frame.shades_water() {
            // No scalar block layout: no frame block for the water to read.
            return None;
        }
        frame.clear_depth();
        frame.set_draw_water(true);
        for triangle in water {
            frame
                .fill_flat_submission(&flat(triangle, D3dFlatBlend::Copy), 64, &mut pipeline)
                .unwrap();
        }
        frame.set_draw_water(false);
        if lamp {
            light.lamps.push(SceneLamp {
                position: [0.0, -60.0, 0.0],
                axis: [0.0, -1.0, 0.0],
                across: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                color: [0.05; 3],
                rear: false,
            });
            light.water_hue = Some([0.3, 0.5, 0.55]);
            frame.raster_lamps(&light);
        }
        frame.finish_raster_world();
        Some(frame.read_rgba().into_owned())
    };
    let Some(plain) = shot(false, false) else {
        return;
    };
    let lit = shot(true, false).unwrap();
    let splashed = shot(true, true).unwrap();
    let columns = |pixels: &[u8], from: usize| -> Vec<u8> {
        pixels
            .chunks_exact(4)
            .enumerate()
            .filter(|(index, _)| index % SIZE[0] as usize >= from)
            .flat_map(|(_, pixel)| pixel.to_vec())
            .collect()
    };
    for (pixels, name) in [(&lit, "the lamp"), (&splashed, "the splash")] {
        assert_eq!(
            columns(pixels, 48),
            columns(&plain, 48),
            "{name} leaves the bank"
        );
    }
    // The water's pixels, each its channels' sum.
    let water = |pixels: &[u8]| -> Vec<u32> {
        pixels
            .chunks_exact(4)
            .enumerate()
            .filter(|(index, _)| index % (SIZE[0] as usize) < 48)
            .map(|(_, pixel)| pixel[..3].iter().map(|&channel| u32::from(channel)).sum())
            .collect()
    };
    let total = |pixels: &[u8]| water(pixels).iter().sum::<u32>();
    assert!(
        total(&lit) > total(&plain) + 48 * 48 * 10,
        "the lamp lights the water: {} against {}",
        total(&lit),
        total(&plain)
    );
    let changed = water(&splashed)
        .iter()
        .zip(water(&lit))
        .filter(|(a, b)| **a != *b)
        .count();
    assert!(changed > 50, "the splash rings the water: {changed} pixels");
}

#[test]
#[ignore = "opens a Vulkan device; run explicitly"]
fn vulkan_thermal_vision_shows_heat_in_grey() {
    use crate::scene_feed::{SceneCamera, SceneFeed, SceneSun, SceneSurface, SceneVertex};
    let target = vulkan_target();
    let Device::Vulkan(device) = &target.device else {
        unreachable!()
    };
    if !device.ray_tracing() {
        return;
    }
    let thermal = |mutant: bool| {
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera {
            eye: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [32.0, 32.0],
            centre: [31.5, 23.5],
            native: SIZE,
            near: 1.0,
            far: 1000.0,
        });
        feed.sun = SceneSun {
            direction: [0.0, 0.0, -1.0],
            color: [1.0, 1.0, 1.0],
            ambient: [0.0; 3],
        };
        feed.fog.start = 900.0;
        feed.fog.end = 1000.0;
        feed.infrared = true;
        feed.model_names.insert(5, "mutant".into());
        let corner = |x: f32, y: f32, z: f32| SceneVertex {
            position: [x, y, z],
            normal: [0.0; 3],
            uv: [0.0; 2],
            color: [200, 10, 20, 255],
            shade: [0; 4],
            fog: [0; 4],
            motion: [0.0; 3],
        };
        // The mutant, and a cold wall filling the view behind it: the gain
        // leaves the sky out of its range, so the scene needs something cold.
        for (half, z, model) in [(2.0, 10.0, Some(5)), (100.0, 20.0, None)] {
            if model.is_some() && !mutant {
                continue;
            }
            feed.polygon(
                &[
                    corner(-half, -half, z),
                    corner(half, -half, z),
                    corner(half, half, z),
                    corner(-half, half, z),
                ],
                None,
                SceneSurface::Opaque,
                model,
                None,
                false,
            );
        }
        let mut frame =
            SoftwareFrame::with_device(&target.device, SIZE[0], SIZE[1], SIZE, CLEAR).unwrap();
        assert!(frame.trace_world(&feed, &[]));
        frame.finish_thermal(false);
        frame.read_rgba().into_owned()
    };
    // The wall alone, all one heat, reads mid-grey, not black.
    let uniform = thermal(false);
    let middle = uniform[(24 * SIZE[0] as usize + 32) * 4];
    assert!((64..=192).contains(&middle), "a uniform view: {middle}");
    let pixels = thermal(true);
    let at = |x: usize, y: usize| pixels[(y * SIZE[0] as usize + x) * 4..][..4].to_vec();
    let warm = at(32, 24);
    assert!(warm[0] > 200, "the mutant: {warm:?}");
    let cold = at(4, 4);
    assert!(cold[0] < warm[0] / 2, "around it: {cold:?}");
    for pixel in pixels.chunks_exact(4) {
        assert!(
            pixel[0] == pixel[1] && pixel[1] == pixel[2],
            "grey: {pixel:?}"
        );
    }
}
