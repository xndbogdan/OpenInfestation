//! Message 22, the furniture states (`0045E0F0` out, `0045E620` in), over
//! the H12 terrain's furniture buckets: 512 of them (`+80014` the counts,
//! `+80414` the records), each record a WORD value `+0`, a state byte `+2`
//! (its low nibble the state) and a lookup byte `+3` (0 once destroyed).
//!
//! The encoder resumes where the last frame ended (`0054E864` the bucket,
//! `0054E860` the first value at or above which it starts), finds the next
//! record, and packs runs from it until the room is spent or the bucket
//! ends; a frame never crosses buckets. Each run is a byte, its form in the
//! top two bits and its count below, then the form's bytes: 0 one byte a
//! record (`0xFF` destroyed), 1 a nibble a record, 2 a run of zero states
//! and 3 a run of destroyed records, both without bytes. Form 0 is taken
//! whenever one of the next three records would break the run's kind. A run
//! of 64 spills into the form's bits, as in retail. The frame's DWORD head
//! is the run count `<< 18`, the start value `<< 9` and the bucket, or
//! `0xFFFFFFFF` when no bucket holds a record at or above the value.
//!
//! The receiver applies every run from the same record (retail does not
//! advance between runs), and only on a client.

use thiserror::Error;

use super::game::GameNetState;

pub const BUCKETS: usize = 0x200;
/// The largest run a form counts.
const RUN_LIMIT: usize = 0x40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureRecord {
    pub value: u16,
    pub state: u8,
    pub lookup: u8,
}

