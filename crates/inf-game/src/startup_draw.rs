//! Startup (language) carousel draw `004330E0` and presentation `00433620`.
//!
//! After the `00439270` preamble the draw returns while the age is at most
//! `00546B20`. Otherwise it resets the batch (`004A4400`) and the depth
//! surface (renderer `+14(0)`), refreshes the shadow colour `+1A4` and the
//! clear colour `+1A8`, then draws six flags on an ellipse: for each, a
//! solid shadow strip (`00432940`, renderer `+84`) and a waving textured
//! cloth strip (`00432DB0`, renderer `+C4`, sprite H3 `0x4A0 + index`). The
//! selected language's H0 string is centred at nine tenths of the height
//! through `004A3AB0`. Presentation `00433620` presents and clears the next
//! colour surface with `+1A8` through `00495810`.
//!
//! The vertex helpers `00432800` and `00432B60` write only the X/Y words of a
//! corner; its depth DWORD is whatever the stack held. Infest01 submitted
//! `0050DCB9`/`1` for shadows and `038286C0`/`038320F0` for cloths, which
//! with the renderer's saved depth policy (enable and write 1) leave the
//! retail occlusion between a shadow and its cloth unknown. The port
//! submits one far depth for every carousel corner
//! (`StartupCarouselDepthResidueApproximation`), so submission order decides
//! overlap. Geometry, colours, shades and UVs follow the native packets.

use thiserror::Error;

use crate::menu::MenuColor;
use crate::menu_text::{
    measure_text_width, LineTextRequest, MenuTextLayoutError, MenuTextResources,
};
use crate::render_resources::{RenderResourceError, RenderResources};
use crate::scene_quad::{DirectFlatQuad, DirectShadeUvQuad};
use crate::startup_screen::{
    StartupState, STARTUP_BACKGROUND_FADE_MICROS, STARTUP_CLEAR_MATERIAL, STARTUP_DELAY_MICROS,
    STARTUP_FADE_IN_MICROS, STARTUP_LANGUAGES,
};

/// The depth DWORD the port submits for every carousel corner, in place of
/// the stack residue the native helpers leave. See the module comment.
pub const STARTUP_CAROUSEL_DEPTH_APPROXIMATION: u32 = 0x7fff_ffff;

/// H7 slot of the shadow colour `+1A4`.
pub const STARTUP_SHADOW_MATERIAL: i32 = 0x22;
/// First flag sprite: H3 `0x4A0 + index`.
pub const STARTUP_FLAG_SPRITE_BASE: i32 = 0x4a0;
/// First cloth shade style: H6 `0x10 + n`, `n` clamped to 0..16.
pub const STARTUP_SHADE_STYLE_BASE: i32 = 0x10;
/// H6 style of the language label.
pub const STARTUP_LABEL_STYLE: i32 = 0x2a;
/// `00432E1E`: the cloth packet's mode word (affine texturing).
pub const STARTUP_CLOTH_MODE: u32 = 0x100;

/// Read-only inputs of one draw besides the screen words.
#[derive(Clone, Copy, Debug)]
pub struct StartupDrawFrame<'a> {
    /// Surface `+0/+4` from `00495900`/`00495910`, in pixels.
    pub surface_pixels: [i32; 2],
    /// Shared `00546F20/24`.
    pub focal: [i32; 2],
    /// Display `+24/+28/+2C` (red, green, blue), as used by the fades.
    pub material_shifts: [u32; 3],
    /// Shared glyph base word `0055F8F8`; descriptor `+1C` is zero.
    pub glyph_base_color: MenuColor,
    /// Table `0053DF00`.
    pub sine_quarter: &'a [u16; 4096],
}

pub trait StartupDrawResources: MenuTextResources {
    /// The converted resident H7 DWORD, as the pool slot holds it.
    fn material_word(&self, argument: i32) -> Result<u32, Self::Error>;
}

