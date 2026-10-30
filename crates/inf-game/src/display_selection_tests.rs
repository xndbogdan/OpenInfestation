use super::*;

/// A 3840x2160 monitor reporting typical modes, desktop at its largest.
fn monitor() -> MonitorModes {
    MonitorModes::new(
        Some([3840, 2160]),
        [
            [3840, 2160],
            [640, 480],
            [800, 600],
            [1024, 768],
            [1280, 720],
            [1920, 1080],
            [1920, 1080],
        ],
    )
}

#[test]
fn display_words_keep_retail_values() {
    for mode in DisplayMode::ALL {
        assert_eq!(DisplayMode::from_index(mode.index()), mode);
        let borderless = mode == DisplayMode::Borderless;
        assert_eq!(
            DisplayMode::from_words(mode.full_screen_word(), borderless),
            mode
        );
    }
    assert_eq!(DisplayMode::Window.full_screen_word(), 0);
    assert_eq!(DisplayMode::FullScreen.full_screen_word(), 1);
    assert_eq!(DisplayMode::Borderless.full_screen_word(), 1);
    // Borderless refines only a covering word; any nonzero byte covers.
    assert_eq!(DisplayMode::from_words(0, true), DisplayMode::Window);
    assert_eq!(DisplayMode::from_words(7, false), DisplayMode::FullScreen);
    assert_eq!(DisplayMode::from_words(0x100, false), DisplayMode::Window);
}

#[test]
fn every_size_draws_with_the_largest_original_inside_it() {
    for (size, tier) in [
        ([320, 240], 0),
        ([512, 384], 1),
        ([640, 480], 2),
        ([800, 600], 3),
        ([1024, 768], 4),
        ([720, 480], 2),
        ([1280, 720], 3),
        ([1920, 1080], 4),
        ([3840, 2160], 4),
        ([600, 800], 1),
        ([200, 100], 0),
    ] {
        assert_eq!(tier_for(size), tier, "{size:?}");
    }
    assert_eq!(original_size(2), [640, 480]);
    assert_eq!(original_size(9), [1024, 768]);
}

#[test]
fn rows_follow_the_mode_and_drop_tier_zero_from_the_menu() {
    let modes = monitor();
    let originals_and_reported = [
        [320, 240],
        [512, 384],
        [640, 480],
        [800, 600],
        [1024, 768],
        [1280, 720],
        [1920, 1080],
        [3840, 2160],
    ];
    assert_eq!(modes.sizes(DisplayMode::Window), originals_and_reported);
    assert_eq!(
        modes.menu_sizes(DisplayMode::Window),
        originals_and_reported[1..]
    );
    assert_eq!(
        modes.menu_sizes(DisplayMode::FullScreen),
        [
            [640, 480],
            [800, 600],
            [1024, 768],
            [1280, 720],
            [1920, 1080],
            [3840, 2160]
        ]
    );
    // Borderless covers the desktop: its only entry.
    assert_eq!(modes.sizes(DisplayMode::Borderless), [[3840, 2160]]);
    assert_eq!(modes.menu_sizes(DisplayMode::Borderless), [[3840, 2160]]);
    // A window must fit the desktop.
    let small = MonitorModes::new(Some([1280, 720]), [[1280, 720], [1920, 1080]]);
    assert_eq!(
        small.menu_sizes(DisplayMode::Window),
        [[512, 384], [640, 480], [800, 600], [1280, 720]]
    );
    assert_eq!(
        small.menu_sizes(DisplayMode::FullScreen),
        [[1280, 720], [1920, 1080]]
    );
    // No report: windows keep the originals, full screen has nothing.
    let unknown = MonitorModes::default();
    assert_eq!(unknown.menu_sizes(DisplayMode::Window), ORIGINAL_SIZES[1..]);
    assert!(unknown.menu_sizes(DisplayMode::FullScreen).is_empty());
    assert!(unknown.menu_sizes(DisplayMode::Borderless).is_empty());
    assert_eq!(unknown.resolve(DisplayMode::FullScreen, [800, 600]), None);
}

