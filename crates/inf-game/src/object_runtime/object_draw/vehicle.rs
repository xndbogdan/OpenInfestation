//! The selector 2 vehicle's draw 00420510. No model/level-specific admission.
use super::*;
use crate::object_runtime::{ProfilingPhase, ProfilingRequest};

impl ObjectArena {
    pub(super) fn draw_ordinary_vehicle<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        //420533 saves full5474E0 before the first fresh55F448 gate.
        let saved_tint = *request.lighting.tint_word;
        let network = match *request.network_flag {
            0 => None,
            _ => Some(
                request
                    .host
                    .network_settings()
                    .ok_or_else(|| unowned(source, ObjectDrawCallbackBoundary::VehicleNetwork))?,
            ),
        };
        // 00420551..004205A2: Powerball's vehicles draw as its pods. A buggy
        // or hovercraft (+1E8 1 or 3) while the host's team flag 0054E81C is
        // set takes model 0x10D, 50 lower; any other 0x75 and dirty bit
        // 0x100. They skip the early return on +1C0.
        let powerball = network.is_some_and(|settings| settings[0x10 / 4] == 4);
        let mut lowered = 0u16;
        if powerball {
            let team_flag = request
                .host
                .network_team_flag()
                .ok_or_else(|| unowned(source, ObjectDrawCallbackBoundary::VehiclePowerball))?;
            let runtime = request
                .host
                .resident_models()
                .ok_or_else(|| unowned(source, ObjectDrawCallbackBoundary::VehiclePowerball))?;
            if matches!(self.selector2_actor(source)?.dwords_1e8_1f4[0], 1 | 3) && team_flag != 0 {
                lowered = 50u16.wrapping_neg();
                self.store_visual_model_word(source, 0x10d, runtime)?;
                self.lower_vehicle(source, lowered)?;
            } else {
                self.owned_mut(source)?.state.base.dirty_word |= 0x100;
                self.store_visual_model_word(source, 0x75, runtime)?;
            }
        } else if self.selector2_actor(source)?.byte_1c0 != 0 {
            //4205A4: authentic ordinary early return, before D0, H12 and+228.
            return Ok(());
        }
        let drawn = self.draw_vehicle_body(source, request, outcome, network, saved_tint);
        // 00420A31..00420A5A: model 0x9C and flag 4 again, at its own height.
        // The height comes back after a port omission too, or the vehicle
        // would sink 50 a frame.
        if powerball {
            self.lower_vehicle(source, lowered.wrapping_neg())?;
            if drawn.is_ok() {
                let runtime = request
                    .host
                    .resident_models()
                    .ok_or_else(|| unowned(source, ObjectDrawCallbackBoundary::VehiclePowerball))?;
                self.store_visual_model_word(source, 0x9c, runtime)?;
                let base = &mut self.owned_mut(source)?.state.base;
                let flags = base.flags | 4;
                mark_flag_change(base, flags);
            }
        }
        drawn
    }

    /// `ADD word ptr [+34], by`: the vehicle's height moved for its draw.
    fn lower_vehicle(&mut self, vehicle: ObjectId, by: u16) -> Result<(), ObjectError> {
        if by != 0 {
            let position = self
                .owned_mut(vehicle)?
                .state
                .base
                .position_words
                .as_mut()
                .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
            position[1] = position[1].wrapping_add(by);
        }
        Ok(())
    }

    /// `004205B2..00420A13`: the name tag, then the late model or the
    /// tinted common draw, and +228 cleared.
    fn draw_vehicle_body<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
        network: Option<[u32; 20]>,
        saved_tint: u32,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        // 004205B2: the name tag over a vehicle that moved in the last two
        // seconds, with Show Player Names (settings +2C).
        if network.is_some_and(|settings| settings[0x2c / 4] != 0)
            && self.selector2_actor(source)?.dwords_59c_5a0[1] != 0
        {
            let actor = self.actor(source)?;
            let tag = VehicleNameTag {
                position: self
                    .object(source)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(source))?,
                slot: (actor.handle.raw() >> 27) as u8,
                style: actor.word_b4.wrapping_add(0x25) as i32,
            };
            match request.host.name_tag(tag) {
                Ok(()) => {}
                Err(ObjectDrawServiceError::Backend(error)) => {
                    return Err(Flag0ObjectDrawError::Host(error))
                }
                Err(ObjectDrawServiceError::UnownedCallback(reason)) => {
                    return Err(unowned(source, reason))
                }
            }
        }
        if self.actor(source)?.byte_d0 as i8 > 0 {
            if self.object(source)?.base.flags & 0x0100_0000 != 0 {
                let packet = self.draw_packet(source, request.process)?;
                submit_model(self, packet, ModelRootMode::ManagerLate, request)?;
                outcome.late_models += 1;
            }
            //42086D reads direct+140, not virtual+7C. The actual behavior owner
            //NULL head skips48CDB0 itself; no invocation diagnostic is
            //incremented. The manager's separate late lane may later call its
            //empty branch after this direct consumer drained the chain.
            if self
                .derived_actor(source)?
                .behavior_140
                .as_ref()
                .is_some_and(|behavior| behavior.fire_head_00.is_some())
            {
                self.draw_weapon_fire(source, request, outcome)?;
            }
        } else {
            // 0042088F..0042092D: a network vehicle's tint goes halfway to
            // its owner's colour, the H6 style +B4 + 0x25.
            let mut tint = saved_tint;
            if network.is_some() {
                let slot = self.actor(source)?.word_b4.wrapping_add(0x25) as i16;
                let style = match request.host.style_word(i32::from(slot)) {
                    Ok(word) => word,
                    Err(ObjectDrawServiceError::Backend(error)) => {
                        return Err(Flag0ObjectDrawError::Host(error))
                    }
                    Err(ObjectDrawServiceError::UnownedCallback(reason)) => {
                        return Err(unowned(source, reason))
                    }
                };
                let mut bytes = tint.to_le_bytes();
                for (byte, colour) in bytes[..3].iter_mut().zip(style.to_le_bytes()) {
                    *byte = ((u32::from(*byte) + u32::from(colour)) >> 1) as u8;
                }
                tint = u32::from_le_bytes(bytes);
            }
            //420937/93B capture unsigned Z thenX before lazy current H12. Its
            //host may mutate pose; Common subsequently captures that fresh pose.
            let [x, _, z] = self.draw_position(source)?;
            let terrain = request
                .terrain
                .terrain(self, source)
                .map_err(Flag0ObjectDrawError::Terrain)?;
            let shade = u32::from(terrain.cell_at([x, z]).shade4().min(8));
            let mut bytes = tint.to_le_bytes();
            for byte in &mut bytes[..3] {
                *byte = ((u32::from(*byte) * shade) >> 3) as u8;
            }
            //43A370 publishes only5474E0, not renderer ambient/view/fade.
            *request.lighting.tint_word = u32::from_le_bytes(bytes);
            request.process.profiling.record(ProfilingRequest {
                index: 13,
                bank: 1,
                phase: ProfilingPhase::Begin,
            })?;
            let drawn = self.draw_common_actor(source, request, outcome);
            // Native always returns here and restores 5474E0. A port omission
            // (an unowned model value or callback) stands in for that normal
            // draw, so the borrowed world tint still returns; otherwise every
            // later frame on a shaded cell darkens the world again. Host,
            // terrain and object faults keep their reached prefix.
            if matches!(drawn, Err(Flag0ObjectDrawError::UnownedCallback { .. })) {
                *request.lighting.tint_word = saved_tint;
            }
            drawn?;
            request.process.profiling.record(ProfilingRequest {
                index: 13,
                bank: 1,
                phase: ProfilingPhase::End,
            })?;
            //Normal-return suffix. Other errors retain tint, marker and Common
            //far/flash/model/queue prefixes; manager omission cannot add a suffix.
            *request.lighting.tint_word = saved_tint;
        }
        self.selector2_actor_mut(source)?.dword_228 = Some(0); //420A13
        Ok(())
    }
}
