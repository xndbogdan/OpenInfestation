use super::*;
use std::path::Path;

use inf_formats::FrontendExecutable;

use crate::{
    frontend_assets::FrontendAssets,
    frontend_display::frontend_startup,
    frontend_screen::FrontendProcessState,
    menu::{MenuColor, MenuData, MenuDrawPlacement, MenuRowDescriptor, MenuSource},
    model_primitives::PrimitiveTables,
    software_frame::SoftwareFrame,
    software_menu::{SoftwareMenuRequest, SoftwareMenuState},
    software_scene::{SoftwareScene, SoftwareSceneRequest},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

#[test]
fn custom_text_gate_retains_full_argument_and_signed_low_word() {
    assert_eq!(custom_text_argument(0), None);
    assert_eq!(custom_text_argument(0x0000_ffff), None);
    assert_eq!(custom_text_argument(0x1234_8000), None);
    assert_eq!(custom_text_argument(0xffff_7fff), Some(-32769));
    assert_eq!(custom_text_argument(0x0001_0000), Some(65536));
    assert_eq!(custom_text_argument(0x380), Some(0x380));
}

#[test]
fn custom_callback_uses_native_reveal_wrap_and_pixel_renderer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    for (tier, dimensions) in [[320, 240], [512, 384], [640, 480], [800, 600], [1024, 768]]
        .into_iter()
        .enumerate()
    {
        runtime
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty: tier as u8,
                    language: 0,
                },
            })
            .unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        runtime
            .apply(ResidencyRequest::Activate { level: 47 })
            .unwrap();
        let resources = RenderResources::new(&runtime);
        let startup = frontend_startup(
            &runtime,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        let process = FrontendProcessState::default();
        process.publish_colors(MenuDrawColors {
            selected_start: resources.style(3).unwrap(),
            ordinary: resources.style(2).unwrap(),
            selected_end: resources.style(2).unwrap(),
        });
        let mut custom_request = row(BRIEFING_TEXT_CALLBACK, 0x157);
        custom_request.descriptor.flags |= 0x20;
        let (custom_pixels, custom_passes) = draw_row(
            &assets,
            &tables,
            resources,
            &startup,
            &process,
            dimensions,
            &mut custom_request,
        );
        assert_eq!(custom_request.descriptor.flags & 0x20, 0);
        assert!(
            custom_passes > 0,
            "tier {tier} must rasterize authored text"
        );
        assert!(custom_pixels
            .chunks_exact(4)
            .any(|p| p != [17, 23, 31, 255]));
        let mut generic_request = row(0x0045_3a80, 0x157);
        let (generic_pixels, generic_passes) = draw_row(
            &assets,
            &tables,
            resources,
            &startup,
            &process,
            dimensions,
            &mut generic_request,
        );
        assert_eq!(
            custom_pixels, generic_pixels,
            "tier {tier} custom row differs from recovered shared text call"
        );
        assert_eq!(custom_passes, generic_passes);
        assert_eq!(
            custom_request.descriptor.position_pixels,
            generic_request.descriptor.position_pixels
        );
        let mut skipped = row(BRIEFING_TEXT_CALLBACK, 0);
        let initial_descriptor = skipped.descriptor;
        let (pixels, passes) = draw_row(
            &assets,
            &tables,
            resources,
            &startup,
            &process,
            dimensions,
            &mut skipped,
        );
        assert_eq!(skipped.descriptor, initial_descriptor);
        assert_eq!(passes, 0);
        assert!(pixels.chunks_exact(4).all(|p| p == [17, 23, 31, 255]));
    }
}

fn row(callback_address: u32, argument: u32) -> MenuRowDrawRequest {
    MenuRowDrawRequest {
        menu_source: MenuSource::AuthoredDefinition {
            address: inf_formats::BRIEFING_DETAIL_MENU_ADDRESS,
            span: inf_formats::ByteSpan::new(0, 20),
        },
        layout_index: 0,
        item_index: 0,
        callback_address,
        arguments: [argument, 0],
        context_origin_pixels: [100, 50],
        placement: MenuDrawPlacement::Linear,
        descriptor: MenuRowDescriptor {
            position_pixels: [30, 40],
            extent_pixels: [75, 200],
            reveal: 32767,
            flags: 0,
            age_millis: 1000,
            color: MenuColor::from_rgb([128; 3]),
        },
    }
}

fn draw_row(
    assets: &FrontendAssets,
    tables: &PrimitiveTables,
    resources: RenderResources<'_>,
    startup: &crate::frontend_app::FrontendAppStartup,
    process: &FrontendProcessState,
    dimensions: [u32; 2],
    request: &mut MenuRowDrawRequest,
) -> (Vec<u8>, usize) {
    let mut frame = SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
    let mut state = SoftwareMenuState::default();
    {
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut scene,
            resources,
            focal: startup.projection.focal,
            process,
            state: &mut state,
        })
        .unwrap();
        let mut host = BriefingMenuDrawHost {
            menu: &mut menu,
            resources,
            omitted_string_slots: Vec::new(),
        };
        host.begin_controller(MenuControllerDrawSetup {
            surface_pixels: dimensions.map(|v| v as i32),
            context_4c: 0,
            context_64: 2048,
            context_68: 4096,
            light_direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
            light_color: [128; 3],
            ambient_color: [32; 3],
        })
        .unwrap();
        let mut menu_state = MenuState::new(
            MenuData::new(assets.root_menu().clone()),
            MenuBasis::INITIAL,
        );
        host.draw_row(request, &mut menu_state).unwrap();
        assert!(host.omitted_string_slots.is_empty());
        assert_eq!(
            host.menu.published_origin_pixels(),
            request.context_origin_pixels
        );
    }
    (frame.rgba().to_vec(), state.glyph_passes)
}
