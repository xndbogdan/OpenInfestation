use inf_formats::{ByteSpan, FrontendMenu, FrontendMenuItem, FrontendMenuLayout};

use super::*;
use crate::menu::{MenuControllerSpec, MenuData};

fn data(count: usize, flags: u8, visible_rows: u8) -> MenuData {
    MenuData::new(FrontendMenu {
        virtual_address: 0x1000,
        definition_span: ByteSpan::new(0, 20),
        items_address: 0x2000,
        layouts_address: 0x3000,
        initial_selection: 0,
        flags: 0,
        items: (0..count)
            .map(|index| FrontendMenuItem {
                virtual_address: 0x2000 + index as u32 * 32,
                span: ByteSpan::new(0, 32),
                flags: 4,
                draw_callback: 0x453a80,
                draw_arguments: [index as u32, 0],
                action_callback: 0,
                action_arguments: [0; 2],
                row_height: 1,
                age: 0,
            })
            .collect(),
        layouts: vec![FrontendMenuLayout {
            virtual_address: 0x3000,
            span: ByteSpan::new(0, 16),
            flags,
            item_count: count as u8,
            visible_rows,
            reserved: 0,
            x: 100,
            y: 200,
            width: 300,
            height: 400,
            row_spacing: 50,
            scroll_animation: 0,
        }],
        layout_terminator: ByteSpan::new(16, 32),
    })
}

fn stack(data: MenuData) -> MenuStack {
    let mut stack = MenuStack::new(MenuControllerSpec {
        fade_rate: 65535,
        viewport: [0, 0, 1024, 1024],
    });
    stack.push(MenuState::new(data, MenuBasis::INITIAL));
    stack.fade = 65535;
    stack
}

fn input(table: &[u16; 4096]) -> MenuDrawInput<'_> {
    MenuDrawInput {
        surface_pixels: [1024, 1024],
        visibility: MenuVisibility::OwnerFlagClear,
        sine_quarter: table,
    }
}

type RowEffect =
    fn(&mut MenuRowDrawRequest, &mut MenuState, &mut Recorder) -> Result<(), &'static str>;

struct Recorder {
    setup: Vec<MenuControllerDrawSetup>,
    basis: Vec<MenuBasis>,
    clips: Vec<[i16; 4]>,
    requests: Vec<MenuRowDrawRequest>,
    colors: MenuDrawColors,
    effect: Option<RowEffect>,
    setup_error: bool,
    /// (setups, rows) seen when each controller frame ran.
    frames: Vec<(usize, usize)>,
    frame_error: bool,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            setup: vec![],
            basis: vec![],
            clips: vec![],
            requests: vec![],
            colors: MenuDrawColors {
                selected_start: u32::from_le_bytes([20, 100, 240, 0xa1]),
                ordinary: u32::from_le_bytes([12, 34, 56, 0xb2]),
                selected_end: u32::from_le_bytes([220, 20, 40, 0xc3]),
            },
            effect: None,
            setup_error: false,
            frames: vec![],
            frame_error: false,
        }
    }
}

impl MenuDrawHost for Recorder {
    type Error = &'static str;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.setup.push(setup);
        if self.setup_error {
            Err("setup")
        } else {
            Ok(())
        }
    }
    fn draw_controller_frame(&mut self) -> Result<(), Self::Error> {
        self.frames.push((self.setup.len(), self.requests.len()));
        if self.frame_error {
            Err("frame")
        } else {
            Ok(())
        }
    }
    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        self.basis.push(basis);
        Ok(())
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Ok([-10, -20, 900, 700])
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        self.clips.push(clip);
        Ok(())
    }
    fn colors(&self) -> MenuDrawColors {
        self.colors
    }
    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), Self::Error> {
        self.requests.push(*request);
        if let Some(effect) = self.effect {
            effect(request, menu, self)?;
        }
        Ok(())
    }
}

#[test]
fn linear_rows_scale_clip_and_feed_back_actual_callback_height() {
    let data = data(3, 4, 3);
    let mut stack = stack(data.clone());
    stack.spec.viewport = [128, 256, 896, 768];
    stack.current_mut().unwrap().age_millis = 200;
    let mut host = Recorder {
        effect: Some(|request, _, _| {
            if request.item_index == 0 {
                request.descriptor.position_pixels[1] += 50;
            }
            Ok(())
        }),
        ..Recorder::default()
    };
    let outcome = stack.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(
        outcome,
        MenuDrawOutcome {
            rows: 3,
            menu_drawn: true
        }
    );
    assert!(stack.drawn());
    assert_eq!(host.clips, [[91, 331, 316, 531], [-10, -20, 900, 700]]);
    assert_eq!(host.requests[0].descriptor.position_pixels, [203, 356]);
    assert_eq!(host.requests[0].descriptor.extent_pixels, [225, 200]);
    assert_eq!(
        host.requests[0].descriptor.color,
        MenuColor::from_rgb([120, 60, 140])
    );
    assert_eq!(host.requests[0].descriptor.flags, 3);
    assert_eq!(host.requests[0].descriptor.reveal, 65535);
    assert_eq!(host.requests[1].descriptor.position_pixels, [203, 431]);
    assert_eq!(data.records().items[0].row_height, 3);
    assert_eq!(host.basis, [MenuBasis::INITIAL]);
    assert_eq!(host.setup[0].ambient_color, [32; 3]);
    assert_eq!(host.setup[0].context_64, 2048);
    assert_eq!(host.setup[0].context_68, 4096);
}

