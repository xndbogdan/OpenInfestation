//! Concrete retained-data host for 0040FD10. Callback dispatch, reselection
//! and player selection are owned here. Ordinary vehicle targets use their
//! arena phase; player attachment retains its actual external factory owner.

use inf_formats::native_basis::{inverse_sine_q15, isqrt_48efc0, NativeBasis};

#[cfg(test)]
use crate::object_runtime::DerivedActorClass;

use super::{
    pose::rotation::rotate_local_vector, CameraDerivedComponent, CameraDerivedTargetRequest,
    CameraHeightRequest, CameraPoseError, CameraPoseHost,
};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorExtension, ActorHandle, ObjectArena, ObjectId, PlayerActorLookupContext,
        PlayerActorLookupRequest, PlayerId,
    },
    process_particles::factories::ParticleFactoryProcessState,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainWaterSurfaceRequest},
};

/// 00410D90's render-context publication. All three axes are signed world
/// units; basis counter and vectors are copied without reorientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraDrawRequest {
    pub eye: [i32; 3],
    pub basis: NativeBasis,
    /// 00410E5A writes byte 0055F250 = 16, wrapping view deltas to i16.
    pub coordinate_shift: u8,
}

pub struct NativeCameraInputs<'a> {
    pub terrain: &'a TerrainRuntime,
    /// H14 entry +4; only bit 40 is used by this camera's water query.
    pub world_flags: u32,
    pub sine_quarter: &'a [u16; 4096],
    pub inverse_sine: &'a [i16; 1024],
    /// Actual live process owner, consumed by vehicle chase targets or loss.
    pub process: Option<&'a ParticleFactoryProcessState>,
}

/// Operations genuinely outside the camera/terrain ownership. Implementors
/// execute them synchronously or return their reached feature boundary.
pub trait NativeCameraServices {
    /// 0046AE90(player,camera.handle), including the actual camera factory
    /// inputs/listeners. Its success byte is ignored by 0040FD10.
    fn attach_player_to_camera(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        camera: ObjectId,
    ) -> Result<bool, CameraPoseError>;
}

pub struct NativeCameraHost<'a, S: NativeCameraServices> {
    inputs: NativeCameraInputs<'a>,
    services: &'a mut S,
}

impl<'a, S: NativeCameraServices> NativeCameraHost<'a, S> {
    pub fn new(inputs: NativeCameraInputs<'a>, services: &'a mut S) -> Self {
        Self { inputs, services }
    }
}

impl ObjectArena {
    /// Supplied renderer fixture only: no camera Tick or retail pose claim.
    #[cfg(test)]
    pub(crate) fn supply_camera_draw_fixture(
        &mut self,
        camera: ObjectId,
        position: [u16; 3],
        basis: NativeBasis,
    ) -> Result<(), crate::object_runtime::ObjectError> {
        self.selector16(camera)?;
        self.owned_mut(camera)?.state.base.position_words = Some(position);
        self.actor_mut(camera)?.rotating.basis = basis;
        Ok(())
    }

    /// Camera's vehicle view: pure virtual +64 identity first, then the
    /// reached current BYTE+1C0 used by 0040FD10's special-view decision.
    pub fn camera_derived_component(
        &self,
        actor: ObjectId,
    ) -> Result<Option<CameraDerivedComponent>, CameraPoseError> {
        Ok(self
            .actor_vehicle_component(actor)?
            .map(|vehicle| CameraDerivedComponent {
                object: actor,
                special_view_active: vehicle.byte_1c0 != 0,
            }))
    }

    /// 0040FB40. Traverse the live G+48 actor list, consuming RNG only after
    /// +60/+B8 both returned null. Equal scores retain the earlier actor.
    pub fn reselect_camera_target(
        &self,
        rng: &mut u32,
    ) -> Result<Option<ActorHandle>, CameraPoseError> {
        let mut best_score = 0;
        let mut best = None;
        for &object in self.actor_list() {
            // An incomplete constructor must not receive a callback or consume
            // camera scoring RNG; its retained native node is unchanged.
            if self.construction_is_stopped(object)? {
                continue;
            }
            if self.object(object)?.base.flags & 1 == 0 {
                continue;
            }
            let actor = self.actor(object)?;
            // +60 returns self only for Selector16. All admitted +B8 slots
            // are 00401300; neither path consumes the score RNG draw.
            let score = if matches!(actor.extension, ActorExtension::Selector16(_)) {
                1
            } else {
                let mut score = name_entry_rng_next(rng);
                if actor.word_b4 == 0x12 {
                    score |= 0x40000;
                }
                if (2..=0x11).contains(&(actor.word_b4 as i32)) {
                    score |= 0x80000;
                }
                if actor.word_c4 == 3 {
                    score |= 0x200000;
                }
                if self.actor_vehicle_component(object)?.is_some() {
                    score |= 0x100000;
                }
                let squares = actor.words_8e_92.map(|word| {
                    let word = i32::from(word as i16);
                    word.wrapping_mul(word) as u32
                });
                let square = squares[0].wrapping_add(squares[1]).wrapping_add(squares[2]);
                // 0040FC3F shifts the unsigned overflowed sum before sqrt.
                let speed = if square & 0x8000_0000 != 0 {
                    isqrt_48efc0((square >> 2) as i32).wrapping_shl(1)
                } else {
                    isqrt_48efc0(square as i32)
                };
                if (speed as i32) > 100 {
                    score |= 0x20000;
                }
                score
            };
            if score > best_score {
                best_score = score;
                best = Some(actor.handle);
            }
        }
        Ok(best)
    }

