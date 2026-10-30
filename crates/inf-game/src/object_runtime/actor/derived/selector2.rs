//! Selector 2, the player vehicle: constructor 004156E0, vtable 00520860.
//!
//! Above 00425E60 the constructor builds the three-slot handle array
//! 00411D60 at +1A4, writes its scalar defaults, zeroes +1B4..+1BF, sets +1BC
//! and derives byte +209 from the allocation's pre-context position through
//! the 00450A30/00410F60 water-wave height. 00426BF0 initializes the payload.
//! Admission 0041A620 copies the 00530158 descriptors into +26C, builds
//! behavior +140 with 0048CC70(2, 8, 8, +26C) and copies +A2 to +A0.
//! Postinit 004217F0 stores WORD +8C in +184 before shared 0047AAD0.
//! Network constructor/guard branches, commands, update and destruction's
//! 00487FE0 releases are separate owners.

use inf_formats::VEHICLE_BEHAVIOR_TABLE_DWORDS;

use super::{DerivedActorClass, DerivedActorConstructionRequest, DerivedActorError};
use crate::object_runtime::{
    actor::{
        behavior::{ActorBehaviorGroup, ActorBehaviorState, ActorBehaviorTable},
        ActorConstructionError,
    },
    actor_terrain::plane_height_at,
    ActorCaptureSavedFlags, ActorExtension, ActorHandle, ActorTerrainError, ObjectArena,
    ObjectError, ObjectId, SoundTeardownService,
};

mod aim;
mod camera;
mod carry;
#[cfg(test)]
mod contact_tests;
mod damage;
mod death;
mod drive;
mod fire;
mod free_look;
mod hover;
mod items;
mod message;
#[cfg(test)]
mod mode_receipts;
#[cfg(test)]
mod network_tests;
mod recorded;
mod remote;
mod skimmer;
#[cfg(test)]
mod snapshot;
#[cfg(test)]
mod test_support;
mod tow;
mod update;
mod vtol;
#[cfg(test)]
mod wheel_tests;

pub use carry::{
    VehicleCarryContext, VehicleCarryEffect, VehicleCarryError, VehicleCarryHost, VehicleDropHost,
    CAPTURED_STATUS_TEXT,
};
pub use damage::{
    VehicleDamageError, VehicleDamageHost, VEHICLE_COLLISION_PITCH, VEHICLE_COLLISION_SOUND,
    VEHICLE_HIT_FLASH_RADIUS,
};
pub use death::{VehicleDeathError, VehicleDyingHost, VehicleDyingRequest};
pub use drive::{
    VehicleDriveError, VehicleDriveHost, VehicleDriveRequest, VehicleDriveState, VehicleGround,
    VehicleNetwork, VehiclePositionStep,
};
pub use message::{ActorMessage, VehicleMessageEffect, VehicleMessageError};
pub use update::{
    VehicleUpdateHost, VehicleUpdateReport, VehicleUpdateRequest, VehicleUpdateState,
    VehicleWreckPending, WakeParticle,
};

/// 00411D60(3): vtable 005206C4, count, element array, byte +0C. Each
/// 16-byte element gets vtable 005206C0 and the null handle at +4 only.
/// [`carry`] fills the slots through 00421810.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleHandleArray {
    pub handles: Vec<ActorHandle>,
    /// Element +8, the saved flags a capture's +8C writes. Unwritten until then.
    pub saved_flags: Vec<ActorCaptureSavedFlags>,
    /// Element +C: 1 for a carried remote actor, else 2. Unwritten until then.
    pub states: Vec<Option<u32>>,
    pub byte_0c: u8,
}