#[test]
fn retail_root_walk_uses_original_callbacks_arguments_and_800_by_600_geometry() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let data = MenuData::new(assets.root_menu().clone());
    let mut stack = stack(data.clone());
    let mut host = Recorder::default();
    let mut input = input(&assets.sine_table().values);
    input.surface_pixels = [800, 600];
    stack.draw(input, &mut host).unwrap();
    assert_eq!(
        host.requests
            .iter()
            .map(|r| (r.callback_address, r.arguments))
            .collect::<Vec<_>>(),
        [
            (0x4835e0, [204, 0]),
            (0x4834f0, [158, 0]),
            (0x4834f0, [210, 0]),
        ]
    );
    assert_eq!(
        host.requests
            .iter()
            .map(|r| r.descriptor.position_pixels)
            .collect::<Vec<_>>(),
        [[390, 375], [390, 404], [390, 433]]
    );
    assert_eq!(host.requests[0].descriptor.extent_pixels, [351, 527]);
    assert_eq!(host.clips[0], [390, 345, 741, 872]);
    assert!(data.records().items.iter().all(|item| item.row_height == 1));
}

#[test]
fn callback_mutations_affect_later_age_palette_visibility_and_spacing_reads() {
    let data = data(4, 2, 4);
    let mut stack = stack(data.clone());
    for entry in data.records_mut().items.iter_mut() {
        entry.age = 700;
    }
    let mut host = Recorder {
        effect: Some(|request, menu, host| {
            if request.item_index == 0 {
                menu.reset_ages();
                let mut records = menu.data().records_mut();
                records.items[1].flags = 0;
                records.layouts[0].row_spacing = 25;
                host.colors.ordinary = u32::from_le_bytes([8, 9, 10, 0xd4]);
                request.descriptor.position_pixels[1] += 50;
            }
            Ok(())
        }),
        ..Recorder::default()
    };
    stack.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(
        host.requests
            .iter()
            .map(|r| r.item_index)
            .collect::<Vec<_>>(),
        [0, 2, 3]
    );
    assert_eq!(host.requests[0].descriptor.reveal, 32767);
    assert_eq!(host.requests[1].descriptor.reveal, 0);
    assert_eq!(host.requests[1].descriptor.age_millis, 0);
    assert_eq!(
        host.requests[1].descriptor.color,
        MenuColor::from_word(u32::from_le_bytes([8, 9, 10, 0xd4]))
    );
    assert_eq!(data.records().items[0].row_height, 3);
    assert_eq!(host.requests[1].descriptor.position_pixels[1], 275);
}

#[test]
fn scrolling_includes_extra_row_and_marks_the_departing_animation_edge() {
    let data = data(4, 2 | 8 | 0x10, 2);
    let mut stack = stack(data.clone());
    data.records_mut().layouts[0].scroll_animation = 100;
    let mut host = Recorder::default();
    stack.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(
        host.requests
            .iter()
            .map(|r| r.item_index)
            .collect::<Vec<_>>(),
        [3, 0, 1]
    );
    assert_eq!(
        host.requests
            .iter()
            .map(|r| r.descriptor.position_pixels[1])
            .collect::<Vec<_>>(),
        [175, 225, 275]
    );
    assert_eq!(host.requests[0].descriptor.flags & 0x20, 0x20);
    assert_eq!(host.requests[1].descriptor.flags & 0x20, 0);
    data.records_mut().layouts[0].scroll_animation = -100;
    let mut host = Recorder::default();
    stack.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(
        host.requests
            .iter()
            .map(|r| r.item_index)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        host.requests
            .iter()
            .map(|r| r.descriptor.position_pixels[1])
            .collect::<Vec<_>>(),
        [175, 225, 275]
    );
    assert_eq!(host.requests[2].descriptor.flags & 0x20, 0x20);
}

