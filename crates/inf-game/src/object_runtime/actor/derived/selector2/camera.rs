//! Vehicle chase targets 004218E0 and signed projection 00421D30.
//! Eye publication precedes the later mode/target consumers. Special view
//! 00421D50 reads constructor-unknown storage and retains reached basis copies.

use inf_formats::native_basis::{
    normalize_words, orient_from_forward, NativeBasis, NativeBasisMath, NativeLengthPrecision,
    RotationPlane,
};

use crate::{
    object_runtime::{
        actor::native_trunc_low32, CameraDerivedTargetMode, CameraDerivedTargetRequest,
        CameraPoseError, DerivedActorClass, ObjectArena, ObjectError,
    },
    process_particles::factories::ParticleFactoryProcessState,
};

impl ObjectArena {
    pub(in crate::object_runtime::actor) fn update_selector2_camera_targets(
        &mut self,
        request: CameraDerivedTargetRequest<'_>,
        process: Option<&ParticleFactoryProcessState>,
    ) -> Result<(), CameraPoseError> {
        if request.mode == CameraDerivedTargetMode::SpecialView {
            return self.update_special_view_camera_targets(request);
        }
        // 004218E4 captures this before any actor/basis/vector consumption.
        let null = process
            .and_then(|owner| owner.process_null_handle)
            .ok_or(CameraPoseError::MissingVehicleProcessNullHandle)?;
        let actor = request.component.object;
        let DerivedActorClass::Selector2(vehicle) = &self.derived_actor(actor)?.class else {
            return Err(CameraPoseError::NotSelector2CameraTarget(actor));
        };
        let bytes = &vehicle.bytes_1b4_1bf;
        let current = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let base: i32 = if current == null.raw() { -400 } else { -1000 };
        let initial_basis = self.actor(actor)?.rotating.basis;
        let forward = horizontal_direction(initial_basis, false);
        let speed = projection_421d30(
            basis_z(initial_basis),
            self.actor(actor)?
                .words_8e_92
                .map(|word| i32::from(word as i16)),
        );
        let distance = base.wrapping_add(negative_fifth(speed)).max(-1000) as i16;
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let mut eye = offset(position, forward, f64::from(distance));
        eye[1] = position[1].wrapping_add(150);
        // 00421B17: desired position then fresh velocity, no current-state write.
        request.eye.set_target(eye, self.actor(actor)?.words_8e_92);

        let mut look_direction = forward;
        let DerivedActorClass::Selector2(vehicle) = &self.derived_actor(actor)?.class else {
            return Err(CameraPoseError::NotSelector2CameraTarget(actor));
        };
        let mode = vehicle.dwords_1e8_1f4[0];
        if matches!(mode, 1 | 2) {
            let quarter = (self.actor(actor)?.words_94_9c[1] as i32) / 4;
            if quarter != 0 {
                let mut basis = self.actor(actor)?.rotating.basis;
                request.math.rotate(&mut basis, quarter);
                // This block retains X; Y/Z and the divisor are stored f32.
                look_direction = horizontal_direction(basis, true);
            }
        }
        if let Some(reference) = self.derived_actor(actor)?.target_148 {
            let mut target = self.native_target(reference)?;
            if target.kind_word() == 2 {
                let result = self.refresh_native_target(&mut target, request.settings_7c);
                self.write_native_target(reference, target)?;
                result?;
                let point = self
                    .native_target(reference)?
                    .raw_focus_08_words()
                    .ok_or(CameraPoseError::UnwrittenTarget("vehicle +148 raw +08"))?;
                let position = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                let delta =
                    std::array::from_fn(|axis| point[axis].wrapping_sub(position[axis]) as i16);
                look_direction =
                    normalize_words(delta, NativeLengthPrecision::Retained).map(f64::from);
            }
        }
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let look = offset(position, look_direction, 400.0);
        request
            .look
            .set_target(look, self.actor(actor)?.words_8e_92);
        Ok(())
    }