#[test]
fn unlisted_sizes_select_the_largest_listed_below() {
    let modes = MonitorModes::new(Some([1920, 1080]), [[1280, 720], [1920, 1080]]);
    let full = DisplayMode::FullScreen;
    assert_eq!(modes.resolve(full, [1920, 1080]), Some([1920, 1080]));
    assert_eq!(modes.resolve(full, [1920, 1200]), Some([1920, 1080]));
    assert_eq!(modes.resolve(full, [1600, 900]), Some([1280, 720]));
    // Nothing below: the smallest.
    assert_eq!(modes.resolve(full, [800, 600]), Some([1280, 720]));
    // Borderless selects the desktop whatever the saved size.
    for size in [[320, 240], [800, 600], [2560, 1440]] {
        assert_eq!(
            modes.resolve(DisplayMode::Borderless, size),
            Some([1920, 1080])
        );
    }
}

#[test]
fn a_display_change_takes_full_screen_at_the_top_and_keeps_the_nearest_listed_size_otherwise() {
    let modes = MonitorModes::new(Some([1920, 1080]), [[1280, 720], [1920, 1080]]);
    let window = DisplaySelection {
        mode: DisplayMode::Window,
        size: [1600, 900],
    };
    // Full screen offers the highest reported size, whatever the old size.
    assert_eq!(
        modes.select_mode(window, DisplayMode::FullScreen),
        DisplaySelection {
            mode: DisplayMode::FullScreen,
            size: [1920, 1080],
        }
    );
    // Any other change keeps the nearest listed size.
    // A fresh install's Borderless (its 640x480 kept size) goes straight to
    // the top too.
    assert_eq!(
        modes.select_mode(
            DisplaySelection {
                mode: DisplayMode::Borderless,
                size: [640, 480],
            },
            DisplayMode::FullScreen
        ),
        DisplaySelection {
            mode: DisplayMode::FullScreen,
            size: [1920, 1080],
        }
    );
    // Borderless covers the desktop and keeps the size for later.
    assert_eq!(
        modes.select_mode(window, DisplayMode::Borderless),
        DisplaySelection {
            mode: DisplayMode::Borderless,
            size: [1600, 900],
        }
    );
    // A tier-0 size takes tier 1's original whatever the row.
    let tiny = DisplaySelection {
        mode: DisplayMode::Window,
        size: [320, 240],
    };
    assert_eq!(
        modes.select_mode(tiny, DisplayMode::FullScreen),
        DisplaySelection {
            mode: DisplayMode::FullScreen,
            size: [512, 384],
        }
    );
    // An empty row changes only the mode.
    assert_eq!(
        MonitorModes::default().select_mode(window, DisplayMode::FullScreen),
        DisplaySelection {
            mode: DisplayMode::FullScreen,
            size: [1600, 900],
        }
    );
}

#[test]
fn the_resolution_row_steps_like_00453b80() {
    use MenuCommand::*;
    let modes = MonitorModes::new(Some([1920, 1080]), [[1280, 720], [1920, 1080]]);
    let full = DisplayMode::FullScreen;
    // Previous stops at the first entry, next at the last; activate wraps.
    assert_eq!(
        modes.step(full, [1920, 1080], AdjustPrevious),
        Some([1280, 720])
    );
    assert_eq!(modes.step(full, [1280, 720], AdjustPrevious), None);
    assert_eq!(
        modes.step(full, [1280, 720], AdjustNext),
        Some([1920, 1080])
    );
    assert_eq!(modes.step(full, [1920, 1080], AdjustNext), None);
    assert_eq!(modes.step(full, [1920, 1080], Activate), Some([1280, 720]));
    assert_eq!(modes.step(full, [1920, 1080], Previous), None);
    // Between entries: previous takes the one below, next the one above.
    assert_eq!(
        modes.step(full, [1600, 900], AdjustPrevious),
        Some([1280, 720])
    );
    assert_eq!(
        modes.step(full, [1600, 900], AdjustNext),
        Some([1920, 1080])
    );
    assert_eq!(modes.step(full, [1600, 900], Activate), Some([1920, 1080]));
    // Above every entry, next clamps to the last and activate wraps.
    assert_eq!(
        modes.step(full, [2560, 1440], AdjustNext),
        Some([1920, 1080])
    );
    assert_eq!(modes.step(full, [2560, 1440], Activate), Some([1280, 720]));
    // Tier 0 sits before tier 1, as in 00462F40.
    let window = DisplayMode::Window;
    assert_eq!(modes.step(window, [320, 240], AdjustPrevious), None);
    assert_eq!(modes.step(window, [320, 240], AdjustNext), Some([512, 384]));
    // Borderless has only the desktop, so nothing steps.
    for command in [AdjustPrevious, AdjustNext, Activate] {
        assert_eq!(
            modes.step(DisplayMode::Borderless, [1920, 1080], command),
            None
        );
    }
    assert_eq!(
        MonitorModes::default().step(full, [800, 600], AdjustNext),
        None
    );
}