/// Selector 2's own fields. Special-view storage retains constructor unknowns;
/// the remaining unwritten holes are not represented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector2State {
    /// Written by postinit 004217F0 from WORD +8C.
    pub word_184: Option<u32>,
    pub words_188_194: [u32; 4],
    pub handles_1a4: VehicleHandleArray,
    /// 004959E0 zeroes +1B4..+1BF before +1BC = 1.
    pub bytes_1b4_1bf: [u8; 12],
    pub byte_1c0: u8,
    pub words_1c2_1c8: [u16; 4],
    /// 004156E0 leaves +1CA..+1E7 unwritten. The free look's first tick
    /// (00421F45) reads the eye position +1CA..+1CE before any write; the
    /// extended fresh-heap carry starts it at zero. The rows stay unknown
    /// until the look's toggle writes them.
    special_position_1ca: [Option<u16>; 3],
    special_forward_1d0: [Option<u32>; 3],
    special_up_1dc: [Option<u32>; 3],
    pub dwords_1e8_1f4: [u32; 4],
    pub dwords_200_204: [u32; 2],
    pub byte_208: u8,
    /// |pre-context Y - wave height| < 64 (static 0053014C).
    pub byte_209: u8,
    pub bytes_20a_20c: [u8; 3],
    pub word_20e: u16,
    pub dwords_210_218: [u32; 3],
    pub dwords_220_224: [u32; 2],
    /// Constructor4156E0 leaves +228 unwritten; normal draw420A13 writes0.
    /// Reset/update writers4184B0/4195F0 remain separate owners.
    pub(in crate::object_runtime) dword_228: Option<u32>,
    pub dword_238: u32,
    pub dword_244: u32,
    pub words_24c_24e: [u16; 2],
    pub dwords_250_25c: [u32; 4],
    /// +260: the constructor leaves it unwritten; the skimmer's entry
    /// `00422380(1)` clears it and its drive keeps it.
    pub dword_260: Option<u32>,
    /// +264: the skimmer's wake ribbon (`00486600`), which only a skimmer
    /// another machine drives makes (`00421189`).
    pub ribbon_264: Option<ObjectId>,
    /// +268, the wreck's remaining life: the constructor leaves it unwritten;
    /// the death `00420B90` stores 10 s and the dying update counts it down.
    pub dword_268: Option<u32>,
    /// The guard's copy of the executable descriptors; behavior +14 names it.
    pub table_26c: Option<Box<[u32; VEHICLE_BEHAVIOR_TABLE_DWORDS]>>,
    /// Byte custody beside the actual DWORD values, not a second cache.
    /// The original 704-byte admission copy establishes every byte as known.
    pub table_26c_known: Option<Box<[bool; 704]>>,
    pub dwords_52c_530: [u32; 2],
    pub bytes_534_535: [u8; 2],
    pub dwords_538_554: [u32; 8],
    pub dword_558: u32,
    pub dword_564: u32,
    pub dword_568: u32,
    pub words_56c_570: [u16; 3],
    pub dwords_580_584: [u32; 2],
    /// +588, the ECM pulse's phase (`0042041C`), which the constructor
    /// leaves unwritten ([`VEHICLE_PULSE_PHASE_APPROXIMATION`]).
    pub pulse_588: Option<u32>,
    pub dword_590: u32,
    pub dwords_59c_5a0: [u32; 2],
    /// +5A4: the last marked attacker's handle (004212F0's network arm).
    pub attacker_5a4: u32,
    /// +5A8: the clock of the attacker's last hit; the constructor leaves it
    /// to the allocator (None until written).
    pub attacked_5a8: Option<u32>,
    pub dwords_5ac_5b0: [u32; 2],
    /// The reticle's corners +5B4..+5BF (an X and Y WORD each), which the
    /// HUD's box and sight ease; the constructor never writes them
    /// ([`crate::vehicle_hud::RETICLE_CORNER_RESIDUE_APPROXIMATION`]).
    pub corners_5b4: [[u16; 2]; 3],
    pub dword_5c0: u32,
    /// The constructor-unwritten fields the mode-1/2 drive owns.
    pub drive: VehicleDriveState,
    /// The constructor-unwritten fields the update owns.
    pub update: VehicleUpdateState,
}

impl Selector2State {
    /// Actual +228 custody. Unknown allocator bytes are not a zero initializer.
    pub fn dword_228(&self) -> Option<u32> {
        self.dword_228
    }
}