/// The resident pools through their registrations; materials convert as
/// the pool slots hold them (`crate::frontend_display`).
#[derive(Clone, Copy)]
pub struct StartupResidentResources<'r>(pub RenderResources<'r>);

impl MenuTextResources for StartupResidentResources<'_> {
    type Error = RenderResourceError;
    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        self.0.string(argument)
    }
    fn font(&self, argument: i32) -> Result<&inf_formats::Font, Self::Error> {
        self.0.font(argument)
    }
    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        self.0.style(argument)
    }
    fn sprite_dimensions(&self, argument: i32) -> Result<[u16; 2], Self::Error> {
        self.0.sprite_dimensions(argument)
    }
}

impl StartupDrawResources for StartupResidentResources<'_> {
    fn material_word(&self, argument: i32) -> Result<u32, Self::Error> {
        crate::frontend_display::resident_material_word(self.0, argument)
    }
}

/// Renderer operations reached after the age gate, in native order.
pub trait StartupDrawHost {
    type Error;
    /// `004A4400` then renderer `+14(0)`: batch reset, depth clear, ZENABLE 0.
    fn begin_carousel(&mut self) -> Result<(), Self::Error>;
    /// Renderer `+84`.
    fn shadow_quad(&mut self, quad: DirectFlatQuad) -> Result<(), Self::Error>;
    /// Renderer `+C4`.
    fn cloth_quad(&mut self, quad: DirectShadeUvQuad) -> Result<(), Self::Error>;
    /// `004A3AB0`.
    fn line_text(&mut self, request: &mut LineTextRequest<'_>) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupDrawOutcome {
    /// The age was at most `00546B20`: only the preamble ran.
    Delayed,
    Drawn,
}

#[derive(Debug, Error)]
pub enum StartupDrawError<R, H> {
    #[error("startup draw resource: {0}")]
    Resource(R),
    #[error("startup draw backend: {0}")]
    Host(H),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
}

/// Execute `004330E0` after its preamble. Failures preserve the reached
/// submissions and the colour words already written.
pub fn draw_startup_carousel<R: StartupDrawResources, H: StartupDrawHost>(
    frame: StartupDrawFrame<'_>,
    state: &mut StartupState,
    resources: &R,
    host: &mut H,
) -> Result<StartupDrawOutcome, StartupDrawError<R::Error, H::Error>> {
    if state.age <= STARTUP_DELAY_MICROS {
        return Ok(StartupDrawOutcome::Delayed);
    }
    host.begin_carousel().map_err(StartupDrawError::Host)?;
    let age = state.age;
    let shadow = resources
        .material_word(STARTUP_SHADOW_MATERIAL)
        .map_err(StartupDrawError::Resource)?;
    state.shadow_color = shadow;
    if age < STARTUP_DELAY_MICROS + STARTUP_FADE_IN_MICROS {
        state.shadow_color = scale_display_word(
            shadow,
            frame.material_shifts,
            age - STARTUP_DELAY_MICROS,
            STARTUP_FADE_IN_MICROS,
        );
    }
    state.clear_color = if age > STARTUP_DELAY_MICROS + STARTUP_FADE_IN_MICROS {
        let base = background_word(frame.material_shifts);
        if age >= STARTUP_BACKGROUND_FADE_MICROS + STARTUP_FADE_IN_MICROS + STARTUP_DELAY_MICROS {
            base
        } else {
            scale_display_word(
                base,
                frame.material_shifts,
                age - STARTUP_FADE_IN_MICROS - STARTUP_DELAY_MICROS,
                STARTUP_BACKGROUND_FADE_MICROS,
            )
        }
    } else {
        resources
            .material_word(STARTUP_CLEAR_MATERIAL)
            .map_err(StartupDrawError::Resource)?
    };
    let mut flags = StartupFlags {
        frame,
        age,
        shadow_color: state.shadow_color,
        resources,
        host,
    };
    for index in 0..STARTUP_LANGUAGES {
        let placement = flag_placement(state, index, frame.sine_quarter);
        flags.shadow_strip(placement)?;
        flags.cloth_strip(placement, index)?;
    }
    let font = resources.font(0).map_err(StartupDrawError::Resource)?;
    let style = resources
        .style(STARTUP_LABEL_STYLE)
        .map_err(StartupDrawError::Resource)?;
    let color = if age < STARTUP_DELAY_MICROS + STARTUP_FADE_IN_MICROS {
        scale_low_bytes(style, age - STARTUP_DELAY_MICROS, STARTUP_FADE_IN_MICROS)
    } else {
        style
    };
    let [width, height] = frame.surface_pixels;
    let x = width / 2;
    let y = height.wrapping_mul(9) / 10;
    // 004335C5: the 16-bit selection indexes H0 directly.
    let text = resources
        .string(i32::from(state.selection as i16))
        .map_err(StartupDrawError::Resource)?;
    let measured = i32::from(measure_text_width(font, text)? as i16);
    host.line_text(&mut LineTextRequest {
        text,
        font,
        position_pixels: [x.wrapping_add((measured / 2).wrapping_neg()), y],
        color: MenuColor::from_word(color),
        base_color: frame.glyph_base_color,
        context_origin_pixels: [0, 0],
    })
    .map_err(StartupDrawError::Host)?;
    Ok(StartupDrawOutcome::Drawn)
}

/// One flag's ellipse position and its `0..255` closeness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlagPlacement {
    pub x: i32,
    pub y: i32,
    pub depth: i32,
}

