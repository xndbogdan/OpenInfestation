//! The vehicle's +28, `00420080`: its headlamp, its rear glow and its pulse,
//! in that order, each a `0045B1A0` deposit on the ground.

use inf_formats::native_basis::isqrt_48efc0;

use super::{Flag0LightPassError, ObjectLightCallbackBoundary};
use crate::object_runtime::actor::native_trunc_low32 as ftol;
use crate::object_runtime::{ObjectArena, ObjectId, WorldCameraLookup};
use crate::terrain_runtime::TerrainRuntime;
use crate::world_lighting::TerrainLightRequest;

/// `00520A64` and `00520A2C`: the headlamp ray's step, 512 along the nose
/// (the basis Z row) and -128 along the up row (its Y row).
const RAY_FORWARD: f64 = 512.0;
const RAY_UP: f64 = -128.0;
/// `0042016C..004201BD`: the ray's samples are steps 2 to 16 from the hull.
const RAY_STEPS: u32 = 16;
/// `00546A38`, the headlamp's colour, which `00415610` sets to grey 0x60 at
/// startup (Infest03's headlamp read it so).
const HEADLAMP_RGB: [i8; 3] = [0x60, 0x60, 0x60];
/// `00520A60`: the rear glow sits 150 behind the hull, along its nose.
const REAR_OFFSET: f64 = -150.0;
/// `004203CC..004203FF`: the rear glow in gear 1, red, and in gear 0, amber.
const REAR_GEAR_1: (i32, [i8; 3]) = (0x15e, [0x14, -2, -2]);
const REAR_GEAR_0: (i32, [i8; 3]) = (0x190, [0x14, 0x14, 0x08]);
/// The flag the ECM Unit sets on its holder, whose pulse `0042041C` adds.
const PULSE_FLAG: u32 = crate::object_runtime::ECM_FLAG;
/// `004204F1`: the pulse's radius.
const PULSE_RADIUS: i32 = 0x400;
/// `0042046B`: the pulse's channels, `>> 12` (-8 to 7).
const PULSE_SHIFT: u32 = 12;

/// VehiclePulsePhaseApproximation: the pulse's phase `+588` before its
/// first light. The constructor `004156E0` leaves `+588` to allocator
/// residue, and `0042041C` only adds the world's delta to it; a fresh heap
/// holds zero. It only shifts where the colour's cycle starts.
pub const VEHICLE_PULSE_PHASE_APPROXIMATION: u32 = 0;

fn log_pulse_phase_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("VehiclePulsePhaseApproximation: 004156E0 leaves the ECM pulse's phase +588 to allocator residue; the port starts it at zero, which only shifts its colour cycle (VEHICLE_DRIVE_RUNTIME.md)");
    }
}

/// What `00420080` lit, in order (the headlamp's pool, the rear glow, then
/// the pulse in `lights`), and the arm the port stopped at, with the hull
/// they all shine from (signed), for the tracer's shadows. With the
/// headlights on or the rear glow lit, `lamp` is the vehicle the tracer's
/// lamps shine from, whether or not the pool found ground.
pub(super) struct VehicleLights {
    pub(super) headlamp: Option<TerrainLightRequest>,
    pub(super) rear: Option<TerrainLightRequest>,
    pub(super) lights: Vec<TerrainLightRequest>,
    pub(super) lamp: Option<VehicleLamp>,
    pub(super) boundary: Option<ObjectLightCallbackBoundary>,
    pub(super) hull: Option<[i32; 3]>,
}

/// A vehicle whose lamps the tracer lights:
/// its hull (signed world words), its basis (the counter, then the across,
/// up and nose rows), whether its headlights are on, and which of its rear
/// lamps are lit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleLamp {
    pub hull: [i32; 3],
    pub basis: [u32; 10],
    pub head: bool,
    pub rear: Option<RearLamps>,
}

/// The buggy's or truck's rear lamps lit as its rear glow is
/// (`0042038E..00420404`): the red pair in gear 1 (standing, braking or
/// coasting), the white in gear 0 (reversing); none driving forward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RearLamps {
    Red,
    White,
}

impl RearLamps {
    fn in_gear(gear: u32) -> Option<Self> {
        match gear {
            1 => Some(Self::Red),
            0 => Some(Self::White),
            _ => None,
        }
    }
}

/// A basis row component times `scale`, truncated (`__ftol`) to a WORD.
fn scaled(basis: &[u32; 10], index: usize, scale: f64) -> u16 {
    ftol(f64::from(f32::from_bits(basis[index])) * scale) as u16
}

