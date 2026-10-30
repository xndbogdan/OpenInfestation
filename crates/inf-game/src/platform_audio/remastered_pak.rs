//! `Remastered.pak`, the sounds the port adds ([the plan](../../../../docs/rt-plans/atmosphere.md#the-sounds-on-disk-remasteredpak)):
//! a header, an index of named entries and the files as they are: Ogg
//! Vorbis at the mixer's 44.1 kHz, and `credits.txt`. Our own archive, not
//! an OVL, so nothing takes it for retail data. `scripts/build-remastered-pak.py`
//! builds it from the player's own sounds through a manifest; the sounds never
//! enter git.
//!
//! Little-endian: the magic `INFPAK1\0`, a u32 version (1), the manifest's
//! name (u32 length, UTF-8), a u32 entry count, then each entry's name (u16
//! length, UTF-8), u64 offset from the file's start, u64 length and SHA-256.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAGIC: &[u8; 8] = b"INFPAK1\0";
pub const VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum PakError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0}: not a Remastered.pak (bad magic or version)")]
    Format(PathBuf),
    #[error("{0}: its index runs past the file")]
    Index(PathBuf),
    #[error("Remastered.pak has no {0}")]
    Missing(String),
    #[error("Remastered.pak's {0} does not match its checksum")]
    Checksum(String),
    #[error("Remastered.pak's {name}: {reason}")]
    Decode { name: String, reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    offset: u64,
    length: u64,
    sha256: [u8; 32],
}

/// An opened pak: its index; the files are read when asked for.
#[derive(Clone, Debug)]
pub struct RemasteredPak {
    path: PathBuf,
    manifest: String,
    entries: BTreeMap<String, Entry>,
}

/// A sound decoded for the mixer: 16-bit frames at 44.1 kHz, interleaved
/// when stereo.
#[derive(Debug, PartialEq)]
pub struct DecodedSound {
    pub channels: u8,
    pub samples: Vec<i16>,
}

impl DecodedSound {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }
}

/// The index read in order.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
    path: &'a Path,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], PakError> {
        let slice = self
            .bytes
            .get(self.at..self.at + count)
            .ok_or_else(|| PakError::Index(self.path.into()))?;
        self.at += count;
        Ok(slice)
    }
}

