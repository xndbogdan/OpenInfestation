//! Canonical H0 capture shared by grant and player-command consumers.
//! Notification, formatting and current process state belong to the caller.

use crate::{grant_text::GrantTextSource, HandlerResources, ResidentRuntime};

pub(crate) struct H0TextCaptureRequest<'a> {
    pub index_word: u16,
    pub resident: Option<&'a ResidentRuntime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum H0TextCaptureError {
    MissingResidency { index: u16 },
    OnePast { index: u16, count: usize },
    InvalidRegistration { index: u16 },
}

/// 0047513F..00475251 and 0046C7C4..0046C804 share this complete phase.
/// A captured occupied record borrows its actual allocation including NUL.
pub(crate) fn capture_h0_text<'a>(
    request: H0TextCaptureRequest<'a>,
) -> Result<GrantTextSource<'a>, H0TextCaptureError> {
    let index_word = request.index_word;
    if (index_word as i16) < 0 {
        return Ok(GrantTextSource::Null);
    }
    let resident = request
        .resident
        .ok_or(H0TextCaptureError::MissingResidency { index: index_word })?;
    let count = resident
        .resource_capacity(0)
        .expect("H0 is a validated resident handler");
    let index = usize::from(index_word);
    if index > count {
        return Ok(GrantTextSource::Null);
    }
    if index == count {
        return Err(H0TextCaptureError::OnePast {
            index: index_word,
            count,
        });
    }
    let Some(resource) = resident.resource(0, index_word) else {
        // Actual final H0 table 00527598 dispatches +0 to 004EFBF0:
        // xor EAX,EAX; RET4, without reading its operand or payload.
        return Ok(GrantTextSource::Null);
    };
    let invalid = || H0TextCaptureError::InvalidRegistration { index: index_word };
    let Some(HandlerResources::Strings(bank)) = resource
        .overlay
        .handlers()
        .get(resource.registration.region_index)
    else {
        return Err(invalid());
    };
    let record = bank
        .entries
        .get(usize::from(resource.registration.local_index))
        .and_then(|entry| entry.record.get(resource.overlay.bytes()))
        .ok_or_else(invalid)?;
    Ok(GrantTextSource::Record(Some(record)))
}
