//! Selector-16 live pose, 0040FD10. Required hosts own terrain and the
//! derived-actor/player operations rather than supplying captured poses.

use crate::object_runtime::{FollowTargetRequest, NativeHeapRef};
use inf_formats::native_basis::{
    normalize_words, orient_from_forward, NativeBasis, NativeBasisMath, NativeLengthPrecision,
};
use thiserror::Error;

use super::{CameraSettings, CameraSlewError, CameraSlewRequest, Selector16State};
use crate::object_runtime::{
    ActorHandle, ObjectArena, ObjectError, ObjectId, PlayerActorLookupError,
};

pub(super) mod rotation;
mod shake;
mod targets;
mod terrain;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraDerivedComponent {
    /// Actor virtual +64, independent of its +C4 lifecycle state.
    pub object: ObjectId,
    /// Component byte +1C0 forces special camera mode four.
    pub special_view_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CameraDesiredTargets {
    pub eye_position: [u16; 3],
    pub eye_velocity: [u16; 3],
    pub look_position: [u16; 3],
    pub look_velocity: [u16; 3],
}

/// Genuine vehicle branches of 0040FD10; storage modes remain native DWORDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraDerivedTargetMode {
    Chase,
    SpecialView,
}

/// Reached mutable parameter custody. An error retains earlier vector writes.
pub struct CameraDerivedTargetRequest<'a> {
    pub component: CameraDerivedComponent,
    pub mode: CameraDerivedTargetMode,
    pub eye: &'a mut super::CameraParameterState,
    pub look: &'a mut super::CameraParameterState,
    pub basis: &'a mut NativeBasis,
    /// Actual refreshed camera +18C owner. SpecialView consumes its +1A
    /// velocity only after the leaf, eye publication and fresh actor XYZ.
    pub primary_target: Option<NativeHeapRef>,
    pub settings_7c: u32,
    pub math: &'a NativeBasisMath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraHeightRequest {
    /// Signed H12 cell byte shifted by 005372F8, 0040F511..580.
    RawTerrainCell { xz: [u16; 2] },
    /// 00450A30(x,z,0), including its interpolation/wrap policy.
    InterpolatedTerrain { xz: [u16; 2] },
    /// 00410F60(x,z,0,H14.flags&40,terrain_height). Coordinates are the
    /// desired eye, and the terrain height is the fifth stack argument.
    WaterSurface { xz: [u16; 2], terrain_height: i16 },
    /// H12 DWORD zero arithmetic-shifted by process 005483BC, low signed WORD.
    WaterPlane,
}