impl RemasteredPak {
    pub fn open(path: &Path) -> Result<Self, PakError> {
        let io = |source| PakError::Io {
            path: path.into(),
            source,
        };
        let mut file = File::open(path).map_err(io)?;
        let size = file.metadata().map_err(io)?.len();
        let mut bytes = Vec::new();
        // The index sits at the front; it is small next to the sounds.
        let head = size.min(1 << 20);
        file.by_ref()
            .take(head)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        let mut index = Cursor {
            bytes: &bytes,
            at: 0,
            path,
        };
        if index.take(8)? != MAGIC {
            return Err(PakError::Format(path.into()));
        }
        let u32_at = |slice: &[u8]| u32::from_le_bytes(slice.try_into().expect("four bytes"));
        let u64_at = |slice: &[u8]| u64::from_le_bytes(slice.try_into().expect("eight bytes"));
        if u32_at(index.take(4)?) != VERSION {
            return Err(PakError::Format(path.into()));
        }
        let length = u32_at(index.take(4)?) as usize;
        let manifest = String::from_utf8_lossy(index.take(length)?).into_owned();
        let count = u32_at(index.take(4)?) as usize;
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let length = usize::from(u16::from_le_bytes(
                index.take(2)?.try_into().expect("two bytes"),
            ));
            let name = String::from_utf8_lossy(index.take(length)?).into_owned();
            let offset = u64_at(index.take(8)?);
            let length = u64_at(index.take(8)?);
            let sha256: [u8; 32] = index.take(32)?.try_into().expect("32 bytes");
            if offset.checked_add(length).is_none_or(|end| end > size) {
                return Err(PakError::Index(path.into()));
            }
            entries.insert(
                name,
                Entry {
                    offset,
                    length,
                    sha256,
                },
            );
        }
        Ok(Self {
            path: path.into(),
            manifest,
            entries,
        })
    }

    /// The manifest the pak was built from.
    /// How many files the pak holds (its sounds and their credits).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn manifest(&self) -> &str {
        &self.manifest
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Entry `name`'s bytes, checked against the index.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, PakError> {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| PakError::Missing(name.into()))?;
        let io = |source| PakError::Io {
            path: self.path.clone(),
            source,
        };
        let mut file = File::open(&self.path).map_err(io)?;
        file.seek(SeekFrom::Start(entry.offset)).map_err(io)?;
        let mut bytes = vec![0; entry.length as usize];
        file.read_exact(&mut bytes).map_err(io)?;
        if <[u8; 32]>::from(Sha256::digest(&bytes)) != entry.sha256 {
            return Err(PakError::Checksum(name.into()));
        }
        Ok(bytes)
    }

    /// Entry `name` decoded: Ogg Vorbis at 44.1 kHz, mono or stereo.
    pub fn decode(&self, name: &str) -> Result<Arc<DecodedSound>, PakError> {
        let bytes = self.read(name)?;
        let fault = |reason: String| PakError::Decode {
            name: name.into(),
            reason,
        };
        let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes))
            .map_err(|error| fault(error.to_string()))?;
        let channels = reader.ident_hdr.audio_channels;
        if reader.ident_hdr.audio_sample_rate != super::OUTPUT_RATE || !(1..=2).contains(&channels)
        {
            return Err(fault(format!(
                "{} channels at {} Hz, not mono or stereo at {}",
                channels,
                reader.ident_hdr.audio_sample_rate,
                super::OUTPUT_RATE
            )));
        }
        let mut samples = Vec::new();
        while let Some(packet) = reader
            .read_dec_packet_itl()
            .map_err(|error| fault(error.to_string()))?
        {
            samples.extend(packet);
        }
        Ok(Arc::new(DecodedSound { channels, samples }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pak as the builder writes it, its index read back and an entry
    /// checked against its checksum.
    #[test]
    fn a_pak_reads_back_its_index_and_checks_its_files() {
        let folder = std::env::temp_dir().join(format!("remastered-pak-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("Remastered.pak");
        let files: [(&str, &[u8]); 2] = [("credits.txt", b"thanks"), ("beds/a.ogg", b"OggS....")];
        let manifest = b"manifest v1";
        let mut index = Vec::new();
        let header = 8 + 4 + 4 + manifest.len() + 4;
        let index_size: usize = files
            .iter()
            .map(|(name, _)| 2 + name.len() + 8 + 8 + 32)
            .sum();
        let mut offset = (header + index_size) as u64;
        for (name, bytes) in files {
            index.extend((name.len() as u16).to_le_bytes());
            index.extend(name.as_bytes());
            index.extend(offset.to_le_bytes());
            index.extend((bytes.len() as u64).to_le_bytes());
            index.extend(Sha256::digest(bytes));
            offset += bytes.len() as u64;
        }
        let mut pak = Vec::new();
        pak.extend(MAGIC);
        pak.extend(VERSION.to_le_bytes());
        pak.extend((manifest.len() as u32).to_le_bytes());
        pak.extend(manifest);
        pak.extend((files.len() as u32).to_le_bytes());
        pak.extend(index);
        for (_, bytes) in files {
            pak.extend(bytes);
        }
        std::fs::write(&path, &pak).unwrap();
        let opened = RemasteredPak::open(&path).unwrap();
        assert_eq!(opened.manifest(), "manifest v1");
        assert!(opened.contains("beds/a.ogg"));
        assert_eq!(opened.read("credits.txt").unwrap(), b"thanks");
        assert!(matches!(opened.read("nothing"), Err(PakError::Missing(_))));
        // A changed byte fails its checksum.
        let last = pak.len() - 1;
        pak[last] ^= 1;
        std::fs::write(&path, &pak).unwrap();
        assert!(matches!(
            RemasteredPak::open(&path).unwrap().read("beds/a.ogg"),
            Err(PakError::Checksum(_))
        ));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
