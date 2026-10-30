//! Native direct renderer +84 and +C4 packets. Resource/color policy belongs
//! to callers.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectFlatQuad {
    pub corners: [[i16; 2]; 4],
    pub vertex_depths: [u32; 4],
    pub descriptor_word: u32,
    pub color_word: u32,
}

/// Native direct renderer `+C4` packet (`004A3940` -> `004E6B70`): corners,
/// mode `+20`, H3 sprite `+24`, shade DWORDs `+28..37` and 16.16 UV pairs
/// `+38..57`. The near consumer never reads `+58..5F`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectShadeUvQuad {
    pub corners: [[i16; 2]; 4],
    pub vertex_depths: [u32; 4],
    pub mode: u32,
    pub sprite_argument: i32,
    pub shades: [u32; 4],
    pub uv: [[i32; 2]; 4],
}