/// The H12 furniture buckets message 22 reads and writes.
pub trait FurnitureBuckets {
    fn count(&self, bucket: usize) -> usize;
    fn record(&self, bucket: usize, index: usize) -> Option<FurnitureRecord>;
    fn set_state(&mut self, bucket: usize, index: usize, state: u8) -> bool;
    fn set_lookup(&mut self, bucket: usize, index: usize, lookup: u8) -> bool;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FurnitureError {
    #[error("bucket {bucket} has no record {index}, which retail reads past its count")]
    Record { bucket: usize, index: usize },
    #[error("message 22's payload ends before offset {0}")]
    Short(usize),
}

fn record<B: FurnitureBuckets>(
    buckets: &B,
    bucket: usize,
    index: usize,
) -> Result<FurnitureRecord, FurnitureError> {
    buckets
        .record(bucket, index)
        .ok_or(FurnitureError::Record { bucket, index })
}

/// `0045E0C0`: 2 destroyed, 1 a nonzero state, else 0.
pub fn record_code(record: FurnitureRecord) -> u8 {
    if record.lookup == 0 {
        2
    } else {
        u8::from(record.state & 0xf != 0)
    }
}

/// `0045E400`, form 0: a byte a record, `0xFF` for a destroyed one.
fn form_bytes<B: FurnitureBuckets>(
    buckets: &B,
    out: &mut [u8],
    bucket: usize,
    start: usize,
    room: i32,
) -> Result<usize, FurnitureError> {
    let count = buckets.count(bucket);
    let mut written = 0;
    let mut left = room;
    while left != 0 && written < RUN_LIMIT && start + written < count {
        let r = record(buckets, bucket, start + written)?;
        out[written] = if r.lookup != 0 { r.state & 0xf } else { 0xff };
        written += 1;
        left -= 1;
    }
    Ok(written)
}

/// `0045E490`, form 1: a nibble a record, low first, up to a destroyed one.
fn form_nibbles<B: FurnitureBuckets>(
    buckets: &B,
    out: &mut [u8],
    bucket: usize,
    start: usize,
    room: i32,
) -> Result<usize, FurnitureError> {
    let count = buckets.count(bucket);
    let (mut entries, mut left, mut at, mut low) = (0, room, 0, true);
    while left != 0 && entries < RUN_LIMIT && start + entries < count {
        let r = record(buckets, bucket, start + entries)?;
        if r.lookup == 0 {
            break;
        }
        if low {
            out[at] = r.state & 0xf;
        } else {
            out[at] |= r.state << 4;
            at += 1;
            left -= 1;
        }
        entries += 1;
        low = !low;
    }
    Ok(entries)
}

/// `0045E550` (zero states) and `0045E5C0` (destroyed records): a run's length.
fn form_run<B: FurnitureBuckets>(
    buckets: &B,
    bucket: usize,
    start: usize,
    destroyed: bool,
) -> Result<usize, FurnitureError> {
    let count = buckets.count(bucket);
    let mut entries = 0;
    while entries < RUN_LIMIT && start + entries < count {
        let r = record(buckets, bucket, start + entries)?;
        let fits = if destroyed {
            r.lookup == 0
        } else {
            r.lookup != 0 && r.state & 0xf == 0
        };
        if !fits {
            break;
        }
        entries += 1;
    }
    Ok(entries)
}

/// `0045E0F0`: the next frame of furniture states into `out`, its length
/// (-1 below 16 bytes of room).
pub fn encode_furniture<B: FurnitureBuckets>(
    out: &mut [u8],
    room: u32,
    state: &mut GameNetState,
    buckets: &B,
) -> Result<i32, FurnitureError> {
    let room = room as i32;
    let mut left = if room > 0x80 {
        0x80
    } else if room >= 0x10 {
        room
    } else {
        return Ok(-1);
    };
    left -= 4;
    let (mut at, mut written, mut runs) = (4usize, 4i32, 0u32);
    let start_bucket = state.furniture_cursor[0];
    let threshold = state.furniture_cursor[1] as u16;
    let mut bucket = start_bucket;
    let mut index = loop {
        let b = (bucket & 0xffff) as usize;
        let count = buckets.count(b);
        let mut found = None;
        for index in 0..count {
            if record(buckets, b, index)?.value >= threshold {
                found = Some(index);
                break;
            }
        }
        if let Some(index) = found {
            break index;
        }
        bucket = if (bucket + 1) as u16 >= 0x200 {
            0
        } else {
            bucket + 1
        };
        if bucket & 0xffff == start_bucket {
            out[..4].copy_from_slice(&u32::MAX.to_le_bytes());
            return Ok(4);
        }
    };
    let b = (bucket & 0xffff) as usize;
    let count = buckets.count(b);
    let mut next_value = u32::from(record(buckets, b, index)?.value);
    state.furniture_cursor = [bucket & 0xffff, next_value];
    while left >= 4 {
        let code = record_code(record(buckets, b, index)?);
        let mut form = match code {
            0 => 2,
            1 => 1,
            _ => 3,
        };
        for ahead in 1..4 {
            if index + ahead >= count {
                break;
            }
            let next = record_code(record(buckets, b, index + ahead)?);
            let breaks = match next {
                2 => form <= 2,
                1 => (2..=3).contains(&form),
                _ => form == 3,
            };
            if breaks {
                form = 0;
            }
        }
        let head = at;
        at += 1;
        left -= 1;
        written += 1;
        let entries = match form {
            0 => form_bytes(buckets, &mut out[at..], b, index, left)?,
            1 => form_nibbles(buckets, &mut out[at..], b, index, left)?,
            2 => form_run(buckets, b, index, false)?,
            _ => form_run(buckets, b, index, true)?,
        };
        out[head] = (form << 6) | entries as u8;
        index += entries;
        let bytes = match form {
            0 => entries,
            1 => (entries + 1) >> 1,
            _ => 0,
        };
        at += bytes;
        written += bytes as i32;
        left -= bytes as i32;
        if index >= count {
            next_value = 0;
            bucket = if (bucket + 1) as u16 >= 0x200 {
                0
            } else {
                bucket + 1
            };
            left = 0;
        } else {
            next_value = u32::from(record(buckets, b, index)?.value);
        }
        runs += 1;
    }
    let [start, value] = state.furniture_cursor;
    let head = (runs << 9 | value) << 9 | start;
    out[..4].copy_from_slice(&head.to_le_bytes());
    state.furniture_cursor = [bucket & 0xffff, next_value & 0xffff];
    Ok(written)
}

/// `0045E620`: a frame of furniture states, applied on a client from the
/// record whose value is the frame's start value; the bytes used.
pub fn receive_furniture<B: FurnitureBuckets>(
    payload: &[u8],
    host: bool,
    buckets: &mut B,
) -> Result<i32, FurnitureError> {
    let byte = |at: usize| {
        payload
            .get(at)
            .copied()
            .ok_or(FurnitureError::Short(at + 1))
    };
    let head = u32::from_le_bytes(
        payload
            .get(..4)
            .ok_or(FurnitureError::Short(4))?
            .try_into()
            .unwrap(),
    );
    if head == u32::MAX {
        return Ok(4);
    }
    let bucket = (head & 0x1ff) as usize;
    let value = ((head >> 9) & 0x1ff) as u16;
    let runs = head >> 18;
    let count = buckets.count(bucket);
    let mut index = 0;
    while index < count && record(buckets, bucket, index)?.value != value {
        index += 1;
    }
    let (mut at, mut used) = (4usize, 4i32);
    for _ in 0..runs {
        let run = byte(at)?;
        at += 1;
        used += 1;
        let (form, entries) = (run >> 6, usize::from(run & 0x3f));
        if !host {
            let body = payload.get(at..).unwrap_or(&[]);
            match form {
                0 => apply_bytes(buckets, body, bucket, index, entries)?,
                1 => apply_nibbles(buckets, body, bucket, index, entries)?,
                // 0045E8B0: zero states.
                2 => apply_each(buckets, bucket, index, entries, |b, i| {
                    let state = record(b, bucket, i)?.state & 0xf0;
                    Ok(b.set_state(bucket, i, state))
                })?,
                // 0045E910: destroyed records.
                _ => apply_each(buckets, bucket, index, entries, |b, i| {
                    Ok(b.set_lookup(bucket, i, 0))
                })?,
            }
        }
        let bytes = match form {
            0 => entries,
            1 => (entries + 1) >> 1,
            _ => 0,
        };
        at += bytes;
        used += bytes as i32;
    }
    Ok(used)
}

fn apply_each<B: FurnitureBuckets>(
    buckets: &mut B,
    bucket: usize,
    start: usize,
    entries: usize,
    mut apply: impl FnMut(&mut B, usize) -> Result<bool, FurnitureError>,
) -> Result<(), FurnitureError> {
    for index in start..start + entries {
        if !apply(buckets, index)? {
            return Err(FurnitureError::Record { bucket, index });
        }
    }
    Ok(())
}

/// `0045E780`: a byte a record; `0xFF` destroys it, else its low nibble is
/// the state's.
fn apply_bytes<B: FurnitureBuckets>(
    buckets: &mut B,
    body: &[u8],
    bucket: usize,
    start: usize,
    entries: usize,
) -> Result<(), FurnitureError> {
    let mut k = 0;
    apply_each(buckets, bucket, start, entries, |b, index| {
        let byte = *body.get(k).ok_or(FurnitureError::Short(k + 1))?;
        k += 1;
        if byte == 0xff {
            return Ok(b.set_lookup(bucket, index, 0));
        }
        let state = record(b, bucket, index)?.state;
        Ok(b.set_state(bucket, index, (state & 0xf0) | (byte & 0xf)))
    })
}

/// `0045E810`: a nibble a record, low first.
fn apply_nibbles<B: FurnitureBuckets>(
    buckets: &mut B,
    body: &[u8],
    bucket: usize,
    start: usize,
    entries: usize,
) -> Result<(), FurnitureError> {
    let mut k = 0;
    apply_each(buckets, bucket, start, entries, |b, index| {
        let byte = *body.get(k / 2).ok_or(FurnitureError::Short(k / 2 + 1))?;
        let nibble = if k % 2 == 0 { byte } else { byte >> 4 };
        k += 1;
        let state = record(b, bucket, index)?.state;
        Ok(b.set_state(bucket, index, (state & 0xf0) | (nibble & 0xf)))
    })
}

#[cfg(test)]
#[path = "furniture_tests.rs"]
mod tests;
