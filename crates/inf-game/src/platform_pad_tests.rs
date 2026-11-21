//! The pad bookkeeping without SDL: the pads a poll lists, which of them the
//! native stream reads (the pad in use), what a detach releases, and the
//! connect and removal notices.

use super::*;

/// A pad as a poll sees it: its device instance id, name and new activity.
fn seen<'a>(id: u32, name: &'a str, activity: bool) -> PadSeen<'a> {
    PadSeen { id, name, activity }
}

/// One poll of a type-2 joystick stream with five axes, the shape a startup
/// joystick latches.
fn poll(book: &mut PadBook, pads: &[PadSeen<'_>]) -> PadPoll {
    book.poll(pads, 2, 5)
}

/// The user's case: a second pad takes the stream when it is pressed, and
/// unplugging the idle first pad then releases nothing and fails nothing.
#[test]
fn the_pad_in_use_survives_an_idle_pads_departure() {
    let mut book = PadBook::default();
    // Startup: A is first, so it is in use; B beside it is idle.
    assert_eq!(
        poll(
            &mut book,
            &[seen(1, "Pad A", false), seen(2, "Pad B", false)]
        ),
        PadPoll {
            active: Some(1),
            released: None,
            failed: false,
        }
    );
    assert!(book.take_changes().is_empty());

    // B's press takes the stream from A that same poll.
    assert_eq!(
        poll(
            &mut book,
            &[seen(1, "Pad A", false), seen(2, "Pad B", true)]
        ),
        PadPoll {
            active: Some(2),
            released: None,
            failed: false,
        }
    );
    assert!(book.take_changes().is_empty());

    // A goes away while idle: its removal is the only trace, and B reads on.
    assert_eq!(
        poll(&mut book, &[seen(2, "Pad B", false)]),
        PadPoll {
            active: Some(2),
            released: None,
            failed: false,
        }
    );
    assert_eq!(book.take_changes(), [PadChange::Removed("Pad A".into())]);
}

/// Losing the pad in use releases it once and then fails, even though the
/// idle pad beside it is still attached; using that pad ends the failures.
#[test]
fn losing_the_pad_in_use_fails_until_another_pad_is_used() {
    let mut book = PadBook::default();
    poll(
        &mut book,
        &[seen(1, "Pad A", false), seen(2, "Pad B", false)],
    );
    assert!(book.take_changes().is_empty());

    // A goes: its fully released reading comes first, then the failed read.
    assert_eq!(
        poll(&mut book, &[seen(2, "Pad B", false)]),
        PadPoll {
            active: None,
            released: Some(PadSample::Read {
                buttons: 0,
                axes: [0x8000, 0x8000, 0x8000, 0x8000, 0x8000, 0],
            }),
            failed: true,
        }
    );
    assert_eq!(book.take_changes(), [PadChange::Removed("Pad A".into())]);

    // B is attached but idle: the stream keeps failing.
    assert_eq!(
        poll(&mut book, &[seen(2, "Pad B", false)]),
        PadPoll {
            active: None,
            released: None,
            failed: true,
        }
    );

    // B is pressed: it takes the stream and is read that same poll.
    assert_eq!(
        poll(&mut book, &[seen(2, "Pad B", true)]),
        PadPoll {
            active: Some(2),
            released: None,
            failed: false,
        }
    );
}

/// SDL may list a stale pad first. It never moves, so the pad the player
/// actually uses takes the stream on its first new activity.
#[test]
fn a_stale_first_listed_pad_gives_way_to_the_pad_that_moves() {
    let mut book = PadBook::default();
    assert_eq!(
        poll(&mut book, &[seen(1, "Stale", false), seen(2, "Pad", false)]).active,
        Some(1)
    );
    assert_eq!(
        poll(&mut book, &[seen(1, "Stale", false), seen(2, "Pad", true)]),
        PadPoll {
            active: Some(2),
            released: None,
            failed: false,
        }
    );
    // The stale pad's later readings leave the choice alone.
    assert_eq!(
        poll(&mut book, &[seen(1, "Stale", false), seen(2, "Pad", false)]).active,
        Some(2)
    );
}

/// The activity test is the last-input rule: a button newly down, or an axis
/// newly past a quarter of its range from the layout's centre. A held press
/// or a resting stick is not new, and a first reading is a baseline.
#[test]
fn new_activity_is_a_new_button_or_a_new_axis_deflection() {
    let rest = PadSample::Read {
        buttons: 0,
        axes: [0x8000, 0x8000, 0x8000, 0x8000, 0x8000, 0],
    };
    let pressed = PadSample::Read {
        buttons: 1,
        axes: [0x8000, 0x8000, 0x8000, 0x8000, 0x8000, 0],
    };
    assert!(newly_used(Some(rest), pressed, 2, 5));
    assert!(!newly_used(Some(pressed), pressed, 2, 5));
    assert!(!newly_used(None, pressed, 2, 5));

    // Past a quarter of the word range, one step beyond 0x4000.
    let deflected = PadSample::Read {
        buttons: 0,
        axes: [0x8000 + 0x4001, 0x8000, 0x8000, 0x8000, 0x8000, 0],
    };
    assert!(newly_used(Some(rest), deflected, 2, 5));
    // At the quarter itself the stick is still inside the dead zone.
    let at_edge = PadSample::Read {
        buttons: 0,
        axes: [0x8000 + 0x4000, 0x8000, 0x8000, 0x8000, 0x8000, 0],
    };
    assert!(!newly_used(Some(rest), at_edge, 2, 5));

    // The PlayStation layout's axes are bytes around 0x80.
    let byte_rest = PadSample::Read {
        buttons: 0,
        axes: [0x80, 0x80, 0x80, 0x80, 0, 0],
    };
    let byte_moved = PadSample::Read {
        buttons: 0,
        axes: [0xc1, 0x80, 0x80, 0x80, 0, 0],
    };
    assert!(newly_used(Some(byte_rest), byte_moved, 9, 4));
}

/// A lone pad unplugged and plugged back in takes the stream straight away,
/// as the startup pad did, without needing a press.
#[test]
fn a_lone_pad_reconnected_is_read_again_straight_away() {
    let mut book = PadBook::default();
    poll(&mut book, &[seen(1, "Pad", false)]);
    let gone = poll(&mut book, &[]);
    assert_eq!(gone.active, None);
    assert!(gone.released.is_some());
    assert!(gone.failed);

    // SDL gives a reconnected device a new instance id: it is a new pad.
    assert_eq!(
        poll(&mut book, &[seen(2, "Pad", false)]),
        PadPoll {
            active: Some(2),
            released: None,
            failed: false,
        }
    );
    assert_eq!(
        book.take_changes(),
        [
            PadChange::Removed("Pad".into()),
            PadChange::Connected("Pad".into()),
        ]
    );
}

/// The pads present at the first poll are silent; every later join and every
/// departure is a change for the notice.
#[test]
fn startup_pads_are_silent_and_later_ones_connect_and_remove() {
    let mut book = PadBook::default();
    poll(
        &mut book,
        &[seen(1, "Pad A", false), seen(2, "Pad B", false)],
    );
    assert!(book.take_changes().is_empty());

    poll(
        &mut book,
        &[seen(1, "Pad A", true), seen(3, "Pad C", false)],
    );
    assert_eq!(
        book.take_changes(),
        [
            PadChange::Removed("Pad B".into()),
            PadChange::Connected("Pad C".into()),
        ]
    );
}

/// A pad plugged in beside the pad in use stays idle until it is used, even
/// though its connection could take an idle stream at once.
#[test]
fn a_pad_plugged_in_beside_the_pad_in_use_stays_idle_until_used() {
    let mut book = PadBook::default();
    poll(&mut book, &[seen(1, "Pad A", false)]);

    assert_eq!(
        poll(
            &mut book,
            &[seen(1, "Pad A", false), seen(2, "Pad B", false)]
        ),
        PadPoll {
            active: Some(1),
            released: None,
            failed: false,
        }
    );
    assert_eq!(book.take_changes(), [PadChange::Connected("Pad B".into())]);

    // Repeated idle polls keep A in use.
    assert_eq!(
        poll(
            &mut book,
            &[seen(1, "Pad A", false), seen(2, "Pad B", false)]
        )
        .active,
        Some(1)
    );

    // B is pressed: it takes the stream.
    assert_eq!(
        poll(
            &mut book,
            &[seen(1, "Pad A", false), seen(2, "Pad B", true)]
        )
        .active,
        Some(2)
    );
}

/// The PlayStation pads' axes are bytes, so their release centres on 0x80;
/// retail's joystick centres its words on 0x8000.
#[test]
fn the_playstation_pads_release_to_their_byte_centre() {
    assert_eq!(
        released_sample(8, 6),
        PadSample::Read {
            buttons: 0,
            axes: [0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
        }
    );
    assert_eq!(
        released_sample(9, 4),
        PadSample::Read {
            buttons: 0,
            axes: [0x80, 0x80, 0x80, 0x80, 0, 0],
        }
    );
    assert_eq!(
        released_sample(2, 5),
        PadSample::Read {
            buttons: 0,
            axes: [0x8000, 0x8000, 0x8000, 0x8000, 0x8000, 0],
        }
    );
}

/// A controller reading for the sample tests: the buttons held and the six
/// SDL axes' signed positions. A real pad's sticks rest at 0 and its
/// triggers at `-0x8000`.
struct FakeController {
    held: Vec<Button>,
    axes: [i16; 6],
}

impl Default for FakeController {
    fn default() -> Self {
        Self {
            held: Vec::new(),
            axes: [0, 0, 0, 0, -0x8000, -0x8000],
        }
    }
}

impl ControllerRead for FakeController {
    fn button(&self, button: Button) -> bool {
        self.held.contains(&button)
    }

    fn axis(&self, axis: Axis) -> i16 {
        match axis {
            Axis::LeftX => self.axes[0],
            Axis::LeftY => self.axes[1],
            Axis::RightX => self.axes[2],
            Axis::RightY => self.axes[3],
            Axis::TriggerLeft => self.axes[4],
            Axis::TriggerRight => self.axes[5],
        }
    }
}

impl FakeController {
    /// The six axes of a sample.
    fn sampled_axes(sample: PadSample) -> [u32; 6] {
        match sample {
            PadSample::Read { axes, .. } => axes,
            PadSample::Failed => panic!("a controller always reads"),
        }
    }
}

/// Every PlayStation code is one mapped SDL button, and the Remastered pad
/// reports the same sixteen codes as the Console pad. The codes are the
/// agreed Remastered bindings: View 1, L3 2, R3 3, Start 4, d-pad 5 to 8,
/// LB 11, RB 12, Y 13, B 14, A 15, X 16 (the triggers give 9 and 10).
#[test]
fn every_mapped_button_sets_its_playstation_code() {
    assert_eq!(
        CONSOLE_BUTTONS,
        [
            (0, Button::Back),
            (1, Button::LeftStick),
            (2, Button::RightStick),
            (3, Button::Start),
            (4, Button::DPadUp),
            (5, Button::DPadRight),
            (6, Button::DPadDown),
            (7, Button::DPadLeft),
            (10, Button::LeftShoulder),
            (11, Button::RightShoulder),
            (12, Button::Y),
            (13, Button::B),
            (14, Button::A),
            (15, Button::X),
        ]
    );
    for &(bit, button) in &CONSOLE_BUTTONS {
        let controller = FakeController {
            held: vec![button],
            ..FakeController::default()
        };
        let expected = PadSample::Read {
            buttons: 1 << bit,
            axes: [0x80; 6],
        };
        assert_eq!(
            remastered_sample(&controller),
            expected,
            "{button:?} is code {}",
            bit + 1
        );
        assert_eq!(
            console_sample(&controller),
            PadSample::Read {
                buttons: 1 << bit,
                axes: [0x80, 0x80, 0x80, 0x80, 0, 0],
            },
            "{button:?} is code {}",
            bit + 1
        );
    }
}

/// A trigger past half is the PlayStation's digital L2 or R2 on both
/// PlayStation layouts, and just below half is nothing.
#[test]
fn a_trigger_past_half_is_l2_or_r2() {
    let with_triggers = |left, right| FakeController {
        axes: [0, 0, 0, 0, left, right],
        ..FakeController::default()
    };
    for (left, right, buttons) in [
        (TRIGGER_PRESSED - 1, -0x8000, 0),
        (-0x8000, TRIGGER_PRESSED - 1, 0),
        (TRIGGER_PRESSED, -0x8000, 1 << 8),
        (-0x8000, TRIGGER_PRESSED, 1 << 9),
        (TRIGGER_PRESSED, TRIGGER_PRESSED, (1 << 8) | (1 << 9)),
    ] {
        let controller = with_triggers(left, right);
        for sample in [remastered_sample(&controller), console_sample(&controller)] {
            match sample {
                PadSample::Read { buttons: read, .. } => assert_eq!(read, buttons),
                PadSample::Failed => panic!("a controller always reads"),
            }
        }
    }
}

/// The Remastered thrust axis: the right trigger raises it (forward), the
/// left lowers it (reverse), both together cancel, and released it rests at
/// 0x80.
#[test]
fn the_right_trigger_is_forward_thrust_and_the_left_reverse() {
    let thrust = |left, right| {
        let controller = FakeController {
            axes: [0, 0, 0, 0, left, right],
            ..FakeController::default()
        };
        FakeController::sampled_axes(remastered_sample(&controller))[4]
    };
    // SDL's triggers rest at 0 and reach 0x7FFF fully pressed.
    assert_eq!(thrust(0, 0), 0x80, "neither trigger");
    assert_eq!(thrust(0, 0x7fff), 0xff, "right at full");
    assert_eq!(thrust(0x7fff, 0), 0x01, "left at full");
    assert_eq!(thrust(0x7fff, 0x7fff), 0x80, "both at full");
    assert_eq!(thrust(0, 0x4000), 0xbf, "right at half");
}

/// The Remastered pad's first four axes are the two sticks' bytes, axes 2
/// and 3 included for a later camera, and its sixth axis stays at centre.
#[test]
fn the_remastered_sticks_are_the_playstation_byte_axes() {
    let controller = FakeController {
        axes: [-0x8000, 0x7fff, 0x7fff, -0x8000, -0x8000, -0x8000],
        ..FakeController::default()
    };
    assert_eq!(
        remastered_sample(&controller),
        PadSample::Read {
            buttons: 0,
            axes: [0x00, 0xff, 0xff, 0x00, 0x80, 0x80],
        }
    );
}

/// The PC layout is retail's joystick exactly: buttons 1 to 10, X and Y, the
/// triggers summed on Z (the left raising it), the right stick on R and U,
/// and V unused.
#[test]
fn the_pc_pad_sample_is_retails_joystick() {
    let controller = FakeController {
        held: XINPUT_BUTTONS.to_vec(),
        axes: [0x1000, -0x2000, 0x3000, -0x4000, 0x2000, -0x1000],
    };
    assert_eq!(
        xinput_sample(&controller),
        PadSample::Read {
            buttons: 0x3ff,
            axes: [0x9000, 0x6000, 0xb000, 0x4000, 0xb000, 0],
        }
    );
}

/// The Console layout's sample is the PlayStation pad's four byte axes and
/// its codes, with the last two axes unused.
#[test]
fn the_console_pad_sample_is_the_playstation_four_axes() {
    let controller = FakeController {
        held: vec![Button::A, Button::LeftShoulder],
        axes: [-0x8000, 0x7fff, 0, -1, -0x8000, -0x8000],
    };
    assert_eq!(
        console_sample(&controller),
        PadSample::Read {
            buttons: (1 << 14) | (1 << 10),
            axes: [0x00, 0xff, 0x80, 0x7f, 0, 0],
        }
    );
}

/// Each layout's native type and axis count, and the unmapped joystick that
/// stays retail's type 2 under every layout.
#[test]
fn each_layout_reports_its_native_type_and_axis_count() {
    assert_eq!(ControllerLayout::Pc.native_type(), 2);
    assert_eq!(ControllerLayout::Console.native_type(), 9);
    assert_eq!(ControllerLayout::Remastered.native_type(), 8);
    assert_eq!(controller_axes(2), 5);
    assert_eq!(controller_axes(9), 4);
    assert_eq!(controller_axes(8), 6);
    for layout in ControllerLayout::ALL {
        assert_eq!(pad_native_type(true, layout), layout.native_type());
        assert_eq!(pad_native_type(false, layout), 2);
    }
}