/// Host boundaries reached by 0040FD10. Implementations must either execute
/// the recovered operation or return a feature-specific error.
pub trait CameraPoseHost {
    fn query_height(&mut self, request: CameraHeightRequest) -> Result<i16, CameraPoseError>;
    fn derived_component(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<CameraDerivedComponent>, CameraPoseError>;
    /// 004218E0 commits eye before its late target reads. 00421D50 also owns
    /// mutable basis custody; an unavailable branch must return an error.
    fn update_derived_camera_targets(
        &mut self,
        arena: &mut ObjectArena,
        request: CameraDerivedTargetRequest<'_>,
    ) -> Result<(), CameraPoseError>;
    /// 0040FB40's actor ranking and random draw sequence.
    fn reselect_target(
        &mut self,
        arena: &ObjectArena,
        rng: &mut u32,
    ) -> Result<Option<ActorHandle>, CameraPoseError>;
    /// 0046B090/00488AA0/0046B060 and conditional 0046AE90 transfer. The
    /// camera has already received its native escape velocity/position.
    fn camera_target_lost(
        &mut self,
        arena: &mut ObjectArena,
        camera: ObjectId,
        previous_handle: ActorHandle,
    ) -> Result<(), CameraPoseError>;
    /// 0048F4D0's signed, saturated lookup in the process inverse-sine table
    /// at 0053FF00. The runtime applies 00410B70 and samples each basis axis
    /// at its native point, including renormalization by the first rotation.
    fn inverse_sine(&mut self, value: i32) -> Result<i32, CameraPoseError>;
    /// 0047A760 transforms a local signed-WORD rotation vector through the
    /// current basis, then 00490FC0 applies it. This is not three Euler calls.
    fn rotate_local_vector(
        &mut self,
        basis: &mut NativeBasis,
        rotation_words: [u16; 3],
    ) -> Result<(), CameraPoseError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraGlobalState {
    /// Native DWORD 0052F510, initialized to 5,000,000 by executable data.
    pub shake_remaining_micros: u32,
    /// Process DWORDs 005469F0/F4; both are BSS zero at startup.
    pub shake_pause_micros: u32,
    pub shake_step_micros: u32,
    /// Process WORDs 00546958/5A/5C; rotation jitter persists across targets.
    pub rotation_jitter: [u16; 3],
    /// File-backed zero WORDs005469E8/EA/EC, then the last successful pose
    ///publication. Explicit unavailable fixture sources may still use None.
    pub cached_position: Option<[u16; 3]>,
    /// World-manager byte +5C5, written before the mode-specific targets.
    pub special_view_active: bool,
    /// World-manager WORD +5C0, the view zoom: zero from the session's
    /// gameplay object (00487C52), written only by the free look 00421DC0,
    /// read by the gameplay draw's focal (00442080) and predraw (00488380).
    pub view_zoom_5c0: std::cell::Cell<u16>,
    /// World-manager byte +5C3, infrared vision as the draws, the light grid
    /// and the tint read it: zero from the gameplay object (00487C5F), then
    /// each tick's copy of +5C4 (004875CE).
    pub infrared_5c3: bool,
    /// World-manager byte +5C4, infrared vision's request: zero from the
    /// gameplay object (00487C65) and each world load (0048DD5B), `!+5C3`
    /// when L is pressed with Infrared vision owned (00417932). While set,
    /// every frame clears to black (00438120).
    pub infrared_5c4: std::cell::Cell<bool>,
    /// Non-native: thermal vision shows ironbow, not white hot (Vulkan's
    /// step of the lamps' cycle, [`Self::step_lamps`]).
    pub ironbow: std::cell::Cell<bool>,
    /// Non-native: the session plays the remaster's game
    /// ([`crate::presentation::GameView`]), whose thermal camera adds the
    /// ironbow step; the draw sets it.
    pub thermal_palettes: std::cell::Cell<bool>,
}

impl CameraGlobalState {
    /// Port fix (2026-10-23): with both Infrared vision and the
    /// Headlights owned, L steps from off to the headlights, then to
    /// infrared (white hot), under Vulkan then to ironbow, then off;
    /// retail's L toggled infrared alone and left the headlights as they
    /// were. Takes the headlights' byte (`+208`, nonzero lit) and gives its
    /// new value, setting infrared's request (`+5C4`) and the palette.
    pub fn step_lamps(&self, headlights: u8) -> u8 {
        let (lit, infrared, ironbow) = if !self.infrared_5c3 && headlights == 0 {
            (true, false, false)
        } else if !self.infrared_5c3 {
            (false, true, false)
        } else if self.thermal_palettes.get() && !self.ironbow.get() {
            (false, true, true)
        } else {
            (false, false, false)
        };
        self.infrared_5c4.set(infrared);
        self.ironbow.set(ironbow);
        u8::from(lit)
    }
}

impl CameraGlobalState {
    pub const INITIAL: Self = Self {
        shake_remaining_micros: 5_000_000,
        shake_pause_micros: 0,
        shake_step_micros: 0,
        rotation_jitter: [0; 3],
        cached_position: Some([0; 3]),
        special_view_active: false,
        view_zoom_5c0: std::cell::Cell::new(0),
        infrared_5c3: false,
        infrared_5c4: std::cell::Cell::new(false),
        ironbow: std::cell::Cell::new(false),
        thermal_palettes: std::cell::Cell::new(false),
    };
}

pub struct CameraPoseTickRequest<'a> {
    pub microseconds: u32,
    pub settings: CameraSettings,
    pub settings_7c: u32,
    /// Process float vector 0055F280; do not derive from the current basis.
    pub up: [f32; 3],
    /// Process DWORD 005483B0. Zero resets the shake interval/pause only.
    pub shake_enabled: u32,
    pub globals: &'a mut CameraGlobalState,
    pub rng: &'a mut u32,
    pub math: &'a NativeBasisMath,
    pub host: &'a mut dyn CameraPoseHost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraPose {
    pub position_words: [u16; 3],
    pub basis: NativeBasis,
    /// No target reaches the native uninitialized lifecycle-state return slot.
    pub target_state: Option<u32>,
}

impl CameraPose {
    /// Selector-16 draw callback 00410D90 sign-extends every position WORD
    /// and copies the complete ten-DWORD basis into the world draw context.
    pub fn draw_request(self) -> super::native::CameraDrawRequest {
        super::native::CameraDrawRequest {
            eye: self.position_words.map(|word| i32::from(word as i16)),
            basis: self.basis,
            coordinate_shift: 16,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CameraPoseError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("camera derived target {0:?} is not the selector-2 vehicle")]
    NotSelector2CameraTarget(ObjectId),
    #[error(transparent)]
    PlayerLookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    Slew(#[from] CameraSlewError),
    #[error("camera target {0} has no refreshed focus/velocity")]
    UnwrittenTarget(&'static str),
    #[error("camera local offset +194 has not been written")]
    UnwrittenOffset,
    #[error("vehicle camera targets require current process NULL at 004218E4/0054E894")]
    MissingVehicleProcessNullHandle,
    #[error("vehicle {actor:?} special-view storage +{offset:03X} lacks {bytes} known bytes")]
    UninitializedSpecialViewStorage {
        actor: ObjectId,
        offset: u16,
        bytes: u8,
    },
    #[error("camera pose needs the unowned native operation {0}")]
    UnownedHostOperation(&'static str),
    #[error("camera terrain line would fault in native signed division")]
    DivisionFault,
    #[error("camera host operation failed: {0}")]
    Host(String),
}

impl ObjectArena {
    /// Live selector-16 callback 0040FD10. Refresh targets before computing
    /// desired coordinates; smooth, clamp, and orient before publishing the
    /// final world pose. Process shake state and RNG remain caller-owned.
    pub fn tick_camera_pose(
        &mut self,
        camera: ObjectId,
        mut request: CameraPoseTickRequest<'_>,
    ) -> Result<CameraPose, CameraPoseError> {
        let mut state = self.selector16(camera)?.clone();
        let result = self.tick_camera_pose_state(camera, &mut state, &mut request);
        // Required-host failure skips the unavailable operation, preserving
        // target refreshes and parameter/timer writes already reached.
        *self.selector16_mut(camera)? = state;
        result
    }

    fn tick_camera_pose_state(
        &mut self,
        camera: ObjectId,
        state: &mut Selector16State,
        request: &mut CameraPoseTickRequest<'_>,
    ) -> Result<CameraPose, CameraPoseError> {
        let mut class = None;
        let mut actor = None;
        if let Some(reference) = state.target_18c {
            class = Some(self.refresh_follow_target_resource(reference, request.settings_7c)?);
            actor = self
                .resolve_actor_handle(self.actor_follow_target(reference)?.handle_04)
                .map_err(|_| ObjectError::MissingActorRegistry)?;
        }
        if state.byte_186 != 0 {
            let valid = if let Some(actor) = actor {
                let flags = self.object(actor)?.base.flags;
                flags & 1 != 0
                    && flags & 0x8000_0000 == 0
                    && request.host.derived_component(self, actor)?.is_some()
                    && class == Some(3)
            } else {
                false
            };
            if valid {
                state.byte_186 = 0;
            } else {
                state.dword_19c = state.dword_19c.wrapping_add(request.microseconds);
                if actor.is_none() || state.dword_19c as i32 > 5_000_000 {
                    state.dword_19c = 0;
                    let selected = request.host.reselect_target(self, request.rng)?;
                    if let Some(old) = state.target_18c {
                        self.release_actor_service(old)?;
                    }
                    let replacement = if let Some(handle) = selected {
                        self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))?
                    } else {
                        None
                    };
                    state.target_18c = replacement;
                    state.mode_188 = 3;
                }
            }
            if let Some(reference) = state.target_18c {
                class = Some(self.refresh_follow_target_resource(reference, request.settings_7c)?);
            }
            // Native retains the actor pointer resolved before 0040FB40.
        }
        let mut basis = self.actor(camera)?.rotating.basis;
        let mut derived = None;
        if state.target_18c.is_some() {
            if actor.is_none() || (class.unwrap_or(0) as i32) < 3 {
                if state.mode_188 != 0 {
                    self.camera_escape_velocity(camera, state, &mut *request.host)?;
                    *self.selector16_mut(camera)? = state.clone();
                    let handle = self
                        .actor_follow_target(self.selector16(camera)?.target_18c.unwrap())?
                        .handle_04;
                    let result = request.host.camera_target_lost(self, camera, handle);
                    // The callback may mutate this same camera through player
                    // attachment. Reload on both success and failure.
                    *state = self.selector16(camera)?.clone();
                    result?;
                }
                state.mode_188 = 0;
            } else if let Some(actor) = actor {
                derived = request.host.derived_component(self, actor)?;
                if let Some(component) = derived {
                    if component.special_view_active {
                        if state.mode_188 != 4 {
                            state.mode_188 = 4;
                            state.byte_187 = 1;
                        }
                    } else if state.mode_188 == 4 {
                        self.pose_view_mode(camera, state, request.settings.first_person as u8)?;
                    }
                }
            }
            request.globals.special_view_active = state.mode_188 == 4;
            self.update_camera_targets(
                camera,
                state,
                actor,
                derived,
                &mut basis,
                request.math,
                &mut *request.host,
                request.settings_7c,
            )?;
            // 00410BB0: modes other than 1, 2 and 4 clip the eye (00410BF0, 0x80).
            if !matches!(state.mode_188, 1 | 2 | 4) {
                let focus = self
                    .actor_follow_target(state.target_18c.unwrap())?
                    .focus_08
                    .ok_or(CameraPoseError::UnwrittenTarget("primary"))?;
                let mut eye = state
                    .eye_138
                    .desired_position
                    .ok_or(CameraSlewError::UnwrittenTarget)?;
                terrain::clip_eye_to_terrain(&mut eye, focus, 128, &mut *request.host)?;
                state.eye_138.desired_position = Some(eye);
            }
            self.smooth_camera_parameters(state, request.microseconds, &mut *request.host)?;
            terrain::clamp_eye(state, &mut *request.host)?;
            let submerged = if let Some(actor) = actor {
                let y = self
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?[1]
                    as i16 as i32;
                i32::from(request.host.query_height(CameraHeightRequest::WaterPlane)?) > y + 256
            } else {
                false
            };
            if submerged {
                if state.mode_188 == 2 {
                    state.byte_1a0 = 1;
                    self.pose_view_mode(camera, state, 0)?;
                }
            } else if state.byte_1a0 != 0 {
                if request.settings.first_person != 0 {
                    self.pose_view_mode(camera, state, 1)?;
                }
                state.byte_1a0 = 0;
            }
            state.byte_187 = 0;
        }
        let mut position = state
            .eye_138
            .current_position
            .ok_or(CameraSlewError::UnwrittenCurrent)?;
        // 004107AF..7B7 publishes position before the orientation callbacks.
        self.owned_mut(camera)?.state.base.position_words = Some(position);
        if state.mode_188 != 4 {
            if state.target_18c.is_some() && state.mode_188 == 2 {
                basis = self
                    .follow_target_basis(&self.actor_follow_target(state.target_18c.unwrap())?)?;
                self.actor_mut(camera)?.rotating.basis = basis;
                if derived.is_some() {
                    self.apply_first_person_tilt(
                        camera,
                        &mut basis,
                        request.math,
                        &mut *request.host,
                    )?;
                }
            } else {
                let look = state
                    .look_15c
                    .current_position
                    .ok_or(CameraSlewError::UnwrittenCurrent)?;
                let direction =
                    std::array::from_fn(|axis| look[axis].wrapping_sub(position[axis]) as i16);
                orient_from_forward(
                    &mut basis,
                    normalize_words(direction, NativeLengthPrecision::Retained),
                    request.up,
                    false,
                );
            }
        }
        self.actor_mut(camera)?.rotating.basis = basis;
        shake::advance(state, &mut position, &mut basis, request)?;
        state.eye_138.current_position = Some(position);
        self.owned_mut(camera)?.state.base.position_words = Some(position);
        self.actor_mut(camera)?.rotating.basis = basis;
        request.globals.cached_position = Some(position);
        Ok(CameraPose {
            position_words: position,
            basis,
            target_state: class,
        })
    }

    fn apply_first_person_tilt(
        &mut self,
        camera: ObjectId,
        basis: &mut NativeBasis,
        math: &NativeBasisMath,
        host: &mut dyn CameraPoseHost,
    ) -> Result<(), CameraPoseError> {
        // 00410828..87B: virtual +48 is queried again after rotating Y/Z.
        // A counter crossing 100 renormalizes X as well, so the second lookup
        // must read the resulting X.y rather than the initial target basis.
        let scaled = |word| targets::ftol(f64::from(f32::from_bits(word)) * 32768.0);
        let yz = compress_first_person_angle(host.inverse_sine(scaled(basis.words[8]))?);
        math.rotate_in_plane(basis, yz, inf_formats::native_basis::RotationPlane::Yz);
        self.actor_mut(camera)?.rotating.basis = *basis;
        let xy =
            compress_first_person_angle(host.inverse_sine(scaled(basis.words[2]))?).wrapping_neg();
        math.rotate_in_plane(basis, xy, inf_formats::native_basis::RotationPlane::Xy);
        self.actor_mut(camera)?.rotating.basis = *basis;
        Ok(())
    }

    fn pose_view_mode(
        &mut self,
        camera: ObjectId,
        state: &mut Selector16State,
        mode: u8,
    ) -> Result<(), ObjectError> {
        // The setter resolves the locally refreshed target, including reselection.
        self.selector16_mut(camera)?.target_18c = state.target_18c;
        self.set_camera_view_mode(camera, mode)?;
        state.mode_188 = self.selector16(camera)?.mode_188;
        Ok(())
    }

    fn smooth_camera_parameters(
        &self,
        state: &mut Selector16State,
        microseconds: u32,
        host: &mut dyn CameraPoseHost,
    ) -> Result<(), CameraPoseError> {
        let request = CameraSlewRequest {
            microseconds,
            snap: state.byte_187,
            terrain_bypass: state.byte_185,
        };
        let mut raw_floor = |xz| {
            host.query_height(CameraHeightRequest::RawTerrainCell { xz })
                .map_err(|error| CameraSlewError::RawTerrain(error.to_string()))
        };
        if request.snap != 0 {
            state
                .eye_138
                .advance(state.block_138, request, &mut raw_floor)?;
            state
                .look_15c
                .advance(state.block_15c, request, &mut raw_floor)?;
        } else if state
            .eye_138
            .advance(state.block_138, request, &mut raw_floor)?
        {
            state.look_15c.advance(
                state.block_15c,
                CameraSlewRequest { snap: 1, ..request },
                &mut raw_floor,
            )?;
        } else if state
            .look_15c
            .advance(state.block_15c, request, &mut raw_floor)?
        {
            state.eye_138.advance(
                state.block_138,
                CameraSlewRequest { snap: 1, ..request },
                &mut raw_floor,
            )?;
        }
        Ok(())
    }
}

/// 00410B70: signed division truncates toward zero on every branch.
fn compress_first_person_angle(angle: i32) -> i32 {
    if angle > 0x4000 {
        angle.wrapping_mul(3).wrapping_sub(0xc000) / 2 + 0x2000
    } else if angle < -0x4000 {
        angle.wrapping_mul(3).wrapping_add(0xc000) / 2 - 0x2000
    } else {
        angle / 2
    }
}

#[cfg(test)]
#[path = "pose_tests.rs"]
mod tests;

#[cfg(test)]
mod lamp_tests {
    use super::CameraGlobalState;

    /// With both lamps owned L steps off, headlights, infrared, then under
    /// Vulkan ironbow, then off; each tick copies infrared's request in.
    #[test]
    fn l_steps_the_headlights_then_infrared_then_ironbow() {
        for vulkan in [false, true] {
            let mut globals = CameraGlobalState::INITIAL;
            globals.thermal_palettes.set(vulkan);
            let mut lamps = 0;
            let mut seen = Vec::new();
            for _ in 0..5 {
                lamps = globals.step_lamps(lamps);
                globals.infrared_5c3 = globals.infrared_5c4.get();
                seen.push((lamps != 0, globals.infrared_5c3, globals.ironbow.get()));
            }
            let on = (true, false, false);
            let white = (false, true, false);
            let ironbow = (false, true, true);
            let off = (false, false, false);
            if vulkan {
                assert_eq!(seen, [on, white, ironbow, off, on]);
            } else {
                assert_eq!(seen, [on, white, off, on, white]);
            }
        }
    }
}