/// Static 0053014C, the +209 proximity threshold.
const WAVE_PROXIMITY: i32 = 64;

impl ObjectArena {
    /// 004156E0 after 00425E60, offline; a network game's form is
    /// [`Self::set_selector2_network_form`]. The wave needs the allocation's
    /// pre-context X/Y/Z and a terrain surface, whose absence dereferences
    /// null natively.
    pub fn allocate_selector2_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let terrain = request.actor.terrain;
        let surface = terrain
            .terrain
            .ok_or(ActorTerrainError::MissingSurfaceForVehicle)?;
        let [x, y, z] = request
            .actor
            .allocation_position
            .ok_or(ActorTerrainError::UnknownAllocationPosition)?;
        let object = self.allocate_derived_actor(request, rng)?;
        let null = self
            .actor_handles()
            .ok_or(ActorConstructionError::MissingRegistry)?
            .null_handle();
        let ground = surface.terrain.height_at([x, z]);
        let plane = surface
            .plane_height_fixed
            .wrapping_shr(terrain.coordinate_shift);
        let height = plane_height_at(
            [x, z],
            plane,
            ground,
            terrain.clock,
            true,
            terrain.sine_quarter,
        );
        let distance = (i32::from(y as i16) - i32::from(height)).abs();
        let eye_carry = self.actor_allocation.initial_extended_word();
        self.derived_actor_mut(object)?.class =
            DerivedActorClass::Selector2(Box::new(Selector2State {
                word_184: None,
                words_188_194: [0; 4],
                handles_1a4: VehicleHandleArray {
                    handles: vec![null; 3],
                    saved_flags: vec![
                        ActorCaptureSavedFlags {
                            bytes: Some([None; 4]),
                        };
                        3
                    ],
                    states: vec![None; 3],
                    byte_0c: 0,
                },
                bytes_1b4_1bf: {
                    let mut bytes = [0; 12];
                    bytes[8] = 1;
                    bytes
                },
                byte_1c0: 0,
                words_1c2_1c8: [0; 4],
                special_position_1ca: [eye_carry; 3],
                special_forward_1d0: [None; 3],
                special_up_1dc: [None; 3],
                dwords_1e8_1f4: [0, 0, 1, 0],
                dwords_200_204: [0; 2],
                byte_208: 0,
                byte_209: u8::from(distance < WAVE_PROXIMITY),
                bytes_20a_20c: [0; 3],
                word_20e: 0,
                dwords_210_218: [0; 3],
                dwords_220_224: [0; 2],
                dword_228: None,
                dword_238: 1,
                dword_244: 0,
                words_24c_24e: [0; 2],
                dwords_250_25c: [0; 4],
                dword_260: None,
                ribbon_264: None,
                dword_268: None,
                table_26c: None,
                table_26c_known: None,
                dwords_52c_530: [0; 2],
                bytes_534_535: [0; 2],
                dwords_538_554: [0; 8],
                dword_558: 0x1_0000,
                dword_564: 0x4000,
                dword_568: 0x1_0000,
                words_56c_570: [0; 3],
                dwords_580_584: [0; 2],
                pulse_588: None,
                dword_590: 0xffff,
                dwords_59c_5a0: [0; 2],
                attacker_5a4: 0,
                attacked_5a8: None,
                dwords_5ac_5b0: [0; 2],
                corners_5b4: crate::vehicle_hud::RETICLE_CORNER_RESIDUE_APPROXIMATION,
                dword_5c0: 0,
                drive: VehicleDriveState {
                    dword_1fc: Some(drive::VEHICLE_TRACK_PHASE_APPROXIMATION),
                    dword_55c: Some(drive::VEHICLE_ENGINE_LOAD_APPROXIMATION),
                    dword_560: Some(drive::VEHICLE_ENGINE_LOAD_APPROXIMATION),
                    dword_240: Some(drive::VEHICLE_TURN_HOLD_APPROXIMATION),
                    dword_248: Some(drive::VEHICLE_ANALOG_HOLD_APPROXIMATION),
                    ..VehicleDriveState::default()
                },
                update: VehicleUpdateState::default(),
            }));
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("VehicleTrackPhaseApproximation: 004156E0 leaves the track phase +1FC to allocator residue; the port starts it at zero, which only shifts the wheel and track animation (VEHICLE_DRIVE_RUNTIME.md)");
            eprintln!("VehicleEngineLoadApproximation: 004156E0 leaves the engine load +55C/+560 to allocator residue, which a world started as the hovercraft or VTOL reads; the port starts it at zero, which only shifts the engine sound's first ticks (VEHICLE_DRIVE_RUNTIME.md)");
            eprintln!("VehicleAnalogHoldApproximation: 004156E0 leaves the analog turn's hold +248 to allocator residue, which a stick already hard over on an Analogue joystick's first tick reads; the port starts it at zero, which only shifts that turn's first ticks (VEHICLE_DRIVE_RUNTIME.md)");
            eprintln!("VehicleTurnHoldApproximation: 004156E0 leaves the turn hold +240 to allocator residue, which a vehicle that has only hovered, flown or skimmed since it was built reads on its first turn; the port starts it at zero, as the lab read in retail (VEHICLE_DRIVE_RUNTIME.md)");
        }
        Ok(object)
    }

    pub fn selector2_actor(&self, object: ObjectId) -> Result<&Selector2State, DerivedActorError> {
        match &self.derived_actor(object)?.class {
            DerivedActorClass::Selector2(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector2(object)),
        }
    }

    /// Tests: the track phase unknown, as the residue the port no longer
    /// assumes leaves it.
    #[cfg(test)]
    pub(crate) fn forget_vehicle_track_phase(
        &mut self,
        vehicle: ObjectId,
    ) -> Result<(), DerivedActorError> {
        self.selector2_actor_mut(vehicle)?.drive.dword_1fc = None;
        Ok(())
    }

    /// Tests: the vehicle's state, for the session's fall tests.
    #[cfg(test)]
    pub(crate) fn selector2_actor_mut_for_tests(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector2State, DerivedActorError> {
        self.selector2_actor_mut(object)
    }

    /// Port fix: L with both lamps owned steps the vehicle's headlights and
    /// infrared vision ([`CameraGlobalState::step_lamps`]).
    pub fn step_vehicle_lamps(
        &mut self,
        vehicle: ObjectId,
        globals: &crate::object_runtime::CameraGlobalState,
    ) -> Result<(), DerivedActorError> {
        let state = self.selector2_actor_mut(vehicle)?;
        state.byte_208 = globals.step_lamps(state.byte_208);
        Ok(())
    }

    pub(in crate::object_runtime) fn selector2_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector2State, DerivedActorError> {
        match &mut self.derived_actor_mut(object)?.class {
            DerivedActorClass::Selector2(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector2(object)),
        }
    }

    /// 004156E0's network branch, which nothing between it and the
    /// constructor's end reads: the form +1F0 in mode 0
    /// is the player's vehicle +18 plus one (0 to 4), in modes 2 to 4 the
    /// arena's (00488A40 over +1C); otherwise it stays 1.
    pub(in crate::object_runtime) fn set_selector2_network_form(
        &mut self,
        object: ObjectId,
        settings: &[u32; 20],
    ) -> Result<(), DerivedActorError> {
        let form = match (settings[0x10 / 4], settings[0x18 / 4]) {
            (0, vehicle @ 0..=4) => vehicle + 1,
            (2..=4, _) => crate::network_menu::arena_at(settings[0x1c / 4]).word_08,
            _ => return Ok(()),
        };
        self.selector2_actor_mut(object)?.dwords_1e8_1f4[2] = form;
        Ok(())
    }

    /// 0041A620. An existing +140 is kept; otherwise the table is copied
    /// and 0048CC70(2, 8, 8, +26C) builds the replacement, leaving +04/+08
    /// zero. In a network game's mode (`network_mode`) the maximum +A2
    /// scales: in mode 0 by form (the truck half again, the VTOL a half, the
    /// skimmer a quarter), sixteen times in modes 2 and 4. Health +A0 then
    /// takes maximum +A2. Always admits.
    pub fn admit_selector2_actor(
        &mut self,
        object: ObjectId,
        table: &[u32; VEHICLE_BEHAVIOR_TABLE_DWORDS],
        network_mode: Option<u32>,
    ) -> Result<bool, DerivedActorError> {
        self.selector2_actor(object)?;
        if self.derived_actor(object)?.behavior_140.is_none() {
            self.selector2_actor_mut(object)?.table_26c = Some(Box::new(*table));
            self.selector2_actor_mut(object)?.table_26c_known = Some(Box::new([true; 704]));
            let (groups, entries, extra) = (2_u8, 8_u8, 8_u8);
            self.derived_actor_mut(object)?.behavior_140 = Some(ActorBehaviorState {
                fire_head_00: None,
                firing_stop_00: None,
                sound_28: None,
                word_04: 0,
                word_08: 0,
                byte_0c: 1,
                byte_0d: groups,
                byte_0e: entries,
                byte_0f: extra,
                byte_10: 0,
                byte_11: entries,
                table_14: ActorBehaviorTable::Selector2Copy,
                entries_18: vec![[u32::MAX, 0, 0, 0, 0, 0, 0, 0]; usize::from(entries + extra)],
                groups_1c: vec![
                    ActorBehaviorGroup {
                        coordinates_00_0a: [None; 6],
                        word_0c: 4,
                    };
                    usize::from(groups)
                ],
                word_20: 0,
                words_24: vec![None; usize::from(groups)],
                word_2c: 0,
                word_30: None,
                word_34: None,
                word_38: 0,
            });
        }
        let mut maximum = self
            .actor(object)?
            .maximum_health
            .ok_or(DerivedActorError::MissingSelector2Health(object))?;
        if let Some(mode) = network_mode {
            let form = self.selector2_actor(object)?.dwords_1e8_1f4[2];
            maximum = match (mode, form) {
                (0, 2) => (u32::from(maximum) * 3 / 2) as u16,
                (0, 4) => maximum >> 1,
                (0, 5) => maximum >> 2,
                (2 | 4, _) => maximum << 4,
                _ => maximum,
            };
            self.actor_mut(object)?.maximum_health = Some(maximum);
        }
        self.actor_mut(object)?.health = maximum;
        Ok(true)
    }

    /// 0046B110's writes into the vehicle's +26C copy for owned upgrade IDs
    /// (0046BDC0 over the player's +100 bitset), ending with WORD +134 = 1C.
    /// Then, when `behavior_owner` has a +140 behavior (its +7C), each player
    /// +150 value above its 0053A310 limit is lowered and stored in the first
    /// DWORD of the matching 32-byte behavior record.
    pub(in crate::object_runtime) fn apply_vehicle_upgrades(
        &mut self,
        vehicle: ObjectId,
        behavior_owner: Option<ObjectId>,
        bits_100: &[u32; 6],
        values_150: &mut [i32; 16],
        limits: &[i32; 16],
        network_active: bool,
    ) -> Result<(), DerivedActorError> {
        let owned = |id: u32| id == 0 || bits_100[(id >> 5) as usize] & 1 << (id & 31) != 0;
        let state = self.selector2_actor_mut(vehicle)?;
        let table = state
            .table_26c
            .as_deref_mut()
            .ok_or(DerivedActorError::MissingSelector2Table(vehicle))?;
        let known = state
            .table_26c_known
            .as_deref_mut()
            .ok_or(DerivedActorError::MissingSelector2Table(vehicle))?;
        let mut word = |offset: usize, value: u16| {
            let index = offset / 4;
            let shift = (offset % 4) * 8;
            table[index] = table[index] & !(0xffff << shift) | u32::from(value) << shift;
            known[offset..offset + 2].fill(true);
        };
        let patches_32: [(u32, usize, u32); 5] = [
            (0x95, 0x08, 120_000),
            (0x97, 0x18, 0x1800),
            (0x9d, 0x34, 150_000),
            (0x9f, 0x44, 0x1000),
            (0x9a, 0x70, 0x3000),
        ];
        // The patched fields never overlap, so only +B0's native writer order
        // matters: A0, then 1E, then A4, the last owned one winning.
        for (id, offset, value) in [(0x96, 0x06, 2), (0x9c, 0x2c, 6), (0x9e, 0x32, 2)] {
            if owned(id) {
                word(offset, value);
            }
        }
        if owned(0x27) && network_active {
            word(0x58, 0x20);
        }
        for (id, value) in [(0xa0, 0x0b), (0x1e, 0x0d), (0xa4, 0x0c)] {
            if owned(id) {
                word(0xb0, value);
            }
        }
        if owned(0xa2) {
            word(0x8a, 1);
        }
        word(0x134, 0x1c);
        for (id, offset, value) in patches_32 {
            if owned(id) {
                table[offset / 4] = value;
                known[offset..offset + 4].fill(true);
            }
        }
        let Some(owner) = behavior_owner else {
            return Ok(());
        };
        let Some(behavior) = self.derived_actor_mut(owner)?.behavior_140.as_mut() else {
            return Ok(());
        };
        if behavior.entries_18.len() < values_150.len() {
            return Err(DerivedActorError::ShortVehicleBehavior(owner));
        }
        for ((value, limit), entry) in values_150
            .iter_mut()
            .zip(limits)
            .zip(behavior.entries_18.iter_mut())
        {
            if *limit < *value {
                *value = *limit;
            }
            entry[0] = *value as u32;
        }
        Ok(())
    }

    /// `00415B10` before the derived `00425F70`: the drive's +548 sound and
    /// +54C (only the unported modes create it) go through `00487FE0`, then
    /// `00411DD0` disposes every actor the +1A4 slots still hold.
    pub(in crate::object_runtime) fn destroy_selector2_owners(
        &mut self,
        vehicle: ObjectId,
        sound: &mut SoundTeardownService<'_>,
    ) -> Result<(), ObjectError> {
        if let Some(reference) = self
            .vehicle_destruction_state(vehicle)?
            .drive
            .sound_548
            .take()
        {
            sound.delete(reference)?;
        }
        if self.vehicle_destruction_state(vehicle)?.dwords_538_554[5] != 0 {
            return Err(ObjectError::VehicleSound54c(vehicle));
        }
        let handles = self
            .vehicle_destruction_state(vehicle)?
            .handles_1a4
            .handles
            .clone();
        for handle in handles {
            // 0047AD90 resolves each slot; a live actor is disposed.
            if let Some(actor) = self
                .resolve_actor_handle(handle)
                .map_err(|_| ObjectError::MissingActorRegistry)?
            {
                self.dispose(actor)?;
            }
        }
        Ok(())
    }

    fn vehicle_destruction_state(
        &mut self,
        vehicle: ObjectId,
    ) -> Result<&mut Selector2State, ObjectError> {
        match &mut self.actor_mut(vehicle)?.extension {
            ActorExtension::Derived(derived) => match &mut derived.class {
                DerivedActorClass::Selector2(state) => Ok(state),
                _ => Err(ObjectError::NotDerivedActor(vehicle)),
            },
            _ => Err(ObjectError::NotDerivedActor(vehicle)),
        }
    }

    /// 004217F0: +184 = zero-extended WORD +8C, then shared 0047AAD0.
    pub fn finish_selector2_initialization(
        &mut self,
        object: ObjectId,
    ) -> Result<(), DerivedActorError> {
        let word = u32::from(self.actor(object)?.word_8c);
        self.selector2_actor_mut(object)?.word_184 = Some(word);
        self.finish_actor_initialization(object)?;
        Ok(())
    }
}
