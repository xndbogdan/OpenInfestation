//! 00407760 controls followed by407D59's direct shot admission. The aim WORDs
//! are distinct from the saved eye origin192 and from the actor's currentXYZ.
use super::*;
use crate::object_runtime::actor::behavior::read_row_word;
use crate::object_runtime::BehaviorRowSelection;
use inf_formats::native_basis::{
    inverse_sine_q31, normalize_words, q31_divide, NativeBasisMath, NativeLengthPrecision,
    RotationPlane,
};

impl ObjectArena {
    pub(super) fn selector77_target_update<H: Selector77UpdateHost>(
        &mut self,
        actor: ObjectId,
        request: &Selector77TickRequest<'_>,
        host: &mut H,
    ) -> Result<(), Selector77UpdateError<H::Error>> {
        let (world, aligned) = self.selector77_target_controls(actor, request, host)?;
        if !aligned {
            return Ok(());
        }
        let Some(behavior) = self.derived_actor(actor)?.behavior_140.as_ref() else {
            return Ok(());
        };
        let index = i32::from(behavior.byte_10 as i8);
        let entry =
            behavior
                .entries_18
                .get(index as usize)
                .ok_or(ActorWeaponFireError::EntryBacking {
                    actor,
                    offset: index.wrapping_mul(32).wrapping_add(0x14),
                    entries: behavior.entries_18.len(),
                })?;
        if entry[5] != 0 || entry[0] == 0 || entry[7] >= 6_000_000 {
            return Ok(());
        }
        let direction_bits =
            normalize_words(world, NativeLengthPrecision::Spilled).map(f32::to_bits);
        let random = host
            .random()
            .map_err(|error| Selector77UpdateError::Random {
                address: 0x407e26,
                error,
            })?;
        host.queue_direction(
            self,
            actor,
            Selector77Shot {
                microseconds: request.microseconds,
                direction_bits,
                speed_scale: ((random & 0xffff) >> 1) + 0xc000,
            },
        )
    }

    pub(super) fn selector77_target_controls<H: Selector77UpdateHost>(
        &mut self,
        actor: ObjectId,
        request: &Selector77TickRequest<'_>,
        host: &mut H,
    ) -> Result<([i16; 3], bool), Selector77UpdateError<H::Error>> {
        let reference =
            self.derived_actor(actor)?
                .target_148
                .ok_or(Selector77UpdateError::RequiredField {
                    actor,
                    offset: 0x148,
                })?;
        let focus = self.native_target(reference)?.raw_focus_08_words().ok_or(
            Selector77UpdateError::RequiredTargetWords {
                reference,
                field: "focus08",
            },
        )?;
        let origin = self.selector77_actor(actor)?.position_192.ok_or(
            Selector77UpdateError::RequiredField {
                actor,
                offset: 0x192,
            },
        )?;
        let mut world: [i16; 3] = std::array::from_fn(|i| focus[i].wrapping_sub(origin[i]) as i16);
        let squared = world
            .into_iter()
            .map(i32::from)
            .fold(0u32, |sum, v| sum.wrapping_add(v.wrapping_mul(v) as u32));
        let distance = if squared & 0x8000_0000 != 0 {
            isqrt_48efc0((squared >> 2) as i32).wrapping_shl(1)
        } else {
            isqrt_48efc0(squared as i32)
        };
        self.derived_actor_mut(actor)?.word_14c = Some(distance);
        if let Some(behavior) = self.derived_actor(actor)?.behavior_140.as_ref() {
            if behavior.byte_0c != 0 {
                let velocity = self
                    .native_target(reference)?
                    .velocity_words(request.process_velocity)?
                    .ok_or(Selector77UpdateError::RequiredTargetWords {
                        reference,
                        field: "velocity1A",
                    })?;
                let behavior = self
                    .derived_actor(actor)?
                    .behavior_140
                    .as_ref()
                    .expect("no intervening service");
                let row =
                    BehaviorRowSelection::from_table(behavior.table_14, behavior.byte_10, actor);
                let tables = host
                    .behavior_tables()
                    .ok_or(Selector77UpdateError::RequiredBehaviorTables)?;
                let speed = i32::from(read_row_word(self, tables, row, 2)? as i16);
                let time = super::super::super::kinematics::divide(
                    distance.wrapping_shl(10) as i32,
                    speed,
                    0x4078f8,
                )?;
                let multiplier = if time < 4096 {
                    time.wrapping_add(time)
                } else {
                    0x7fff
                };
                for axis in 0..3 {
                    let lead = (i32::from(velocity[axis] as i16).wrapping_mul(multiplier) >> 15)
                        .wrapping_shl(4);
                    world[axis] = world[axis].wrapping_add(lead as i16);
                }
                //48D6C0 reselects the row/kind after the position lead writes.
                let behavior = self
                    .derived_actor(actor)?
                    .behavior_140
                    .as_ref()
                    .expect("no intervening service");
                let row =
                    BehaviorRowSelection::from_table(behavior.table_14, behavior.byte_10, actor);
                let kind = i32::from(read_row_word(self, tables, row, 0)? as i16);
                if tables.read_dword(0x53dbb4_u32.wrapping_add(kind.wrapping_mul(16) as u32))? & 1
                    != 0
                {
                    let square = time.wrapping_mul(time);
                    let correction = ((i64::from(square) * 0x180000_i64) >> 31) as i32;
                    world[1] = world[1].wrapping_add(correction as i16);
                }
            }
            let spread = self
                .derived_actor(actor)?
                .behavior_140
                .as_ref()
                .expect("no intervening service")
                .word_04 as i32;
            if spread != 0 {
                let amplitude = spread.wrapping_mul(distance as i32) >> 16;
                for (axis, address) in [0x407986, 0x40799d, 0x4079b5].into_iter().enumerate() {
                    let random = host
                        .random()
                        .map_err(|error| Selector77UpdateError::Random { address, error })?;
                    let correction = amplitude
                        .wrapping_sub(((random & 0xffff) as i32).wrapping_mul(amplitude) >> 15);
                    world[axis] = world[axis].wrapping_add(correction as i16);
                }
            }
        }
        //77 +48 is4026B0; copy all ten current basis words before rotations.
        let mut basis = self.actor(actor)?.rotating.basis;
        let math = NativeBasisMath::new(*request.sine_quarter);
        for (index, plane) in [
            (3, RotationPlane::Xz),
            (2, RotationPlane::Yz),
            (5, RotationPlane::Xz),
            (1, RotationPlane::Yz),
        ] {
            let angle = i32::from(self.selector77_actor(actor)?.words_184_190[index] as i16);
            math.rotate_in_plane(&mut basis, angle, plane);
        }
        let local = super::super::super::kinematics::inverse_point_490970(basis, world);
        let angle = |axis| -> Result<i32, Selector77UpdateError<H::Error>> {
            let mut value = inverse_sine_q31(
                q31_divide(local[axis] as u32, distance) as i32,
                request.inverse_sine,
            )?;
            if local[2] < 0 {
                value = if value > 0 {
                    value.wrapping_add(0x2000)
                } else {
                    -0x2000
                };
            }
            Ok(value)
        };
        let yaw = angle(0)?;
        let pitch = angle(1)?;
        let aligned = self.selector77_steering(actor, pitch, yaw)?;
        self.selector77_extension(actor, request.microseconds, request.sine_quarter, host)?;
        Ok((world, aligned))
    }
}
