//! Complete canonical resource-read policy from 004F3DE0.
//!
//! The callback/chunk/clock/progress owner persists across calls. Applying this
//! policy to a whole OVL is the caller's WholeFileReadCadenceApproximation:
//! native handlers instead request their own scalar and block reads. Physical
//! backing checks contain unsafe native ranges; they do not describe faults,
//! pointer aliases, CRT buffering or partial handler publication.

use std::io::{self, Read};

use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceReadCallback {
    None,
    LoadingCallback,
}

/// Semantic 00544CC8/0065ECA0/0065ECA8/AC/0065ECB0 process custody.
#[derive(Debug, PartialEq, Eq)]
pub struct ResourceReadState {
    pub callback: ResourceReadCallback,
    pub chunk: u32,
    pub last_clock: u64,
    pub pending: u32,
}

impl ResourceReadState {
    /// Supported EXE: 00544CC8 is file-backed 8000; the remaining owners lie
    /// in the PE-loader-zero virtual tail. This is not a per-file reset. The
    /// module's start (004F3D50) stamps 0065ECA8 with the clock (004A4890,
    /// `timeGetTime` times 1000); the port's clock reads zero at startup.
    pub const fn native_startup() -> Self {
        Self {
            callback: ResourceReadCallback::None,
            chunk: 8000,
            last_clock: 0,
            pending: 0,
        }
    }

    /// Complete 004F40A0: return the old selection and replace only that field.
    pub fn exchange_callback(&mut self, callback: ResourceReadCallback) -> ResourceReadCallback {
        std::mem::replace(&mut self.callback, callback)
    }
}

/// Actual current clock and installed callback. The canonical 0040EDB0
/// returns zero even after contained rendering errors. A nonzero return here
/// preserves the generic read caller's native stopped-prefix policy.
pub trait ResourceReadHost {
    fn sample_clock(&mut self) -> u64;
    fn invoke_loading_callback(&mut self, state: &mut ResourceReadState, raw_progress: u32) -> u32;
}

pub struct ResourceReadRequest<'a, R: Read + ?Sized, H: ResourceReadHost + ?Sized> {
    pub state: &'a mut ResourceReadState,
    pub source: &'a mut R,
    pub destination: &'a mut [u8],
    pub host: &'a mut H,
}

#[derive(Debug, Error)]
pub enum ResourceReadError {
    #[error("resource destination length {length} does not fit the native DWORD total")]
    TotalLength { length: usize },
    #[error("resource read [{offset}, {offset}+{length}) exceeds supplied backing {total}")]
    BackingRange {
        offset: u32,
        length: u32,
        total: u32,
    },
    #[error("resource read reached a zero-progress chunk at {offset} before total {total}")]
    ZeroChunk { offset: u32, total: u32 },
    #[error("resource exact read at {offset}, length {length}, failed: {source}")]
    Io {
        offset: u32,
        length: u32,
        #[source]
        source: io::Error,
    },
    #[error("resource callback returned native nonzero value {value:#010x}")]
    CallbackReturn { value: u32 },
}

/// One actual exact read, including a zero-length request. Read::read_exact
/// retries interruptions and fills until EOF/error; bytes already read remain
/// in the caller's destination on failure, before any later state publication.
fn exact_read<R: Read + ?Sized>(
    source: &mut R,
    destination: &mut [u8],
    offset: u32,
    length: u32,
    total: u32,
) -> Result<(), ResourceReadError> {
    let start = u64::from(offset);
    let end = start + u64::from(length);
    if end > u64::from(total) {
        return Err(ResourceReadError::BackingRange {
            offset,
            length,
            total,
        });
    }
    source
        .read_exact(&mut destination[start as usize..end as usize])
        .map_err(|source| ResourceReadError::Io {
            offset,
            length,
            source,
        })
}

fn elapsed(sample: u64, previous: u64) -> i32 {
    (sample as u32).wrapping_sub(previous as u32) as i32
}

/// Complete strict signed adaptation phase, using fresh chunk after callback.
fn adapt_chunk(state: &mut ResourceReadState, elapsed: i32) {
    if elapsed <= 40000 && 40000_i32.wrapping_sub(elapsed) > 4000 {
        state.chunk = state.chunk.wrapping_add(1000);
    }
    if elapsed.wrapping_sub(40000) > 4000 && state.chunk > 2000 {
        state.chunk = state.chunk.wrapping_sub(1000);
    }
}

/// Complete 004F3DE0 for exclusive, initialized destination backing. Callback
/// mode is read afresh at native guards; its reached mutations are retained.
pub fn read_resource_4f3de0<R: Read + ?Sized, H: ResourceReadHost + ?Sized>(
    request: ResourceReadRequest<'_, R, H>,
) -> Result<(), ResourceReadError> {
    let ResourceReadRequest {
        state,
        source,
        destination,
        host,
    } = request;
    let total = u32::try_from(destination.len()).map_err(|_| ResourceReadError::TotalLength {
        length: destination.len(),
    })?;
    if state.callback == ResourceReadCallback::None {
        return exact_read(source, destination, 0, total, total);
    }

    let mut cursor = 0_u32;
    while state.chunk.wrapping_add(cursor) < total {
        if state.chunk == 0 {
            // Native would repeat a zero-length read without advancing. The
            // supported startup is positive; a supplied late zero is a stop.
            return Err(ResourceReadError::ZeroChunk {
                offset: cursor,
                total,
            });
        }
        exact_read(source, destination, cursor, state.chunk, total)?;
        let consumed = state.chunk;
        cursor = cursor.wrapping_add(consumed);
        if state.callback != ResourceReadCallback::None {
            let sample = host.sample_clock();
            let delta = elapsed(sample, state.last_clock);
            state.pending = state.pending.wrapping_add(consumed);
            if delta > 40000 {
                let progress = state.pending;
                let value = host.invoke_loading_callback(state, progress);
                if value != 0 {
                    return Err(ResourceReadError::CallbackReturn { value });
                }
                // 004F3E98, then 004F3E9D/A3. Callback writes do not replace
                // the captured read-clock sample or these subsequent stores.
                state.pending = 0;
                state.last_clock = sample;
            }
            adapt_chunk(state, delta);
        }
    }

    exact_read(
        source,
        destination,
        cursor,
        total.wrapping_sub(cursor),
        total,
    )?;
    // 004F3F57 adds the requested TOTAL, not the final remainder.
    state.pending = state.pending.wrapping_add(total);
    if state.pending <= state.chunk >> 2 {
        return Ok(());
    }
    let captured = state.pending;
    if state.callback != ResourceReadCallback::None {
        let sample = host.sample_clock();
        let delta = elapsed(sample, state.last_clock);
        // 004F3F9B adds captured pending again, before the elapsed gate.
        state.pending = state.pending.wrapping_add(captured);
        if delta > 40000 {
            let progress = state.pending;
            let value = host.invoke_loading_callback(state, progress);
            if value != 0 {
                return Err(ResourceReadError::CallbackReturn { value });
            }
            state.last_clock = sample;
        }
        adapt_chunk(state, delta);
    }
    state.pending = 0;
    Ok(())
}

#[cfg(test)]
#[path = "loading_read/tests.rs"]
mod tests;
