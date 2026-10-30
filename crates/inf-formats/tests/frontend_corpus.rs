use std::fs;
use std::path::Path;

use inf_formats::{
    parse_frontend_camera_presets, parse_frontend_menu, parse_frontend_scene_models,
    FrontendExecutable, BRIEFING_DETAIL_MENU_ADDRESS, BRIEFING_LIST_MENU_ADDRESS,
    BRIEFING_MENU_ADDRESS, FRONTEND_CAMERA_PRESETS_ADDRESS, FRONTEND_ROOT_MENU_ADDRESS,
    FRONTEND_SCENE_MODELS_ADDRESS, PHASE12_MENU_ADDRESS, PHASE5_MENU_ADDRESS,
};

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn retail_root_menu_matches_its_immediate_draw_and_action_consumers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let exe = fs::read(root.join("Infest.exe")).unwrap();
    let source = FrontendExecutable::parse(&exe).unwrap().data();
    let menu = parse_frontend_menu(source, FRONTEND_ROOT_MENU_ADDRESS).unwrap();
    assert_eq!(menu.items_address, 0x0053_c008);
    assert_eq!(menu.layouts_address, 0x0053_c068);
    assert_eq!((menu.initial_selection, menu.flags), (0, 0));
    assert_eq!(menu.items.len(), 3);
    assert_eq!(menu.layouts.len(), 1);
    assert_eq!(
        menu.items
            .iter()
            .map(|row| row.draw_callback)
            .collect::<Vec<_>>(),
        [0x0048_35e0, 0x0048_34f0, 0x0048_34f0]
    );
    // Handler-0 global strings: Start Game, Options Table, Network.
    assert_eq!(
        menu.items
            .iter()
            .map(|row| row.draw_arguments)
            .collect::<Vec<_>>(),
        [[204, 0], [158, 0], [210, 0]]
    );
    assert_eq!(
        menu.items
            .iter()
            .map(|row| row.action_callback)
            .collect::<Vec<_>>(),
        [0x0048_3490, 0x0048_34b0, 0x0045_fd40]
    );
    assert_eq!(menu.items[1].action_arguments, [0, 0x0053_9378]);
    assert!(menu
        .items
        .iter()
        .all(|row| row.flags == 4 && row.row_height == 0 && row.age == 0));
    let layout = menu.layouts[0];
    assert_eq!(
        (layout.flags, layout.item_count, layout.visible_rows),
        (2, 3, 3)
    );
    assert_eq!(
        (layout.x, layout.y, layout.width, layout.height),
        (500, 640, 450, 900)
    );
    assert_eq!(layout.row_spacing, 50);
    assert_eq!(layout.scroll_animation, 0);
}

