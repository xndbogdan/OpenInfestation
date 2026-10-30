//! 004777A0, the Force Field's dome, which its +2C 00477370 draws while the
//! field is on, and 004774E0, one meridian of it.
//!
//! The dome has 0053B784 meridians (32 at detail +70 = 1, else 16), each
//! cut into 0053B780 segments (16 or 8) from its top, the field's height
//! above the centre, to its equator at the radius. Each point is projected
//! through the context's projection at the field (0048FE60); the first
//! point below the ground (the higher of H12 00450A30 and its water
//! 00410F60, at clock 0) + 50 and every point below it collapse onto that
//! level, and the equator always does. Between neighbouring meridians each
//! segment is one D6 quad, near (0044EA60) when no corner carries fog, else
//! far (0044EA70) unless all four are fully fogged, passed by the clip
//! table 00528BD0. Its corners are lit by the light 00477370 set (0055F26C
//! through 00490360) times a pulse of the clock and the fade +160, over
//! half the corner's depth + 2000. The bottom band takes sprite 0x2B4, the
//! rest 0x2B3, and one quad of the last strip's middle band 0x2B2.

use thiserror::Error;

use crate::model_primitives::{PrimitiveCallback, PrimitiveCommand};
use crate::render_resources::RenderResource;

/// 004777A0's sprites: the bottom band, the bands above it, and the middle
/// band's one quad in the last strip.
pub const FORCE_FIELD_BOTTOM_SPRITE: i32 = 0x2b4;
pub const FORCE_FIELD_BAND_SPRITE: i32 = 0x2b3;
pub const FORCE_FIELD_DECAL_SPRITE: i32 = 0x2b2;
/// 0047749D..004774A5: the grey of the light 00477370 sets.
pub const FORCE_FIELD_LIGHT_GREY: u8 = 0x20;
/// 004777A0's packets' +20.
pub const FORCE_FIELD_MODE: u32 = 4;

/// 00528BD0: the quads the OR of four clip codes admits.
const CLIP_GATE: [u8; 50] = [
    0x12, 0x13, 0x15, 0x16, 0x17, 0x1a, 0x1b, 0x1d, 0x1e, 0x1f, 0x2a, 0x2b, 0x2d, 0x2e, 0x2f, 0x32,
    0x33, 0x35, 0x36, 0x37, 0x3a, 0x3b, 0x3d, 0x3e, 0x3f, 0x92, 0x93, 0x95, 0x96, 0x97, 0x9a, 0x9b,
    0x9d, 0x9e, 0x9f, 0xaa, 0xab, 0xad, 0xae, 0xaf, 0xb2, 0xb3, 0xb5, 0xb6, 0xb7, 0xba, 0xbb, 0xbd,
    0xbe, 0xbf,
];

/// The field and process values 00477370 and 004777A0 read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForceFieldDome {
    /// +32/+34/+36.
    pub position: [u16; 3],
    /// +154 and +158.
    pub radius: u32,
    pub height: u32,
    /// +160.
    pub fade: i32,
    /// 0053B780 and 0053B784.
    pub segments: i32,
    pub meridians: i32,
    /// The animation clock 005471C0.
    pub clock: u32,
    /// 0055F26C, which 00490360 copies into each packet's +28: the light's
    /// grey over the ambient, its high byte never written.
    pub light: u32,
}

impl ForceFieldDome {
    /// 00477463..00477493: settings +70 = 1 draws 16 segments of 32
    /// meridians, anything else 8 of 16.
    pub fn rings(detail: u32) -> (i32, i32) {
        if detail == 1 {
            (16, 32)
        } else {
            (8, 16)
        }
    }
}

/// One point's projection entry: +0 XY, +4 depth, +8 clip, +9 fog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomePoint {
    pub xy: u32,
    pub depth: u32,
    pub clip: u8,
    /// Written only by the far dispatch, at a depth of 64 or more.
    pub fog: Option<u8>,
}

/// One packet's contents before its sort key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomeQuad {
    /// 0044EA70 when a corner carries fog, else 0044EA60.
    pub far: bool,
    /// The depth its sort key starts from: P[j]'s, but Q[j]'s for the
    /// second strip's near quads.
    pub depth: u32,
    pub sprite: i32,
    /// P[j], Q[j], Q[j+1], P[j+1] at +00..+1F.
    pub corners: [DomePoint; 4],
    /// The same corners' colours at +28, +2C, +30 and +34.
    pub colors: [[u8; 3]; 4],
    /// +3C..+3F of a far quad, in corner order.
    pub fog: [u8; 4],
}

