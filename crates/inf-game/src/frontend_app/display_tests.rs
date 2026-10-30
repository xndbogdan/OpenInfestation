use super::*;

fn window(size: [u32; 2]) -> DisplaySelection {
    DisplaySelection {
        mode: DisplayMode::Window,
        size,
    }
}

#[test]
fn final_report_retains_heterogeneous_counters_across_renderer_epochs() {
    // Three distinct renderer lifetimes: frontend, terrain and model effects.
    // Sparse, overlapping categories distinguish accumulation from replacement
    // and catch omitted native consumers without deriving expected sums in code.
    let scene_epochs = [
        SoftwareSceneStats {
            frame_begins: 2,
            retired_resource_batches: 4,
            presentation_clears: 3,
            begins: 5,
            flushes: 7,
            models: 11,
            model_value_prefix_skips: 6,
            object_group_prefix_closures: 3,
            stars: 13,
            footers: 17,
            direct_sprites: 19,
            depth_resets: 23,
            model_commands: 29,
            rejected_primitives: 31,
            ..Default::default()
        },
        SoftwareSceneStats {
            frame_begins: 37,
            presentation_clears: 41,
            direct_flat_quads: 43,
            direct_shade_uv_quads: 2,
            terrain_commands: 47,
            water_commands: 53,
            depth_resets: 59,
            ..Default::default()
        },
        SoftwareSceneStats {
            models: 61,
            model_value_prefix_skips: 10,
            object_group_prefix_closures: 5,
            retired_resource_batches: 9,
            direct_sprites: 67,
            direct_flat_quads: 71,
            terrain_commands: 73,
            water_commands: 79,
            model_commands: 83,
            rejected_primitives: 89,
            ..Default::default()
        },
    ];
    let mut scene_total = SoftwareSceneStats::default();
    for epoch in scene_epochs {
        add_scene_stats(&mut scene_total, epoch);
    }
    add_scene_stats(&mut scene_total, SoftwareSceneStats::default());
    assert_eq!(
        scene_total,
        SoftwareSceneStats {
            frame_begins: 39,
            retired_resource_batches: 13,
            presentation_clears: 44,
            begins: 5,
            flushes: 7,
            models: 72,
            model_value_prefix_skips: 16,
            object_group_prefix_closures: 8,
            stars: 13,
            footers: 17,
            direct_sprites: 86,
            direct_flat_quads: 114,
            direct_shade_uv_quads: 2,
            terrain_commands: 120,
            water_commands: 132,
            depth_resets: 82,
            model_commands: 112,
            rejected_primitives: 120,
        }
    );
    let epochs = [
        SoftwarePixelStats {
            triangles: 2,
            solid_quads: 3,
            solid_triangles: 113,
            textured_triangles: 5,
            textured_quads: 7,
            textured_uv_triangles: 11,
            textured_uv_quads: 13,
            shared_shade_uv_triangles: 83,
            textured_shade_uv_quads: 103,
            effect_quads: 17,
            skipped: 19,
            fragments: 101,
            unsupported: 23,
            ..Default::default()
        },
        SoftwarePixelStats {
            shared_shade_far_triangles: 29,
            ordinary_far_quads: 31,
            colored_quads: 37,
            flat_shade_quads: 41,
            shared_shade_uv_quads: 89,
            textured_shade_uv_quads: 107,
            fragments: 103,
            ..Default::default()
        },
        SoftwarePixelStats {
            shared_shade_far_triangles: 43,
            ordinary_far_quads: 47,
            d2_triangles: 53,
            d6_quads: 59,
            colored_quads: 61,
            da_triangles: 67,
            flat_shade_quads: 71,
            shared_shade_uv_triangles: 97,
            shared_shade_uv_quads: 101,
            textured_shade_uv_quads: 109,
            skipped: 73,
            fragments: 107,
            unsupported: 79,
            ..Default::default()
        },
    ];
    let mut total = SoftwarePixelStats::default();
    for epoch in epochs {
        add_pixel_stats(&mut total, epoch);
    }
    add_pixel_stats(&mut total, SoftwarePixelStats::default());
    assert_eq!(
        [
            total.triangles,
            total.solid_quads,
            total.solid_triangles,
            total.textured_triangles,
            total.shared_shade_far_triangles,
            total.textured_quads,
            total.ordinary_far_quads,
            total.textured_uv_triangles,
            total.textured_uv_quads,
            total.shared_shade_uv_triangles,
            total.shared_shade_uv_quads,
            total.effect_quads,
            total.d2_triangles,
            total.d6_quads,
            total.textured_shade_uv_quads,
            total.colored_quads,
            total.da_triangles,
            total.flat_shade_quads,
            total.skipped,
            total.fragments,
            total.unsupported,
        ],
        [2, 3, 113, 5, 72, 7, 78, 11, 13, 180, 190, 17, 53, 59, 319, 98, 67, 112, 92, 311, 102]
    );
}