    /// 00421D50's temporary XYZ and persistent caller basis. A missing later
    /// scalar retains earlier copies; this does not initialize native holes.
    pub(super) fn special_view_421d50(
        &self,
        actor: crate::object_runtime::ObjectId,
        xyz: &mut [Option<u16>; 3],
        basis: &mut NativeBasis,
        math: &NativeBasisMath,
    ) -> Result<(), CameraPoseError> {
        let DerivedActorClass::Selector2(vehicle) = &self.derived_actor(actor)?.class else {
            return Err(CameraPoseError::NotSelector2CameraTarget(actor));
        };
        let missing = |offset, bytes| CameraPoseError::UninitializedSpecialViewStorage {
            actor,
            offset,
            bytes,
        };
        // 00421D64 reads both WORDs in one DWORD before either output store.
        let (x, y) = vehicle.special_position_1ca[0]
            .zip(vehicle.special_position_1ca[1])
            .ok_or_else(|| missing(0x1ca, 4))?;
        xyz[0] = Some(x);
        xyz[1] = Some(y);
        xyz[2] = Some(vehicle.special_position_1ca[2].ok_or_else(|| missing(0x1ce, 2))?);
        xyz[1] = Some(y.wrapping_add(50));

        // 00490370 copies forward into Z, then up into Y one DWORD at a time.
        // Cross products, normalization and the count reset follow all copies.
        for axis in 0..3 {
            basis.words[7 + axis] = vehicle.special_forward_1d0[axis]
                .ok_or_else(|| missing(0x1d0 + axis as u16 * 4, 4))?;
        }
        for axis in 0..3 {
            basis.words[4 + axis] =
                vehicle.special_up_1dc[axis].ok_or_else(|| missing(0x1dc + axis as u16 * 4, 4))?;
        }
        let forward = basis_z(*basis);
        let up = std::array::from_fn(|axis| f32::from_bits(basis.words[4 + axis]));
        orient_from_forward(basis, forward, up, false);
        // Both calls execute even with zero angles; 16-bit turns are signed
        // here, independent of Chase's signed DWORD +98 divided by four.
        math.rotate(basis, i32::from(vehicle.words_1c2_1c8[2] as i16));
        math.rotate_in_plane(
            basis,
            i32::from(vehicle.words_1c2_1c8[3] as i16),
            RotationPlane::Yz,
        );
        Ok(())
    }

    /// 0041006C..00410104: leaf success precedes eye, fresh vehicle XYZ,
    /// then primary target virtual +08. Process NULL and settings are unused.
    fn update_special_view_camera_targets(
        &self,
        request: CameraDerivedTargetRequest<'_>,
    ) -> Result<(), CameraPoseError> {
        let actor = request.component.object;
        let mut xyz = [None; 3];
        self.special_view_421d50(actor, &mut xyz, request.basis, request.math)?;
        request.eye.set_target(
            xyz.map(|word| word.expect("successful special-view XYZ copy")),
            self.actor(actor)?.words_8e_92,
        );
        // 005205B8 is 500.0. All three conversions precede the fresh XYZ reads.
        let delta = basis_z(*request.basis)
            .map(|value| native_trunc_low32(f64::from(value) * 500.0) as u16);
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let look = std::array::from_fn(|axis| position[axis].wrapping_add(delta[axis]));
        let reference = request
            .primary_target
            .ok_or(CameraPoseError::UnwrittenTarget("primary"))?;
        let velocity = self
            .actor_follow_target(reference)?
            .velocity_1a
            .ok_or(CameraPoseError::UnwrittenTarget("primary"))?;
        request.look.set_target(look, velocity);
        Ok(())
    }
}

/// ECX is the three floats at actor+60; arguments are full signed DWORDs.
fn projection_421d30(z: [f32; 3], velocity: [i32; 3]) -> i32 {
    let [x, y, z_velocity] = velocity.map(f64::from);
    native_trunc_low32((y * f64::from(z[1]) + z_velocity * f64::from(z[2])) + x * f64::from(z[0]))
}

fn negative_fifth(value: i32) -> i32 {
    let product = i64::from(value) * i64::from(0x9999_9999_u32 as i32);
    let shifted = ((product >> 32) as i32) >> 1;
    shifted.wrapping_add(((shifted as u32) >> 31) as i32)
}

fn basis_z(basis: NativeBasis) -> [f32; 3] {
    std::array::from_fn(|axis| f32::from_bits(basis.words[7 + axis]))
}

/// 00421915/421B87 quantize X then Z as DWORDs. The zero arm is (1,0,0).
/// Initial X is stored; rotated-look X stays in x87 until its scale consumer.
fn horizontal_direction(basis: NativeBasis, retain_x: bool) -> [f64; 3] {
    let [x, _, z] = basis_z(basis);
    let x = f64::from(native_trunc_low32(f64::from(x) * 4096.0));
    let z = f64::from(native_trunc_low32(f64::from(z) * 4096.0));
    let stored_z = f64::from(z as f32);
    let length = (stored_z * z + x * x).sqrt();
    if length == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let divisor = f64::from(length as f32);
    let x = x / divisor;
    [
        if retain_x { x } else { f64::from(x as f32) },
        f64::from((0.0 / divisor) as f32),
        f64::from((stored_z / divisor) as f32),
    ]
}

fn offset(position: [u16; 3], direction: [f64; 3], scale: f64) -> [u16; 3] {
    std::array::from_fn(|axis| {
        position[axis].wrapping_add(native_trunc_low32(direction[axis] * scale) as u16)
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod native_receipts;

#[cfg(test)]
mod special_view_native_receipts;
