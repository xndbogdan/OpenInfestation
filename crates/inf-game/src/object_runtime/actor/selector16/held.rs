//! The camera's +44 `0040F890`: the player block calls it while the player
//! holds the camera (+D8), as after the vehicle's death.
//!
//! The camera's +180 counts the time held (its +88 `0040F880` cleared it
//! when the player took it). Before the wait (2 s, or 0.5 s while the first
//! player's vehicle is alive, or 0.75 s in network play) the controls are
//! cleared every tick, and a target that no longer resolves (a detonated
//! Cranial Implanter's) gives way to the nearest actor within 512. Then any
//! control input, or 5 s held, hands the player back: `0046AE90` detaches
//! the camera, `0046A890` respawns a vehicle when the first player's is gone
//! or dying, and the camera follows the vehicle.

use thiserror::Error;

use crate::object_runtime::{
    CameraFactoryInputs, FollowTargetRequest, ObjectArena, ObjectError, ObjectId,
    PlayerActorLookupContext, PlayerActorLookupError, PlayerActorLookupRequest, PlayerAttachError,
    PlayerControlBlock, PlayerId,
};

/// `0040F8AE`: the wait before the controls hand the player back.
const DEAD_WAIT: u32 = 2_000_000;
/// `0040F8C7`: the wait while the first player's vehicle is alive.
const LIVE_WAIT: u32 = 500_000;
/// `0040F8D6`: the wait in network play.
const NETWORK_WAIT: u32 = 750_000;
/// `0040F9EA`: without input, the camera hands the player back after 5 s.
const IDLE_LIMIT: u32 = 5_000_000;