/// `0043334A..00433494`: radii 300/220 around (500, 280), grown over the
/// fade-in together with an extra angle; index `selection` sits in front.
pub fn flag_placement(
    state: &StartupState,
    index: i32,
    sine_quarter: &[u16; 4096],
) -> FlagPlacement {
    let mut angle = state.angle;
    let mut radius_x = 300;
    let mut radius_y = 220;
    let since = state.age.wrapping_sub(STARTUP_DELAY_MICROS);
    if state.age < STARTUP_DELAY_MICROS + STARTUP_FADE_IN_MICROS {
        radius_x = since.wrapping_mul(300) / STARTUP_FADE_IN_MICROS;
        radius_y = since.wrapping_mul(220) / STARTUP_FADE_IN_MICROS;
        angle = angle
            .wrapping_add((since / 10_000).wrapping_shl(16) / (STARTUP_FADE_IN_MICROS / 10_000));
    }
    let phase = (index
        .wrapping_sub(state.selection)
        .wrapping_add(STARTUP_LANGUAGES)
        .wrapping_shl(16)
        / STARTUP_LANGUAGES)
        .wrapping_add(angle)
        & 0xffff;
    let x = i32::from((table_sine(sine_quarter, phase).wrapping_mul(radius_x) >> 15) as i16) + 500;
    let y = i32::from(
        (table_sine(sine_quarter, phase.wrapping_add(0x4000)).wrapping_mul(radius_y) >> 15) as i16,
    ) + 0x118;
    let closeness = if phase < 0x8000 {
        0x7fff - phase
    } else {
        phase - 0x8000
    };
    FlagPlacement {
        x,
        y,
        depth: closeness >> 7,
    }
}

/// Native quarter-wave table read: bit 14 mirrors, bit 15 negates.
pub fn table_sine(sine_quarter: &[u16; 4096], phase: i32) -> i32 {
    let mut index = phase & 0x3ffc;
    if phase & 0x4000 != 0 {
        index = 0x3ffc - index;
    }
    let value = i32::from(sine_quarter[(index >> 2) as usize]);
    if phase & 0x8000 != 0 {
        value.wrapping_neg()
    } else {
        value
    }
}

/// `00433229..00433251`: blue `F8`, red and green `40` in display format.
pub fn background_word(shifts: [u32; 3]) -> u32 {
    let [red, green, blue] = shifts;
    (0xf8_u32 >> (blue.wrapping_sub(1) & 31))
        | (0x40_u32 << (green.wrapping_add(1) & 31))
        | (0x40_u32 << (red.wrapping_add(1) & 31))
}