#[test]
#[ignore = "uses an SDL window; run explicitly in release mode"]
fn classic_output_preserves_native_texture_and_exact_bars_for_every_scaling_mode() {
    declare_dpi_awareness();
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let mut canvas = video
        .window("Infestation classic framebuffer check", 1280, 720)
        .hidden()
        .build()
        .unwrap()
        .into_canvas()
        .software()
        .build()
        .unwrap();
    let creator = canvas.texture_creator();
    let framebuffer = [640, 480];
    let frame = SoftwareFrame::new(framebuffer[0], framebuffer[1], [0, 0, 0, 255]);
    let mut texture = texture(Some(&creator), framebuffer)
        .unwrap()
        .expect("canvas texture");
    let colors = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    let mut native = Vec::with_capacity(640 * 480 * 4);
    for y in 0..480 {
        for x in 0..640 {
            native.extend_from_slice(&colors[usize::from(x >= 320) + usize::from(y >= 240) * 2]);
        }
    }
    for output in [[1280, 720], [1920, 1080]] {
        let resolution = format!("{}x{}", output[0], output[1]);
        assert_eq!(
            replace_window(canvas.window_mut(), 0, window(output)).unwrap(),
            output
        );
        let surface = crate::software_frame::SoftwareSurface::ClassicFramebuffer {
            window: canvas.window(),
            output_dimensions: output,
        };
        assert_eq!(surface.dimensions(&frame), framebuffer);
        surface.validate_dimensions(&frame).unwrap();
        assert_eq!(
            crate::software_frame::SoftwareSurface::Window(canvas.window())
                .validate_dimensions(&frame),
            Err(crate::software_frame::SoftwareSurfaceMismatch {
                actual: output,
                expected: framebuffer
            })
        );
        for scaling in [
            ClassicScaling::FourThree,
            ClassicScaling::Stretched,
            ClassicScaling::Integer,
        ] {
            assert_eq!(
                copy_pixels(&mut canvas, &mut texture, &native, framebuffer, scaling).unwrap(),
                output
            );
            assert_eq!([texture.query().width, texture.query().height], framebuffer);
            let rect = scaling.destination(framebuffer, output);
            let pixels = canvas.read_pixels(None, PixelFormatEnum::RGBA32).unwrap();
            for y in 0..output[1] {
                for x in 0..output[0] {
                    let local = [x as i32 - rect.x, y as i32 - rect.y];
                    let expected = if local[0] >= 0
                        && local[1] >= 0
                        && local[0] < rect.width as i32
                        && local[1] < rect.height as i32
                    {
                        colors[usize::from(local[0] >= rect.width as i32 / 2)
                            + usize::from(local[1] >= rect.height as i32 / 2) * 2]
                    } else {
                        [0, 0, 0, 255]
                    };
                    let offset = ((y * output[0] + x) * 4) as usize;
                    assert_eq!(
                        &pixels[offset..offset + 4],
                        &expected,
                        "{resolution}/{scaling:?} output pixel ({x},{y})"
                    );
                }
            }
            // Every source pixel distinct: the OpenGL snapshot export must
            // pick the same source pixel as SDL's scaler, ties included.
            let distinct: Vec<u8> = (0..480_u32)
                .flat_map(|y| {
                    (0..640_u32)
                        .map(move |x| [x as u8, y as u8, (x >> 8) as u8 * 64 + (y >> 8) as u8, 255])
                })
                .flatten()
                .collect();
            copy_pixels(&mut canvas, &mut texture, &distinct, framebuffer, scaling).unwrap();
            let pixels = canvas.read_pixels(None, PixelFormatEnum::RGBA32).unwrap();
            assert!(
                pixels == scale_pixels(&distinct, framebuffer, output, scaling),
                "{resolution}/{scaling:?} export sampling"
            );
            canvas.present();
        }
    }
}