/// The respawn the held camera reaches.
pub trait HeldCameraHost {
    type Error;
    /// `0046A890(player, network)`.
    fn respawn(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        factory: &mut CameraFactoryInputs<'_>,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum HeldCameraError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Lookup(#[from] PlayerActorLookupError),
    #[error("camera {0:?}'s +180 is unwritten where 0040F8DF reads it")]
    UnwrittenTimer(ObjectId),
    #[error("0040F890 requires the process NULL 0054E894")]
    MissingProcessNull,
    #[error("0046A890 respawn: {0}")]
    Respawn(E),
}

/// `0040F92C..0040F94C`: the scanner's squared X/Z range, 512 units.
const NEAREST_RANGE_SQUARED: u32 = 0x40000;

impl ObjectArena {
    /// Scanner B `00475F50` (`0051F194`) from the camera itself over the
    /// actor list: flag 0x20 set and 0x80 clear, X/Z within 512, not the
    /// camera. Its filter `0052059C` (`00411080`) scores -1 less the squared
    /// distance, so the nearest wins, the first of equals in list order.
    fn held_camera_nearest(&self, camera: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        let position = |actor: ObjectId| {
            self.object(actor)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(actor))
        };
        let source = position(camera)?;
        let (mut best, mut best_score) = (None, 0_u32);
        for &candidate in self.actor_list() {
            let flags = self.object(candidate)?.base.flags;
            if flags & 0x80 != 0 || flags & 0x20 != 0x20 {
                continue;
            }
            let other = position(candidate)?;
            let dx = i32::from(source[0].wrapping_sub(other[0]) as i16);
            let dz = i32::from(source[2].wrapping_sub(other[2]) as i16);
            let squared = dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)) as u32;
            if squared >= NEAREST_RANGE_SQUARED || candidate == camera {
                continue;
            }
            let score = u32::MAX.wrapping_sub(squared);
            if score > best_score {
                (best, best_score) = (Some(candidate), score);
            }
        }
        Ok(best)
    }

    /// `0040F890(delta, controls)` on the camera the player holds.
    pub fn tick_held_camera<H: HeldCameraHost>(
        &mut self,
        camera: ObjectId,
        delta: u32,
        controls: &mut PlayerControlBlock,
        factory: &mut CameraFactoryInputs<'_>,
        host: &mut H,
    ) -> Result<(), HeldCameraError<H::Error>> {
        let process = factory.process.ok_or(HeldCameraError::MissingProcessNull)?;
        let wait = match self.first_player_primary_actor()? {
            Some(vehicle) if self.actor(vehicle)?.word_c4 == 3 => LIVE_WAIT,
            _ if process.network_flag != 0 => NETWORK_WAIT,
            _ => DEAD_WAIT,
        };
        let held = self
            .selector16(camera)?
            .dword_180
            .ok_or(HeldCameraError::UnwrittenTimer(camera))?
            .wrapping_add(delta);
        self.selector16_mut(camera)?.dword_180 = Some(held);
        if (held as i32) < wait as i32 {
            // 004959E0 clears the 44h-byte block.
            *controls = PlayerControlBlock::default();
            if let Some(target) = self.selector16(camera)?.target_18c {
                // The follow target's +10 (004827C0) resolves its handle.
                let handle = self.actor_follow_target(target)?.handle_04;
                let resolved = self
                    .resolve_actor_handle(handle)
                    .map_err(|_| ObjectError::MissingActorRegistry)?;
                // 0040F91F..0040F9A2: gone, the nearest actor takes its
                // place; without one the camera keeps the lost target.
                if resolved.is_none() {
                    if let Some(found) = self.held_camera_nearest(camera)? {
                        self.release_actor_service(target)?;
                        let handle = self.actor(found)?.handle;
                        let next = self
                            .follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
                        self.selector16_mut(camera)?.target_18c = next;
                    }
                }
            }
            return Ok(());
        }
        let idle = controls.turn_14 == 0
            && controls.pitch_18 == 0
            && controls.thrust_1c == 0
            && controls.fire_20 == 0
            && controls.dword_28 == 0
            && controls.dword_2c == 0
            && controls.dword_34 == 0
            && controls.bytes_38[2..5] == [0, 0, 0];
        if idle && held as i32 <= IDLE_LIMIT as i32 {
            return Ok(());
        }
        *controls = PlayerControlBlock::default();
        let handle = self.actor(camera)?.handle;
        let player = self.find_player_for_actor_handle(
            PlayerActorLookupRequest { handle },
            PlayerActorLookupContext {
                process: Some(process),
            },
        )?;
        self.actor_mut(camera)?.words_8e_92 = [0; 3];
        let Some(player) = player else {
            return Ok(());
        };
        if self.world_player(player)?.byte_ee == 0 {
            return Ok(());
        }
        let null = process
            .process_null_handle
            .ok_or(HeldCameraError::MissingProcessNull)?;
        self.attach_player(player, null, factory)?;
        let alive = |arena: &mut Self| -> Result<Option<ObjectId>, ObjectError> {
            match arena.first_player_primary_actor()? {
                Some(vehicle) if arena.actor(vehicle)?.word_c4 as i32 >= 3 => Ok(Some(vehicle)),
                _ => Ok(None),
            }
        };
        if alive(self)?.is_none() {
            // The block is the player's +04 in place: the respawn's 0046A890
            // writes +28, +2C and +34 into it after the clear above, and the
            // caller stores this copy back.
            self.world_player_mut(player)?.controls = *controls;
            host.respawn(self, player, factory)
                .map_err(HeldCameraError::Respawn)?;
            *controls = self.world_player(player)?.controls;
        }
        match self.first_player_primary_actor()? {
            Some(vehicle) if self.actor(vehicle)?.word_c4 == 3 => {
                let handle = self.actor(vehicle)?.handle;
                let target =
                    self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
                self.replace_camera_target(camera, target, factory.settings)?;
                self.selector16_mut(camera)?.byte_187 = 1;
                self.set_camera_view_mode(camera, factory.settings.first_person as u8)?;
            }
            _ => {
                self.selector16_mut(camera)?.dword_180 = Some(IDLE_LIMIT);
                self.attach_player(player, null, factory)?;
                self.bind_player_vehicle(player, null)?;
            }
        }
        Ok(())
    }
}