#[test]
fn selected_layout_uses_visible_skip_and_later_layout_backfills_by_height() {
    let data = data(5, 1, 2);
    let mut stack = stack(data.clone());
    stack.current_mut().unwrap().selection = 4;
    stack.current_mut().unwrap().scroll_start = 2;
    data.records_mut().items[0].flags = 0;
    let mut host = Recorder::default();
    stack.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(
        host.requests
            .iter()
            .map(|r| r.item_index)
            .collect::<Vec<_>>(),
        [3, 4]
    );
    // Selection lies beyond this layout: native adds unsigned row-height
    // feedback from its tail, not a fixed number of item records.
    stack.current_mut().unwrap().selection = 5;
    data.records_mut().items[0].flags = 4;
    data.records_mut().items[4].row_height = 2;
    let mut host = Recorder::default();
    stack.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(
        host.requests
            .iter()
            .map(|r| r.item_index)
            .collect::<Vec<_>>(),
        [4]
    );
}

#[test]
fn circular_positions_use_original_table_integer_q31_and_fixed_item_flag() {
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let table = &assets.sine_table().values;
    let data = data(4, 0x20, 4);
    let mut stack = stack(data.clone());
    data.records_mut().items[2].flags |= 8;
    let mut host = Recorder::default();
    stack.draw(input(table), &mut host).unwrap();
    assert_eq!(host.clips, [[-10, -20, 900, 700]; 2]);
    assert_eq!(
        host.requests[0].placement,
        MenuDrawPlacement::Circular { angle: 0 }
    );
    assert_eq!(host.requests[0].descriptor.position_pixels, [0, -74]);
    assert_eq!(host.requests[0].context_origin_pixels, [100, 200]);
    assert_eq!(host.requests[0].descriptor.flags, 0x11);
    // Integer division is before multiplication: 65535/(4*200)=81.
    assert_eq!(
        host.requests[1].placement,
        MenuDrawPlacement::Circular { angle: 16200 }
    );
    assert_eq!(
        host.requests[2].placement,
        MenuDrawPlacement::Circular { angle: 0 }
    );
}

#[test]
fn row_and_layout_flags_preserve_the_native_color_and_text_branches() {
    let data = data(4, 0x80 | 4, 4);
    let mut stack = stack(data.clone());
    data.records_mut().items[0].flags |= 0x10;
    data.records_mut().items[1].flags |= 0x40;
    data.records_mut().items[2].flags |= 0x48;
    let mut host = Recorder::default();
    stack.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(host.requests[0].descriptor.flags, 2);
    assert_eq!(
        host.requests[0].descriptor.color,
        MenuColor::from_word(host.colors.ordinary)
    );
    assert_eq!(host.requests[1].descriptor.flags, 0x42);
    assert_eq!(
        host.requests[1].descriptor.color,
        MenuColor::from_word(host.colors.selected_end)
    );
    assert_eq!(
        host.requests[2].descriptor.color,
        MenuColor::from_word(host.colors.selected_end)
    );
    assert_eq!(
        host.requests[3].descriptor.color,
        MenuColor::from_word(host.colors.ordinary)
    );
}

#[test]
fn selected_color_only_preserves_the_high_byte_on_full_word_branches() {
    let colors = Recorder::default().colors;
    let mut menu = MenuState::new(data(1, 0, 1), MenuBasis::INITIAL);
    menu.age_millis = 200;
    let records = menu.data.records();
    let mut item = records.items[0];
    let layout = records.layouts[0];
    // 0x40 alone does not bypass interpolation for the selected item. 0x48
    // forces the complete selected-end word; 0x10 takes precedence over both.
    for (flags, expected) in [
        (4, MenuColor::from_rgb([120, 60, 140])),
        (4 | 0x40, MenuColor::from_rgb([120, 60, 140])),
        (4 | 0x48, MenuColor::from_word(colors.selected_end)),
        (4 | 0x58, MenuColor::from_word(colors.ordinary)),
    ] {
        item.flags = flags;
        assert_eq!(row_style(&menu, 0, item, layout, colors).1, expected);
    }
}

#[test]
fn failed_row_preserves_prior_feedback_restores_clip_and_does_not_mark_drawn() {
    let data = data(3, 1, 3);
    let mut stack = stack(data.clone());
    let mut host = Recorder {
        effect: Some(|request, _, _| {
            if request.item_index == 1 {
                return Err("row");
            }
            request.descriptor.position_pixels[1] += 50;
            Ok(())
        }),
        ..Recorder::default()
    };
    assert!(matches!(
        stack.draw(input(&[0; 4096]), &mut host),
        Err(MenuError::Host("row"))
    ));
    assert_eq!(data.records().items[0].row_height, 2);
    assert_eq!(data.records().items[1].row_height, 1);
    assert_eq!(host.clips.last(), Some(&[-10, -20, 900, 700]));
    assert!(!stack.drawn());
}

