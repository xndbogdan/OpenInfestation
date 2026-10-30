//! Runtime copies of retained native allocations. Original OVL bytes remain
//! immutable evidence; writes apply to one shared allocation image, including
//! overlapping retained ranges and later command reads.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Weak,
    },
};

use inf_formats::{AssetRelocation, ByteSpan, ConstructorAssetBank, ConstructorPayloadView};
use thiserror::Error;

use crate::LoadedOverlay;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum RuntimeAllocationError {
    #[error("runtime allocation span is outside its original source")]
    SourceRange,
    #[error("runtime allocation access at {offset:#x} for {length} bytes exceeds its {allocation_bytes} bytes")]
    Range {
        offset: u32,
        length: usize,
        allocation_bytes: usize,
    },
    #[error("runtime write at {offset:#x} overlaps relocated pointer {field_offset:#x}; native allocation-address arithmetic is not implemented")]
    RelocatedWrite { offset: u32, field_offset: u32 },
    #[error(
        "runtime write at {offset:#x} overlaps a constructor record with immutable retained state"
    )]
    ImmutableConstructorWrite { offset: u32 },
    #[error("runtime write at {offset:#x} overlaps constructor dispatch header {header_offset:#x}; class-changing aliases are not implemented")]
    ConstructorHeaderWrite { offset: u32, header_offset: u32 },
    #[error("scalar access at {offset:#x} overlaps typed runtime pointer {field_offset:#x}")]
    RuntimePointerScalar { offset: u32, field_offset: u32 },
    #[error("runtime pointer at {offset:#x} has unowned native bits {value:#010x}")]
    UnownedRuntimePointer { offset: u32, value: u32 },
    #[error("runtime write at {offset:#x} from the view shown's draw, which may change nothing")]
    ShownDrawWrite { offset: u32 },
}

/// The next allocation's serial: allocations made later have larger ones.
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// Port option: while the world draw draws the view shown apart from
    /// the game's draw ([`refusing_writes`]), the first serial of the
    /// allocations made since. It draws on a copy of the arena, which shares
    /// the allocations made before, so nothing may write those; the copy's
    /// own, which go with it, take writes.
    static REFUSING_WRITES: Cell<Option<u64>> = const { Cell::new(None) };
    /// A write was refused since [`take_refused`] last looked.
    static REFUSED: Cell<bool> = const { Cell::new(false) };
}

/// Run `f` with every runtime allocation made before it refusing writes.
pub(crate) fn refusing_writes<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(Option<u64>);
    impl Drop for Restore {
        fn drop(&mut self) {
            REFUSING_WRITES.set(self.0);
        }
    }
    let first = NEXT_SERIAL.load(Ordering::Relaxed);
    let _restore = Restore(REFUSING_WRITES.replace(Some(first)));
    f()
}

/// Whether a write was refused since the last look, which this one ends.
pub(super) fn take_refused() -> bool {
    REFUSED.replace(false)
}

/// Stable identity of one 00480460 row allocation. Slots are never reused;
/// the arena identity prevents a retained pointer from naming another arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Selector60RowId {
    pub(in crate::object_runtime) arena: u64,
    pub(in crate::object_runtime) index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimePointer {
    Selector60Row(Selector60RowId),
}

#[derive(Debug)]
pub struct RuntimeAllocation {
    /// Its place in the order allocations are made ([`refusing_writes`]).
    serial: u64,
    source: Arc<[u8]>,
    main_payload: ByteSpan,
    bytes: RefCell<Vec<u8>>,
    relocation_fields: Vec<u32>,
    immutable_constructor_ranges: Vec<std::ops::Range<usize>>,
    constructor_headers: Vec<u32>,
    pointers: RefCell<BTreeMap<u32, RuntimePointer>>,
}

impl RuntimeAllocation {
    /// Explicit plain-byte allocation with no relocated native pointers.
    /// Canonical H13/H14 owners register their relocation fields separately
    /// so unsupported address-dependent writes are rejected.
    pub fn new(source: Arc<[u8]>, main_payload: ByteSpan) -> Result<Self, RuntimeAllocationError> {
        let bytes = source
            .get(main_payload.start..main_payload.end)
            .ok_or(RuntimeAllocationError::SourceRange)?
            .to_vec();
        Ok(Self {
            serial: NEXT_SERIAL.fetch_add(1, Ordering::Relaxed),
            source,
            main_payload,
            bytes: RefCell::new(bytes),
            relocation_fields: vec![],
            immutable_constructor_ranges: vec![],
            constructor_headers: vec![],
            pointers: RefCell::new(BTreeMap::new()),
        })
    }

    pub(super) fn with_relocations(
        source: Arc<[u8]>,
        main_payload: ByteSpan,
        relocations: &[AssetRelocation],
    ) -> Result<Self, RuntimeAllocationError> {
        let mut allocation = Self::new(source, main_payload)?;
        allocation.relocation_fields = relocations.iter().map(|entry| entry.field_offset).collect();
        Ok(allocation)
    }

