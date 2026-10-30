//! Selector 31's draw, 00485E60 (the laser's +2C). It poses the laser as the
//! cannon's shot does (00485F90), scales the aim by the laser's reach, moves
//! the context to the laser (0048FE60) and draws the beam (00485520): two
//! strips of sixteen quads from the muzzle along the aim, one jittered by
//! the RNG and one wound around the aim, each quad six pixels wide.

use inf_formats::native_basis::{isqrt_48efc0, sine_q15};
use thiserror::Error;

use super::super::derived::unit_48f080;
use crate::name_entry::name_entry_rng_next;
use crate::object_runtime::{
    ObjectArena, ObjectError, ObjectId, RibbonCorner, Selector69Projected, Selector69Quad,
    Selector69UnwrittenFog,
};

/// 00485520's sprites, H3 slot 0x24E, or 0x116 for a negative strength.
pub const LASER_TEXTURE_SLOT: i32 = 0x24e;
pub const LASER_NEGATIVE_TEXTURE_SLOT: i32 = 0x116;
/// 00485B7E and 00485D0D: the quads' packet mode.
pub const LASER_MODE: u32 = 4;

/// LaserWidthResidueApproximation: a segment the clip table rejects keeps
/// the widths the last drawn quad wrote ([EBP-2C] and [EBP-28]); before any
/// quad of the draw, those are stack words 00485520 never writes. In the
/// first person a beam's first segment starts behind the near plane, so
/// almost every draw reaches them: in `laser-fire-16ms-fp.json` 544 of 564
/// draws did, and the next quad widened its first corner by their low WORDs,
/// a long band towards the screen's edge. Retail's words were saved
/// registers of the muzzle lookup 0048CC00 (the vehicle, `0x286C3330`, and
/// `0x20031010`) or another heap pointer (`0x20031F50` in both): addresses
/// the port does not have. The port uses the widths each strip starts from.
pub const LASER_WIDTH_RESIDUE_APPROXIMATION: [i32; 2] = [6, 6];

#[derive(Debug, Error)]
pub enum Laser31DrawError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Fog(#[from] Selector69UnwrittenFog),
    #[error("laser {laser:?} draws an aim that no pose wrote (+138)")]
    UnknownAim { laser: ObjectId },
    #[error("laser draw host")]
    Host(E),
}

/// What 00485520 reads of the laser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Laser31Beam {
    pub aim: [i16; 3],
    /// +150.
    pub angle: u16,
    /// +144, which winds the second strip backwards when nonzero.
    pub group: u16,
    /// +146 below zero.
    pub negative: bool,
}

impl ObjectArena {
    pub fn is_laser31(&self, object: ObjectId) -> bool {
        self.shot32(object)
            .is_ok_and(|state| state.selector31.is_some())
    }

    /// 00485E68..00485F1C, after the pose: while the age +154 / 5 is below
    /// 0x7FFF the aim is scaled by it over 0x8000; after that, once the life
    /// +140 is below 0xFFFE, by half the life. In a network game a laser
    /// flagged 0x80000000 keeps its aim.
    pub fn reach_laser31(
        &mut self,
        laser: ObjectId,
        network_active: bool,
    ) -> Result<(), ObjectError> {
        if network_active && self.object(laser)?.base.flags & 0x8000_0000 != 0 {
            return Ok(());
        }
        let state = self.shot32_mut(laser)?;
        let (_, age) = state
            .selector31
            .ok_or(ObjectError::NotBehaviorResource(laser))?;
        let fifth = ((u64::from(age) * 0xcccc_cccd) >> 34) as u32;
        let scale = if fifth < 0x7fff {
            fifth
        } else if state.dword_140 & !1 < 0xfffe {
            state.dword_140 >> 1
        } else {
            return Ok(());
        };
        if let Some(aim) = &mut state.words_138 {
            *aim = aim.map(|word| (i32::from(word as i16).wrapping_mul(scale as i32) >> 15) as u16);
        }
        Ok(())
    }

    pub fn laser31_beam<E>(&self, laser: ObjectId) -> Result<Laser31Beam, Laser31DrawError<E>> {
        let state = self.shot32(laser)?;
        let (angle, _) = state
            .selector31
            .ok_or(ObjectError::NotBehaviorResource(laser))?;
        let aim = state
            .words_138
            .ok_or(Laser31DrawError::UnknownAim { laser })?;
        Ok(Laser31Beam {
            aim: aim.map(|word| word as i16),
            angle,
            group: state.word_144,
            negative: state.word_146.is_some_and(|strength| (strength as i16) < 0),
        })
    }
}

/// `(a * b) >> 31` of the signed product: `IMUL`, then `SHL EAX, 1; RCL EDX, 1`.
fn q31(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 31) as i32
}

/// Signed division by `1 << shift`, toward zero (`CDQ; AND EDX; ADD; SAR`).
fn divide(value: i32, shift: u32) -> i32 {
    value.wrapping_add((value >> 31) & ((1 << shift) - 1)) >> shift
}

