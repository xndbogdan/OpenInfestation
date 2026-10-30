//! The transport's message ring (`004927B0..00492800`): a read index `+0`, a
//! write index `+4`, a slot count `+8`, a slot size `+C` and the slots `+10`.
//! One slot always stays free: a commit that would meet the read index is
//! refused, and the slot is written again by the next message.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RingError {
    #[error("004927B0 refuses a ring of {count} slots of {slot_size} bytes (error 00528D08)")]
    Refused { count: i32, slot_size: i32 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageRing {
    read: usize,
    write: usize,
    count: usize,
    slot_size: usize,
    storage: Vec<u8>,
}

impl MessageRing {
    /// `004927B0`: at least two slots of a nonnegative size.
    pub fn build(count: i32, slot_size: i32) -> Result<Self, RingError> {
        if count < 2 || slot_size < 0 {
            return Err(RingError::Refused { count, slot_size });
        }
        let (count, slot_size) = (count as usize, slot_size as usize);
        Ok(Self {
            read: 0,
            write: 0,
            count,
            slot_size,
            storage: vec![0; count * slot_size],
        })
    }

    /// `00492800`: frees the slots and clears every field.
    pub fn free(&mut self) {
        *self = Self::default();
    }

    pub fn indices(&self) -> [usize; 2] {
        [self.read, self.write]
    }

    pub fn slot_size(&self) -> usize {
        self.slot_size
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// `00492770`: the slot at the write index, whether or not the ring is
    /// full; none from a freed ring, whose storage is NULL.
    pub fn write_slot(&mut self) -> Option<&mut [u8]> {
        if self.count == 0 {
            return None;
        }
        let start = self.write * self.slot_size;
        Some(&mut self.storage[start..start + self.slot_size])
    }

    /// `00492790`: the write index moves on unless it would meet the read index.
    pub fn commit(&mut self) {
        let next = if self.write + 1 == self.count {
            0
        } else {
            self.write + 1
        };
        if next != self.read {
            self.write = next;
        }
    }

    /// `00492720`: the slot at the read index, none when empty or freed.
    pub fn read_slot(&self) -> Option<&[u8]> {
        (self.count != 0 && self.read != self.write).then(|| {
            let start = self.read * self.slot_size;
            &self.storage[start..start + self.slot_size]
        })
    }

    /// `00492740`: the read index moves on when the ring holds a slot.
    pub fn pop(&mut self) {
        if self.read != self.write {
            self.read = if self.read + 1 >= self.count {
                0
            } else {
                self.read + 1
            };
        }
    }

    pub fn storage(&self) -> &[u8] {
        &self.storage
    }
}
