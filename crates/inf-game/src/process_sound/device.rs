//! Native 00510250 initialization and 00510330 device release.
//!
//! Enabled, device pointer and configuration are independent retained fields.
//! A failed initialization consumes its native error and leaves enabled/config
//! unchanged; teardown clears the pointer but deliberately retains enabled.

use inf_formats::{FrontendExecutable, FrontendExecutableError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundDeviceIdentity(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundDeviceRuntime {
    pub enabled: u8,
    pub device: Option<SoundDeviceIdentity>,
    pub configuration: u8,
}

pub trait SoundDeviceHost {
    type Error;

    /// Actual output-pointer loan for DirectSoundCreate's portable backend.
    /// Failure can still have changed the output, so it is not transactional.
    fn create(&mut self, output: &mut Option<SoundDeviceIdentity>) -> Result<i32, Self::Error>;
    fn hresult_error(&mut self, result: i32) -> Result<Option<u32>, Self::Error>;
    fn report_error(&mut self, error: u32) -> Result<u32, Self::Error>;
    fn dispose_error(&mut self, error: u32) -> Result<(), Self::Error>;
    fn release(&mut self, device: SoundDeviceIdentity) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SoundDeviceError<E> {
    #[error("enabled sound device requires its retained pointer at 00510339")]
    MissingDevice,
    #[error("required sound device service: {0}")]
    Host(E),
}

impl SoundDeviceRuntime {
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        executable.loader_zero_data(0x0065_f248, 9)?;
        Ok(Self {
            enabled: 0,
            device: None,
            configuration: 0,
        })
    }

    pub fn initialize<H: SoundDeviceHost>(
        &mut self,
        host: &mut H,
    ) -> Result<(), SoundDeviceError<H::Error>> {
        let result = host
            .create(&mut self.device)
            .map_err(SoundDeviceError::Host)?;
        let error = host.hresult_error(result).map_err(SoundDeviceError::Host)?;
        if let Some(error) = error {
            let error = host.report_error(error).map_err(SoundDeviceError::Host)?;
            host.dispose_error(error).map_err(SoundDeviceError::Host)?;
        } else {
            // Native AL is zero from the successful error conversion, before
            // the separate enabled-byte store. There is no once-only guard.
            self.configuration = 0;
            self.enabled = 1;
        }
        Ok(())
    }

    pub fn release<H: SoundDeviceHost>(
        &mut self,
        host: &mut H,
    ) -> Result<(), SoundDeviceError<H::Error>> {
        if self.enabled != 0 {
            let device = self.device.ok_or(SoundDeviceError::MissingDevice)?;
            host.release(device).map_err(SoundDeviceError::Host)?;
        }
        self.device = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
