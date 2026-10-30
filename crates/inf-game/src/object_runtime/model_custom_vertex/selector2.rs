//! The player vehicle's virtual +10, 00415D30: exact selector 3 places a
//! wheel. The vertex row's point is lifted by 20 (50 in truck mode, whose
//! wheel reach scales by 300/182) along the hull's up axis, probed down to
//! the ground with 0043A380 over half the reach, and published at the
//! contact plus the full reach along up. While the dust intensity +228 is
//! above 100, a wheel on the ground or in the water rolls 0048F060 against
//! 40 times the intensity for one or two dust sprites (004894A0) or water
//! splashes (00489600).

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::object_runtime::{segment_probe_43a380, SegmentProbe};

static RESIDUE_INTENSITY_LOGGED: AtomicBool = AtomicBool::new(false);

/// MSVCRT __ftol at 00511BA8.
fn ftol(value: f64) -> i32 {
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    let truncated = value.trunc();
    if truncated.is_finite() && (-BOUND..BOUND).contains(&truncated) {
        truncated as i64 as i32
    } else {
        0
    }
}

impl ObjectArena {
    pub(super) fn selector2_wheel_vertex(
        &mut self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let source = request.source;
        let selector = request.compute.selector;
        let extra = |index: usize| {
            request.compute.extras.get(index).copied().ok_or(
                ObjectCustomVertexError::RequiredExtra {
                    object: source,
                    selector,
                    index,
                },
            )
        };
        let mut point = request.vertices.row_words_439e10(extra(0)?)?;
        let mut reach = i32::from(extra(1)?);
        let state = self.selector2_actor(source)?;
        let lift = if state.dwords_1e8_1f4[0] == 2 {
            reach = reach.wrapping_mul(300) / 182;
            50
        } else {
            20
        };
        let intensity = state.dword_228();
        let dust = state.drive.words_22c_236;
        let flags = self.object(source)?.base.flags;
        let basis = self.actor(source)?.rotating.basis;
        let up = [4, 5, 6].map(|index| f32::from_bits(basis.words[index]));
        for (word, axis) in point.iter_mut().zip(up) {
            *word = word.wrapping_sub(ftol(f64::from(lift) * f64::from(axis)) as u16);
        }
        let effects = request
            .effects
            .ok_or(ObjectCustomVertexError::RequiredEffects {
                object: source,
                selector,
            })?;
        let effect_error = |reason: String| ObjectCustomVertexError::Effects {
            object: source,
            reason,
        };
        let legs = effects.legs().map_err(effect_error)?;
        let ground = legs.services;
        let clock = ground.clock();
        let world_flags = ground.world_flags();
        // Port setting: the wheels reach down into the puddles' carve, as
        // the hull's samples do (drive.rs); retail's ground otherwise.
        let (probed, hit) = segment_probe_43a380(
            &mut crate::object_runtime::WheelGround(&mut *ground),
            SegmentProbe {
                input: point,
                direction: up,
                length: reach / 2,
                water: 0,
                iterations: 2,
                clock,
                world_flags,
            },
        )
        .map_err(|error| ObjectCustomVertexError::Legs {
            object: source,
            error,
        })?;
        point = probed;
        // ~(flags >> 30) & 1: the contact pass has not classified the hull
        // wholly above the water, so the wheel may be in it.
        let near_water = flags & 0x4000_0000 == 0;
        let mut wet = false;
        let mut roll = None;
        if ground.update_byte() == 0 {
            let intensity = match intensity {
                Some(intensity) => intensity as i32,
                // 004875A0 builds a record's vehicle (00492830) after the
                // actor walk, so it draws before its first +3C writes +228.
                None if flags & 0x8000_0000 != 0 => {
                    if !RESIDUE_INTENSITY_LOGGED.swap(true, Ordering::Relaxed) {
                        eprintln!("RemoteVehicleIntensityApproximation: a vehicle built from a record draws before its first 00420F40 writes +228; retail reads allocator residue, the port the fresh heap's zero (fresh_heap.rs)");
                    }
                    0
                }
                None => {
                    return Err(ObjectCustomVertexError::RequiredField {
                        object: source,
                        offset: 0x228,
                    })
                }
            };
            if intensity > 100 && (hit || near_water) {
                if near_water {
                    // 00415E78 forces 00410F60's wave gate with a literal 1.
                    let terrain = ground.height(point[0], point[2], clock);
                    let water = ground.water(point[0], point[2], clock, 0x40, terrain);
                    wet = (point[1] as i16) < water;
                }
                if hit || wet {
                    let threshold = intensity.wrapping_mul(40);
                    roll = Some((threshold, (ground.random() & 0xffff) as i32));
                }
            }
        }
        // A wet wheel splashes with a zero velocity; a dry one raises dust
        // with the drive's +22C velocity.
        let mut birth = |at: [u16; 3]| {
            if wet {
                return effects.birth_impact(at).map_err(effect_error);
            }
            let words = dust.ok_or(ObjectCustomVertexError::RequiredField {
                object: source,
                offset: 0x22c,
            })?;
            effects
                .birth_material_sprite(at, [words[0], words[1], words[2]])
                .map_err(effect_error)
        };
        if let Some((threshold, roll)) = roll {
            if roll < threshold {
                birth(point)?;
                if roll < threshold / 2 {
                    let words = dust.ok_or(ObjectCustomVertexError::RequiredField {
                        object: source,
                        offset: 0x232,
                    })?;
                    birth(std::array::from_fn(|axis| {
                        point[axis].wrapping_add(words[3 + axis])
                    }))?;
                }
            }
        }
        let output: [i32; 3] = std::array::from_fn(|axis| {
            let offset = ftol(f64::from(reach) * f64::from(up[axis])) as u16;
            i32::from(point[axis].wrapping_add(offset) as i16)
        });
        *request.output = Some(output);
        Ok(())
    }
}
