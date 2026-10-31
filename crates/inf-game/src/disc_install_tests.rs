use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "inf-install-{name}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn both(value: u32) -> [u8; 8] {
    let mut field = [0; 8];
    field[..4].copy_from_slice(&value.to_le_bytes());
    field[4..].copy_from_slice(&value.to_be_bytes());
    field
}

/// An ISO9660 directory record.
fn record(name: &[u8], extent: u32, bytes: u32, directory: bool) -> Vec<u8> {
    let size = 33 + name.len() + (name.len() + 1) % 2;
    let mut record = vec![0; size];
    record[0] = size as u8;
    record[2..10].copy_from_slice(&both(extent));
    record[10..18].copy_from_slice(&both(bytes));
    record[25] = if directory { 2 } else { 0 };
    record[32] = name.len() as u8;
    record[33..33 + name.len()].copy_from_slice(name);
    record
}

const SECTOR: usize = DATA_SECTOR as usize;

/// A volume: root files, `OVERLAY/` files and a `DIRECTX/` folder that is
/// never entered. Sectors: 16 descriptor, 17 terminator, 18 root, 19
/// OVERLAY, 20 DIRECTX, then one or more per file.
fn iso(root_files: &[(&str, &[u8])], overlays: &[(&str, &[u8])]) -> Vec<u8> {
    let mut next = 21u32;
    let mut place = |files: &[(&str, &[u8])]| -> Vec<(String, u32, Vec<u8>)> {
        files
            .iter()
            .map(|(name, bytes)| {
                let extent = next;
                next += (bytes.len().div_ceil(SECTOR)).max(1) as u32;
                (format!("{name};1"), extent, bytes.to_vec())
            })
            .collect()
    };
    let root_placed = place(root_files);
    let overlay_placed = place(overlays);
    let mut image = vec![0; next as usize * SECTOR];
    let mut directory = |sector: u32, parent: u32, entries: Vec<Vec<u8>>| {
        let mut bytes = record(&[0], sector, SECTOR as u32, true);
        bytes.extend(record(&[1], parent, SECTOR as u32, true));
        for entry in entries {
            bytes.extend(entry);
        }
        let at = sector as usize * SECTOR;
        image[at..at + bytes.len()].copy_from_slice(&bytes);
    };
    let mut root_entries = vec![
        record(b"DIRECTX", 20, SECTOR as u32, true),
        record(b"OVERLAY", 19, SECTOR as u32, true),
    ];
    root_entries.extend(
        root_placed.iter().map(|(name, extent, bytes)| {
            record(name.as_bytes(), *extent, bytes.len() as u32, false)
        }),
    );
    directory(18, 18, root_entries);
    directory(
        19,
        18,
        overlay_placed
            .iter()
            .map(|(name, extent, bytes)| {
                record(name.as_bytes(), *extent, bytes.len() as u32, false)
            })
            .collect(),
    );
    directory(20, 18, vec![record(b"DXSETUP.EXE;1", 21, 0, false)]);
    for (_, extent, bytes) in root_placed.iter().chain(&overlay_placed) {
        let at = *extent as usize * SECTOR;
        image[at..at + bytes.len()].copy_from_slice(bytes);
    }
    let pvd = 16 * SECTOR;
    image[pvd] = 1;
    image[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
    image[pvd + 80..pvd + 88].copy_from_slice(&both(next));
    image[pvd + 128..pvd + 130].copy_from_slice(&2048u16.to_le_bytes());
    image[pvd + 130..pvd + 132].copy_from_slice(&2048u16.to_be_bytes());
    image[pvd + 156..pvd + 190].copy_from_slice(&record(&[0], 18, SECTOR as u32, true));
    image[17 * SECTOR] = 255;
    image[17 * SECTOR + 1..17 * SECTOR + 6].copy_from_slice(b"CD001");
    image
}

/// `iso` in raw 2352-byte sectors, its data `payload` bytes in.
fn raw(iso: &[u8], payload: usize) -> Vec<u8> {
    iso.chunks(SECTOR)
        .flat_map(|data| {
            let mut sector = vec![0xAA; 2352];
            sector[payload..payload + SECTOR].copy_from_slice(data);
            sector
        })
        .collect()
}

fn game_like() -> Vec<u8> {
    let big: Vec<u8> = (0..5000u32).map(|i| i as u8).collect();
    iso(
        &[
            ("INFEST.EXE", b"exe"),
            ("PRELOAD.DAT", &big),
            ("INFEST~1.SWG", b"swg"),
            ("SETUP.EXE", b"setup"),
        ],
        &[
            ("0.OVL", b"zero"),
            ("0D0L1.OVL", b"one"),
            ("NOTES.TXT", b"x"),
        ],
    )
}

fn open_ccd(dir: &Path, image: &[u8], mode: u8) -> DiscImage {
    fs::write(dir.join("d.img"), image).unwrap();
    let sectors = image.len() / 2352;
    fs::write(
        dir.join("d.ccd"),
        format!(
            "[CloneCD]\nVersion=3\n[Entry 0]\nSession=1\nPoint=0xa2\nControl=0x04\nPLBA={sectors}\n\
             [Entry 1]\nSession=1\nPoint=0x01\nControl=0x04\nPLBA=0\n[TRACK 1]\nMODE={mode}\nINDEX 1=0\n"
        ),
    )
    .unwrap();
    DiscImage::open(&dir.join("d.ccd")).unwrap()
}

#[test]
fn the_volume_gives_its_root_and_overlay_files_through_any_sector_layout() {
    let dir = scratch("layouts");
    let volume = game_like();
    fs::write(dir.join("d.iso"), &volume).unwrap();
    let images = [
        DiscImage::open(&dir.join("d.iso")).unwrap(),
        open_ccd(&dir, &raw(&volume, 16), 1),
    ];
    for image in &images {
        let data = image.data_track().unwrap();
        let files = iso_files(&data).unwrap();
        let names: Vec<&str> = files.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "INFEST.EXE",
                "INFEST~1.SWG",
                "OVERLAY/0.OVL",
                "OVERLAY/0D0L1.OVL",
                "OVERLAY/NOTES.TXT",
                "PRELOAD.DAT",
                "SETUP.EXE"
            ]
        );
        let preload = files["PRELOAD.DAT"];
        let bytes = data
            .read(u64::from(preload.extent), u64::from(preload.bytes))
            .unwrap();
        assert!(bytes.iter().enumerate().all(|(i, &b)| b == i as u8));
    }
    // Mode 2 Form 1 keeps its data 24 bytes in.
    let mode2 = open_ccd(&dir, &raw(&volume, 24), 2);
    assert!(iso_files(&mode2.data_track().unwrap())
        .unwrap()
        .contains_key("OVERLAY/0.OVL"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_install_list_keeps_the_ports_files_under_retails_names() {
    let dir = scratch("list");
    fs::write(dir.join("d.iso"), game_like()).unwrap();
    let image = DiscImage::open(&dir.join("d.iso")).unwrap();
    let files = iso_files(&image.data_track().unwrap()).unwrap();
    let names: Vec<String> = install_list(&files)
        .unwrap()
        .into_iter()
        .map(|f| f.0)
        .collect();
    // No Title.avi on this disc: optional. Setup files and notes stay behind.
    assert_eq!(
        names,
        [
            "Infest.exe",
            "PreLoad.dat",
            "Overlay/0.ovl",
            "Overlay/0d0l1.ovl"
        ]
    );
    let mut missing = files.clone();
    missing.remove("PRELOAD.DAT");
    assert!(install_list(&missing).unwrap_err().contains("PRELOAD.DAT"));
    missing = files.clone();
    missing.retain(|path, _| !path.starts_with("OVERLAY/"));
    assert!(install_list(&missing).unwrap_err().contains("OVERLAY"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn malformed_volumes_are_refused() {
    let dir = scratch("malformed");
    let good = game_like();
    let mut no_descriptor = good.clone();
    no_descriptor[16 * SECTOR + 1] = b'X';
    let mut disagreeing = good.clone();
    disagreeing[16 * SECTOR + 84] ^= 1;
    let mut cut_record = good.clone();
    // DIRECTX's record claims to be shorter than a record's fixed part.
    cut_record[18 * SECTOR + 34 + 34] = 20;
    let mut cycle = good.clone();
    // OVERLAY's record points back at the root directory.
    let overlay = 18 * SECTOR + 34 + 34 + 40;
    cycle[overlay + 2..overlay + 10].copy_from_slice(&both(18));
    let cases: [(&str, &[u8]); 5] = [
        ("truncated", &good[..19 * SECTOR]),
        ("no descriptor", &no_descriptor),
        ("disagreeing fields", &disagreeing),
        ("cut record", &cut_record),
        ("cycle", &cycle),
    ];
    for (name, bytes) in cases {
        fs::write(dir.join("d.iso"), bytes).unwrap();
        let image = DiscImage::open(&dir.join("d.iso")).unwrap();
        let result = image.data_track().and_then(|data| iso_files(&data));
        assert!(result.is_err(), "{name}: {result:?}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn files_that_fail_the_check_install_nothing() {
    let dir = scratch("refused");
    fs::write(dir.join("d.iso"), game_like()).unwrap();
    let image = DiscImage::open(&dir.join("d.iso")).unwrap();
    let destination = dir.join("game");
    let error = image
        .install(&destination, &dir.join("settings.json"), |_| {})
        .unwrap_err();
    assert!(error.contains("nothing was installed"), "{error}");
    // The staging folder is gone and nothing else was written.
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    fs::remove_dir_all(dir).unwrap();
}

/// The retail disc as a CloneCD image, when `INF_DISC_IMAGE` names its
/// `.ccd`; skipped otherwise.
fn retail_disc() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("INF_DISC_IMAGE")?);
    path.is_file().then_some(path)
}

#[test]
fn the_retail_disc_installs_a_ready_game_and_keeps_what_it_replaces() {
    let Some(path) = retail_disc() else {
        eprintln!("skipped: INF_DISC_IMAGE names no disc image");
        return;
    };
    let image = DiscImage::open(&path).unwrap();
    let dir = scratch("retail");
    let destination = dir.join("Infestation");
    fs::create_dir_all(destination.join("OVERLAY")).unwrap();
    fs::write(destination.join("PRELOAD.DAT"), b"old").unwrap();
    let mut steps = 0;
    let result = image
        .install(&destination, &dir.join("settings.json"), |_| steps += 1)
        .unwrap();
    assert_eq!(result.files_total, 1935);
    assert_eq!(result.files_written, 1935);
    assert!(result.report.is_ready(), "{}", result.report.description());
    assert!(result.report.retail_executable && result.report.checksums_verified);
    assert_eq!(result.music.as_ref().unwrap().written.len(), 8);
    assert!(steps > 1935);
    let backup = result.backup.as_ref().unwrap();
    assert_eq!(fs::read(backup.join("PreLoad.dat")).unwrap(), b"old");
    assert!(!fs::read_dir(&result.root).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".infestation-stage")));
    // Again: everything is already there.
    let again = image
        .install(&destination, &dir.join("settings.json"), |_| {})
        .unwrap();
    assert_eq!(again.files_written, 0);
    assert!(again.backup.is_none());
    fs::remove_dir_all(dir).unwrap();
}