/// The draw's per-channel fade of a display word: each channel's top five
/// bits, as a byte, times `numerator / denominator`, masked back to `F8`.
pub fn scale_display_word(word: u32, shifts: [u32; 3], numerator: i32, denominator: i32) -> u32 {
    let [red, green, blue] = shifts;
    let scale =
        |channel: u32| ((channel as i32).wrapping_mul(numerator) / denominator) as u32 & 0xf8;
    let blue_shift = blue.wrapping_sub(1) & 31;
    let blue_byte = u32::from(((word as u8) << (blue_shift & 7)) & 0xf8);
    let blue_byte = if blue_shift >= 8 { 0 } else { blue_byte };
    let red_shift = red.wrapping_add(1) & 31;
    let green_shift = green.wrapping_add(1) & 31;
    (scale(blue_byte) >> blue_shift)
        | (scale((word >> red_shift) & 0xf8) << red_shift)
        | (scale((word >> green_shift) & 0xf8) << green_shift)
}

/// `00432D3B..00432D86` and `0043353B..0043356E`: the low three bytes of a
/// style word times `numerator / denominator`; the high byte is kept.
pub fn scale_low_bytes(word: u32, numerator: i32, denominator: i32) -> u32 {
    let mut bytes = word.to_le_bytes();
    for byte in &mut bytes[..3] {
        *byte = (i32::from(*byte).wrapping_mul(numerator) / denominator) as u8;
    }
    u32::from_le_bytes(bytes)
}

/// The corner sequence shared by `00432940` and `00432DB0`: edge A walks the
/// top then the right side, edge B the left then the bottom, both in 16.16
/// fractions of the flag; each step emits the quad between the two edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StripWalker {
    pub a_state: u8,
    pub a_u: i32,
    pub a_v: i32,
    pub b_state: u8,
    pub b_u: i32,
    pub b_v: i32,
}

impl StripWalker {
    /// `004329CB..00432AAB` (registers differ in `00432DB0`, not effects).
    pub fn step(&mut self) {
        let mut step: i32 = 0x800;
        match self.a_state {
            0 => {
                let next = self.a_u.wrapping_add(0x3a1);
                if next > 0x10000 {
                    // 00432A29: the signed magic 0xB97F7E03 >> 13 divide.
                    let high = ((i64::from(self.a_u.wrapping_shl(16))
                        * i64::from(0xb97f_7e03_u32 as i32))
                        >> 32) as i32;
                    let quotient = high >> 13;
                    step = quotient.wrapping_add((quotient as u32 >> 31) as i32);
                    self.a_state = 1;
                    self.a_u = 0x10000;
                } else {
                    self.a_u = next;
                }
            }
            1 => {
                let next = self.a_v.wrapping_add(0x1000);
                if next > 0x10000 {
                    step = (self.a_v.wrapping_shl(16) / 0x20000).wrapping_neg();
                    self.b_u = 0x10000;
                    self.a_state = 2;
                    self.a_v = 0x10000;
                    self.b_v = 0x10000;
                    self.b_state = 2;
                } else {
                    self.a_v = next;
                }
            }
            _ => {}
        }
        match self.b_state {
            0 => {
                let next = (step.wrapping_shl(17) >> 16).wrapping_add(self.b_v);
                if next > 0x10000 {
                    self.b_v = 0x10000;
                    self.b_state = 1;
                } else {
                    self.b_v = next;
                }
            }
            1 => {
                let next = self.b_u.wrapping_add(step.wrapping_mul(0x7432) >> 16);
                if next > 0x10000 {
                    self.b_u = 0x10000;
                    self.a_state = 2;
                    self.a_u = 0x10000;
                    self.a_v = 0x10000;
                    self.b_state = 2;
                } else {
                    self.b_u = next;
                }
            }
            _ => {}
        }
    }

    pub fn edge_a(&self) -> [i32; 2] {
        [self.a_u >> 8, self.a_v >> 8]
    }
    pub fn edge_b(&self) -> [i32; 2] {
        [self.b_u >> 8, self.b_v >> 8]
    }
}

