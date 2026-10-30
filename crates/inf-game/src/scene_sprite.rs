//! Data shared by direct native renderer virtual +9C/+A4 sprite quads.
//! Scene owners supply original screen WORDs, signed-depth DWORDs, descriptor
//! word and (for +A4) RGB word; resource decoding remains intrinsic.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectSpriteMode {
    /// +9C / 004D5410: ordinary texture; no effect-color DWORD is consumed.
    Textured,
    /// +A4 / 004D78A0: three low bytes at packet+28 supply R/G/B.
    Effect { color_word: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectSpriteQuad {
    pub sprite_slot: u16,
    pub corners: [[i16; 2]; 4],
    /// Native per-vertex DWORDs, interpreted as signed depth by the backend.
    pub vertex_depths: [u32; 4],
    /// Packet+20 selects the physical sprite and the native diagonal policy.
    pub descriptor_word: u32,
    pub mode: DirectSpriteMode,
}
