//! Retained scalar prefix and complete guarded callback 0040EDB0.
//!
//! Startup reads the authored file-backed 00546918..00546954 range once.
//! Effects and mode belong to the current text-entry/platform owners and are
//! borrowed, rather than copied into another loading process. A callback
//! commits clock/progress/age/pulse/RNG before testing the current renderer.
//! Required Tick/presentation failures inside the native guard are disposed;
//! they do not roll back the prefix, retry, or stop the resource reader.
//! Physical exception frames, aliased writes and native lock ownership remain
//! outside this typed scalar/required-host domain.

use inf_formats::{ByteSpan, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use crate::name_entry::name_entry_rng_next;

const STARTUP_ADDRESS: u32 = 0x0054_6918;
const STARTUP_BYTES: usize = 60;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum LoadingStartupError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("loading startup requires the supported file-backed .data profile")]
    UnsupportedDataProfile,
}

/// Actual scalar words retained across world reads. No Default or per-world
/// initializer supplies these fields. Counter names preserve their distinct
/// native stores; only the low WORD of pulse is changed by the callback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadingProcessState {
    pub counter_546918: u32,
    pub counter_54691c: u32,
    pub counter_546920: u32,
    pub previous_clock: u64,
    pub raw_progress: u32,
    pub age_micros: u32,
    pub pulse_direction: u8,
    pub selector: u16,
    pub pulse: u32,
}

impl LoadingProcessState {
    /// Decode genuine authored startup backing. This validates the existing
    /// structural PE/.data profile, not an executable hash or later writers.
    /// Shared effect bytes/mode are not reset by creating this scalar owner.
    pub fn native_startup(executable: &[u8]) -> Result<Self, LoadingStartupError> {
        let image = FrontendExecutable::parse(executable)?;
        if image.data().virtual_address != 0x0052_e000
            || image.data().bytes.len() != 0x19000
            || image.file_span() != ByteSpan::new(0x12e000, 0x147000)
        {
            return Err(LoadingStartupError::UnsupportedDataProfile);
        }
        let source = image.mapped(STARTUP_ADDRESS, STARTUP_BYTES)?.bytes;
        let word = |offset: usize| {
            u32::from_le_bytes(source[offset..offset + 4].try_into().expect("DWORD"))
        };
        Ok(Self {
            counter_546918: word(0),
            counter_54691c: word(4),
            counter_546920: word(8),
            previous_clock: u64::from(word(16)) | (u64::from(word(20)) << 32),
            raw_progress: word(24),
            age_micros: word(48),
            pulse_direction: source[53],
            selector: u16::from_le_bytes(source[54..56].try_into().expect("WORD")),
            pulse: word(56),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadingCallbackOrigin {
    /// 004F3DE0 passes the literal stack argument1.
    ResourceRead,
    /// 0048DCCB/DD01 passes the literal stack argument0.
    AttractHandshake,
}

impl LoadingCallbackOrigin {
    pub const fn caller_status(self) -> i32 {
        match self {
            Self::ResourceRead => 1,
            Self::AttractHandshake => 0,
        }
    }
}

pub struct LoadingCallbackRequest<'a> {
    pub state: &'a mut LoadingProcessState,
    pub effect_bytes: &'a mut [u8; 16],
    pub rng: &'a mut u32,
    /// Incoming EDX, including the reader's native total/doubling quirk.
    /// This is not a semantic byte-count or a presentation percentage.
    pub raw_progress: u32,
    pub origin: LoadingCallbackOrigin,
}

/// Borrowed actual owners for one native Tick. Direct typed Tick callers may
/// supply status3+; the two real callback origins supply only1/0.
pub struct LoadingTickRequest<'a> {
    pub state: &'a mut LoadingProcessState,
    pub effect_bytes: &'a mut [u8; 16],
    pub rng: &'a mut u32,
    pub delta_micros: i32,
    pub caller_status: i32,
}

pub trait LoadingCallbackHost {
    type Error;
    /// Actual 004A4890 clock, outside the guarded renderer phases.
    fn sample_clock(&mut self) -> u64;
    /// Initial 004391C0/+04 check only; Tick and Draw acquire afresh.
    fn renderer_available(&mut self) -> Result<bool, Self::Error>;
    fn tick_loading(&mut self, request: LoadingTickRequest<'_>) -> Result<(), Self::Error>;
    /// Complete current Draw/presentation owner, including a fresh renderer.
    fn present_loading(&mut self) -> Result<(), Self::Error>;
    /// Required representation of 004923C0's contained error disposal.
    fn dispose_loading_error(&mut self, error: Self::Error);
}

/// Complete 0040EDB0 scalar/guard policy. The native return is always0,
/// including renderer-NULL and disposed guarded errors. Host operations own
/// real clock/renderer/presentation; none has a default success implementation.
pub fn loading_callback_40edb0<H: LoadingCallbackHost>(
    request: LoadingCallbackRequest<'_>,
    host: &mut H,
) -> u32 {
    let now = host.sample_clock();
    let delta = (now as u32).wrapping_sub(request.state.previous_clock as u32) as i32;
    let delta = delta.clamp(7_800, 125_000);
    request.state.previous_clock = now;
    request.state.raw_progress = request
        .state
        .raw_progress
        .wrapping_add(request.raw_progress);
    request.state.age_micros = request.state.age_micros.wrapping_add(delta as u32);

    let low = request.state.pulse as u16;
    if request.state.pulse_direction != 0 {
        let step = delta >> 4;
        if i32::from(low) < 0xffff - step {
            request.state.pulse =
                (request.state.pulse & 0xffff_0000) | u32::from(low.wrapping_add(step as u16));
        } else {
            request.state.pulse = (request.state.pulse & 0xffff_0000) | 0xffff;
            request.state.pulse_direction = 0;
        }
    } else {
        let step = delta >> 3;
        if i32::from(low) > step {
            request.state.pulse =
                (request.state.pulse & 0xffff_0000) | u32::from(low.wrapping_sub(step as u16));
        } else {
            // Original zero store precedes the genuine shared RNG call.
            request.state.pulse &= 0xffff_0000;
            let random = name_entry_rng_next(request.rng) & 0xffff;
            request.state.pulse_direction = 1;
            request.state.selector = (random % 5) as u16;
        }
    }

    let guarded = (|| {
        if host.renderer_available()? {
            host.tick_loading(LoadingTickRequest {
                state: request.state,
                effect_bytes: request.effect_bytes,
                rng: request.rng,
                delta_micros: delta,
                caller_status: request.origin.caller_status(),
            })?;
            host.present_loading()?;
        }
        Ok::<(), H::Error>(())
    })();
    if let Err(error) = guarded {
        host.dispose_loading_error(error);
    }
    0
}

#[cfg(test)]
mod tests;