impl DomeQuad {
    /// 00477B38..00477D97 (far) and their near twins: four blocks, the
    /// corners, mode 4 at +20, the sprite at +24, the light at +28
    /// (00490360) under the corners' colours, which leave each colour's
    /// fourth byte; a far quad adds the context's far colour at +38 and the
    /// fog bytes.
    pub fn command<'r>(
        &self,
        sort: u32,
        texture: Option<RenderResource<'r>>,
        light: u32,
        far_color: u32,
    ) -> PrimitiveCommand<'r> {
        let callback = if self.far {
            PrimitiveCallback::D6QuadFar
        } else {
            PrimitiveCallback::D6QuadNear
        };
        let mut command = PrimitiveCommand::new(callback, sort, 4);
        for (index, corner) in self.corners.iter().enumerate() {
            command.write_u32(index * 8, corner.xy);
            command.write_u32(index * 8 + 4, corner.depth);
        }
        command.write_u32(0x20, FORCE_FIELD_MODE);
        command.write_texture(0x24, texture);
        command.write_u32(0x28, light);
        for (index, color) in self.colors.iter().enumerate() {
            command.write_rgb(0x28 + index * 4, *color);
        }
        if self.far {
            command.write_u32(0x38, far_color);
            for (index, fog) in self.fog.iter().enumerate() {
                command.write_byte(0x3c + index, *fog);
            }
        }
        command
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ForceFieldDrawError<E> {
    /// The colour's IDIV by half a corner's depth + 2000 faults, which the
    /// draw's 00511C90 frame catches, ending the dome.
    #[error("force field dome corner at depth {0} divides by zero")]
    ZeroDivisor(i32),
    #[error("force field dome reads an unwritten fog byte")]
    UnwrittenFog,
    #[error("force field dome needs segments and meridians, not {0} and {1}")]
    Rings(i32, i32),
    #[error(transparent)]
    Host(E),
}

/// The quarter-sine WORD table 0053DF00 over a 16-bit angle: the low
/// fourteen bits fold at 0x4000, and 0x8000 negates.
fn sine(table: &[u16; 4096], angle: u32) -> i32 {
    let mut index = angle & 0x3ffc;
    if angle & 0x4000 != 0 {
        index = 0x3ffc - index;
    }
    let value = i32::from(table[(index >> 2) as usize]);
    if angle & 0x8000 != 0 {
        -value
    } else {
        value
    }
}

/// 004778B6..00477924: 400 times the sine over 0x8000 as a WORD, + 1600,
/// times the fade >> 10, over 488 by the compiler's magic multiply.
fn pulse(table: &[u16; 4096], fade: i32, angle: u32) -> i32 {
    let wave = i32::from((sine(table, angle).wrapping_mul(400) >> 15) as i16);
    let product = (wave + 0x640).wrapping_mul(fade >> 10);
    let quotient = ((i64::from(product) * 0x4325_c53f) >> 32) as i32 >> 7;
    quotient + ((quotient as u32) >> 31) as i32
}

/// 004774E0: the meridian at `theta`, from the equator (entry 0) to the top
/// (entry `segments`).
fn meridian<E>(
    dome: &ForceFieldDome,
    table: &[u16; 4096],
    theta: u32,
    ground: &mut impl FnMut(u16, u16) -> Result<i16, E>,
    project: &mut impl FnMut([i32; 3]) -> Result<DomePoint, E>,
) -> Result<Vec<DomePoint>, ForceFieldDrawError<E>> {
    let segments = dome.segments;
    let step = 0x4000 / segments;
    let radius = dome.radius as i32;
    let across = i32::from((radius.wrapping_mul(sine(table, theta)) >> 15) as i16);
    let along =
        i32::from((radius.wrapping_mul(sine(table, theta.wrapping_add(0x4000))) >> 15) as i16);
    let mut out: Vec<Option<DomePoint>> = vec![None; segments as usize + 1];
    let mut phi: u32 = 0x8000;
    let mut count = segments;
    let (mut x, mut z, mut floor);
    loop {
        let lean = sine(table, phi);
        x = across.wrapping_mul(lean) >> 15;
        z = along.wrapping_mul(lean) >> 15;
        let y = (dome.height as i32).wrapping_mul(sine(table, phi.wrapping_sub(0x4000))) >> 15;
        let height = ground(
            dome.position[0].wrapping_add(x as u16),
            dome.position[2].wrapping_add(z as u16),
        )
        .map_err(ForceFieldDrawError::Host)?;
        floor = i32::from(height) - i32::from(dome.position[1] as i16) + 50;
        if i32::from(y as i16) < floor {
            break;
        }
        out[count as usize] = Some(
            project([
                i32::from(x as i16),
                i32::from(y as i16),
                i32::from(z as i16),
            ])
            .map_err(ForceFieldDrawError::Host)?,
        );
        phi = phi.wrapping_sub(step as u32);
        count -= 1;
        if count < 0 {
            break;
        }
    }
    // 00477746..0047777B: the equator, and every point from the one below
    // the floor down, at the floor.
    let point = [i32::from(x as i16), floor, i32::from(z as i16)];
    out[0] = Some(project(point).map_err(ForceFieldDrawError::Host)?);
    for entry in (1..=count.max(0)).rev() {
        out[entry as usize] = Some(project(point).map_err(ForceFieldDrawError::Host)?);
    }
    Ok(out
        .into_iter()
        .map(|point| point.expect("every entry projected"))
        .collect())
}

