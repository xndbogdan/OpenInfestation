//! 00421810, a vehicle taking an actor into its +1A4 carry slots.
//!
//! The slot array's +8 (004122C0) rejects an actor it already carries, then
//! takes the first slot holding the process NULL handle: the actor's +8C
//! captures it into the slot's saved-flags DWORD, the slot stores the actor's
//! handle, the array's +0C (004122A0) births effect 00489A90(2) at the actor
//! and the slot state becomes 1 for a remote actor, else 2. A carried actor
//! with a nonzero +B4 then joins the vehicle's team through its +5C view
//! (00468C40) and refills health +A0 from +A2. Three 00487EC0 one-shots of
//! sound 2 at the vehicle close the attach.
//!
//! Selector 12's team change 00468C40 marks +38 bit 1, stores the team,
//! empties its +1A0 table, sets +22E bit 0 and drops its targets +1DC,
//! +1D4, +1EC and +1F4. Each actor of the new team whose +58 component
//! answers +CC then forgets it in its own +1A0 table (00482F90). Every
//! family-428650 table answers with itself (00404610: 00520F34, 00521164,
//! 00520B84 and selectors 10, 12, 34, 35 and 36); the other derived tables
//! answer NULL (00401300: 00520E58 and selectors 2, 11, 60 and 77).
//!
//! The drop 00412880 (the array's +0, through 00412110) takes the first
//! slot from its index whose state is not 3, places the actor beside the
//! vehicle (the array's +4, 004121D0), marks a remote actor's slot 3, and
//! otherwise releases the actor through its +8C (operation 0, the process
//! NULL as source, the slot's saved flags), attaches the death-style effect
//! 00405420, clears the slot and compacts the array (00411F60).
//!
//! A scientist also boards by touching a vehicle: selector 12's pair
//! reaction 00468160 (+A0), alive (+C4 at least 3) and outside team 0x12,
//! merges its grants into the vehicle's player and broadcasts event 22
//! (00475350, 0047DAF0), then takes itself into the hold (00421810).
//!
//! A mining rig's +8C (004547E0) refuses a take while it holds a level,
//! with the first player's hint 0x18; an empty rig gives its loader hint
//! 0x1F and the rig's grant 0x23 before the shared body. A drop the shared
//! body admits calls the mines within 0x400 (0047DD20(0x400, 0xF, 0)), and
//! any answer starts the rig filling.
//!
//! Selector 12 binds +8C to its capture reaction 00468220. The capture arm
//! looks up the source's player (0046B090). When a player owns the source, it
//! merges the actor's grants into that player (00475350), broadcasts event 22
//! with zero radius (0047DAF0), and sets player commands 0 and 1 silently and
//! 0x48 with its message (0046C760). It then stores "Captured" (0053A160) at
//! +228 before the shared body 0047D7C0.

use inf_formats::FrontendExecutable;
use thiserror::Error;

use crate::{
    grant_text::GrantTextData,
    object_runtime::{
        ActorCaptureContext, ActorCaptureError, ActorCaptureHost, ActorCapturePlayerCommand,
        ActorCaptureRequest, ActorCaptureSavedFlags, ActorGrantDifferenceDelivery,
        ActorGrantDifferenceNotificationError, ActorGrantDifferenceNotificationRequest,
        ActorHandle, DerivedActorError, GrantOwner, LiveGrantContext, LiveGrantError,
        LiveGrantOperation, LiveGrantRequest, ObjectArena, ObjectError, ObjectId,
        PlayerActorLookupContext, PlayerActorLookupError, PlayerActorLookupRequest,
        PlayerCommandContext, PlayerCommandError, PlayerCommandOperation, PlayerCommandRequest,
    },
    process_particles::{audio::ParticleSoundRequest, factories::ParticleFactoryProcessState},
    ResidentRuntime,
};

/// "Captured", the status text selector 12's capture arm stores at +228.
pub const CAPTURED_STATUS_TEXT: u32 = 0x0053_a160;
/// 004122A0 passes ECX = 2 to 00489A90.
const SLOT_EFFECT_SELECTOR: u32 = 2;
/// The slot state of a remote actor the drop left in place.
const SLOT_REMOTE: u32 = 3;
/// 004547FE/00454823: the rig's hints, refusing a take of a rig that holds
/// a level and taking an empty one.
const RIG_HOLDS_LEVEL_HINT: u32 = 0x18;
const RIG_TAKEN_HINT: u32 = 0x1f;
/// 0045482C: the grant a rig's taker learns, the portable mining rig.
const RIG_GRANT: u32 = 0x23;
/// 00454857: the reach of a dropped rig's call to the mines.
const RIG_MINE_REACH: u32 = 0x400;
/// 0046E56B: the player command a Prison Unit's attach sends.
const PRISON_ATTACH_COMMAND: u32 = 0x10;
/// 00421810 passes ECX = 2 to each 00487EC0.
const ATTACH_SOUND: u32 = 2;
const ATTACH_VOLUME: u32 = 0x10000;
const ATTACH_PITCHES: [u32; 3] = [0x4000, 0x6000, 0x8000];

/// The immutable owners an attach reads.
#[derive(Clone, Copy)]
pub struct VehicleCarryContext<'a> {
    pub process: &'a ParticleFactoryProcessState,
    pub executable: FrontendExecutable<'a>,
    pub resident: &'a ResidentRuntime,
    pub text_data: Option<&'a GrantTextData<'a>>,
}

impl VehicleCarryContext<'_> {
    fn commands(&self) -> PlayerCommandContext<'_> {
        PlayerCommandContext {
            process: Some(self.process),
            executable: Some(self.executable),
            resident: Some(self.resident),
        }
    }
}

