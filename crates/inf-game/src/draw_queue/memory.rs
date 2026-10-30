//! Optional byte provenance for native draw allocation carry. No heap address
//! is fabricated. Unresolved pointers and sorted scratch values stay unknown.
use super::*;

#[derive(Debug)]
pub(super) struct NativeDrawMemory<C> {
    bytes: Box<[Option<u8>]>,
    recorder: fn(&C, &mut [Option<u8>]),
}

/// The Win32 heap's virtual-memory threshold (0x7F000 bytes on a 32-bit
/// process): a larger block, as 004A43A0's `malloc(0xC8000)` arena is, comes
/// from fresh `NtAllocateVirtualMemory` pages, which are all zero.
const FRESH_PAGES_THRESHOLD: u32 = 0x7_f000;

impl<C> DrawQueue<C> {
    ///Same allocator as new(), with an explicit consumer-owned store recorder.
    ///A smaller fresh malloc's contents are heap residue, unknown; a block
    ///past the heap's virtual-memory threshold starts zero. reset() keeps them.
    pub fn with_native_memory(
        capacity: u32,
        recorder: fn(&C, &mut [Option<u8>]),
    ) -> Result<Self, DrawQueueFault> {
        let mut queue = Self::new(capacity)?;
        let fresh = (capacity > FRESH_PAGES_THRESHOLD).then_some(0);
        queue.memory = Some(NativeDrawMemory {
            bytes: vec![fresh; queue.capacity as usize].into_boxed_slice(),
            recorder,
        });
        Ok(queue)
    }

    /// The queue as [`Self::with_native_memory`] makes it, keeping this
    /// queue's allocations where its size is the same: a frame's new arena
    /// (004A43A0) holds what a fresh one would, its byte record refilled with
    /// a fresh block's (zero past the heap's threshold, else unknown), without
    /// allocating and freeing 0xC8000 records a frame.
    pub fn renew_with_native_memory(
        &mut self,
        capacity: u32,
        recorder: fn(&C, &mut [Option<u8>]),
    ) -> Result<(), DrawQueueFault> {
        let aligned = capacity
            .checked_add(15)
            .ok_or(DrawQueueFault::AllocationSize)?
            & !15;
        match self.memory.as_mut() {
            Some(memory) if self.capacity == aligned => {
                let fresh = (aligned > FRESH_PAGES_THRESHOLD).then_some(0);
                memory.bytes.fill(fresh);
                memory.recorder = recorder;
                self.reset();
            }
            _ => *self = Self::with_native_memory(capacity, recorder)?,
        }
        Ok(())
    }

    ///Read the next allocation's payload without allocating or advancing it.
    ///An unknown byte is not a zero or a sampled native address.
    pub fn next_payload_word(&self, offset: u32) -> Option<u32> {
        let memory = self.memory.as_ref()?;
        let start = self.front.checked_add(16)?.checked_add(offset)? as usize;
        let bytes = memory.bytes.get(start..start.checked_add(4)?)?;
        Some(u32::from_le_bytes([
            bytes[0]?, bytes[1]?, bytes[2]?, bytes[3]?,
        ]))
    }
}

impl<C> NativeDrawMemory<C> {
    fn store(&mut self, offset: u32, value: Option<u32>) {
        let bytes = value
            .map(|v| v.to_le_bytes().map(Some))
            .unwrap_or([None; 4]);
        if let Some(target) = self.bytes.get_mut(offset as usize..offset as usize + 4) {
            target.copy_from_slice(&bytes);
        }
    }
    pub(super) fn pending(&mut self, back: u32, sorting: bool, depth: u32) {
        if let Some(pointer) = back.checked_sub(16) {
            self.store(pointer, None);
        }
        if sorting {
            if let Some(key) = back.checked_sub(32) {
                self.store(key, Some(depth));
            }
        }
    }
    pub(super) fn record(&mut self, offset: u32, length: u32, receiver: u32, command: &C) {
        (self.recorder)(
            command,
            &mut self.bytes[offset as usize..(offset + length) as usize],
        );
        // Header+04 is supplied to the allocator independently of C.
        self.store(offset + 4, Some(receiver));
    }
    pub(super) fn group(&mut self, offset: u32) {
        self.store(offset, Some(0x4a4430));
        for at in [4, 8, 12] {
            self.store(offset + at, Some(0));
        }
        //Sorting byte is filled by begin_group; the other three bytes survive.
        self.store(offset + 24, None); //Saved pending pointer.
        self.store(offset + 28, None); //Parent group pointer.
    }
    pub(super) fn group_sorting(&mut self, offset: u32, sorting: bool) {
        self.bytes[offset as usize + 16] = Some(u8::from(sorting));
    }
    pub(super) fn group_closed(&mut self, offset: u32) {
        self.store(offset + 20, None); //Forward list start pointer.
        self.store(offset + 24, None); //Forward list end pointer.
    }
    pub(super) fn close(&mut self, front: u32, back: u32, start: u32, count: usize, sorting: bool) {
        if sorting {
            //4A46F9 writes only one pointer DWORD per reserved16 bytes.
            for index in 0..count as u32 {
                self.store(front + index * 16, None);
            }
            //Native heap swaps only the key/pointer DWORDs. Its intermediate
            //permutation isn't represented by the typed ordering owner; mark
            //those overwritten words unknown, retaining untouched padding.
            for offset in (back..start).step_by(16) {
                self.store(offset, None);
            }
        } else {
            //4A44CB calls5129F0 with the entire pending range, including
            //the twelve untouched bytes in each16-byte pointer cell.
            self.bytes
                .copy_within(back as usize..start as usize, front as usize);
        }
    }
}

#[cfg(test)]
mod tests;
