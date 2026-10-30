//! Shared 0048EE20/0048EEC0 dependency acquisition used by H9 and voices.
//!
//! Counts and nesting are process state. Dependencies initialize in table
//! order, unwind in reverse order on a native error, and release after their
//! owner's cleanup. A missing portable service retains the reached prefix.

use std::collections::BTreeMap;

use inf_formats::{FrontendExecutable, FrontendExecutableError};

pub const SOUND_HANDLER_RESOURCE: u32 = 0x0054_5478;
pub const SOUND_VOICE_RESOURCE: u32 = 0x0054_4cf0;
pub const SOUND_DEVICE_RESOURCE: u32 = 0x0054_5550;
pub const SOUND_RECORD_RESOURCE: u32 = 0x0053_bb88;

const RESOURCES: [u32; 6] = [
    SOUND_HANDLER_RESOURCE,
    SOUND_VOICE_RESOURCE,
    SOUND_DEVICE_RESOURCE,
    0x0054_0da8, // allocation services
    0x0054_0d28, // cleanup registration
    SOUND_RECORD_RESOURCE,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundResourceLifecycle {
    counts: BTreeMap<u32, u32>,
    nesting: u32,
}

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum SoundLifecycleError<E> {
    #[error(transparent)]
    Source(#[from] FrontendExecutableError),
    #[error("required process lifecycle record {0:08X}")]
    UnknownResource(u32),
    #[error("unterminated dependency table {0:08X}")]
    DependencyTable(u32),
    #[error("required sound lifecycle callback: {0}")]
    Host(E),
}

pub trait SoundLifecycleHost {
    type Error;

    /// Native EAX is an opaque error identity; zero means normal completion.
    /// Body callbacks must execute their real device/list/resource operation.
    fn callback(&mut self, address: u32) -> Result<u32, Self::Error>;

    /// 00492240 reports and returns the same error; it does not dispose it.
    fn report_error(&mut self, error: u32) -> Result<(), Self::Error>;
}

impl SoundResourceLifecycle {
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        executable.loader_zero_data(0x0055_f188, 4)?;
        let counts = RESOURCES
            .into_iter()
            .map(|address| executable.word(address).map(|count| (address, count)))
            .collect::<Result<_, _>>()?;
        Ok(Self { counts, nesting: 0 })
    }

    pub fn count(&self, resource: u32) -> Option<u32> {
        self.counts.get(&resource).copied()
    }

    pub fn nesting(&self) -> u32 {
        self.nesting
    }

    pub fn acquire<H: SoundLifecycleHost>(
        &mut self,
        resource: u32,
        executable: FrontendExecutable<'_>,
        host: &mut H,
    ) -> Result<u32, SoundLifecycleError<H::Error>> {
        let count = self
            .counts
            .get_mut(&resource)
            .ok_or(SoundLifecycleError::UnknownResource(resource))?;
        let old = *count;
        *count = count.wrapping_add(1);
        if old != 0 {
            return Ok(0);
        }
        self.nesting = self.nesting.wrapping_add(1);
        // 0048C850 at this boundary is a proven RET, with no state effects.
        let table = executable.word(resource + 4)?;
        let mut index = 1_u32;
        let mut error = 0;
        loop {
            if index > 512 {
                return Err(SoundLifecycleError::DependencyTable(table));
            }
            let acquire = executable.word(table + index * 8)?;
            if acquire == 0 {
                break;
            }
            error = self.invoke(acquire, executable, host)?;
            if error != 0 {
                break;
            }
            index += 1;
        }
        if error == 0 {
            error = self.invoke(executable.word(table)?, executable, host)?;
        }
        if error == 0 {
            self.nesting = self.nesting.wrapping_sub(1);
            return Ok(0);
        }
        // A failing dependency is not among the successfully acquired prefix.
        while index > 1 {
            index -= 1;
            self.invoke(executable.word(table + index * 8 + 4)?, executable, host)?;
        }
        self.nesting = self.nesting.wrapping_sub(1);
        let count = self.counts.get_mut(&resource).expect("retained record");
        *count = count.wrapping_sub(1);
        if (self.nesting as i32) <= 0 {
            host.report_error(error)
                .map_err(SoundLifecycleError::Host)?;
        }
        Ok(error)
    }

    pub fn release<H: SoundLifecycleHost>(
        &mut self,
        resource: u32,
        executable: FrontendExecutable<'_>,
        host: &mut H,
    ) -> Result<(), SoundLifecycleError<H::Error>> {
        let count = self
            .counts
            .get_mut(&resource)
            .ok_or(SoundLifecycleError::UnknownResource(resource))?;
        *count = count.wrapping_sub(1);
        if *count != 0 {
            return Ok(());
        }
        self.nesting = self.nesting.wrapping_add(1);
        let table = executable.word(resource + 4)?;
        let mut index = 1_u32;
        while executable.word(table + index * 8 + 4)? != 0 {
            index += 1;
            if index > 512 {
                return Err(SoundLifecycleError::DependencyTable(table));
            }
        }
        self.invoke(executable.word(table + 4)?, executable, host)?;
        while index > 1 {
            index -= 1;
            self.invoke(executable.word(table + index * 8 + 4)?, executable, host)?;
        }
        self.nesting = self.nesting.wrapping_sub(1);
        Ok(())
    }

    fn invoke<H: SoundLifecycleHost>(
        &mut self,
        routine: u32,
        executable: FrontendExecutable<'_>,
        host: &mut H,
    ) -> Result<u32, SoundLifecycleError<H::Error>> {
        let dependency = match routine {
            0x0050_f740 => Some((SOUND_HANDLER_RESOURCE, true)),
            0x0050_f750 => Some((SOUND_HANDLER_RESOURCE, false)),
            0x004f_4210 => Some((SOUND_VOICE_RESOURCE, true)),
            0x004f_4220 => Some((SOUND_VOICE_RESOURCE, false)),
            0x0051_0350 => Some((SOUND_DEVICE_RESOURCE, true)),
            0x0051_0360 => Some((SOUND_DEVICE_RESOURCE, false)),
            0x0047_ff40 => Some((SOUND_RECORD_RESOURCE, true)),
            0x0047_ff50 => Some((SOUND_RECORD_RESOURCE, false)),
            0x0049_5920 => Some((0x0054_0da8, true)),
            0x0049_5930 => Some((0x0054_0da8, false)),
            0x0049_4880 => Some((0x0054_0d28, true)),
            0x0049_4890 => Some((0x0054_0d28, false)),
            _ => None,
        };
        if let Some((resource, acquire)) = dependency {
            if acquire {
                self.acquire(resource, executable, host)
            } else {
                self.release(resource, executable, host)?;
                Ok(0) // release EAX is unused by the native caller
            }
        } else if matches!(
            routine,
            0x0048_c860 | 0x0048_ef20 | 0x0040_13a0 | 0x0048_c850
        ) {
            // First two bodies are XOR EAX,EAX; RET. The last two are RET
            // only, used exclusively in result-discarding release slots.
            Ok(0)
        } else {
            host.callback(routine).map_err(SoundLifecycleError::Host)
        }
    }
}
