use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("inf-retail-case-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    // Windows looks names up ignoring case unless a folder asks otherwise
    // (no admin needed; its new subfolders inherit it), so the tests match
    // names here as Linux would. Where it is refused they still pass.
    #[cfg(windows)]
    let _ = std::process::Command::new("fsutil")
        .args(["file", "setCaseSensitiveInfo"])
        .arg(&dir)
        .arg("enable")
        .output();
    dir
}

/// The scan past the exact try, on what `read_dir` reports: the stored
/// names, whatever case the filesystem looks names up in.
#[test]
fn the_scan_matches_names_ignoring_ascii_case_only() {
    let root = scratch("scan");
    fs::create_dir(root.join("OVERLAY")).unwrap();
    fs::write(root.join("OVERLAY/0d0L0.OVL"), b"").unwrap();
    fs::write(root.join("\u{e9}.txt"), b"").unwrap();
    let found = |parent: &Path, name: &str| {
        any_case_child(parent, OsStr::new(name)).map(|path| path.file_name().unwrap().to_owned())
    };
    assert_eq!(found(&root, "Overlay"), Some("OVERLAY".into()));
    assert_eq!(
        found(&root.join("OVERLAY"), "0d0l0.ovl"),
        Some("0d0L0.OVL".into())
    );
    // Windows folds É to é too; retail's names are ASCII.
    assert_eq!(found(&root, "\u{c9}.txt"), None);
    assert_eq!(found(&root, "Overlay2"), None);
    assert_eq!(found(&root.join("missing"), "Overlay"), None);
    fs::remove_dir_all(root).unwrap();
}

/// Two names differing in case (one file on Windows, two elsewhere): the
/// exact one is the file.
#[test]
fn the_exact_name_wins_over_another_case() {
    let root = scratch("exact");
    fs::write(root.join("PRELOAD.DAT"), b"other").unwrap();
    fs::write(root.join("PreLoad.dat"), b"exact").unwrap();
    let exact = root.join("PreLoad.dat");
    assert_eq!(
        find_child(&root, OsStr::new("PreLoad.dat")),
        Some(exact.clone())
    );
    assert_eq!(retail_path(&root, "PreLoad.dat"), exact);
    assert_eq!(fs::read(&exact).unwrap(), b"exact");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_retail_path_matches_each_component_or_names_the_exact_path() {
    let root = scratch("walk");
    fs::create_dir(root.join("overlay")).unwrap();
    fs::write(root.join("overlay/0D0.OVL"), b"overlay").unwrap();
    assert_eq!(
        fs::read(retail_path(&root, "Overlay/0d0.ovl")).unwrap(),
        b"overlay"
    );
    for missing in ["Overlay/0d1.ovl", "cdaudio/track02.ogg", "Infest.exe"] {
        assert_eq!(retail_path(&root, missing), root.join(missing));
    }
    fs::remove_dir_all(root).unwrap();
}
