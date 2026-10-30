//! Indexed process profiler4395A0/4395C0, separate from4875A0 world timing.
//!
//! The supported domain covers native constant callers: indices0..26, banks0/1.
//! Native helpers have no bounds checks; this portable bound is custody, not a
//! recovered native precondition. Other callers/cadence and non-Windows clock
//! epoch parity remain separate owners. Slots overwrite globally, never stacked.
use inf_formats::{FrontendExecutable, FrontendExecutableError};
#[cfg(not(windows))]
use std::time::Instant;
use thiserror::Error;

const WIDTH: usize = 27;
const SLOTS: usize = WIDTH * 2;
const ELAPSED_BASE: u32 = 0x0054_6f38;
const START_BASE: u32 = 0x0054_71c8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfilingPhase {
    Begin,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfilingRequest {
    pub index: u32,
    pub bank: u32,
    pub phase: ProfilingPhase,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("profiling index{index}, bank{bank} is outside supported native callers")]
pub struct ProfilingError {
    pub index: u32,
    pub bank: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessProfiling {
    starts: [u64; SLOTS],
    elapsed: [u64; SLOTS],
    clock: ProfilingClock,
}

impl ProcessProfiling {
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        let mut elapsed_bytes = [0_u8; SLOTS * 8];
        //Original .data raw end547000: first200 bytes are original file bytes;
        //the rest and the full starts array need genuine loader-zero proof.
        elapsed_bytes[..200].copy_from_slice(executable.mapped(ELAPSED_BASE, 200)?.bytes);
        executable.loader_zero_data(0x0054_7000, elapsed_bytes.len() - 200)?;
        executable.loader_zero_data(START_BASE, SLOTS * 8)?;
        Ok(Self {
            starts: [0; SLOTS],
            elapsed: std::array::from_fn(|i| {
                u64::from_le_bytes(elapsed_bytes[i * 8..i * 8 + 8].try_into().unwrap())
            }),
            clock: ProfilingClock::new(),
        })
    }

    pub fn record(&mut self, request: ProfilingRequest) -> Result<(), ProfilingError> {
        let milliseconds = self.clock.milliseconds();
        self.record_milliseconds(request, milliseconds)
    }

    /// Required platform sample in native timeGetTime DWORD millisecond units.
    /// 4A4890 CDQ sign-extends then512D30 multiplies by1000 modulo2^64;
    /// EAX=0 makes composed4923C0 a NULL return, with no pump callback.
    pub fn record_milliseconds(
        &mut self,
        request: ProfilingRequest,
        milliseconds: u32,
    ) -> Result<(), ProfilingError> {
        let slot = slot(request.index, request.bank)?;
        let sample = (i64::from(milliseconds as i32) as u64).wrapping_mul(1000);
        match request.phase {
            ProfilingPhase::Begin => self.starts[slot] = sample,
            ProfilingPhase::End => self.elapsed[slot] = sample.wrapping_sub(self.starts[slot]),
        }
        Ok(())
    }

    /// Actual retained QWORD pair (start, elapsed), not a draw-duration stack.
    pub fn counters(&self, index: u32, bank: u32) -> Result<(u64, u64), ProfilingError> {
        let slot = slot(index, bank)?;
        Ok((self.starts[slot], self.elapsed[slot]))
    }
}

fn slot(index: u32, bank: u32) -> Result<usize, ProfilingError> {
    if index >= WIDTH as u32 || bank >= 2 {
        return Err(ProfilingError { index, bank });
    }
    Ok(index as usize + WIDTH * bank as usize)
}

/// Native Windows clock; other platforms explicitly use the replaceable
/// ProcessProfilingClockEpochApproximation with a process-local monotonic epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProfilingClock {
    #[cfg(not(windows))]
    epoch: Instant,
}
impl ProfilingClock {
    fn new() -> Self {
        Self {
            #[cfg(not(windows))]
            epoch: Instant::now(),
        }
    }
    fn milliseconds(&self) -> u32 {
        #[cfg(windows)]
        {
            //Original4A4893 calls WINMM!timeGetTime via IAT6615A4. It takes
            //no arguments and writes no caller memory; DWORD wrap is retained.
            unsafe { timeGetTime() }
        }
        #[cfg(not(windows))]
        {
            self.epoch.elapsed().as_millis() as u32
        }
    }
}
#[cfg(windows)]
#[link(name = "winmm")]
extern "system" {
    fn timeGetTime() -> u32;
}

#[cfg(test)]
mod tests;