/// `00420097..00420352`: the headlamp's ray steps from the hull by 512
/// along the nose and -128 along the up row (WORD sums). Its samples are
/// steps 2 to 16, until the ground (`00450A30`) stands above one; a ray
/// that never climbs or falls lights nothing. At a hit the point moves back
/// toward the first step by `(ground - z) / (z - first z)` of its distance,
/// in 1/32768ths (the native divides by the ray's Z, not its height, so the
/// move is almost always zero; equal Zs first move Z on by one). The light
/// sits at the point's X/Z with radius `3/4` of its distance from the first
/// step plus 512, and each grey channel becomes `channel * 500 / distance`,
/// saturated to a signed byte.
pub(super) fn headlamp_light(
    position: [u16; 3],
    basis: &[u32; 10],
    height_at: impl Fn([u16; 2]) -> i16,
) -> Option<TerrainLightRequest> {
    let step = [
        scaled(basis, 7, RAY_FORWARD).wrapping_add(scaled(basis, 4, RAY_UP)),
        scaled(basis, 8, RAY_FORWARD).wrapping_add(scaled(basis, 5, RAY_UP)),
        scaled(basis, 9, RAY_FORWARD).wrapping_add(scaled(basis, 6, RAY_UP)),
    ];
    if step[1] == 0 {
        return None;
    }
    let advance =
        |point: [u16; 3]| std::array::from_fn(|axis| point[axis].wrapping_add(step[axis]));
    let first: [u16; 3] = advance(position);
    let mut point = first;
    let mut ground = None;
    for _ in 1..RAY_STEPS {
        point = advance(point);
        let height = height_at([point[0], point[2]]);
        if height > point[1] as i16 {
            ground = Some(height);
            break;
        }
    }
    let ground = i32::from(ground?);
    if point[2] == first[2] {
        point[2] = point[2].wrapping_add(1);
    }
    let signed = |word: u16| i32::from(word as i16);
    let rise = signed(point[2]) - signed(first[2]);
    let fraction = (ground - signed(point[2])) / rise;
    let back = |delta: i32| (delta.wrapping_mul(fraction) >> 15) as u16;
    point[0] = point[0].wrapping_sub(back(signed(point[0]) - signed(first[0])));
    point[1] = point[1].wrapping_sub(back(signed(point[1]) - signed(first[1])));
    point[2] = point[2].wrapping_sub(back(rise));
    let distance: [i32; 3] =
        std::array::from_fn(|axis| signed(first[axis].wrapping_sub(point[axis])));
    let squared = distance.iter().fold(0i32, |sum, value| {
        sum.wrapping_add(value.wrapping_mul(*value))
    });
    let root = if squared as u32 & 0x8000_0000 != 0 {
        isqrt_48efc0((squared as u32 >> 2) as i32) << 1
    } else {
        isqrt_48efc0(squared)
    } as u16;
    let length = if root == 0 { 1 } else { i32::from(root as i16) };
    let channel = |colour: i8| {
        let value = i32::from(colour).wrapping_mul(500) / length;
        if value == i32::from(value as i8) {
            value as i8
        } else if value > 0 {
            i8::MAX
        } else {
            i8::MIN
        }
    };
    Some(TerrainLightRequest {
        world_xz: [signed(point[0]), signed(point[2])],
        radius: length.wrapping_mul(3) / 4 + 0x200,
        rgb: HEADLAMP_RGB.map(channel),
    })
}

/// `0042038E..00420404`: 150 behind the hull, along its nose, a red glow in
/// gear 1 or an amber one in gear 0; none in the boost's gear 2.
pub(super) fn rear_glow(
    position: [u16; 3],
    basis: &[u32; 10],
    gear: u32,
) -> Option<TerrainLightRequest> {
    let x = scaled(basis, 7, REAR_OFFSET).wrapping_add(position[0]);
    let z = scaled(basis, 9, REAR_OFFSET).wrapping_add(position[2]);
    let (radius, rgb) = match gear {
        1 => REAR_GEAR_1,
        0 => REAR_GEAR_0,
        _ => return None,
    };
    Some(TerrainLightRequest {
        world_xz: [i32::from(x as i16), i32::from(z as i16)],
        radius,
        rgb,
    })
}

