use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("inf-disc-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A CloneCD sheet of `tracks` (number, control, PLBA) and a lead-out.
fn ccd(tracks: &[(u8, u8, u64)], lead_out: u64) -> String {
    let mut text = "[CloneCD]\nVersion=3\n\n[Disc]\nTocEntries=4\nSessions=1\n".to_string();
    let mut entries = vec![(0xa2u8, 0u8, lead_out)];
    entries.extend_from_slice(tracks);
    for (index, (point, control, plba)) in entries.iter().enumerate() {
        text.push_str(&format!(
            "\n[Entry {index}]\nSession=1\nPoint=0x{point:02x}\nADR=0x01\nControl=0x{control:02x}\n\
             ALBA=-150\nPLBA={plba}\n"
        ));
    }
    text
}

/// Byte `i` of a fake image: a pattern that shows misplaced sectors.
fn pattern(i: u64) -> u8 {
    (i / RAW_SECTOR * 7 + i % 251) as u8
}

fn image(path: &Path, sectors: u64) {
    let bytes: Vec<u8> = (0..sectors * RAW_SECTOR).map(pattern).collect();
    fs::write(path, bytes).unwrap();
}

const TRACK2: u64 = TRACK_SECTORS[0] as u64;

#[test]
fn a_ccd_track_runs_from_its_index_1_to_the_next_and_the_last_to_the_lead_out() {
    let disc = parse_ccd(
        &ccd(&[(1, 4, 0), (2, 0, 100), (3, 0, 250)], 400),
        PathBuf::from("x.img"),
    )
    .unwrap();
    let ranges: Vec<_> = disc
        .tracks
        .iter()
        .map(|t| (t.number, t.audio, t.segments.clone().unwrap()[0].clone()))
        .collect();
    let segment = |start, end| Segment {
        file: 0,
        start,
        end,
    };
    assert_eq!(
        ranges,
        [
            (1, false, segment(0, 100)),
            (2, true, segment(100, 250)),
            (3, true, segment(250, 400)),
        ]
    );
}

#[test]
fn extract_music_copies_the_disc_track_into_a_wav_and_names_the_rest() {
    let dir = scratch("extract");
    let disc_dir = dir.join("disc");
    let root = dir.join("game");
    fs::create_dir_all(&disc_dir).unwrap();
    let start = 10;
    let lead_out = start + TRACK2;
    // Track 3 is data here, so it is no music.
    fs::write(
        disc_dir.join("Game.ccd"),
        ccd(&[(1, 4, 0), (2, 0, start), (3, 4, lead_out - 1)], lead_out),
    )
    .unwrap();
    image(&disc_dir.join("game.IMG"), lead_out);
    // Selecting the image finds its sheet, in any case.
    let disc = DiscImage::open(&disc_dir.join("game.IMG")).unwrap();
    // Track 2 ends at track 3's start, one sector short of the retail length.
    assert!(disc
        .music(2)
        .unwrap_err()
        .contains("is this the Infestation CD?"));

    fs::write(
        disc_dir.join("Game.ccd"),
        ccd(&[(1, 4, 0), (2, 0, start)], lead_out),
    )
    .unwrap();
    let disc = DiscImage::open(&disc_dir.join("Game.ccd")).unwrap();
    let mut told = Vec::new();
    let result = disc.extract_music(&root, |track| told.push(track)).unwrap();
    assert_eq!(told, [2]);
    assert_eq!(result.written, [2]);
    assert_eq!(result.unavailable.len(), 7);
    assert!(result
        .unavailable
        .iter()
        .all(|(_, why)| why == "the image has no such track"));
    let wav = root.join("cdaudio/track02.wav");
    assert_eq!(
        crate::platform_audio::cd_music::music_track(&root.join("cdaudio"), 2),
        Ok(Some(wav.clone()))
    );
    let bytes = fs::read(&wav).unwrap();
    assert_eq!(bytes[..44], wav_header((TRACK2 * RAW_SECTOR) as u32)[..]);
    let first = start * RAW_SECTOR;
    assert!(bytes[44..]
        .iter()
        .enumerate()
        .all(|(i, &byte)| byte == pattern(first + i as u64)));
    assert!(!root.join("cdaudio/track02.wav.part").exists());
    // The summary names what happened to every track.
    let summary = result.summary();
    assert!(summary.contains("Extracted tracks 02 as WAV."), "{summary}");
    assert!(summary.contains("Track 09 unavailable: the image has no such track"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_truncated_image_gives_no_music_and_writes_nothing() {
    let dir = scratch("truncated");
    let lead_out = 10 + TRACK2;
    fs::write(dir.join("d.ccd"), ccd(&[(1, 4, 0), (2, 0, 10)], lead_out)).unwrap();
    image(&dir.join("d.img"), 20);
    let disc = DiscImage::open(&dir.join("d.ccd")).unwrap();
    assert!(disc.music(2).unwrap_err().contains("truncated"));
    let root = dir.join("game");
    let error = disc.extract_music(&root, |_| {}).unwrap_err();
    assert!(
        error.starts_with("This image holds none of Infestation's music."),
        "{error}"
    );
    assert!(!root.join("cdaudio").exists());
    // No image at all: every track says so.
    fs::remove_file(dir.join("d.img")).unwrap();
    let disc = DiscImage::open(&dir.join("d.ccd")).unwrap();
    assert!(disc.music(2).unwrap_err().ends_with("cannot be read"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn malformed_ccd_sheets_are_refused() {
    let good = ccd(&[(1, 4, 0), (2, 0, 100)], 400);
    let cases = [
        ("no CloneCD section", good.replace("[CloneCD]", "[Other]")),
        ("bad number", good.replace("PLBA=100", "PLBA=1x0")),
        ("no PLBA", good.replace("PLBA=100\n", "")),
        ("negative start", good.replace("PLBA=100", "PLBA=-5")),
        ("garbage line", format!("{good}\nnonsense\n")),
        ("no tracks", ccd(&[], 400)),
        (
            "duplicate track",
            ccd(&[(1, 4, 0), (2, 0, 100), (2, 0, 200)], 400),
        ),
    ];
    for (name, text) in cases {
        assert!(parse_ccd(&text, PathBuf::from("x.img")).is_err(), "{name}");
    }
    // Problems confined to one track leave the others readable.
    let disc = parse_ccd(
        &ccd(&[(1, 4, 0), (2, 0, 300), (3, 0, 200)], 400),
        PathBuf::from("x.img"),
    )
    .unwrap();
    assert!(disc.tracks[1].segments.is_err());
    let no_lead_out = good.replace("Point=0xa2", "Point=0xa0");
    let disc = parse_ccd(&no_lead_out, PathBuf::from("x.img")).unwrap();
    assert_eq!(
        disc.tracks[1].segments,
        Err("its session has no lead-out".to_string())
    );
}

#[test]
fn open_wants_the_sheet_beside_an_image_and_refuses_other_files() {
    let dir = scratch("open");
    image(&dir.join("lone.img"), 1);
    let error = DiscImage::open(&dir.join("lone.img")).unwrap_err();
    assert!(error.contains("holds the track boundaries"), "{error}");
    let error = DiscImage::open(&dir.join("disc.nrg")).unwrap_err();
    assert!(error.contains(".ccd"), "{error}");
    let error = DiscImage::open(&dir.join("missing.iso")).unwrap_err();
    assert!(error.starts_with("Cannot open"), "{error}");
    fs::write(
        dir.join("huge.ccd"),
        vec![b' '; MAX_SHEET_BYTES as usize + 1],
    )
    .unwrap();
    assert!(DiscImage::open(&dir.join("huge.ccd"))
        .unwrap_err()
        .contains("too large"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_cue_track_ends_at_the_next_index_1_across_files() {
    let dir = scratch("cue");
    image(&dir.join("data.bin"), 50);
    image(&dir.join("Track 02.bin"), 300);
    image(&dir.join("track03.bin"), 200);
    let sheet = dir.join("data.cue");
    fs::write(
        &sheet,
        "REM ripped\nFILE \"data.bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n\
         FILE \"Track 02.bin\" BINARY\n  TRACK 02 AUDIO\n    FLAGS DCP\n    INDEX 00 00:00:00\n\
         \x20   INDEX 01 00:02:00\nFILE TRACK03.BIN BINARY\n  TRACK 03 AUDIO\n    INDEX 00 00:00:00\n\
         \x20   INDEX 01 00:00:40\n",
    )
    .unwrap();
    let disc = DiscImage::open(&sheet).unwrap();
    let segments: Vec<_> = disc
        .tracks
        .iter()
        .map(|t| t.segments.clone().unwrap())
        .collect();
    let segment = |file, start, end| Segment { file, start, end };
    // Each track takes the next one's pregap, the data track too.
    assert_eq!(segments[0], [segment(0, 0, 50), segment(1, 0, 150)]);
    // Track 2's file from INDEX 01, then track 3's pregap.
    assert_eq!(segments[1], [segment(1, 150, 300), segment(2, 0, 40)]);
    assert_eq!(segments[2], [segment(2, 40, 200)]);
    assert!(!disc.tracks[0].audio && disc.tracks[1].audio);
    // A BIN finds the CUE of its own name, and only that.
    assert!(DiscImage::open(&dir.join("data.bin")).is_ok());
    let error = DiscImage::open(&dir.join("track03.bin")).unwrap_err();
    assert!(error.contains("holds the track boundaries"), "{error}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn malformed_cue_sheets_are_refused() {
    let dir = scratch("bad-cue");
    image(&dir.join("d.bin"), 10);
    let sheet = |body: &str| format!("FILE \"d.bin\" BINARY\n{body}");
    let cases = [
        (
            "escape",
            "FILE \"../d.bin\" BINARY\n  TRACK 01 AUDIO\n".to_string(),
        ),
        (
            "drive",
            "FILE \"C:d.bin\" BINARY\n  TRACK 01 AUDIO\n".to_string(),
        ),
        (
            "wave",
            "FILE \"d.wav\" WAVE\n  TRACK 01 AUDIO\n".to_string(),
        ),
        ("track first", "TRACK 01 AUDIO\n".to_string()),
        ("unknown", sheet("  TRACK 01 AUDIO\n  JUNK 1\n")),
        ("bad time", sheet("  TRACK 01 AUDIO\n  INDEX 01 00:60:00\n")),
        ("order", sheet("  TRACK 02 AUDIO\n  TRACK 01 AUDIO\n")),
        ("mode", sheet("  TRACK 01 MODE1/2048\n")),
        (
            "two index 1",
            sheet("  TRACK 01 AUDIO\n  INDEX 01 00:00:00\n  INDEX 01 00:00:01\n"),
        ),
        ("no tracks", sheet("")),
    ];
    for (name, text) in cases {
        let path = dir.join("d.cue");
        fs::write(&path, text).unwrap();
        assert!(DiscImage::open(&path).is_err(), "{name}");
    }
    // A track without INDEX 01, or past its file, is unreadable alone.
    fs::write(
        dir.join("d.cue"),
        sheet("  TRACK 01 AUDIO\n  TRACK 02 AUDIO\n  INDEX 01 00:00:20\n"),
    )
    .unwrap();
    let disc = DiscImage::open(&dir.join("d.cue")).unwrap();
    assert_eq!(
        disc.tracks[0].segments,
        Err("it has no INDEX 01".to_string())
    );
    assert!(disc.tracks[1].segments.is_err());
    fs::remove_dir_all(dir).unwrap();
}

/// The retail disc as a CloneCD image, when one is at `INF_DISC_IMAGE` or
/// `retail/disc/`; skipped otherwise.
fn retail_disc() -> Option<PathBuf> {
    let path = std::env::var_os("INF_DISC_IMAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/disc/Infestation.ccd")
        });
    if path.is_file() {
        Some(path)
    } else {
        eprintln!("skipped: no disc image at {}", path.display());
        None
    }
}

#[test]
fn the_retail_disc_gives_every_music_track_matching_the_installed_ogg() {
    let Some(path) = retail_disc() else {
        return;
    };
    let disc = DiscImage::open(&path).unwrap();
    for number in MUSIC_TRACKS {
        disc.music(number).unwrap();
    }
    let dir = scratch("retail");
    let wav = dir.join("track02.wav");
    disc.write_wav(disc.music(2).unwrap(), &wav).unwrap();
    let pcm = crate::platform_audio::decode_track(&wav, 2).unwrap();
    let ogg = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/cdaudio/track02.ogg");
    let reference = crate::platform_audio::decode_track(&ogg, 2).unwrap();
    // The OGG is lossy: the same music, not the same samples. Byte-swapped
    // or shifted audio would differ by about as much as the signal itself.
    let power = |samples: &mut dyn Iterator<Item = f64>| samples.map(|s| s * s).sum::<f64>();
    let signal = power(&mut reference.iter().map(|&s| f64::from(s)));
    let error = power(
        &mut pcm
            .iter()
            .zip(reference.iter())
            .map(|(&a, &b)| f64::from(a) - f64::from(b)),
    );
    assert!(error < signal / 100.0, "error {error}, signal {signal}");
    fs::remove_dir_all(dir).unwrap();
}