/// Wave amplitude selector: zero behind the middle, up to 5 in front.
fn wave_weight(depth: i32) -> i32 {
    if depth < 0x80 {
        0
    } else {
        (depth.wrapping_mul(3) - 0x180).wrapping_shl(1) >> 7
    }
}

fn wave_phase(age: i32, [u, v]: [i32; 2]) -> i32 {
    u.wrapping_mul(0x234)
        .wrapping_sub(age >> 4)
        .wrapping_add(v.wrapping_shl(7))
}

/// `00432800`: a shadow corner, pushed right and down by the flag height
/// above the ground and bent by the same wave as the cloth.
pub fn shadow_corner(
    age: i32,
    placement: FlagPlacement,
    point: [i32; 2],
    focal: [i32; 2],
    sine_quarter: &[u16; 4096],
) -> [i16; 2] {
    let [u, v] = point;
    let weight = wave_weight(placement.depth);
    let z = placement.depth + 0x100;
    let sx = ((((u.wrapping_mul(0xa0) >> 8) - 0x50).wrapping_mul(z) >> 9) + placement.x)
        .wrapping_mul(focal[0])
        >> 9;
    let vp = (v.wrapping_mul(0x68) >> 8) - 0x34;
    let lift = (0x86 - vp).wrapping_mul(z) >> 9;
    let sy = ((lift * 2) / 3 + placement.y + (vp.wrapping_mul(z) >> 9)).wrapping_mul(focal[1]) >> 9;
    let sine = table_sine(sine_quarter, wave_phase(age, point));
    let wave = ((u.wrapping_mul(u).wrapping_mul(10) >> 11)
        .wrapping_mul(weight + 2)
        .wrapping_mul(sine))
        >> 0x17;
    let y = sy.wrapping_sub(wave) as i16;
    let x = (((lift / 4).wrapping_mul(focal[0]) >> 9).wrapping_add(sx)) as i16;
    [x, y]
}

/// A cloth corner from `00432B60`, before its shade style is resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClothCorner {
    pub xy: [i16; 2],
    /// H6 style `0x10 + shade_step`.
    pub shade_step: i32,
    pub uv: [i32; 2],
}

/// `00432B60`: the waving cloth corner and its lighting step.
pub fn cloth_corner(
    age: i32,
    placement: FlagPlacement,
    point: [i32; 2],
    focal: [i32; 2],
    sine_quarter: &[u16; 4096],
) -> ClothCorner {
    let [u, v] = point;
    let weight = wave_weight(placement.depth);
    let z = placement.depth + 0x100;
    let sx = ((((u.wrapping_mul(0xa0) >> 8) - 0x50).wrapping_mul(z) >> 9) + placement.x)
        .wrapping_mul(focal[0])
        >> 9;
    let sy = ((((v.wrapping_mul(0x68) >> 8) - 0x34).wrapping_mul(z) >> 9) + placement.y)
        .wrapping_mul(focal[1])
        >> 9;
    let phase = wave_phase(age, point);
    let sine = table_sine(sine_quarter, phase);
    let squared = u.wrapping_mul(u);
    let wave = ((squared.wrapping_mul(10) >> 11)
        .wrapping_mul(weight + 2)
        .wrapping_mul(sine))
        >> 0x17;
    let y = sy.wrapping_sub(wave) as i16;
    let x = ((wave * 3) / 4).wrapping_add(sx) as i16;
    let cosine = table_sine(sine_quarter, phase.wrapping_add(0x4000));
    let light = (squared.wrapping_mul(12) >> 11)
        .wrapping_mul(weight + 2)
        .wrapping_mul(cosine)
        >> 0x16;
    ClothCorner {
        xy: [x, y],
        shade_step: (weight + 4).wrapping_add(light).clamp(0, 16),
        uv: [u.wrapping_shl(8), v.wrapping_shl(8)],
    }
}