#[test]
fn retail_briefing_menus_parse_through_the_shared_menu_decoder() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let exe = fs::read(root.join("Infest.exe")).unwrap();
    let source = FrontendExecutable::parse(&exe).unwrap().data();
    // The pushed briefing menu: a header row plus three selectable rows with
    // the generic text draw and thin action forwarders, selection starting
    // at row 1, over a 1-row header layout and a 3-row body layout.
    let menu = parse_frontend_menu(source, BRIEFING_MENU_ADDRESS).unwrap();
    assert_eq!(menu.items_address, 0x0052_f100);
    assert_eq!(menu.layouts_address, 0x0052_f180);
    assert_eq!((menu.initial_selection, menu.flags), (1, 0));
    assert_eq!(menu.items.len(), 4);
    assert_eq!(menu.layouts.len(), 2);
    assert_eq!(
        menu.items.iter().map(|row| row.flags).collect::<Vec<_>>(),
        [9, 1, 1, 1]
    );
    assert_eq!(
        menu.items
            .iter()
            .map(|row| row.draw_callback)
            .collect::<Vec<_>>(),
        [0x0045_3a80, 0x0045_3a80, 0x0045_3a80, 0x0045_3a80]
    );
    assert_eq!(
        menu.items
            .iter()
            .map(|row| row.draw_arguments)
            .collect::<Vec<_>>(),
        [[0, 0], [0x157, 0], [0x158, 0], [0x159, 0]]
    );
    assert_eq!(
        menu.items
            .iter()
            .map(|row| row.action_callback)
            .collect::<Vec<_>>(),
        [0, 0x0040_a3f0, 0x0040_a400, 0x0040_a410]
    );
    assert!(menu.items.iter().all(|row| row.action_arguments == [0, 0]));
    assert_eq!(
        menu.layouts
            .iter()
            .map(|layout| (
                layout.flags,
                layout.item_count,
                layout.visible_rows,
                layout.x,
                layout.y,
                layout.width,
                layout.height,
                layout.row_spacing,
                layout.scroll_animation
            ))
            .collect::<Vec<_>>(),
        [
            (0x82, 1, 1, 80, 60, 900, 60, 50, 0),
            (0x82, 3, 3, 100, 120, 900, 400, 50, 0),
        ]
    );
    // The custom single-row menu and its four-row reuse of the same record:
    // custom draw/action pair, full-surface layouts.
    for (address, rows, geometry) in [
        (BRIEFING_DETAIL_MENU_ADDRESS, 1usize, (0, 0, 1024, 1024)),
        (BRIEFING_LIST_MENU_ADDRESS, 4usize, (0, 0, 1200, 1024)),
    ] {
        let menu = parse_frontend_menu(source, address).unwrap();
        assert_eq!(menu.items.len(), rows);
        assert_eq!((menu.initial_selection, menu.flags), (0, 0));
        assert_eq!(menu.layouts.len(), 1);
        assert!(menu.items.iter().all(|row| row.flags == 0x11
            && row.draw_callback == 0x0040_a350
            && row.draw_arguments == [0, 0]
            && row.action_callback == 0x0040_a340
            && row.action_arguments == [0, 0]));
        let layout = menu.layouts[0];
        assert_eq!(layout.flags, 0x9a);
        assert_eq!(
            (
                usize::from(layout.item_count),
                usize::from(layout.visible_rows)
            ),
            (rows, rows)
        );
        assert_eq!((layout.x, layout.y, layout.width, layout.height), geometry);
        assert_eq!((layout.row_spacing, layout.scroll_animation), (100, 0));
    }
}

#[test]
fn retail_phase5_menu_parses_through_the_shared_menu_decoder() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let exe = fs::read(root.join("Infest.exe")).unwrap();
    let source = FrontendExecutable::parse(&exe).unwrap().data();
    let menu = parse_frontend_menu(source, PHASE5_MENU_ADDRESS).unwrap();
    assert_eq!(menu.items_address, 0x0053_5298);
    assert_eq!(menu.layouts_address, 0x0053_5318);
    assert_eq!((menu.initial_selection, menu.flags), (1, 0));
    assert_eq!(menu.items.len(), 4);
    assert_eq!(
        menu.items
            .iter()
            .map(|row| (
                row.flags,
                row.draw_callback,
                row.draw_arguments,
                row.action_callback,
                row.action_arguments
            ))
            .collect::<Vec<_>>(),
        [
            (9, 0x0045_3a80, [0x102, 0], 0, [0, 0]),
            (1, 0x0045_3a80, [0x104, 0], 0x0044_3150, [0, 0]),
            (1, 0x0045_3a80, [0x9e, 0], 0x0045_3be0, [0, 0x0053_9390]),
            (1, 0x0045_3a80, [0x106, 0], 0x0045_3be0, [0, 0x0053_5280]),
        ]
    );
    // Rows 2 and 3 reuse the generic child-push dispatcher with different
    // pushed definitions; row 1 invokes virtual +0x64 on command 4 only.
    assert_eq!(menu.layouts.len(), 2);
    let layout = menu.layouts;
    assert_eq!(
        layout
            .iter()
            .map(|layout| (
                layout.flags,
                layout.item_count,
                layout.visible_rows,
                layout.x,
                layout.y,
                layout.width,
                layout.height,
                layout.row_spacing,
                layout.scroll_animation
            ))
            .collect::<Vec<_>>(),
        [
            (1, 1, 1, 50, 100, 900, 100, 80, 0),
            (2, 3, 3, 80, 200, 900, 400, 80, 0),
        ]
    );
}