    /// 0040FB10, the search a camera built with no first-player actor starts
    /// (00478EFB): +186 = 1, view mode 0 (0040F800), +188 = 3, then 0040FB40
    /// scores the actor list, releases the old target and follows the best
    /// actor, and +19C restarts.
    pub fn start_camera_search(
        &mut self,
        camera: ObjectId,
        rng: &mut u32,
    ) -> Result<(), CameraPoseError> {
        self.selector16_mut(camera)?.byte_186 = 1;
        self.set_camera_view_mode(camera, 0)?;
        self.selector16_mut(camera)?.mode_188 = 3;
        let selected = self.reselect_camera_target(rng)?;
        if let Some(old) = self.selector16(camera)?.target_18c {
            self.release_actor_service(old)?;
        }
        let target = match selected {
            Some(handle) => self.follow_target_resource(
                crate::object_runtime::FollowTargetRequest::resolve_actor(handle),
            )?,
            None => None,
        };
        let state = self.selector16_mut(camera)?;
        state.target_18c = target;
        state.dword_19c = 0;
        Ok(())
    }

    /// 00410D90 can publish the reached arena pose independently of a tick's
    /// return value; position remains required if no native writer reached it.
    pub fn camera_draw_request(
        &self,
        camera: ObjectId,
    ) -> Result<CameraDrawRequest, CameraPoseError> {
        self.selector16(camera)?;
        let position = self.object(camera)?.base.position_words.ok_or(
            crate::object_runtime::ObjectError::ActorPositionUnknown(camera),
        )?;
        Ok(CameraDrawRequest {
            eye: position.map(|word| i32::from(word as i16)),
            basis: self.actor(camera)?.rotating.basis,
            coordinate_shift: 16,
        })
    }
}

impl<S: NativeCameraServices> CameraPoseHost for NativeCameraHost<'_, S> {
    fn query_height(&mut self, request: CameraHeightRequest) -> Result<i16, CameraPoseError> {
        let terrain = self.inputs.terrain;
        Ok(match request {
            CameraHeightRequest::RawTerrainCell { xz } => {
                let shift = match terrain.sampling().height_scale {
                    TerrainHeightScale::Units16 => 4,
                    TerrainHeightScale::Units32 => 5,
                };
                i16::from(terrain.cell_at(xz).height_i8()).wrapping_shl(shift)
            }
            CameraHeightRequest::InterpolatedTerrain { xz } => terrain.height_at(xz),
            CameraHeightRequest::WaterSurface { xz, terrain_height } => terrain
                .water_surface_height(
                    TerrainWaterSurfaceRequest {
                        xz,
                        // 004106D1 explicitly pushes zero, not the process clock.
                        wave_clock: 0,
                        world_flags: self.inputs.world_flags,
                        terrain_height,
                    },
                    self.inputs.sine_quarter,
                ),
            CameraHeightRequest::WaterPlane => {
                (terrain.plane_base() >> terrain.plane_shift()) as i16
            }
        })
    }

    fn derived_component(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<CameraDerivedComponent>, CameraPoseError> {
        arena.camera_derived_component(actor)
    }

    fn update_derived_camera_targets(
        &mut self,
        arena: &mut ObjectArena,
        request: CameraDerivedTargetRequest<'_>,
    ) -> Result<(), CameraPoseError> {
        arena.update_selector2_camera_targets(request, self.inputs.process)
    }

    fn reselect_target(
        &mut self,
        arena: &ObjectArena,
        rng: &mut u32,
    ) -> Result<Option<ActorHandle>, CameraPoseError> {
        arena.reselect_camera_target(rng)
    }

    fn camera_target_lost(
        &mut self,
        arena: &mut ObjectArena,
        camera: ObjectId,
        previous_handle: ActorHandle,
    ) -> Result<(), CameraPoseError> {
        // 0040FF2C: matching owner first, else first player. 0046B060 then
        // tests the FIRST player's +D8 only, even if a different owner matched.
        let Some(player) = arena
            .find_player_for_actor_handle(
                PlayerActorLookupRequest {
                    handle: previous_handle,
                },
                PlayerActorLookupContext {
                    process: self.inputs.process,
                },
            )?
            .or_else(|| arena.first_player())
        else {
            return Ok(());
        };
        let first = arena
            .first_player()
            .expect("selected player implies nonempty list");
        let handle = arena
            .world_player(first)
            .map_err(|error| CameraPoseError::Host(error.to_string()))?
            .handle_d8;
        let attached = arena
            .resolve_actor_handle(handle)
            .map_err(|error| CameraPoseError::Host(error.to_string()))?;
        if let Some(attached) = attached {
            if (arena.actor(attached)?.word_c4 as i32) >= 3 {
                return Ok(());
            }
        }
        arena.set_camera_view_mode(camera, 0)?;
        self.services
            .attach_player_to_camera(arena, player, camera)?;
        Ok(())
    }

    fn inverse_sine(&mut self, value: i32) -> Result<i32, CameraPoseError> {
        Ok(inverse_sine_q15(value, self.inputs.inverse_sine))
    }

    fn rotate_local_vector(
        &mut self,
        basis: &mut NativeBasis,
        rotation_words: [u16; 3],
    ) -> Result<(), CameraPoseError> {
        rotate_local_vector(basis, rotation_words, self.inputs.sine_quarter);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
