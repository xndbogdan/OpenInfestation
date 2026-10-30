//! Read-only mapping of the supported executable's authored data sections.
//!
//! This validates the structural profile used by the passive inspector. It
//! does not authenticate the file hash, load code, relocate pointers, or map
//! virtual zero-fill as original data.

use thiserror::Error;

use crate::{ByteSpan, FrontendData};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrontendExecutableError {
    #[error("executable range at file offset {offset:#x} requires {bytes} bytes")]
    Truncated { offset: usize, bytes: usize },
    #[error("invalid executable header: {0}")]
    Header(&'static str),
    #[error("unsupported Infest.exe structural profile")]
    UnsupportedBuild,
    #[error("executable must contain exactly one file-backed .data section")]
    DataSection,
    #[error("invalid, duplicated, or overlapping read-only data section")]
    ReadOnlyDataSection,
    #[error("data address {0:#010x} is outside the file-backed section")]
    DataAddress(u32),
    #[error("address {address:#010x} + {length} bytes is not proven loader-zero .data storage")]
    LoaderZeroRange { address: u32, length: usize },
}

/// A borrowed original data section, retaining its physical and virtual origin.
#[derive(Clone, Copy, Debug)]
pub struct FrontendExecutable<'a> {
    data: FrontendData<'a>,
    file_span: ByteSpan,
    read_only: Option<FrontendExecutableRange<'a>>,
    data_virtual_size: u32,
    sections: &'a [u8],
}

/// A file-backed range in .data or .rdata. Its span is an executable FILE span,
/// not an offset relative to either mapped section.
#[derive(Clone, Copy, Debug)]
pub struct FrontendExecutableRange<'a> {
    pub bytes: &'a [u8],
    pub virtual_address: u32,
    pub file_span: ByteSpan,
}

/// A checked absolute-address range in the .data loader-zero tail. This is
/// evidence about initial process storage, not file bytes or a later reset.
/// Private bounds keep callers from fabricating a zero-fill proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrontendLoaderZeroRange {
    start: u32,
    end: u32,
}

impl FrontendLoaderZeroRange {
    pub fn start(self) -> u32 {
        self.start
    }
    pub fn end(self) -> u32 {
        self.end
    }
}