/// 00453783 calls the controller's virtual +0C after the setup and before
/// the current menu, whatever the fade; a failure leaves +25 clear.
#[test]
fn controller_frame_follows_setup_and_precedes_rows_at_any_fade() {
    let mut controller = stack(data(2, 1, 2));
    let mut host = Recorder::default();
    controller.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(host.frames, [(1, 0)]);
    assert_eq!(host.requests.len(), 2);
    controller.fade = 200;
    controller.draw(input(&[0; 4096]), &mut host).unwrap();
    assert_eq!(host.frames, [(1, 0), (2, 2)]);
    assert_eq!(host.requests.len(), 2);
    let mut failing = stack(data(2, 1, 2));
    let mut host = Recorder {
        frame_error: true,
        ..Recorder::default()
    };
    assert!(matches!(
        failing.draw(input(&[0; 4096]), &mut host),
        Err(MenuError::Host("frame"))
    ));
    assert!(host.requests.is_empty());
    assert!(!failing.drawn());
}

#[test]
fn partial_fade_and_empty_controller_still_run_setup_without_fabricating_rows() {
    let mut stack = stack(data(2, 1, 2));
    stack.fade = 200;
    let mut host = Recorder::default();
    assert_eq!(
        stack.draw(input(&[0; 4096]), &mut host).unwrap(),
        MenuDrawOutcome {
            rows: 0,
            menu_drawn: false
        }
    );
    assert!(stack.drawn());
    assert_eq!(host.setup.len(), 1);
    assert!(host.basis.is_empty());
    assert!(host.clips.is_empty());
    stack.pop(1);
    stack.fade = 65535;
    host.setup_error = true;
    assert!(matches!(
        stack.draw(input(&[0; 4096]), &mut host),
        Err(MenuError::Host("setup"))
    ));
    assert!(!stack.drawn());
}

#[test]
fn invalid_divisors_report_state_failure_after_actual_callback() {
    let data = data(1, 1, 1);
    let mut stack = stack(data.clone());
    data.records_mut().layouts[0].row_spacing = 0;
    let mut host = Recorder::default();
    assert!(matches!(
        stack.draw(input(&[0; 4096]), &mut host),
        Err(MenuError::State(MenuStateError::ZeroDrawDivisor {
            field: "row spacing"
        }))
    ));
    assert_eq!(host.requests.len(), 1);
    assert!(!stack.drawn());
}

/// The in-game Options (00539390) has six items under a layout that counts
/// thirteen rows from item one, so the census 00452B60 reads the flags of
/// the eight records past the array (items 6 to 13); the catalog carries
/// them from the executable.
#[test]
fn the_in_game_options_census_reads_the_records_past_its_array() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut catalog = crate::frontend_assets::FrontendMenuCatalog::new(
        crate::frontend_assets::FrontendAssets::open(&root).unwrap(),
    );
    let options = catalog.menu_data(0x0053_9390).unwrap();
    assert_eq!(options.records().items.len(), 6);
    assert_eq!(options.census_flags(5), Some(5));
    let adjacent: Vec<_> = (6..15).map(|index| options.census_flags(index)).collect();
    assert_eq!(
        adjacent,
        [
            Some(0x0053_91b8),
            Some(0x0053_8d58),
            Some(0x000d_0d02),
            Some(0x0c),
            Some(4),
            Some(4),
            Some(4),
            Some(0x0053_93d8),
            None
        ]
    );
}

/// Display (005390A0), Sounds (00538E80) and Controls (005391A0) name the
/// same layout chain 00538D58; the catalog binds them, and the in-game
/// Options, to one shared copy of it.
#[test]
fn the_options_submenus_share_their_layout_chain() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut catalog = crate::frontend_assets::FrontendMenuCatalog::new(
        crate::frontend_assets::FrontendAssets::open(&root).unwrap(),
    );
    let menus: Vec<_> = [0x0053_9390, 0x0053_90a0, 0x0053_8e80, 0x0053_91a0]
        .into_iter()
        .map(|address| catalog.menu_data(address).unwrap())
        .collect();
    for menu in &menus {
        assert_eq!(menu.layout_source().unwrap().address, 0x0053_8d58);
    }
    menus[0].layouts().records_mut()[1].scroll_animation = 7;
    for menu in &menus[1..] {
        assert_eq!(menu.records().layouts[1].scroll_animation, 7);
    }
    // Reopening a definition still returns its own interned tables.
    let again = catalog.menu_data(0x0053_90a0).unwrap();
    again.records_mut().items[1].row_height = 3;
    assert_eq!(menus[1].records().items[1].row_height, 3);
}