#[test]
fn only_sizes_without_authored_strings_get_port_text() {
    let mut selection = DisplaySelection {
        mode: DisplayMode::Window,
        size: [640, 480],
    };
    assert_eq!(selection.resolution_text(None), None);
    assert_eq!(selection.mode_text(0), None);
    selection.size = [1920, 1080];
    assert_eq!(selection.resolution_text(None).unwrap(), b"1920x1080");
    selection.mode = DisplayMode::FullScreen;
    assert_eq!(selection.mode_text(0), None);
    // Borderless shows the desktop it covers.
    selection.mode = DisplayMode::Borderless;
    selection.size = [800, 600];
    assert_eq!(
        selection.resolution_text(Some([3840, 2160])).unwrap(),
        b"3840x2160"
    );
    assert_eq!(selection.resolution_text(Some([1024, 768])), None);
    assert_eq!(selection.mode_text(0).unwrap(), b"Borderless");
    assert_eq!(selection.mode_text(2).unwrap(), b"Randlos");
}

#[test]
fn borderless_draws_the_desktop_tier_and_keeps_the_saved_size() {
    let mut selection = DisplaySelection {
        mode: DisplayMode::Window,
        size: [800, 600],
    };
    let desktop = Some([3840, 2160]);
    selection.adopt(DisplayMode::Borderless, [3840, 2160]);
    assert_eq!(selection.size, [800, 600]);
    assert_eq!(selection.resolution(desktop), [3840, 2160]);
    assert_eq!(selection.tier(desktop), 4);
    // Until the desktop is known, the saved size stands in.
    assert_eq!(selection.tier(None), 3);
    selection.adopt(DisplayMode::Window, [800, 600]);
    assert_eq!(selection.size, [800, 600]);
    assert_eq!(selection.tier(desktop), 3);
}

#[test]
fn startup_order_descends_each_mode_like_00438ad0() {
    let modes = MonitorModes::new(Some([1280, 720]), [[640, 480], [1280, 720]]);
    let order: Vec<_> = modes
        .startup_order(DisplaySelection {
            mode: DisplayMode::FullScreen,
            size: [1280, 720],
        })
        .into_iter()
        .map(|selection| (selection.mode, selection.size))
        .collect();
    use DisplayMode::*;
    assert_eq!(
        order,
        [
            (FullScreen, [1280, 720]),
            (FullScreen, [640, 480]),
            // The desktop, keeping the saved size.
            (Borderless, [1280, 720]),
            (Window, [1280, 720]),
            (Window, [800, 600]),
            (Window, [640, 480]),
            (Window, [512, 384]),
            (Window, [320, 240]),
        ]
    );
    // A saved size the monitor lacks starts below it.
    let first = modes.startup_order(DisplaySelection {
        mode: FullScreen,
        size: [1024, 768],
    })[0];
    assert_eq!((first.mode, first.size), (FullScreen, [640, 480]));
    // Borderless tries the desktop once, keeping the saved size.
    let order = modes.startup_order(DisplaySelection {
        mode: Borderless,
        size: [800, 600],
    });
    assert_eq!((order[0].mode, order[0].size), (Borderless, [800, 600]));
    assert_eq!(order[0].tier(modes.desktop), 3);
    assert_eq!(order[1].mode, Window);
    // No known desktop, no Borderless candidate.
    assert!(MonitorModes::default()
        .startup_order(order[0])
        .iter()
        .all(|selection| selection.mode != Borderless));
}
