//! The pad bookkeeping without SDL: who is open, what a detach releases, and
//! which pad a poll takes next.

use super::*;

/// The startup poll and the first pad: no connect is reported for it, and a
/// second pad beside it changes nothing while it is open.
#[test]
fn a_startup_pad_connects_silently_and_a_second_changes_nothing() {
    let mut book = PadBook::default();
    let first = book.poll(false, None, 2, 5, &[0]);
    assert_eq!(
        first,
        PadPoll::Replace {
            released: None,
            index: Some(0),
            startup: true,
        }
    );
    book.opened("Pad A".into(), true);
    assert_eq!(book.take_changes(), Vec::new());

    // The open pad answers, so the list's second pad is left alone.
    let second = book.poll(true, Some("Pad A"), 2, 5, &[0, 1]);
    assert_eq!(second, PadPoll::Keep);
    assert!(book.take_changes().is_empty());
}

/// Retail's failed read keeps the axes (`004F548C`), so the detach has to
/// release the buttons and centre every axis before it.
#[test]
fn an_unplugged_pad_releases_every_button_and_axis() {
    let mut book = PadBook::default();
    book.poll(false, None, 2, 5, &[0]);
    book.opened("Pad A".into(), true);

    let detached = book.poll(false, Some("Pad A"), 2, 5, &[]);
    assert_eq!(
        detached,
        PadPoll::Replace {
            released: Some(PadSample::Read {
                buttons: 0,
                axes: [0x8000, 0x8000, 0x8000, 0x8000, 0x8000, 0],
            }),
            index: None,
            startup: false,
        }
    );
    assert_eq!(book.take_changes(), [PadChange::Removed("Pad A".into())]);
}

/// The same pad can come back, or the one beside it takes over; either is a
/// connect, and the re-opened pad is read again.
#[test]
fn a_pad_that_comes_back_connects_and_is_read_again() {
    let mut book = PadBook::default();
    book.poll(false, None, 2, 5, &[]);
    assert_eq!(
        book.poll(false, None, 2, 5, &[0]),
        PadPoll::Replace {
            released: None,
            index: Some(0),
            startup: false,
        }
    );
    book.opened("Pad A".into(), false);
    assert_eq!(book.take_changes(), [PadChange::Connected("Pad A".into())]);
    assert_eq!(book.poll(true, Some("Pad A"), 2, 5, &[0]), PadPoll::Keep);

    // Gone with nothing attached, then back at SDL's first device index.
    assert!(matches!(
        book.poll(false, Some("Pad A"), 2, 5, &[]),
        PadPoll::Replace { index: None, .. }
    ));
    assert_eq!(
        book.poll(false, None, 2, 5, &[0]),
        PadPoll::Replace {
            released: None,
            index: Some(0),
            startup: false,
        }
    );
    book.opened("Pad A".into(), false);
    assert_eq!(
        book.take_changes(),
        [
            PadChange::Removed("Pad A".into()),
            PadChange::Connected("Pad A".into()),
        ]
    );
    assert_eq!(book.poll(true, Some("Pad A"), 2, 5, &[0]), PadPoll::Keep);
}

/// Removing the open pad while another is attached switches to the second:
/// SDL renumbers it to the first device index, so the first listed pad is
/// the one that remains.
#[test]
fn the_second_pad_takes_over_when_the_first_leaves() {
    let mut book = PadBook::default();
    book.poll(false, None, 2, 5, &[0]);
    book.opened("Pad A".into(), true);
    assert_eq!(book.poll(true, Some("Pad A"), 2, 5, &[0, 1]), PadPoll::Keep);

    let switched = book.poll(false, Some("Pad A"), 2, 5, &[0]);
    assert_eq!(
        switched,
        PadPoll::Replace {
            released: Some(PadSample::Read {
                buttons: 0,
                axes: [0x8000, 0x8000, 0x8000, 0x8000, 0x8000, 0],
            }),
            index: Some(0),
            startup: false,
        }
    );
    book.opened("Pad B".into(), false);
    assert_eq!(
        book.take_changes(),
        [
            PadChange::Removed("Pad A".into()),
            PadChange::Connected("Pad B".into()),
        ]
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