/// `(channel * pulse) / (depth / 2 + 2000)`, its low byte.
fn shade<E>(light: u32, pulse: i32, depth: u32) -> Result<[u8; 3], ForceFieldDrawError<E>> {
    let divisor = (depth as i32) / 2 + 2000;
    if divisor == 0 {
        return Err(ForceFieldDrawError::ZeroDivisor(depth as i32));
    }
    let bytes = light.to_le_bytes();
    Ok(std::array::from_fn(|channel| {
        (i32::from(bytes[channel]).wrapping_mul(pulse) / divisor) as u8
    }))
}

/// One strip between meridians P and Q: 00477A10..00478056 (`first`) and
/// 00478140..0047878B.
#[allow(clippy::too_many_arguments)]
fn strip<E>(
    dome: &ForceFieldDome,
    p: &[DomePoint],
    q: &[DomePoint],
    pulse_p: i32,
    pulse_q: i32,
    first: bool,
    decal: bool,
    emit: &mut impl FnMut(DomeQuad) -> Result<(), E>,
) -> Result<(), ForceFieldDrawError<E>> {
    for j in 0..dome.segments as usize {
        let corners = [p[j], q[j], q[j + 1], p[j + 1]];
        let clip = corners.iter().fold(0, |codes, corner| codes | corner.clip);
        if !CLIP_GATE.contains(&clip) {
            continue;
        }
        let mut sprite = if j == 0 {
            FORCE_FIELD_BOTTOM_SPRITE
        } else {
            FORCE_FIELD_BAND_SPRITE
        };
        if first && j as i32 == dome.segments / 2 && decal {
            sprite = FORCE_FIELD_DECAL_SPRITE;
        }
        let mut fog = [0; 4];
        for (fog, corner) in fog.iter_mut().zip(&corners) {
            *fog = corner.fog.ok_or(ForceFieldDrawError::UnwrittenFog)?;
        }
        let far = fog.iter().any(|&fog| fog != 0);
        if far && fog.iter().all(|&fog| fog == 0xff) {
            continue;
        }
        let depth = if !first && !far {
            q[j].depth
        } else {
            p[j].depth
        };
        let colors = [
            shade(dome.light, pulse_p, corners[0].depth)?,
            shade(dome.light, pulse_q, corners[1].depth)?,
            shade(dome.light, pulse_q, corners[2].depth)?,
            shade(dome.light, pulse_p, corners[3].depth)?,
        ];
        emit(DomeQuad {
            far,
            depth,
            sprite,
            corners,
            colors,
            fog,
        })
        .map_err(ForceFieldDrawError::Host)?;
    }
    Ok(())
}

/// 004777A0 over `ground` (the higher of 00450A30 and 00410F60 at a point's
/// X/Z) and `project` (the context's projection of a local offset), emitting
/// each quad in the native order.
pub fn force_field_dome<E>(
    dome: &ForceFieldDome,
    table: &[u16; 4096],
    mut ground: impl FnMut(u16, u16) -> Result<i16, E>,
    mut project: impl FnMut([i32; 3]) -> Result<DomePoint, E>,
    mut emit: impl FnMut(DomeQuad) -> Result<(), E>,
) -> Result<(), ForceFieldDrawError<E>> {
    if dome.segments <= 0 || dome.meridians <= 0 {
        return Err(ForceFieldDrawError::Rings(dome.segments, dome.meridians));
    }
    let step = 0x10000 / dome.meridians;
    let base = dome.clock << 9;
    let mut p = meridian(dome, table, 0x10000, &mut ground, &mut project)?;
    let mut pulse_p = pulse(table, dome.fade, base);
    let mut a = 0x10000 - 2 * step;
    if a < 0 {
        return Ok(());
    }
    let mut b = a + step;
    loop {
        let q = meridian(dome, table, b as u32, &mut ground, &mut project)?;
        let pulse_q = pulse(table, dome.fade, base.wrapping_add(a as u32));
        strip(dome, &p, &q, pulse_p, pulse_q, true, a < step, &mut emit)?;
        p = meridian(dome, table, a as u32, &mut ground, &mut project)?;
        pulse_p = pulse(table, dome.fade, base.wrapping_add(a as u32));
        strip(dome, &p, &q, pulse_p, pulse_q, false, false, &mut emit)?;
        a -= 2 * step;
        b -= 2 * step;
        if a < 0 {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "selector18_draw_tests.rs"]
mod tests;
