//! Current Handler-1 DWORD acquisition, independent of its consumer's units.
//! World start interprets even/odd slots as position/heading; attract setup
//! captures slot0 as packed X/Z. The loader 0050ED90 advances four raw bytes.

use thiserror::Error;

use crate::{HandlerResources, ResidentRuntime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum H1WordError {
    #[error("H1 slot {slot} is outside its {capacity}-slot table")]
    CapacityBoundary { slot: u16, capacity: usize },
    #[error("H1 slot {slot} is not resident; canonical virtual+0 returns NULL")]
    NotResident { slot: u16 },
    #[error("H1 slot {slot} does not resolve to its registered word resource")]
    Registration { slot: u16 },
}

/// Authenticate the current allocation/registration and return its raw DWORD.
/// Signed slot arithmetic belongs to the caller. Capacity validation bounds
/// the typed domain; attract's literal slot0 lookup does not read a count.
pub(crate) fn resolve_h1_word(runtime: &ResidentRuntime, slot: u16) -> Result<u32, H1WordError> {
    let capacity = runtime.resource_capacity(1).unwrap_or(0);
    if usize::from(slot) >= capacity {
        return Err(H1WordError::CapacityBoundary { slot, capacity });
    }
    let resource = runtime
        .resource(1, slot)
        .ok_or(H1WordError::NotResident { slot })?;
    let Some(HandlerResources::Words(bank)) = resource
        .overlay
        .handlers()
        .get(resource.registration.region_index)
    else {
        return Err(H1WordError::Registration { slot });
    };
    Ok(bank
        .entries
        .get(usize::from(resource.registration.local_index))
        .filter(|_| bank.handler == 1)
        .ok_or(H1WordError::Registration { slot })?
        .raw)
}

#[cfg(test)]
mod tests;