/// 00485520 after its sprite lookup. `project` is the context's +50 at the
/// laser (local points), `clip` the table 00528BD0, and `quad` takes each
/// quad in order, its `depth` the previous point's, before material sorting.
pub fn laser31_draw<E>(
    beam: Laser31Beam,
    sine: &[u16; 4096],
    clip: &[u8; 256],
    rng: &mut u32,
    mut project: impl FnMut([i32; 3]) -> Result<Selector69Projected, E>,
    mut quad: impl FnMut(Selector69Quad) -> Result<(), E>,
) -> Result<(), Laser31DrawError<E>> {
    let aim = beam.aim.map(i32::from);
    let squared = aim[0]
        .wrapping_mul(aim[0])
        .wrapping_add(aim[1].wrapping_mul(aim[1]))
        .wrapping_add(aim[2].wrapping_mul(aim[2]));
    // 00485605: a sum past 31 bits is quartered (SHR) and its root doubled.
    let length = if squared < 0 {
        (isqrt_48efc0((squared as u32 >> 2) as i32) << 1) as i32
    } else {
        isqrt_48efc0(squared) as i32
    };
    let [nx, ny, nz] = unit_48f080(aim);
    let u = [
        q31(nz, nz).wrapping_sub(q31(nx, ny)),
        q31(nx, nx).wrapping_sub(q31(ny.wrapping_neg(), nz)),
        q31(ny.wrapping_neg(), ny).wrapping_sub(q31(nz, nx)),
    ];
    let v = [
        q31(u[1], nz).wrapping_sub(q31(u[2], ny)),
        q31(u[2], nx).wrapping_sub(q31(u[0], nz)),
        q31(u[0], ny).wrapping_sub(q31(u[1], nx)),
    ];
    let mut angle = u32::from(beam.angle);
    // [EBP-28] and [EBP-2C], the widths of the last drawn quad.
    let mut last: Option<[i32; 2]> = None;
    for pass in 0..2 {
        let mut previous = project([0; 3]).map_err(Laser31DrawError::Host)?;
        let mut previous_widths = Some([6, 6]);
        for k in 1..=16_i32 {
            let along = |axis: usize| divide(aim[axis].wrapping_mul(k), 4);
            let point = if pass == 1 {
                let radius = if k - 1 < 8 { k - 1 } else { 16 - k }.wrapping_mul(length);
                let radius = divide(radius, 7);
                let cosine = sine_q15(angle.wrapping_add(0x4000) as i32, sine);
                let sine = sine_q15(angle as i32, sine);
                let rc = (radius.wrapping_mul(cosine) >> 15) as i16 as i32;
                let rs = (radius.wrapping_mul(sine) >> 15) as i16 as i32;
                std::array::from_fn(|axis| {
                    let offset = q31(rc, u[axis]).wrapping_add(q31(rs, v[axis])) as i16;
                    along(axis).wrapping_add(i32::from(offset))
                })
            } else {
                // Z, then Y, then X: each the top four bits of a draw, less 8.
                let mut jitter = |axis: usize| {
                    ((name_entry_rng_next(rng) & 0xffff) >> 12) as i32 + along(axis) - 8
                };
                let z = jitter(2);
                let y = jitter(1);
                [jitter(0), y, z]
            };
            let current = project(point).map_err(Laser31DrawError::Host)?;
            let fog = (previous.fog, current.fog);
            let drawn = clip[usize::from(current.clip | previous.clip)] != 0
                && !matches!(fog, (Some(a), Some(b)) if a & b == 0xff);
            if drawn {
                let (Some(previous_fog), Some(current_fog)) = fog else {
                    return Err(Selector69UnwrittenFog.into());
                };
                let across = |xy: u32, shift: u32| i32::from((xy >> shift) as u16 as i16);
                let dy = (across(previous.xy, 16) - across(current.xy, 16)).abs();
                let dx = (across(previous.xy, 0) - across(current.xy, 0)).abs();
                let width_x = if dx == 0 { 6 } else { (dy * 6 / dx).min(6) };
                let width_y = if dy == 0 { 6 } else { (dx * 6 / dy).min(6) };
                let widened = |xy: u32, [x, y]: [i32; 2]| {
                    u32::from((xy as u16).wrapping_add(x as u16))
                        | (u32::from(((xy >> 16) as u16).wrapping_add(y as u16)) << 16)
                };
                let start = previous_widths.unwrap_or(LASER_WIDTH_RESIDUE_APPROXIMATION);
                let corner = |xy, depth| RibbonCorner { xy, depth };
                quad(Selector69Quad {
                    far: (previous_fog | current_fog) != 0,
                    depth: previous.depth as u32,
                    corners: [
                        corner(widened(previous.xy, start), previous.depth),
                        corner(previous.xy, previous.depth),
                        corner(current.xy, current.depth),
                        corner(widened(current.xy, [width_x, width_y]), current.depth),
                    ],
                    fog: [previous_fog, previous_fog, current_fog, current_fog],
                })
                .map_err(Laser31DrawError::Host)?;
                last = Some([width_x, width_y]);
            }
            previous_widths = last;
            previous = current;
            if pass == 1 {
                angle = angle.wrapping_add(if beam.group != 0 { 0x1800 } else { 0xe800 });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "laser31_tests.rs"]
mod tests;