struct StartupFlags<'a, 'f, R, H> {
    frame: StartupDrawFrame<'f>,
    age: i32,
    shadow_color: u32,
    resources: &'a R,
    host: &'a mut H,
}

impl<R: StartupDrawResources, H: StartupDrawHost> StartupFlags<'_, '_, R, H> {
    /// `00432940`.
    fn shadow_strip(
        &mut self,
        placement: FlagPlacement,
    ) -> Result<(), StartupDrawError<R::Error, H::Error>> {
        let corner = |point| {
            shadow_corner(
                self.age,
                placement,
                point,
                self.frame.focal,
                self.frame.sine_quarter,
            )
        };
        let mut walker = StripWalker::default();
        let mut a = corner([0, 0]);
        let mut b = corner([0, 0]);
        loop {
            if walker.b_state == 2 {
                return Ok(());
            }
            walker.step();
            let (previous_a, previous_b) = (a, b);
            a = corner(walker.edge_a());
            b = corner(walker.edge_b());
            self.host
                .shadow_quad(DirectFlatQuad {
                    corners: [previous_a, a, b, previous_b],
                    vertex_depths: [STARTUP_CAROUSEL_DEPTH_APPROXIMATION; 4],
                    descriptor_word: 0,
                    color_word: self.shadow_color,
                })
                .map_err(StartupDrawError::Host)?;
            if walker.a_state == 2 {
                return Ok(());
            }
        }
    }

    /// `00432DB0`. The sprite's +14/+16 size is read and passed on to
    /// `00432B60`, which never uses it.
    fn cloth_strip(
        &mut self,
        placement: FlagPlacement,
        index: i32,
    ) -> Result<(), StartupDrawError<R::Error, H::Error>> {
        let sprite_argument = i32::from((STARTUP_FLAG_SPRITE_BASE + index) as i16);
        self.resources
            .sprite_dimensions(sprite_argument)
            .map_err(StartupDrawError::Resource)?;
        let mut walker = StripWalker::default();
        let mut a = self.cloth_vertex(placement, [0, 0])?;
        let mut b = self.cloth_vertex(placement, [0, 0])?;
        loop {
            if walker.b_state == 2 {
                return Ok(());
            }
            walker.step();
            let (previous_a, previous_b) = (a, b);
            a = self.cloth_vertex(placement, walker.edge_a())?;
            b = self.cloth_vertex(placement, walker.edge_b())?;
            let corners = [previous_a, a, b, previous_b];
            self.host
                .cloth_quad(DirectShadeUvQuad {
                    corners: corners.map(|corner| corner.0.xy),
                    vertex_depths: [STARTUP_CAROUSEL_DEPTH_APPROXIMATION; 4],
                    mode: STARTUP_CLOTH_MODE,
                    sprite_argument,
                    shades: corners.map(|corner| corner.1),
                    uv: corners.map(|corner| corner.0.uv),
                })
                .map_err(StartupDrawError::Host)?;
            if walker.a_state == 2 {
                return Ok(());
            }
        }
    }

    fn cloth_vertex(
        &mut self,
        placement: FlagPlacement,
        point: [i32; 2],
    ) -> Result<(ClothCorner, u32), StartupDrawError<R::Error, H::Error>> {
        let corner = cloth_corner(
            self.age,
            placement,
            point,
            self.frame.focal,
            self.frame.sine_quarter,
        );
        let style = self
            .resources
            .style(STARTUP_SHADE_STYLE_BASE + corner.shade_step)
            .map_err(StartupDrawError::Resource)?;
        let shade = if self.age < STARTUP_DELAY_MICROS + STARTUP_FADE_IN_MICROS {
            scale_low_bytes(
                style,
                self.age - STARTUP_DELAY_MICROS,
                STARTUP_FADE_IN_MICROS,
            )
        } else {
            style
        };
        Ok((corner, shade))
    }
}

pub(crate) mod software;

#[cfg(test)]
#[path = "startup_draw_tests.rs"]
mod tests;