    /// Original bytes for source-custody checks and immutable asset inspection.
    pub fn source(&self) -> &[u8] {
        &self.source
    }
    pub fn main_payload(&self) -> ByteSpan {
        self.main_payload
    }

    /// Fresh scalar image with original file offsets. Typed runtime pointers
    /// remain separate opaque slots with zero image bytes; they are never host
    /// addresses or fabricated native addresses. Runtime consumers must check
    /// each actual read with `validate_scalar_read` before using decoded data.
    pub fn source_snapshot(&self) -> Vec<u8> {
        let mut source = self.source.to_vec();
        source[self.main_payload.start..self.main_payload.end]
            .copy_from_slice(&self.bytes.borrow());
        source
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.borrow().clone()
    }

    pub fn runtime_pointers(&self) -> Vec<(u32, RuntimePointer)> {
        self.pointers
            .borrow()
            .iter()
            .map(|(&at, &pointer)| (at, pointer))
            .collect()
    }

    pub(in crate::object_runtime) fn validate_scalar_read(
        &self,
        offset: u32,
        length: usize,
    ) -> Result<(), RuntimeAllocationError> {
        let range = self.range(offset, length)?;
        if let Some((&field_offset, _)) =
            self.pointers.borrow().iter().find(|(field, _)| {
                range.start < **field as usize + 4 && range.end > **field as usize
            })
        {
            return Err(RuntimeAllocationError::RuntimePointerScalar {
                offset,
                field_offset,
            });
        }
        Ok(())
    }