/// The attach's lower effects, in native order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleCarryEffect {
    /// 004122A0: 00489A90(selector, actor +32, flags).
    Particle {
        selector: u32,
        position: [u16; 3],
        flags: u32,
    },
    /// 00487EC0 at the vehicle; 004923C0 releases its result.
    Sound(ParticleSoundRequest),
}

pub trait VehicleCarryHost {
    type Error;

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: VehicleCarryEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

/// The drop's services beyond the take's.
pub trait VehicleDropHost: VehicleCarryHost {
    /// `0047DD20(radius, 0xF, 0)` from a dropped mining rig: every actor
    /// within the radius gets its +98, and the answers add up.
    fn call_mines(
        &mut self,
        arena: &mut ObjectArena,
        rig: ObjectId,
        radius: u32,
    ) -> Result<u32, Self::Error>;
    /// Selector 12's +104 (`00468350`) with both arguments zero.
    fn select_targets(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// `00405420` on the dropped actor.
    fn attach_effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

/// Errors keep every write that precedes them; nothing is rolled back.
#[derive(Debug, Error)]
pub enum VehicleCarryError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    PlayerLookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    Grants(#[from] LiveGrantError),
    #[error(transparent)]
    Notification(#[from] ActorGrantDifferenceNotificationError),
    #[error(transparent)]
    Command(#[from] PlayerCommandError),
    #[error(transparent)]
    Capture(#[from] ActorCaptureError<PlayerCommandError>),
    #[error("004122C0 requires the actual process NULL handle 0054E894")]
    MissingProcessNullHandle,
    #[error(transparent)]
    Construction(#[from] crate::object_runtime::ActorConstructionError),
    #[error(transparent)]
    Target(#[from] crate::object_runtime::NativeTargetError),
    #[error(transparent)]
    HelperHistory(#[from] crate::object_runtime::FamilyHelperHistoryError),
    #[error("00412110 reads vehicle {vehicle:?}'s unwritten slot {index} state")]
    UnknownSlotState { vehicle: ObjectId, index: usize },
    #[error("vehicle {vehicle:?} has no represented +1A4 slot {index}")]
    SlotStorage { vehicle: ObjectId, index: usize },
    #[error("actor {0:?} binds +8C to a capture body the port does not own")]
    CaptureBoundary(ObjectId),
    /// 00458CF6 (the Bomb) and 00454800 (the mining rig) call 0046C760 on
    /// 00488AA0's answer without a NULL test.
    #[error("{0:?}'s hint finds no first player (00488AA0)")]
    MissingFirstPlayer(ObjectId),
    #[error("actor {0:?} has a +5C view the port does not own")]
    ViewBoundary(ObjectId),
    /// 0046E57F: a Prison Unit holding its prisoner names its holder (or
    /// itself, dropped) the team's task source (00482C60) and messages the
    /// holder (0047DD40(4, 0x13E)), which the hold's carry path cannot reach.
    #[error(
        "Prison Unit {0:?} holds a prisoner; its +8C's task source and message are not ported"
    )]
    PrisonerTaskSource(ObjectId),
    #[error(transparent)]
    Table(#[from] crate::object_runtime::FamilyTargetTableError),
    #[error("carried actor {0:?} has no +A2 maximum health")]
    MissingMaximumHealth(ObjectId),
    #[error("actor {0:?} has no +32 position for its carry effect")]
    MissingPosition(ObjectId),
    #[error("vehicle carry effect failed: {0}")]
    Host(E),
}

/// The shared body's lower command 0046C760(player, 0x42, 0), reached only
/// when the source's +6C differs from the actor's.
struct CaptureCommands<'a> {
    context: VehicleCarryContext<'a>,
}

impl ActorCaptureHost for CaptureCommands<'_> {
    type Error = PlayerCommandError;

    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        _: &mut ActorCaptureContext<'_>,
        request: ActorCapturePlayerCommand,
    ) -> Result<(), Self::Error> {
        arena
            .apply_player_command(
                PlayerCommandRequest {
                    player: request.player,
                    operation: PlayerCommandOperation::Set {
                        raw_id: request.code,
                        silent: request.value,
                    },
                },
                &self.context.commands(),
            )
            .map(|_| ())
    }
}

impl ObjectArena {
    /// 00421810 on a selector-2 vehicle. Returns whether a slot took `item`.
    pub fn attach_vehicle_item<H: VehicleCarryHost>(
        &mut self,
        vehicle: ObjectId,
        item: ObjectId,
        context: VehicleCarryContext<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<bool, VehicleCarryError<H::Error>> {
        self.selector2_actor(vehicle)?;
        let source = self.actor(vehicle)?.handle;
        if !self.take_into_vehicle_slot(vehicle, source, item, context, host, rng)? {
            return Ok(false);
        }
        if self.actor(item)?.word_b4 != 0 {
            if let Some(view) = self.carried_actor_view(item)? {
                let team = self.actor(vehicle)?.word_b4;
                self.join_vehicle_team(view, team, context)?;
                let maximum = self
                    .actor(view)?
                    .maximum_health
                    .ok_or(VehicleCarryError::MissingMaximumHealth(view))?;
                self.actor_mut(view)?.health = maximum;
            }
        }
        for pitch in ATTACH_PITCHES {
            // Each 00487EC0 copies the vehicle's +32 words again.
            let position = self
                .object(vehicle)?
                .base
                .position_words
                .ok_or(VehicleCarryError::MissingPosition(vehicle))?;
            host.effect(
                self,
                VehicleCarryEffect::Sound(ParticleSoundRequest {
                    sound: ATTACH_SOUND,
                    position,
                    volume: ATTACH_VOLUME,
                    pitch,
                    flags: 0,
                }),
                rng,
            )
            .map_err(VehicleCarryError::Host)?;
        }
        Ok(true)
    }

    /// Message 8 received (`0045EB30`) for this machine's `actor`: its +8C
    /// with operation 1 and `holder` as source, into a stack slot the
    /// capture writes and nothing reads.
    pub fn network_hold_take<E>(
        &mut self,
        actor: ObjectId,
        holder: ActorHandle,
        context: VehicleCarryContext<'_>,
    ) -> Result<bool, VehicleCarryError<E>> {
        let mut slot = ActorCaptureSavedFlags {
            bytes: Some([None; 4]),
        };
        self.capture_carried_actor(actor, holder, &mut slot, context)
    }

    /// Message 9 received (`0045EBA0`), or a record whose holder has gone
    /// (`0047E35B`): `actor` lets go, its +8C with operation 0 and the
    /// process NULL, from its own +D4.
    pub fn network_hold_release<H: VehicleDropHost>(
        &mut self,
        actor: ObjectId,
        null: ActorHandle,
        context: VehicleCarryContext<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), VehicleCarryError<H::Error>> {
        let kept = self.actor(actor)?.capture_flags_d4;
        let mut saved = ActorCaptureSavedFlags {
            bytes: Some(kept.map_or([None; 4], |flags| flags.to_le_bytes().map(Some))),
        };
        let released = self.release_carried_actor(actor, null, &mut saved, context, host, rng);
        if let Some([Some(a), Some(b), Some(c), Some(d)]) = saved.bytes {
            self.actor_mut(actor)?.capture_flags_d4 = Some(u32::from_le_bytes([a, b, c, d]));
        }
        released
    }

    /// `004122A0`, a hold's `+C`: the slot flash 00489A90(2) at `actor`.
    pub fn hold_flash<H: VehicleCarryHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), VehicleCarryError<H::Error>> {
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(VehicleCarryError::MissingPosition(actor))?;
        host.effect(
            self,
            VehicleCarryEffect::Particle {
                selector: SLOT_EFFECT_SELECTOR,
                position,
                flags: 0,
            },
            rng,
        )
        .map_err(VehicleCarryError::Host)
    }

    /// `004121D0`, a hold's `+4`: `actor` set down before `holder`.
    pub fn hold_set_down<H: VehicleCarryHost>(
        &mut self,
        holder: ObjectId,
        actor: ObjectId,
        terrain: &crate::terrain_runtime::TerrainRuntime,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), VehicleCarryError<H::Error>> {
        self.place_dropped_item(holder, actor, terrain, host, rng)
    }

    /// Virtual `+BC`: whether `actor` has a hold (the vehicle's +1A4,
    /// `0041ED30`; the Prison Unit's +144, `0046E6E0`; elsewhere
    /// `00401300`'s none).
    pub fn has_hold(&self, actor: ObjectId) -> bool {
        self.carry_slots(actor).is_ok()
    }

    /// `004120B0` on `holder`'s hold: whether a slot holds `handle`.
    pub fn hold_has(
        &self,
        holder: ObjectId,
        handle: ActorHandle,
    ) -> Result<bool, DerivedActorError> {
        Ok(self.carry_slots(holder)?.handles.contains(&handle))
    }

    /// The hold the slot array's methods (00412880, 004121D0, 004122C0,
    /// 004122A0) run on, the owner's +BC: the vehicle's +1A4 (0041ED30) or
    /// the Prison Unit's +144 (0046E6E0).
    pub(in crate::object_runtime) fn carry_slots(
        &self,
        owner: ObjectId,
    ) -> Result<&crate::object_runtime::VehicleHandleArray, DerivedActorError> {
        if let Ok(prison) = self.selector17(owner) {
            return Ok(&prison.hold_144);
        }
        Ok(&self.selector2_actor(owner)?.handles_1a4)
    }

    pub(in crate::object_runtime) fn carry_slots_mut(
        &mut self,
        owner: ObjectId,
    ) -> Result<&mut crate::object_runtime::VehicleHandleArray, DerivedActorError> {
        if self.selector17(owner).is_ok() {
            return Ok(&mut self.selector17_mut(owner)?.hold_144);
        }
        Ok(&mut self.selector2_actor_mut(owner)?.handles_1a4)
    }

    /// 004122C0 on `owner`'s hold with its own handle as source: whether a
    /// slot took `item`.
    pub fn take_into_hold<H: VehicleCarryHost>(
        &mut self,
        owner: ObjectId,
        item: ObjectId,
        context: VehicleCarryContext<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<bool, VehicleCarryError<H::Error>> {
        let source = self.actor(owner)?.handle;
        self.take_into_vehicle_slot(owner, source, item, context, host, rng)
    }

    /// 004122C0, the +1A4 array's +8.
    fn take_into_vehicle_slot<H: VehicleCarryHost>(
        &mut self,
        vehicle: ObjectId,
        source: ActorHandle,
        item: ObjectId,
        context: VehicleCarryContext<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<bool, VehicleCarryError<H::Error>> {
        let count = self.carry_slots(vehicle)?.handles.len();
        let handle = self.actor(item)?.handle;
        if self.carry_slots(vehicle)?.handles.contains(&handle) {
            return Ok(false);
        }
        let null = context
            .process
            .process_null_handle
            .ok_or(VehicleCarryError::MissingProcessNullHandle)?;
        for index in 0..count {
            if self.carry_slots(vehicle)?.handles[index] != null {
                continue;
            }
            // The capture writes the slot's +8 DWORD in place.
            let mut saved = *self.vehicle_slot(vehicle, index)?.0;
            let captured = self.capture_carried_actor(item, source, &mut saved, context);
            *self.vehicle_slot_mut(vehicle, index)?.0 = saved;
            if !captured? {
                return Ok(false);
            }
            let handle = self.actor(item)?.handle;
            self.carry_slots_mut(vehicle)?.handles[index] = handle;
            let position = self
                .object(item)?
                .base
                .position_words
                .ok_or(VehicleCarryError::MissingPosition(item))?;
            host.effect(
                self,
                VehicleCarryEffect::Particle {
                    selector: SLOT_EFFECT_SELECTOR,
                    position,
                    flags: 0,
                },
                rng,
            )
            .map_err(VehicleCarryError::Host)?;
            let state = if self.object(item)?.base.flags & 0x8000_0000 != 0 {
                1
            } else {
                2
            };
            *self.vehicle_slot_mut(vehicle, index)?.1 = Some(state);
            return Ok(true);
        }
        Ok(false)
    }

    fn vehicle_slot<E>(
        &self,
        vehicle: ObjectId,
        index: usize,
    ) -> Result<(&ActorCaptureSavedFlags, &Option<u32>), VehicleCarryError<E>> {
        let slots = self.carry_slots(vehicle)?;
        slots
            .saved_flags
            .get(index)
            .zip(slots.states.get(index))
            .ok_or(VehicleCarryError::SlotStorage { vehicle, index })
    }

    fn vehicle_slot_mut<E>(
        &mut self,
        vehicle: ObjectId,
        index: usize,
    ) -> Result<(&mut ActorCaptureSavedFlags, &mut Option<u32>), VehicleCarryError<E>> {
        let slots = self.carry_slots_mut(vehicle)?;
        slots
            .saved_flags
            .get_mut(index)
            .zip(slots.states.get_mut(index))
            .ok_or(VehicleCarryError::SlotStorage { vehicle, index })
    }

    /// The carried actor's +8C with operation 1 and the vehicle as source.
    pub(super) fn capture_carried_actor<E>(
        &mut self,
        actor: ObjectId,
        source: ActorHandle,
        saved: &mut ActorCaptureSavedFlags,
        context: VehicleCarryContext<'_>,
    ) -> Result<bool, VehicleCarryError<E>> {
        if self.selector12_actor(actor).is_ok() {
            return self.capture_selector12(actor, source, saved, context);
        }
        if self.selector73(actor).is_ok() {
            return self.capture_selector73(actor, source, saved, context);
        }
        if self.is_bomb29(actor) {
            return self.capture_bomb29(actor, 1, source, saved, context);
        }
        if self.selector52(actor).is_ok() {
            return self.capture_selector52(actor, source, saved, context);
        }
        // 00473FB0, the Remote Beamer's +8C: the shared body, and an
        // admitted attach empties the charge.
        if self.selector55(actor).is_ok() {
            let captured = self.shared_capture(actor, 1, source, saved, context)?;
            if captured {
                self.attach_selector55(actor)?;
            }
            return Ok(captured);
        }
        // The receiver's +8C is the shared body 0047D7C0 itself.
        if self.is_selector56(actor) {
            return self.shared_capture(actor, 1, source, saved, context);
        }
        // 0046E530, the Prison Unit's +8C: after the shared body an attach
        // takes the holder's team (dirty 2) and tells the first player 0x10
        // (0046C760, not silent).
        if self.selector17(actor).is_ok() {
            if !self.shared_capture(actor, 1, source, saved, context)? {
                return Ok(false);
            }
            if let Some(holder) = self.resolve_actor_handle(source)? {
                let team = self.actor(holder)?.word_b4;
                self.owned_mut(actor)?.state.base.dirty_word |= 2;
                self.actor_mut(actor)?.word_b4 = team;
                let player = self
                    .first_player()
                    .ok_or(VehicleCarryError::MissingFirstPlayer(actor))?;
                self.apply_player_command(
                    PlayerCommandRequest {
                        player,
                        operation: PlayerCommandOperation::Set {
                            raw_id: PRISON_ATTACH_COMMAND,
                            silent: 0,
                        },
                    },
                    &context.commands(),
                )?;
            }
            self.prison_carry_tail(actor, source, context)?;
            return Ok(true);
        }
        Err(VehicleCarryError::CaptureBoundary(actor))
    }

    /// `0046E57F..0046E625`, both arms of the Prison Unit's +8C: one
    /// holding its prisoner is [`VehicleCarryError::PrisonerTaskSource`];
    /// one that kept a kind `+15C` grants it to the player of `source`
    /// (0046B090, 004750C0).
    fn prison_carry_tail<E>(
        &mut self,
        prison: ObjectId,
        source: ActorHandle,
        context: VehicleCarryContext<'_>,
    ) -> Result<(), VehicleCarryError<E>> {
        let state = self.selector17(prison)?;
        if state.holding_15e != 0 {
            return Err(VehicleCarryError::PrisonerTaskSource(prison));
        }
        let kind = state.kind_15c;
        if kind == 0 {
            return Ok(());
        }
        let lookup = PlayerActorLookupContext {
            process: Some(context.process),
        };
        if let Some(player) =
            self.find_player_for_actor_handle(PlayerActorLookupRequest { handle: source }, lookup)?
        {
            self.mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::SetRaw {
                        id: u32::from(kind),
                    },
                },
                &LiveGrantContext {
                    process: Some(context.process),
                    executable: Some(context.executable),
                    resident: Some(context.resident),
                    text_data: context.text_data,
                },
            )?;
        }
        Ok(())
    }

    /// The shared body `0047D7C0` with `operation`, as a class's +8C
    /// reaches it.
    fn shared_capture<E>(
        &mut self,
        actor: ObjectId,
        operation: u32,
        source: ActorHandle,
        saved: &mut ActorCaptureSavedFlags,
        context: VehicleCarryContext<'_>,
    ) -> Result<bool, VehicleCarryError<E>> {
        // The shared body reads only the process NULL handle; it borrows a
        // scratch copy of the immutable owner.
        let mut process = *context.process;
        Ok(self.apply_shared_actor_capture(
            ActorCaptureRequest {
                actor,
                operation,
                source,
            },
            &mut ActorCaptureContext {
                process: &mut process,
                saved_flags: saved,
            },
            &mut CaptureCommands { context },
        )?)
    }

    /// 004547E0, the mining rig's +8C, on its take arm: a rig that holds a
    /// level refuses with the first player's hint 0x18 (0046C760); an empty
    /// one gives its loader's player (0046B090) hint 0x1F and the rig's grant
    /// 0x23 (004750C0 on +9C), then the shared body 0047D7C0.
    fn capture_selector52<E>(
        &mut self,
        actor: ObjectId,
        source: ActorHandle,
        saved: &mut ActorCaptureSavedFlags,
        context: VehicleCarryContext<'_>,
    ) -> Result<bool, VehicleCarryError<E>> {
        let hint = |arena: &mut Self, player, raw_id| {
            arena.apply_player_command(
                PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set { raw_id, silent: 0 },
                },
                &context.commands(),
            )
        };
        if self.selector52(actor)?.level_140 != 0 {
            let player = self
                .first_player()
                .ok_or(VehicleCarryError::MissingFirstPlayer(actor))?;
            hint(self, player, RIG_HOLDS_LEVEL_HINT)?;
            return Ok(false);
        }
        let lookup = PlayerActorLookupContext {
            process: Some(context.process),
        };
        if let Some(player) =
            self.find_player_for_actor_handle(PlayerActorLookupRequest { handle: source }, lookup)?
        {
            hint(self, player, RIG_TAKEN_HINT)?;
            self.mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::SetRaw { id: RIG_GRANT },
                },
                &LiveGrantContext {
                    process: Some(context.process),
                    executable: Some(context.executable),
                    resident: Some(context.resident),
                    text_data: context.text_data,
                },
            )?;
        }
        let mut process = *context.process;
        Ok(self.apply_shared_actor_capture(
            ActorCaptureRequest {
                actor,
                operation: 1,
                source,
            },
            &mut ActorCaptureContext {
                process: &mut process,
                saved_flags: saved,
            },
            &mut CaptureCommands { context },
        )?)
    }

    /// 00458CC0, the Bomb's +8C: its own part (a take disarms it, refills
    /// its fuse and names the holder its owner), the first player's hint
    /// (0x1A on a take, 0x19 on a drop; 0046C760, not silent), then the
    /// shared body 0047D7C0.
    fn capture_bomb29<E>(
        &mut self,
        actor: ObjectId,
        operation: u32,
        source: ActorHandle,
        saved: &mut ActorCaptureSavedFlags,
        context: VehicleCarryContext<'_>,
    ) -> Result<bool, VehicleCarryError<E>> {
        let hint = self.bomb29_capture(actor, operation, source)?;
        let player = self
            .first_player()
            .ok_or(VehicleCarryError::MissingFirstPlayer(actor))?;
        self.apply_player_command(
            PlayerCommandRequest {
                player,
                operation: PlayerCommandOperation::Set {
                    raw_id: hint,
                    silent: 0,
                },
            },
            &context.commands(),
        )?;
        let mut process = *context.process;
        Ok(self.apply_shared_actor_capture(
            ActorCaptureRequest {
                actor,
                operation,
                source,
            },
            &mut ActorCaptureContext {
                process: &mut process,
                saved_flags: saved,
            },
            &mut CaptureCommands { context },
        )?)
    }

    /// 0042D9E0, the ECM Unit's +8C, on its attach arm: the shared body
    /// 0047D7C0, and an admitted attach names the vehicle as the holder at
    /// +144.
    fn capture_selector73<E>(
        &mut self,
        actor: ObjectId,
        source: ActorHandle,
        saved: &mut ActorCaptureSavedFlags,
        context: VehicleCarryContext<'_>,
    ) -> Result<bool, VehicleCarryError<E>> {
        let mut process = *context.process;
        let captured = self.apply_shared_actor_capture(
            ActorCaptureRequest {
                actor,
                operation: 1,
                source,
            },
            &mut ActorCaptureContext {
                process: &mut process,
                saved_flags: saved,
            },
            &mut CaptureCommands { context },
        )?;
        if captured {
            self.attach_selector73(actor, source)?;
        }
        Ok(captured)
    }

    /// 00468220, selector 12's +8C, on its capture arm (a nonzero low byte
    /// of the operation; the take passes 1). The drop's release arm is
    /// [`Self::release_carried_actor`].
    fn capture_selector12<E>(
        &mut self,
        actor: ObjectId,
        source: ActorHandle,
        saved: &mut ActorCaptureSavedFlags,
        context: VehicleCarryContext<'_>,
    ) -> Result<bool, VehicleCarryError<E>> {
        let lookup = PlayerActorLookupContext {
            process: Some(context.process),
        };
        if let Some(player) =
            self.find_player_for_actor_handle(PlayerActorLookupRequest { handle: source }, lookup)?
        {
            self.merge_scientist_grants(actor, player, context)?;
            for (raw_id, silent) in [(0, 1), (1, 1), (0x48, 0)] {
                self.apply_player_command(
                    PlayerCommandRequest {
                        player,
                        operation: PlayerCommandOperation::Set { raw_id, silent },
                    },
                    &context.commands(),
                )?;
            }
        }
        self.family428650_actor_mut(actor)?.word_228 = CAPTURED_STATUS_TEXT;
        // The shared body reads only the process NULL handle; it borrows a
        // scratch copy of the immutable owner.
        let mut process = *context.process;
        Ok(self.apply_shared_actor_capture(
            ActorCaptureRequest {
                actor,
                operation: 1,
                source,
            },
            &mut ActorCaptureContext {
                process: &mut process,
                saved_flags: saved,
            },
            &mut CaptureCommands { context },
        )?)
    }

    /// 00475350 into the player's +9C from the scientist's +24C, then
    /// event 22 with zero radius at the scientist (0047DAF0).
    fn merge_scientist_grants<E>(
        &mut self,
        actor: ObjectId,
        player: crate::object_runtime::PlayerId,
        context: VehicleCarryContext<'_>,
    ) -> Result<(), VehicleCarryError<E>> {
        self.mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Player9c(player),
                operation: LiveGrantOperation::Merge {
                    source: GrantOwner::Selector12Actor24c(actor),
                },
            },
            &LiveGrantContext {
                process: Some(context.process),
                executable: Some(context.executable),
                resident: Some(context.resident),
                text_data: context.text_data,
            },
        )?;
        self.notify_actor_grant_difference(
            ActorGrantDifferenceNotificationRequest {
                delivery: ActorGrantDifferenceDelivery::ActorList,
            },
            PlayerActorLookupContext {
                process: Some(context.process),
            },
        )?;
        Ok(())
    }