impl<'a> FrontendExecutable<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, FrontendExecutableError> {
        use FrontendExecutableError as E;
        if range(bytes, 0, 2)? != b"MZ" {
            return Err(E::Header("missing MZ signature"));
        }
        let pe =
            usize::try_from(word(bytes, 0x3c)?).map_err(|_| E::Header("PE offset overflow"))?;
        let coff = range(bytes, pe, 24)?;
        if &coff[..4] != b"PE\0\0" {
            return Err(E::Header("missing PE signature"));
        }
        let optional_offset = pe.checked_add(24).ok_or(E::Header("PE offset overflow"))?;
        let optional_size = usize::from(half(coff, 20)?);
        if optional_size < 64 {
            return Err(E::Header("short optional header"));
        }
        let optional = range(bytes, optional_offset, optional_size)?;
        if half(coff, 4)? != 0x014c
            || word(coff, 8)? != 0x396f_38a3
            || half(optional, 0)? != 0x010b
            || word(optional, 28)? != 0x0040_0000
            || word(optional, 56)? != 0x0027_b000
        {
            return Err(E::UnsupportedBuild);
        }
        let section_offset = optional_offset
            .checked_add(optional_size)
            .ok_or(E::Header("section offset overflow"))?;
        let section_count = usize::from(half(coff, 6)?);
        let sections = range(bytes, section_offset, section_count * 40)?;
        let mut data = None;
        let mut read_only = None;
        for section in sections.chunks_exact(40) {
            let is_data = &section[..8] == b".data\0\0\0";
            let is_read_only = &section[..8] == b".rdata\0\0";
            if !is_data && !is_read_only {
                continue;
            }
            if is_data && data.is_some() {
                return Err(E::DataSection);
            }
            if is_read_only && read_only.is_some() {
                return Err(E::ReadOnlyDataSection);
            }
            let rva = word(section, 12)?;
            let size = usize::try_from(word(section, 16)?).map_err(|_| E::DataSection)?;
            let offset = usize::try_from(word(section, 20)?).map_err(|_| E::DataSection)?;
            if size == 0 || u64::from(rva) + size as u64 > 0x0027_b000 {
                return Err(E::DataSection);
            }
            let source = range(bytes, offset, size)?;
            let mapped = FrontendExecutableRange {
                bytes: source,
                virtual_address: 0x0040_0000 + rva,
                file_span: ByteSpan::new(offset, offset + size),
            };
            if is_read_only {
                read_only = Some(mapped);
                continue;
            }
            data = Some(Self {
                data: FrontendData {
                    bytes: source,
                    virtual_address: 0x0040_0000 + rva,
                },
                file_span: ByteSpan::new(offset, offset + size),
                read_only: None,
                data_virtual_size: word(section, 8)?,
                sections,
            });
        }
        let mut data = data.ok_or(E::DataSection)?;
        if let Some(other) = read_only {
            let start = u64::from(data.data.virtual_address);
            let other_start = u64::from(other.virtual_address);
            if start < other_start + other.bytes.len() as u64
                && other_start < start + data.data.bytes.len() as u64
            {
                return Err(E::ReadOnlyDataSection);
            }
        }
        data.read_only = read_only;
        Ok(data)
    }

    /// Prove a requested absolute-address range belongs only to the declared
    /// .data virtual tail. `mapped` remains restricted to original file bytes.
    /// This structural proof neither authenticates code nor initializes memory.
    pub fn loader_zero_data(
        self,
        address: u32,
        length: usize,
    ) -> Result<FrontendLoaderZeroRange, FrontendExecutableError> {
        let invalid = || FrontendExecutableError::LoaderZeroRange { address, length };
        let image_end = 0x0040_0000_u64 + 0x0027_b000;
        let data_start = u64::from(self.data.virtual_address);
        let file_end = data_start + self.data.bytes.len() as u64;
        let virtual_end = data_start + u64::from(self.data_virtual_size);
        let start = u64::from(address);
        let end = start
            .checked_add(u64::try_from(length).map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        if start < file_end || end > virtual_end || virtual_end > image_end {
            return Err(invalid());
        }
        for section in self.sections.chunks_exact(40) {
            if &section[..8] == b".data\0\0\0" {
                continue;
            }
            let other_start = 0x0040_0000_u64 + u64::from(word(section, 12)?);
            let extent = word(section, 8)?.max(word(section, 16)?);
            let other_end = other_start + u64::from(extent);
            if other_end > image_end || (start < other_end && other_start < end) {
                return Err(invalid());
            }
        }
        Ok(FrontendLoaderZeroRange {
            start: address,
            end: end as u32,
        })
    }

    pub fn data(self) -> FrontendData<'a> {
        self.data
    }

    pub fn file_span(self) -> ByteSpan {
        self.file_span
    }

    pub fn word(self, virtual_address: u32) -> Result<u32, FrontendExecutableError> {
        word(self.mapped(virtual_address, 4)?.bytes, 0)
    }

    pub fn mapped(
        self,
        address: u32,
        length: usize,
    ) -> Result<FrontendExecutableRange<'a>, FrontendExecutableError> {
        let data = FrontendExecutableRange {
            bytes: self.data.bytes,
            virtual_address: self.data.virtual_address,
            file_span: self.file_span,
        };
        for section in [Some(data), self.read_only].into_iter().flatten() {
            let Some(offset) = address
                .checked_sub(section.virtual_address)
                .map(|v| v as usize)
            else {
                continue;
            };
            let Some(end) = offset.checked_add(length) else {
                continue;
            };
            if let Some(bytes) = section.bytes.get(offset..end) {
                return Ok(FrontendExecutableRange {
                    bytes,
                    virtual_address: address,
                    file_span: ByteSpan::new(
                        section.file_span.start + offset,
                        section.file_span.start + end,
                    ),
                });
            }
        }
        Err(FrontendExecutableError::DataAddress(address))
    }
}