#[test]
#[ignore = "opens an OpenGL window; run explicitly in release mode"]
fn opengl_output_draws_the_native_frame_at_its_presented_size_with_exact_bars() {
    declare_dpi_awareness();
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let framebuffer = [640, 480];
    let mut output = Output::open(&video, RendererChoice::OpenGl, [1280, 720], true).unwrap();
    assert_eq!(output.backend(), RendererBackend::OpenGl);
    assert!(output.texture_creator().is_none());
    let colors = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    let mut native = Vec::with_capacity(640 * 480 * 4);
    for y in 0..480 {
        for x in 0..640 {
            native.extend_from_slice(&colors[usize::from(x >= 320) + usize::from(y >= 240) * 2]);
        }
    }
    // Authored output: the device frame is the native frame.
    let mut frame = output
        .frame(
            framebuffer,
            framebuffer,
            ClassicScaling::FourThree,
            [0, 0, 0, 255],
        )
        .unwrap();
    assert!(frame.rgba().is_empty());
    assert_eq!(frame.read_size(), framebuffer);
    assert!(frame.replace_surface(&native));
    assert_eq!(&*frame.read_rgba(), native.as_slice());
    for size in [[1280, 720], [1920, 1080]] {
        let resolution = format!("{}x{}", size[0], size[1]);
        replace_window(output.window_mut(), 0, window(size)).unwrap();
        assert_eq!(output.output_size().unwrap(), size);
        for scaling in [
            ClassicScaling::FourThree,
            ClassicScaling::Stretched,
            ClassicScaling::Integer,
        ] {
            // The device draws at the presented size; native layout stays.
            let rect = scaling.destination(framebuffer, size);
            let mut frame = output
                .frame(framebuffer, size, scaling, [0, 0, 0, 255])
                .unwrap();
            assert_eq!([frame.width(), frame.height()], framebuffer);
            assert_eq!(frame.read_size(), [rect.width, rect.height]);
            assert!(frame.replace_surface(&native));
            assert_eq!(
                copy_framebuffer(&mut output, None, &mut frame, scaling).unwrap(),
                size
            );
            let Output::Hardware { device, .. } = &output else {
                unreachable!("OpenGL output");
            };
            let pixels = device.read_output(size);
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let local = [x as i32 - rect.x, y as i32 - rect.y];
                    let expected = if local[0] >= 0
                        && local[1] >= 0
                        && local[0] < rect.width as i32
                        && local[1] < rect.height as i32
                    {
                        colors[usize::from(local[0] >= rect.width as i32 / 2)
                            + usize::from(local[1] >= rect.height as i32 / 2) * 2]
                    } else {
                        [0, 0, 0, 255]
                    };
                    let offset = ((y * size[0] + x) * 4) as usize;
                    assert_eq!(
                        &pixels[offset..offset + 4],
                        &expected,
                        "{resolution}/{scaling:?} output pixel ({x},{y})"
                    );
                }
            }
            // A snapshot remembers the drawn frame and exports it one to one.
            let drawn = frame.read_rgba().into_owned();
            let (exported_size, exported) = output
                .export_scaled(None, &drawn, frame.read_size(), scaling)
                .unwrap();
            assert_eq!(exported_size, size);
            assert!(exported == pixels, "{resolution}/{scaling:?} export");
            present_observed(&mut output, None, "opengl check", None);
        }
    }
}