    /// 00468160's vehicle arm, for a scientist the pair pass found alive,
    /// outside team 0x12 and touching `vehicle`. Returns the take's result,
    /// which ends the pair before its physics.
    pub fn board_vehicle<H: VehicleCarryHost>(
        &mut self,
        scientist: ObjectId,
        vehicle: ObjectId,
        context: VehicleCarryContext<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<bool, VehicleCarryError<H::Error>> {
        self.selector12_actor(scientist)?;
        let handle = self.actor(vehicle)?.handle;
        let lookup = PlayerActorLookupContext {
            process: Some(context.process),
        };
        if let Some(player) =
            self.find_player_for_actor_handle(PlayerActorLookupRequest { handle }, lookup)?
        {
            self.merge_scientist_grants(scientist, player, context)?;
        }
        self.attach_vehicle_item(vehicle, scientist, context, host, rng)
    }

    /// 00412880 / 00412110 on the vehicle's +1A4 from `index`.
    pub fn drop_vehicle_item<H: VehicleDropHost>(
        &mut self,
        vehicle: ObjectId,
        index: usize,
        terrain: &crate::terrain_runtime::TerrainRuntime,
        context: VehicleCarryContext<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), VehicleCarryError<H::Error>> {
        let count = self.carry_slots(vehicle)?.handles.len();
        let mut index = index;
        while index < count {
            match self.vehicle_slot(vehicle, index)?.1 {
                Some(SLOT_REMOTE) => index += 1,
                Some(_) => break,
                None => return Err(VehicleCarryError::UnknownSlotState { vehicle, index }),
            }
        }
        if index >= count {
            return Ok(());
        }
        let handle = self.carry_slots(vehicle)?.handles[index];
        let Some(actor) = self.resolve_actor_handle(handle)? else {
            return Ok(());
        };
        self.place_dropped_item(vehicle, actor, terrain, host, rng)?;
        if self.object(actor)?.base.flags & 0x8000_0000 != 0 {
            *self.vehicle_slot_mut(vehicle, index)?.1 = Some(SLOT_REMOTE);
            return Ok(());
        }
        let null = context
            .process
            .process_null_handle
            .ok_or(VehicleCarryError::MissingProcessNullHandle)?;
        // The release reads (and may write) the slot's +8 DWORD in place.
        let mut saved = *self.vehicle_slot(vehicle, index)?.0;
        let released = self.release_carried_actor(actor, null, &mut saved, context, host, rng);
        *self.vehicle_slot_mut(vehicle, index)?.0 = saved;
        released?;
        host.attach_effect(self, actor, rng)
            .map_err(VehicleCarryError::Host)?;
        self.carry_slots_mut(vehicle)?.handles[index] = null;
        *self.vehicle_slot_mut(vehicle, index)?.1 = Some(0);
        self.compact_vehicle_slots::<VehicleCarryError<H::Error>>(vehicle, null)?;
        Ok(())
    }

    /// 004121D0, the array's +4: three times the vehicle's radius +28 plus
    /// the actor's, as a signed WORD, along the vehicle's forward row from
    /// its +32. The actor takes the vehicle's +6C, that position with a
    /// zero velocity (0047DDF0), the ground height there (00479DD0) and the
    /// slot flash 00489A90(2).
    fn place_dropped_item<H: VehicleCarryHost>(
        &mut self,
        vehicle: ObjectId,
        actor: ObjectId,
        terrain: &crate::terrain_runtime::TerrainRuntime,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), VehicleCarryError<H::Error>> {
        let reach = (u32::from(self.object(vehicle)?.base.word_28) * 3)
            .wrapping_add(u32::from(self.object(actor)?.base.word_28)) as i16;
        let basis = self.actor(vehicle)?.rotating.basis;
        let origin = self
            .object(vehicle)?
            .base
            .position_words
            .ok_or(VehicleCarryError::MissingPosition(vehicle))?;
        // x87 multiplies exactly before __ftol truncates.
        let mut position: [u16; 3] = std::array::from_fn(|axis| {
            let offset = f64::from(reach) * f64::from(f32::from_bits(basis.words[7 + axis]));
            origin[axis].wrapping_add(offset as i32 as u16)
        });
        let (identity, unknown) = {
            let vehicle = self.actor(vehicle)?;
            (vehicle.word_6c, vehicle.unknown_6c)
        };
        let state = self.actor_mut(actor)?;
        state.word_6c = identity;
        state.unknown_6c = unknown;
        self.owned_mut(actor)?.state.base.position_words = Some(position);
        self.actor_mut(actor)?.words_8e_92 = [0; 3];
        position[1] = terrain.height_at([position[0], position[2]]) as u16;
        self.owned_mut(actor)?.state.base.position_words = Some(position);
        host.effect(
            self,
            VehicleCarryEffect::Particle {
                selector: SLOT_EFFECT_SELECTOR,
                position,
                flags: 0,
            },
            rng,
        )
        .map_err(VehicleCarryError::Host)
    }

    /// The dropped actor's +8C with operation 0. Selector 12's 00468220
    /// releases (004682A1): +22E |= 0xC and its helper history +218
    /// cleared (00483400), its targets +1EC, +274 and +27C dropped
    /// (00427110 with nothing), a fresh target search (+104), then the
    /// shared release 0047D7C0.
    fn release_carried_actor<H: VehicleDropHost>(
        &mut self,
        actor: ObjectId,
        null: ActorHandle,
        saved: &mut ActorCaptureSavedFlags,
        context: VehicleCarryContext<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), VehicleCarryError<H::Error>> {
        if self.selector73(actor).is_ok() {
            return self.release_selector73(actor, null, saved, context);
        }
        // 0046E530's release arm: the shared body, then its tail.
        if self.selector17(actor).is_ok() {
            if self.shared_capture(actor, 0, null, saved, context)? {
                self.prison_carry_tail(actor, null, context)?;
            }
            return Ok(());
        }
        // The Remote Beamer's 00473FB0 and the receiver's 0047D7C0 release
        // through the shared body alone.
        if self.selector55(actor).is_ok() || self.is_selector56(actor) {
            return self
                .shared_capture(actor, 0, null, saved, context)
                .map(|_| ());
        }
        if self.is_bomb29(actor) {
            return self
                .capture_bomb29(actor, 0, null, saved, context)
                .map(|_| ());
        }
        if self.selector52(actor).is_ok() {
            // 00454839..00454867: a release the shared body admits calls
            // the mines, and any answer starts the filling.
            let mut process = *context.process;
            let released = self.apply_shared_actor_capture(
                ActorCaptureRequest {
                    actor,
                    operation: 0,
                    source: null,
                },
                &mut ActorCaptureContext {
                    process: &mut process,
                    saved_flags: saved,
                },
                &mut CaptureCommands { context },
            )?;
            if released
                && host
                    .call_mines(self, actor, RIG_MINE_REACH)
                    .map_err(VehicleCarryError::Host)?
                    != 0
            {
                self.selector52_mut(actor)?.filling_142 = 1;
            }
            return Ok(());
        }
        if self.selector12_actor(actor).is_err() {
            return Err(VehicleCarryError::CaptureBoundary(actor));
        }
        let helper = self.family428650_actor(actor)?.helper_218.is_some();
        self.family428650_actor_mut(actor)?.word_22e |= 0xc;
        if helper {
            self.clear_family_helper_history(
                actor,
                crate::object_runtime::FamilyHelperHistoryInputs {
                    process_null_handle: Some(null),
                },
            )?;
        }
        for slot in [
            crate::object_runtime::FamilyTargetSlot::Target1ec,
            crate::object_runtime::FamilyTargetSlot::Target274,
            crate::object_runtime::FamilyTargetSlot::Target27c,
        ] {
            self.publish_family_target(
                actor,
                slot,
                crate::object_runtime::WeightedTargetRequest {
                    referent: crate::object_runtime::WeightedTargetReferent::Actor(None),
                    score: 0,
                    constructor_null: crate::object_runtime::WeightedTargetConstructorNull::Process(
                        context.process,
                    ),
                },
            )?;
        }
        host.select_targets(self, actor, rng)
            .map_err(VehicleCarryError::Host)?;
        // The shared body reads only the process NULL handle; it borrows a
        // scratch copy of the immutable owner.
        let mut process = *context.process;
        self.apply_shared_actor_capture(
            ActorCaptureRequest {
                actor,
                operation: 0,
                source: null,
            },
            &mut ActorCaptureContext {
                process: &mut process,
                saved_flags: saved,
            },
            &mut CaptureCommands { context },
        )?;
        Ok(())
    }

    /// 0042D9E0's detach arm: the shared release 0047D7C0, and an admitted
    /// release drops the holder (`0042DA0F..0042DA43`).
    fn release_selector73<E>(
        &mut self,
        actor: ObjectId,
        null: ActorHandle,
        saved: &mut ActorCaptureSavedFlags,
        context: VehicleCarryContext<'_>,
    ) -> Result<(), VehicleCarryError<E>> {
        let mut process = *context.process;
        let released = self.apply_shared_actor_capture(
            ActorCaptureRequest {
                actor,
                operation: 0,
                source: null,
            },
            &mut ActorCaptureContext {
                process: &mut process,
                saved_flags: saved,
            },
            &mut CaptureCommands { context },
        )?;
        if released {
            self.release_selector73_holder(actor, null)?;
        }
        Ok(())
    }

    /// 00411E70, the A key's request (control +2C): the hold compacts
    /// (00411F60), then its first N slots turn by one, N the slots whose
    /// handle is not the process NULL (the count less 004120E0's): the first
    /// to the last when `forward`, else the last to the first. A handle that
    /// stays behind in the compaction keeps its place, so a hole among the
    /// first N turns with them, as retail's does.
    pub(super) fn rotate_vehicle_slots<Error>(
        &mut self,
        vehicle: ObjectId,
        forward: bool,
        null: ActorHandle,
    ) -> Result<(), Error>
    where
        Error: From<DerivedActorError> + From<crate::object_runtime::ActorConstructionError>,
    {
        self.compact_vehicle_slots::<Error>(vehicle, null)?;
        let slots = self.carry_slots_mut(vehicle)?;
        let turned = slots
            .handles
            .iter()
            .filter(|&&handle| handle != null)
            .count();
        if turned == 0 {
            return Ok(());
        }
        fn turn<T>(values: &mut [T], turned: usize, forward: bool) {
            if forward {
                values[..turned].rotate_left(1);
            } else {
                values[..turned].rotate_right(1);
            }
        }
        turn(&mut slots.handles, turned, forward);
        turn(&mut slots.saved_flags, turned, forward);
        turn(&mut slots.states, turned, forward);
        Ok(())
    }

    /// 00411F60: a handle repeated later in the array is cleared there; then
    /// the live handles move to the front in order (with their saved flags
    /// and state), leaving the process NULL where they were. A handle that
    /// no longer resolves is dropped from the count without moving.
    pub(super) fn compact_vehicle_slots<Error>(
        &mut self,
        vehicle: ObjectId,
        null: ActorHandle,
    ) -> Result<(), Error>
    where
        Error: From<DerivedActorError> + From<crate::object_runtime::ActorConstructionError>,
    {
        let count = self.carry_slots(vehicle)?.handles.len();
        let mut occupied = 0usize;
        for index in 0..count {
            let handle = self.carry_slots(vehicle)?.handles[index];
            if handle == null {
                continue;
            }
            occupied += 1;
            let slots = self.carry_slots_mut(vehicle)?;
            if let Some(later) = (index + 1..count).find(|&later| slots.handles[later] == handle) {
                slots.handles[later] = null;
            }
        }
        let mut source = 0usize;
        let mut destination = 0usize;
        while destination < occupied {
            // The live handles counted above bound this scan.
            let handles = &self.carry_slots(vehicle)?.handles;
            let Some(next) = handles[source..].iter().position(|&handle| handle != null) else {
                break;
            };
            source += next;
            let handle = handles[source];
            if self.resolve_actor_handle(handle)?.is_none() {
                occupied -= 1;
            } else {
                if destination != source {
                    let slots = self.carry_slots_mut(vehicle)?;
                    slots.handles[destination] = handle;
                    slots.saved_flags[destination] = slots.saved_flags[source];
                    slots.states[destination] = slots.states[source];
                    slots.handles[source] = null;
                }
                destination += 1;
            }
            source += 1;
        }
        Ok(())
    }

    /// The carried actor's +5C. Selector 12 binds 00404610, itself. The
    /// hold's items bind 00401300, NULL, so 00421810 skips the team change:
    /// the Prison Unit (005272A0), whose +8C takes the carrier's team first,
    /// the Bomb (00525AB4), the mining rig (00526714), the Remote Beamer and
    /// its receiver (005274C4, 00526530) and the ECM Unit (005262CC).
    fn carried_actor_view<E>(
        &self,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, VehicleCarryError<E>> {
        if self.selector12_actor(actor).is_ok() {
            return Ok(Some(actor));
        }
        let item = self.is_bomb29(actor)
            || matches!(
                self.actor(actor)?.extension,
                crate::object_runtime::ActorExtension::Selector17(_)
                    | crate::object_runtime::ActorExtension::Selector52(_)
                    | crate::object_runtime::ActorExtension::Selector55(_)
                    | crate::object_runtime::ActorExtension::Selector56
                    | crate::object_runtime::ActorExtension::Selector73(_)
            );
        if item {
            return Ok(None);
        }
        Err(VehicleCarryError::ViewBoundary(actor))
    }

    /// 00468C40 on a selector-12 view. An unchanged team returns at once.
    /// 00468C40, selector 12's team change, on the carried actor's view.
    pub(in crate::object_runtime) fn join_vehicle_team<E>(
        &mut self,
        view: ObjectId,
        team: u32,
        context: VehicleCarryContext<'_>,
    ) -> Result<(), VehicleCarryError<E>> {
        if self.actor(view)?.word_b4 == team {
            return Ok(());
        }
        self.owned_mut(view)?.state.base.dirty_word |= 2;
        self.actor_mut(view)?.word_b4 = team;
        let family = self.family428650_actor_mut(view)?;
        family.count_1a0 = 0;
        family.word_22e |= 1;
        for slot in [
            crate::object_runtime::FamilyTargetSlot::Target1dc,
            crate::object_runtime::FamilyTargetSlot::Target1d4,
            crate::object_runtime::FamilyTargetSlot::Target1ec,
            crate::object_runtime::FamilyTargetSlot::Target1f4,
        ] {
            self.publish_family_target(
                view,
                slot,
                crate::object_runtime::WeightedTargetRequest {
                    referent: crate::object_runtime::WeightedTargetReferent::Actor(None),
                    score: 0,
                    constructor_null: crate::object_runtime::WeightedTargetConstructorNull::Process(
                        context.process,
                    ),
                },
            )?;
        }
        let handle = self.actor(view)?.handle;
        for candidate in self.actor_list().to_vec() {
            if self.actor(candidate)?.word_b4 != team {
                continue;
            }
            let Some(component) = self.actor_derived_component(candidate)? else {
                continue;
            };
            if !matches!(
                self.derived_actor(component)?.class,
                crate::object_runtime::DerivedActorClass::Family428650(_)
            ) {
                continue;
            }
            self.forget_family_table_handle(
                crate::object_runtime::FamilyTargetTable { owner: component },
                handle,
                context.process,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "carry_tests.rs"]
mod tests;