fn range(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], FrontendExecutableError> {
    let end = offset
        .checked_add(length)
        .ok_or(FrontendExecutableError::Truncated {
            offset,
            bytes: length,
        })?;
    bytes
        .get(offset..end)
        .ok_or(FrontendExecutableError::Truncated {
            offset,
            bytes: length,
        })
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, FrontendExecutableError> {
    Ok(u32::from_le_bytes(
        range(bytes, offset, 4)?.try_into().expect("four bytes"),
    ))
}

fn half(bytes: &[u8], offset: usize) -> Result<u16, FrontendExecutableError> {
    Ok(u16::from_le_bytes(
        range(bytes, offset, 2)?.try_into().expect("two bytes"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0x300];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x40_u32.to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
        bytes[0x44..0x46].copy_from_slice(&0x14c_u16.to_le_bytes());
        bytes[0x46..0x48].copy_from_slice(&1_u16.to_le_bytes());
        bytes[0x48..0x4c].copy_from_slice(&0x396f38a3_u32.to_le_bytes());
        bytes[0x54..0x56].copy_from_slice(&0xe0_u16.to_le_bytes());
        bytes[0x58..0x5a].copy_from_slice(&0x10b_u16.to_le_bytes());
        bytes[0x74..0x78].copy_from_slice(&0x400000_u32.to_le_bytes());
        bytes[0x90..0x94].copy_from_slice(&0x27b000_u32.to_le_bytes());
        bytes[0x138..0x140].copy_from_slice(b".data\0\0\0");
        bytes[0x144..0x148].copy_from_slice(&0x12e000_u32.to_le_bytes());
        bytes[0x148..0x14c].copy_from_slice(&0x100_u32.to_le_bytes());
        bytes[0x14c..0x150].copy_from_slice(&0x200_u32.to_le_bytes());
        bytes[0x200..0x204].copy_from_slice(&0x12345678_u32.to_le_bytes());
        bytes
    }

    #[test]
    fn maps_only_file_backed_original_data_with_both_origins() {
        let bytes = fixture();
        let exe = FrontendExecutable::parse(&bytes).unwrap();
        assert_eq!(exe.file_span(), ByteSpan::new(0x200, 0x300));
        assert_eq!(exe.data().virtual_address, 0x52e000);
        assert_eq!(exe.data().bytes, &bytes[0x200..]);
        assert_eq!(exe.word(0x52e000).unwrap(), 0x12345678);
        assert!(exe.word(0x52dfff).is_err());
        assert!(exe.word(0x52e0fd).is_err());
    }

    #[test]
    fn every_truncated_prefix_and_invalid_profile_is_rejected() {
        let bytes = fixture();
        for end in 0..bytes.len() {
            assert!(
                FrontendExecutable::parse(&bytes[..end]).is_err(),
                "prefix {end}"
            );
        }
        for offset in [0x44, 0x48, 0x58, 0x74, 0x90] {
            let mut altered = bytes.clone();
            altered[offset] ^= 1;
            assert!(matches!(
                FrontendExecutable::parse(&altered),
                Err(FrontendExecutableError::UnsupportedBuild)
            ));
        }
    }

    #[test]
    fn rejects_duplicate_missing_and_out_of_image_data_sections() {
        let mut bytes = fixture();
        bytes[0x138] = b'X';
        assert!(matches!(
            FrontendExecutable::parse(&bytes),
            Err(FrontendExecutableError::DataSection)
        ));
        let mut bytes = fixture();
        bytes[0x46..0x48].copy_from_slice(&2_u16.to_le_bytes());
        bytes.copy_within(0x138..0x160, 0x160);
        assert!(matches!(
            FrontendExecutable::parse(&bytes),
            Err(FrontendExecutableError::DataSection)
        ));
        let mut bytes = fixture();
        bytes[0x144..0x148].copy_from_slice(&0xfffffff0_u32.to_le_bytes());
        assert!(matches!(
            FrontendExecutable::parse(&bytes),
            Err(FrontendExecutableError::DataSection)
        ));
    }
}
