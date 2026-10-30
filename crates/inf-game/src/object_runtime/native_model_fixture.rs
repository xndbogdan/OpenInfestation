//! Canonical synthetic H8 residency shared by native object receipt tests.

use crate::{Installation, ResidencyRequest, ResidentRuntime};
use inf_formats::HANDLER_COUNT;
use serde::Deserialize;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
pub(super) struct Models {
    pub(super) capacity: usize,
    entries: Vec<Model>,
}

#[derive(Deserialize)]
struct Model {
    slot: u16,
    dword_00: u32,
    dword_0e: u32,
}

/// A canonical resident H8 bank with the same count and header inputs as the
/// native synthetic table. The retail oracle backs its inclusive boundary with
/// an extra slot; this bank deliberately does not enlarge the portable count.
pub(super) struct Fixture {
    root: PathBuf,
    pub(super) runtime: ResidentRuntime,
}

impl Fixture {
    pub(super) fn new(models: &Models) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-light-native-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let entries: Vec<_> = models
            .entries
            .iter()
            .filter(|entry| usize::from(entry.slot) < models.capacity)
            .collect();
        for (index, entry) in entries.iter().enumerate() {
            assert_eq!(usize::from(entry.slot), index, "synthetic contiguous slots");
        }
        let mut preload = vec![0, 0];
        for handler in 0..HANDLER_COUNT {
            let count = if handler == 8 {
                models.capacity as u16
            } else {
                0
            };
            preload.extend_from_slice(&count.to_le_bytes());
        }
        preload.extend_from_slice(&0_u32.to_le_bytes());
        preload.extend_from_slice(&[1, 1, 0, 0]);
        for table in 0..4 {
            for handler in 0..HANDLER_COUNT {
                let populated = table == 0 && handler == 8 && !entries.is_empty();
                let count = if populated { entries.len() as i16 } else { 0 };
                preload.extend_from_slice(&count.to_le_bytes());
                preload.extend_from_slice(&u16::from(populated).to_le_bytes());
                if populated {
                    preload.extend_from_slice(&0_u32.to_le_bytes());
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), preload).unwrap();
        let mut payload = vec![];
        for entry in &entries {
            let mut header = [0_u8; 32];
            header[0..4].copy_from_slice(&entry.dword_00.to_le_bytes());
            header[4..6].copy_from_slice(&1_u16.to_le_bytes()); // model terminator
            header[6] = 1; // auxiliary terminator and padding
            header[7] = 0x40; // unnamed

            // No raw normals. +10 belongs to another field; only the native
            // +0E WORD is a live visual input. Retain a valid archive header.
            header[12..14].copy_from_slice(&2_u16.to_le_bytes());
            header[14..16].copy_from_slice(&(entry.dword_0e as u16).to_le_bytes());
            payload.extend_from_slice(&header);
            payload.extend_from_slice(&[0x88, 0, 0, 0, 0, 0, 0, 0]);
        }
        let mut overlay = vec![];
        if !entries.is_empty() {
            overlay.extend_from_slice(&(payload.len() as u16).to_le_bytes());
            overlay.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            overlay.extend_from_slice(&payload);
        }
        fs::write(root.join("Overlay/0.ovl"), overlay).unwrap();
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        assert_eq!(runtime.resource_capacity(8), Some(models.capacity));
        Self { root, runtime }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("inf-light-native-"));
        fs::remove_dir_all(&self.root).unwrap();
    }
}
