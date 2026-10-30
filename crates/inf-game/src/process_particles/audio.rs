//! Positional one-shot enqueue arguments shared by 00487F10/00487EC0.
//! Both wrappers resolve H9 and pass copied XYZ/gain/pitch to 0047FF60.

/// Actual native inputs; enqueue does not immediately resolve an alias or
/// play PCM. 00480110 later applies the current listener and services it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleSoundRequest {
    /// The H9 wrapper consumes the signed low WORD of this selector.
    pub sound: u32,
    /// Copied world WORDs, before any later particle movement or retirement.
    pub position: [u16; 3],
    pub volume: u32,
    pub pitch: u32,
    /// 0047FF60 consumes the low byte as its final native flag.
    pub flags: u32,
}