impl ObjectArena {
    /// `00420080`, the vehicle's +28, which keeps flag 08000000. With its
    /// headlights on (+208) the headlamp lights the ground ahead; in the
    /// buggy or the truck (mode 1 or 2) the rear glow lights it behind,
    /// unless the camera 00478E00 finds is in first person (+188 = 2); with
    /// the ECM's flag 0x40000 a pulse follows (`0042041C`): its phase +588
    /// gains the world's delta and lights the ground at the hull's X/Z
    /// within 0x400 in three waves a third of a turn apart (`>> 12`). None
    /// for another class or a stopped construction, which the single-light
    /// callback reports.
    pub(super) fn vehicle_lights(
        &mut self,
        object: ObjectId,
        terrain: Option<&TerrainRuntime>,
        world_delta: u32,
        sine_quarter: &[u16; 4096],
    ) -> Result<Option<VehicleLights>, Flag0LightPassError> {
        if self.construction_is_stopped(object)? {
            return Ok(None);
        }
        let Ok(vehicle) = self.selector2_actor(object) else {
            return Ok(None);
        };
        let (headlights, mode, gear, pulse) = (
            vehicle.byte_208 != 0,
            vehicle.dwords_1e8_1f4[0],
            vehicle.dword_238,
            vehicle.pulse_588,
        );
        let basis = self.actor(object)?.rotating.basis.words;
        let flags = self.object(object)?.base.flags;
        let position = self.object(object)?.base.position_words;
        let mut lights = Vec::new();
        let hull = position.map(|words| words.map(|word| i32::from(word as i16)));
        let mut headlamp = None;
        let mut rear = None;
        let mut lamp = None;
        let stop = |headlamp, rear, lamp, lights, reason| {
            Ok(Some(VehicleLights {
                headlamp,
                rear,
                lights,
                lamp,
                boundary: Some(reason),
                hull,
            }))
        };
        if headlights {
            let (Some(position), Some(terrain)) = (position, terrain) else {
                let reason = if position.is_none() {
                    ObjectLightCallbackBoundary::Position
                } else {
                    ObjectLightCallbackBoundary::HeadlampTerrain
                };
                return stop(headlamp, rear, lamp, lights, reason);
            };
            headlamp = headlamp_light(position, &basis, |xz| terrain.height_at(xz));
            lamp = hull.map(|hull| VehicleLamp {
                hull,
                basis,
                head: true,
                rear: None,
            });
        }
        if matches!(mode, 1 | 2) {
            match self.world_camera_lookup()? {
                WorldCameraLookup::Found(camera) => {
                    if self.selector16(camera)?.mode_188 != 2 {
                        let Some(position) = position else {
                            return stop(
                                headlamp,
                                rear,
                                lamp,
                                lights,
                                ObjectLightCallbackBoundary::Position,
                            );
                        };
                        rear = rear_glow(position, &basis, gear);
                        if let (Some(pair), Some(hull)) = (RearLamps::in_gear(gear), hull) {
                            lamp.get_or_insert(VehicleLamp {
                                hull,
                                basis,
                                head: false,
                                rear: None,
                            })
                            .rear = Some(pair);
                        }
                    }
                }
                WorldCameraLookup::NoCamera => {}
                WorldCameraLookup::NeedsCamera => {
                    return stop(
                        headlamp,
                        rear,
                        lamp,
                        lights,
                        ObjectLightCallbackBoundary::VehicleCamera,
                    );
                }
            }
        }
        if flags & PULSE_FLAG != 0 {
            let phase = pulse
                .unwrap_or_else(|| {
                    log_pulse_phase_approximation();
                    VEHICLE_PULSE_PHASE_APPROXIMATION
                })
                .wrapping_add(world_delta);
            if let Ok(vehicle) = self.selector2_actor_mut(object) {
                vehicle.pulse_588 = Some(phase);
            }
            let Some([x, _, z]) = position else {
                return stop(
                    headlamp,
                    rear,
                    lamp,
                    lights,
                    ObjectLightCallbackBoundary::Position,
                );
            };
            lights.push(TerrainLightRequest {
                world_xz: [i32::from(x as i16), i32::from(z as i16)],
                radius: PULSE_RADIUS,
                rgb: crate::object_runtime::actor::wave_colour(sine_quarter, phase, PULSE_SHIFT),
            });
        }
        Ok(Some(VehicleLights {
            headlamp,
            rear,
            lights,
            lamp,
            boundary: None,
            hull,
        }))
    }
}

#[cfg(test)]
#[path = "vehicle_tests.rs"]
mod tests;