#[test]
#[ignore = "changes a visible SDL display mode; run explicitly in release mode"]
fn visible_exclusive_mode_matches_requested_pixels_and_restores_desktop() {
    declare_dpi_awareness();
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let display = 0;
    let available = (0..video.num_display_modes(display).unwrap()).any(|index| {
        let mode = video.display_mode(display, index).unwrap();
        [mode.w, mode.h] == [800, 600]
    });
    if !available {
        eprintln!("800x600 exclusive display mode unavailable; visible hardware case skipped");
        return;
    }
    let desktop = video.current_display_mode(display).unwrap();
    let mut window = video
        .window("Infestation display reset check", 800, 600)
        .position_centered()
        .build()
        .unwrap();
    assert_ne!(
        window.window_flags() & sdl2::sys::SDL_WindowFlags::SDL_WINDOW_SHOWN as u32,
        0
    );
    assert!(!window.is_minimized());

    let full_screen = DisplaySelection {
        mode: DisplayMode::FullScreen,
        size: [800, 600],
    };
    assert_eq!(
        replace_window(&mut window, display, full_screen).unwrap(),
        [800, 600]
    );
    assert_eq!(window.fullscreen_state(), FullscreenType::True);
    assert_eq!(window.size(), (800, 600));
    assert_eq!(window.display_index().unwrap(), display);
    let requested = window.display_mode().unwrap();
    let actual = video.current_display_mode(display).unwrap();
    assert_eq!([requested.w, requested.h], [800, 600]);
    assert_eq!(
        [actual.w, actual.h],
        [800, 600],
        "hidden flags or an enumerated mode do not prove the physical switch"
    );

    // A mode the monitor does not report fails before touching the window.
    let unreported = DisplaySelection {
        mode: DisplayMode::FullScreen,
        size: [801, 600],
    };
    assert!(matches!(
        replace_window(&mut window, display, unreported),
        Err(FrontendRuntimeError::DisplayReset(_))
    ));
    // An invalid window leaves exclusive mode before failing. Recovery must
    // use a live native descriptor too, restoring the actual old mode.
    assert!(matches!(
        replace_window(&mut window, display, self::window([0, 600])),
        Err(FrontendRuntimeError::DisplayReset(_))
    ));
    assert_eq!(window.fullscreen_state(), FullscreenType::True);
    assert_eq!(window.size(), (800, 600));
    let restored_fullscreen = video.current_display_mode(display).unwrap();
    assert_eq!([restored_fullscreen.w, restored_fullscreen.h], [800, 600]);

    replace_window(&mut window, display, self::window([640, 480])).unwrap();
    assert_eq!(window.fullscreen_state(), FullscreenType::Off);
    assert_eq!(window.size(), (640, 480));
    let restored = video.current_display_mode(display).unwrap();
    assert_eq!(
        [restored.w, restored.h, restored.refresh_rate],
        [desktop.w, desktop.h, desktop.refresh_rate]
    );
}

#[test]
#[ignore = "covers the desktop with a visible SDL window; run explicitly in release mode"]
fn visible_borderless_covers_the_desktop_without_a_mode_change() {
    declare_dpi_awareness();
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let display = 0;
    let desktop = video.desktop_display_mode(display).unwrap();
    let mut window = video
        .window("Infestation borderless check", 640, 480)
        .position_centered()
        .build()
        .unwrap();
    // The selection's size picks only the layout; the desktop is the output.
    let borderless = DisplaySelection {
        mode: DisplayMode::Borderless,
        size: [1280, 720],
    };
    let output = replace_window(&mut window, display, borderless).unwrap();
    assert_eq!(output, [desktop.w as u32, desktop.h as u32]);
    assert_eq!(window.fullscreen_state(), FullscreenType::Desktop);
    assert_eq!(window.size(), (desktop.w as u32, desktop.h as u32));
    let active = video.current_display_mode(display).unwrap();
    assert_eq!(
        [active.w, active.h, active.refresh_rate],
        [desktop.w, desktop.h, desktop.refresh_rate]
    );
    // DPI awareness: the desktop is the monitor's physical size.
    let modes = monitor_modes(&video, display);
    assert_eq!(modes.desktop, Some(output));
    assert_eq!(modes.reported().last(), Some(&output));

    replace_window(&mut window, display, self::window([640, 480])).unwrap();
    assert_eq!(window.fullscreen_state(), FullscreenType::Off);
    assert_eq!(window.size(), (640, 480));
}
