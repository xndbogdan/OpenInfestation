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

/// The PlayStation pad's axes are bytes, so its release centres on 0x80.
#[test]
fn the_console_pad_releases_to_its_byte_centre() {
    assert_eq!(
        released_sample(9, 4),
        PadSample::Read {
            buttons: 0,
            axes: [0x80, 0x80, 0x80, 0x80, 0, 0],
        }
    );
}