#[test]
fn retail_phase12_menu_parses_through_the_shared_menu_decoder() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let exe = fs::read(root.join("Infest.exe")).unwrap();
    let source = FrontendExecutable::parse(&exe).unwrap().data();
    let menu = parse_frontend_menu(source, PHASE12_MENU_ADDRESS).unwrap();
    assert_eq!(menu.items_address, 0x0053_5ba0);
    assert_eq!(menu.layouts_address, 0x0053_5b40);
    assert_eq!((menu.initial_selection, menu.flags), (1, 0));
    assert_eq!(menu.items.len(), 21);
    assert_eq!(
        (
            menu.items[0].flags,
            menu.items[0].draw_callback,
            menu.items[0].draw_arguments,
            menu.items[0].action_callback
        ),
        (0xc, 0x0045_3a80, [0x384, 0], 0)
    );
    // Seventeen indexed rows driving 00444B20 with their own index.
    for (index, row) in menu.items[1..18].iter().enumerate() {
        assert_eq!(
            (
                row.flags,
                row.draw_callback,
                row.draw_arguments,
                row.action_callback,
                row.action_arguments
            ),
            (
                4,
                0x0044_52a0,
                [index as u32, 0],
                0x0044_4b20,
                [index as u32, 0]
            ),
            "indexed row {index}"
        );
    }
    assert_eq!(
        menu.items[18..]
            .iter()
            .map(|row| (
                row.flags,
                row.draw_callback,
                row.draw_arguments,
                row.action_callback,
                row.action_arguments
            ))
            .collect::<Vec<_>>(),
        [
            (4, 0x0045_3a80, [0x3b4, 0], 0x0045_3be0, [0, 0x0053_5b88]),
            (4, 0x0045_3a80, [0x3b5, 0], 0x0045_3be0, [0, 0x0053_5b70]),
            (4, 0x0045_3a80, [0x0c3, 0], 0x0044_4b50, [0, 0]),
        ]
    );
    assert_eq!(menu.layouts.len(), 2);
    assert_eq!(
        menu.layouts
            .iter()
            .map(|layout| (
                layout.flags,
                layout.item_count,
                layout.visible_rows,
                layout.x,
                layout.y,
                layout.width,
                layout.height,
                layout.row_spacing,
                layout.scroll_animation
            ))
            .collect::<Vec<_>>(),
        [
            (1, 1, 1, 100, 60, 900, 70, 55, 0),
            (2, 21, 21, 100, 140, 900, 880, 38, 0),
        ]
    );
}

#[test]
fn retail_frontend_scene_and_camera_targets_retain_authored_order_and_units() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let exe = fs::read(root.join("Infest.exe")).unwrap();
    let source = FrontendExecutable::parse(&exe).unwrap().data();
    let count_offset = (0x0053_bfb0 - source.virtual_address) as usize;
    let model_count = word(source.bytes, count_offset);
    assert_eq!(model_count, 9);
    assert_eq!(word(source.bytes, count_offset + 4), 10); // Consumer's separate scale.
    let models =
        parse_frontend_scene_models(source, FRONTEND_SCENE_MODELS_ADDRESS, model_count).unwrap();
    assert_eq!(
        models
            .iter()
            .map(|record| record.model_slot)
            .collect::<Vec<_>>(),
        [744, 749, 754, 756, 745, 748, 747, 755, 757]
    );
    assert_eq!(models[0].position, [-1300, 500, 0]);
    assert_eq!(models[2].rotation_controls, [-1, -5, 49152]);
    assert_eq!(models[6].position, [100, -150, -5500]);
    assert_eq!(models[8].position, [12000, 3500, 3000]);
    assert!(models
        .iter()
        .all(|record| record.reserved_02 == 0 && record.reserved_16 == 0));

    // 00484AC0 addresses presets with index*24; the frontend's authored table
    // contains the ten records addressed by its preset calls (0 through 9).
    let cameras =
        parse_frontend_camera_presets(source, FRONTEND_CAMERA_PRESETS_ADDRESS, 10).unwrap();
    assert_eq!(cameras[0].eye, [0, 0, -7000]);
    assert_eq!(cameras[0].target, [0, 0, 0]);
    assert_eq!(cameras[2].eye, [-2200, 800, -1800]);
    assert_eq!(cameras[2].target, [750, 800, -600]);
    assert_eq!(cameras[9].eye, [1000, 0, -2000]);
    assert_eq!(cameras[9].target, [-3000, 0, -2000]);
}