    fn range(
        &self,
        offset: u32,
        length: usize,
    ) -> Result<std::ops::Range<usize>, RuntimeAllocationError> {
        let start = offset as usize;
        let allocation_bytes = self.main_payload.end - self.main_payload.start;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= allocation_bytes)
            .ok_or(RuntimeAllocationError::Range {
                offset,
                length,
                allocation_bytes,
            })?;
        Ok(start..end)
    }

    pub(crate) fn word(&self, offset: u32) -> Result<u16, RuntimeAllocationError> {
        self.validate_scalar_read(offset, 2)?;
        let range = self.range(offset, 2)?;
        Ok(u16::from_le_bytes(
            self.bytes.borrow()[range].try_into().unwrap(),
        ))
    }

    pub(crate) fn dword(&self, offset: u32) -> Result<u32, RuntimeAllocationError> {
        self.validate_scalar_read(offset, 4)?;
        let range = self.range(offset, 4)?;
        Ok(u32::from_le_bytes(
            self.bytes.borrow()[range].try_into().unwrap(),
        ))
    }

    pub(in crate::object_runtime) fn byte(
        &self,
        offset: u32,
    ) -> Result<u8, RuntimeAllocationError> {
        self.validate_scalar_read(offset, 1)?;
        Ok(self.bytes.borrow()[offset as usize])
    }

    pub(in crate::object_runtime) fn write_dword(
        &self,
        offset: u32,
        value: u32,
    ) -> Result<(), RuntimeAllocationError> {
        self.validate_write(offset, 4, value == 0)?;
        let range = self.range(offset, 4)?;
        self.bytes.borrow_mut()[range].copy_from_slice(&value.to_le_bytes());
        if value == 0 {
            self.pointers.borrow_mut().remove(&offset);
        }
        Ok(())
    }

    pub(in crate::object_runtime) fn runtime_pointer(
        &self,
        offset: u32,
    ) -> Result<Option<RuntimePointer>, RuntimeAllocationError> {
        self.range(offset, 4)?;
        if let Some(pointer) = self.pointers.borrow().get(&offset) {
            return Ok(Some(*pointer));
        }
        match self.dword(offset)? {
            0 => Ok(None),
            value => Err(RuntimeAllocationError::UnownedRuntimePointer { offset, value }),
        }
    }

    pub(in crate::object_runtime) fn set_runtime_pointer(
        &self,
        offset: u32,
        pointer: RuntimePointer,
    ) -> Result<(), RuntimeAllocationError> {
        self.validate_write(offset, 4, true)?;
        let range = self.range(offset, 4)?;
        self.bytes.borrow_mut()[range].fill(0);
        self.pointers.borrow_mut().insert(offset, pointer);
        Ok(())
    }

    pub(super) fn write_word(&self, offset: u32, value: u16) -> Result<(), RuntimeAllocationError> {
        self.validate_word_write(offset)?;
        let range = self.range(offset, 2)?;
        self.bytes.borrow_mut()[range].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub(super) fn validate_word_write(&self, offset: u32) -> Result<(), RuntimeAllocationError> {
        self.validate_write(offset, 2, false)
    }

    fn validate_write(
        &self,
        offset: u32,
        length: usize,
        replace_pointer: bool,
    ) -> Result<(), RuntimeAllocationError> {
        if REFUSING_WRITES
            .get()
            .is_some_and(|first| self.serial < first)
        {
            REFUSED.set(true);
            return Err(RuntimeAllocationError::ShownDrawWrite { offset });
        }
        let range = self.range(offset, length)?;
        if let Some((&field_offset, _)) = self.pointers.borrow().iter().find(|(field, _)| {
            range.start < **field as usize + 4
                && range.end > **field as usize
                && !(replace_pointer && offset == **field && length == 4)
        }) {
            return Err(RuntimeAllocationError::RuntimePointerScalar {
                offset,
                field_offset,
            });
        }
        if let Some(&header_offset) = self
            .constructor_headers
            .iter()
            .find(|&&header| range.start < header as usize + 4 && range.end > header as usize)
        {
            return Err(RuntimeAllocationError::ConstructorHeaderWrite {
                offset,
                header_offset,
            });
        }
        if let Some(&field_offset) = self
            .relocation_fields
            .iter()
            .find(|&&field| range.start < field as usize + 4 && range.end > field as usize)
        {
            return Err(RuntimeAllocationError::RelocatedWrite {
                offset,
                field_offset,
            });
        }
        if self
            .immutable_constructor_ranges
            .iter()
            .any(|other| range.start < other.end && range.end > other.start)
        {
            return Err(RuntimeAllocationError::ImmutableConstructorWrite { offset });
        }
        Ok(())
    }

    pub(super) fn validate_range(
        &self,
        offset: u32,
        length: usize,
    ) -> Result<(), RuntimeAllocationError> {
        self.range(offset, length).map(|_| ())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ConstructorAllocations(Vec<ConstructorAllocation>);

#[derive(Clone, Debug)]
struct ConstructorAllocation {
    source: Weak<LoadedOverlay>,
    region: usize,
    allocation: Arc<RuntimeAllocation>,
}

impl ConstructorAllocations {
    pub(super) fn retain(
        &mut self,
        source: &Arc<LoadedOverlay>,
        region: usize,
        assets: &ConstructorAssetBank,
    ) -> Result<Arc<RuntimeAllocation>, RuntimeAllocationError> {
        // Keep writes after the final object's destruction while this source
        // remains resident. Retained objects can independently outlive unload.
        self.0.retain(|entry| entry.source.strong_count() != 0);
        let identity = Arc::downgrade(source);
        if let Some(entry) = self
            .0
            .iter()
            .find(|entry| entry.region == region && Weak::ptr_eq(&entry.source, &identity))
        {
            return Ok(Arc::clone(&entry.allocation));
        }
        let bank = &assets.prefixes;
        let mut allocation = RuntimeAllocation::with_relocations(
            Arc::from(source.bytes()),
            bank.main_payload,
            &bank.relocations,
        )?;
        allocation.constructor_headers = bank
            .entries
            .iter()
            .map(|entry| entry.allocation_offset)
            .collect();
        // Other class owners currently retain immutable parsed payloads. Gate
        // a cross-class overlap rather than silently leave their live data
        // stale. These conservative intervals are write boundaries, not a
        // claim that the gap to the next registered address is a record size.
        let mut offsets: Vec<_> = bank
            .entries
            .iter()
            .map(|entry| entry.allocation_offset as usize)
            .collect();
        offsets.sort_unstable();
        offsets.dedup();
        for entry in &bank.entries {
            if entry.constructor_selector != 86 {
                let start = entry.allocation_offset as usize;
                let end = offsets
                    .iter()
                    .copied()
                    .find(|offset| *offset > start)
                    .unwrap_or(bank.main_payload.len());
                allocation.immutable_constructor_ranges.push(start..end);
            }
        }
        // A different registered prefix may be nested inside a larger typed
        // payload. Protect the actual read spans too, independent of entry
        // order, so retained actor damage tables cannot become stale.
        for asset in &assets.assets {
            if let ConstructorPayloadView::SharedActor(actor) = &asset.payload {
                let span = actor.prefix_span;
                allocation
                    .immutable_constructor_ranges
                    .push(span.start - bank.main_payload.start..span.end - bank.main_payload.start);
                if let Some(derived) = &actor.derived {
                    allocation.immutable_constructor_ranges.push(
                        derived.span.start - bank.main_payload.start
                            ..derived.span.end - bank.main_payload.start,
                    );
                }
                let specialized = actor
                    .selector30
                    .iter()
                    .flat_map(|payload| {
                        [
                            payload.signed_dword_44.span,
                            payload.words_48_4c.span,
                            payload.dword_50.span,
                            payload.dword_54.span,
                        ]
                    })
                    .chain(
                        actor
                            .selector35
                            .iter()
                            .flat_map(|payload| [payload.dword_6c.span, payload.dword_70.span]),
                    );
                for span in specialized {
                    allocation.immutable_constructor_ranges.push(
                        span.start - bank.main_payload.start..span.end - bank.main_payload.start,
                    );
                }
            }
        }
        let allocation = Arc::new(allocation);
        self.0.push(ConstructorAllocation {
            source: identity,
            region,
            allocation: Arc::clone(&allocation),
        });
        Ok(allocation)
    }
}
