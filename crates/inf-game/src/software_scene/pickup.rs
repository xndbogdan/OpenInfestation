//! Live 0046DAF0 services, sharing the renderer's RNG and process effect owners.
use super::*;
use crate::menu::MenuBasis;
use crate::object_runtime::{
    ActorHandle, ObjectArena, ObjectDrawCallbackBoundary, ObjectDrawContext, ObjectDrawLighting,
    ObjectDrawProcessState, ObjectDrawServiceError, ObjectId, ObjectModelPacket,
    PickupCollectionEffect, PickupDrawHost, PickupDrawRequest, PlayerId,
};
use crate::process_particles::{
    births::{ParticleBirthInputs, ParticleBirthRequest},
    oriented::OrientedParticleRequest,
};

pub struct SoftwarePickupServices<'a> {
    pub text_data: Option<crate::grant_text::PickupTextData<'a>>,
    pub terrain: &'a crate::terrain_runtime::TerrainRuntime,
    pub mission_save: &'a mut inf_formats::MissionSaveState,
    pub current_world: &'a u32,
    pub load_factor: i32,
}

#[derive(Debug, thiserror::Error)]
enum PickupServiceError {
    #[error("{0}")]
    Owner(String),
    #[error("pickup model submission failed")]
    Model {
        model: i16,
        root_was_open: bool,
        error: SoftwareSceneError,
    },
}
fn owner(error: impl std::fmt::Display) -> PickupServiceError {
    PickupServiceError::Owner(error.to_string())
}

struct PickupServices<'a, 'b, 'f, 'r> {
    effects: &'a mut SoftwareObjectEffectServices<'b>,
    scene: &'a mut SoftwareScene<'f, 'r>,
    animation: &'a dyn crate::frontend_camera::AnimationClockSource,
    executable: inf_formats::FrontendExecutable<'a>,
    tint: &'a u32,
    direction: [f32; 3],
}
impl PickupServices<'_, '_, '_, '_> {
    /// The current world's pickup word holding bit `index`: W+B0..W+3CF, an
    /// arena world's words past the 25 worlds'.
    fn pickup_word(&mut self, index: u32) -> Result<&mut u32, PickupServiceError> {
        let context = self
            .effects
            .pickup
            .as_mut()
            .ok_or_else(|| owner("pickup mission owner is required"))?;
        let word = (*context.current_world as i32)
            .wrapping_mul(4)
            .wrapping_add((index as i32) / 32);
        usize::try_from(word)
            .ok()
            .and_then(|i| match i {
                0..100 => context.mission_save.owner_b0.get_mut(i),
                100..200 => context.mission_save.owner_240.get_mut(i - 100),
                _ => None,
            })
            .ok_or_else(|| owner("pickup mission bit index is outside W+B0..W+3CF"))
    }
}

impl PickupDrawHost for PickupServices<'_, '_, '_, '_> {
    type Error = PickupServiceError;
    fn network_flag(&mut self) -> u32 {
        self.effects.factory_process.network_flag
    }
    fn pickup_network(&mut self) -> Option<crate::object_runtime::PickupNetwork> {
        let process = self.effects.factory_process;
        Some(crate::object_runtime::PickupNetwork {
            host: process.host_flag != 0,
            respawning: process.settings[0x20 / 4] != 0,
        })
    }
    fn pickup_collected(&mut self, index: u32) -> Option<bool> {
        if index == u32::MAX {
            return Some(false);
        }
        let word = *self.pickup_word(index).ok()?;
        Some(word & 1 << (index & 31) != 0)
    }
    fn clear_pickup_collected(&mut self, index: u32) -> Option<()> {
        if index != u32::MAX {
            *self.pickup_word(index).ok()? &= !(1 << (index & 31));
        }
        Some(())
    }
    fn player_for_handle(
        &mut self,
        arena: &ObjectArena,
        handle: ActorHandle,
    ) -> Result<Option<PlayerId>, Self::Error> {
        arena
            .find_player_for_actor_handle(
                crate::object_runtime::PlayerActorLookupRequest { handle },
                crate::object_runtime::PlayerActorLookupContext {
                    process: Some(self.effects.factory_process),
                },
            )
            .map_err(owner)
    }
    fn grant_pickup(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        player: PlayerId,
        _rng: &mut u32,
    ) -> Result<bool, Self::Error> {
        let pickup = arena.timed_pickup(object).map_err(owner)?;
        let kind = pickup
            .kind_44
            .ok_or_else(|| owner("pickup kind+44 is unavailable"))?;
        let amount = pickup.value_46 as i16;
        let grants = inf_formats::parse_grant_table(self.executable).map_err(owner)?;
        let texts = inf_formats::parse_grant_texts(self.executable).map_err(owner)?;
        let limits = inf_formats::parse_vehicle_upgrade_limits(self.executable).map_err(owner)?;
        let mut result = arena
            .grant_player_pickup(
                player,
                kind,
                amount,
                crate::object_runtime::PickupTables {
                    grants: &grants,
                    texts: &texts,
                    upgrade_limits: &limits,
                    network_active: self.effects.factory_process.network_flag != 0,
                },
            )
            .map_err(owner)?;
        // 004231E5: the boost's sound, inside the class case.
        if let Some(sound) = result.sound.take() {
            self.effects
                .sounds
                .as_deref_mut()
                .ok_or_else(|| owner("the boost's sound needs the process sound owner"))?
                .enqueue(
                    self.effects.residency,
                    crate::process_particles::audio::ParticleSoundRequest {
                        sound: sound.sound,
                        position: sound.position,
                        volume: 0x10000,
                        pitch: 0x10000,
                        flags: 0,
                    },
                )
                .map_err(owner)?;
        }
        let accepted = result.accepted;
        let flags = inf_formats::parse_grant_flags(self.executable).map_err(owner)?;
        let data = self
            .effects
            .pickup
            .as_ref()
            .and_then(|context| context.text_data.as_ref());
        // 0046C760 itself returns online; the rest is offline only.
        arena
            .deliver_pickup_outcome(
                player,
                result,
                &crate::object_runtime::PickupDelivery {
                    online: self.effects.factory_process.network_flag != 0,
                    flags: &flags,
                    residency: self.effects.residency,
                    text: data,
                    commands: &crate::object_runtime::PlayerCommandContext {
                        process: Some(self.effects.factory_process),
                        executable: Some(self.executable),
                        resident: Some(self.effects.residency),
                    },
                    clock: self.animation.animation_value(),
                },
            )
            .map_err(owner)?;
        Ok(accepted)
    }
    fn collection_effect(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        effect: PickupCollectionEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        match effect {
            PickupCollectionEffect::MarkCollected { index } => {
                //0048E980 tests -1 before reading current-world or backing.
                if index == u32::MAX {
                    return Ok(());
                }
                *self.pickup_word(index)? |= 1u32 << (index & 31);
            }
            PickupCollectionEffect::Particle => {
                let position = arena
                    .object(object)
                    .map_err(owner)?
                    .base
                    .position_words
                    .ok_or_else(|| owner("pickup position+32 is required"))?;
                self.effects
                    .particles
                    .birth_pickup_flash(
                        ParticleBirthRequest {
                            particle_type: 3,
                            position,
                            velocity: None,
                            flags: 0,
                            inputs: ParticleBirthInputs {
                                network_active: self.effects.factory_process.network_flag != 0,
                                phase: self.effects.phase,
                                process: self.effects.factory_process,
                            },
                        },
                        self.executable,
                        rng,
                    )
                    .map_err(owner)?;
            }
            PickupCollectionEffect::Sound {
                sound,
                volume,
                pitch,
            } => {
                let position = arena
                    .object(object)
                    .map_err(owner)?
                    .base
                    .position_words
                    .ok_or_else(|| owner("pickup position+32 is required"))?;
                let sounds = self
                    .effects
                    .sounds
                    .as_deref_mut()
                    .ok_or_else(|| owner("pickup process sound owner is required"))?;
                sounds
                    .enqueue(
                        self.effects.residency,
                        crate::process_particles::audio::ParticleSoundRequest {
                            sound,
                            position,
                            volume,
                            pitch,
                            flags: 0,
                        },
                    )
                    .map_err(owner)?;
            }
        }
        Ok(())
    }
    fn base_color(&mut self) -> u32 {
        *self.tint
    }
    fn animation_clock(&mut self) -> u32 {
        self.animation.animation_value()
    }
    fn emit_trail(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        basis: &MenuBasis,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        let context = self
            .effects
            .pickup
            .as_ref()
            .ok_or_else(|| owner("pickup terrain/load-factor owners are required"))?;
        let position = arena
            .object(object)
            .map_err(owner)?
            .base
            .position_words
            .ok_or_else(|| owner("pickup position+32 is required"))?;
        let plane = (context.terrain.plane_base() >> (context.terrain.plane_shift() & 31)) as i16;
        self.effects
            .particles
            .emit_oriented_particles(
                OrientedParticleRequest {
                    position: &position,
                    velocity: &[0, (-32i16) as u16, 0],
                    basis,
                    strength: 200,
                    load_factor: context.load_factor,
                    plane,
                    sine_quarter: &self.scene.tables.sine_table,
                },
                ParticleBirthInputs {
                    network_active: self.effects.factory_process.network_flag != 0,
                    phase: self.effects.phase,
                    process: self.effects.factory_process,
                },
                self.executable,
                rng,
            )
            .map_err(owner)
    }
    fn model(
        &mut self,
        arena: &mut ObjectArena,
        packet: ObjectModelPacket,
        color: u32,
        context: &ObjectDrawContext,
    ) -> Result<(), Self::Error> {
        //00490030 consumes the current terrain direction548380 and stack RGB.
        let bytes = color.to_le_bytes();
        crate::scene_resources::SceneResourceBackend::submit(
            self.scene,
            crate::scene_resources::ResolvedSceneCommand::DirectionalLight {
                direction: self.direction,
                rgb: [bytes[0], bytes[1], bytes[2]],
            },
        )
        .map_err(|error| PickupServiceError::Model {
            model: packet.model_slot,
            root_was_open: self.scene.queue.has_only_implicit_root(),
            error,
        })?;
        self.scene.environment.lighting.directional_word = color;
        let root_was_open = self.scene.queue.has_only_implicit_root();
        // The tracer keeps most of the light retail gives a pick-up.
        let own = crate::modern_tuning::current().materials().pickups;
        if let Some(feed) = self.scene.scene_feed_mut() {
            feed.own_light = own;
        }
        let drawn = self
            .scene
            .draw_object_model(SoftwareObjectModelDrawRequest {
                terrain: ModelTerrainRead::Live(
                    self.effects
                        .pickup
                        .as_ref()
                        .ok_or_else(|| owner("pickup current terrain is unavailable"))?
                        .terrain,
                ),
                packet,
                root_mode: crate::model_execution::ModelRootMode::CommonVisual,
                context,
                arena,
                animation: self.animation,
                executable: self.executable,
            });
        if let Some(feed) = self.scene.scene_feed_mut() {
            feed.own_light = 0.0;
        }
        drawn.map_err(|error| PickupServiceError::Model {
            model: packet.model_slot,
            root_was_open,
            error,
        })
    }
}
impl SoftwareObjectDrawHost<'_, '_, '_> {
    pub(super) fn draw_pickup(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        context: &mut ObjectDrawContext,
        process: &mut ObjectDrawProcessState,
        lighting: &ObjectDrawLighting<'_>,
    ) -> Result<(), ObjectDrawServiceError<SoftwareSceneError>> {
        let missing = || {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::MissingPickupServices,
            )
        };
        let effects = self.effects.as_mut().ok_or_else(missing)?;
        let mut rng = self.scene.process_rng().ok_or_else(missing)?;
        let sine = Arc::clone(&self.scene.tables.sine_table);
        let result = arena.draw_timed_pickup(
            object,
            PickupDrawRequest {
                context,
                process,
                sine_quarter: &sine,
            },
            &mut PickupServices {
                effects,
                scene: self.scene,
                animation: self.animation,
                executable: self.executable,
                tint: lighting.tint_word,
                direction: lighting.direction,
            },
            &mut rng,
        );
        self.scene.set_process_rng(Some(rng));
        match result {
            Ok(()) => Ok(()),
            Err(crate::object_runtime::PickupError::Service {
                error:
                    PickupServiceError::Model {
                        model,
                        root_was_open,
                        error,
                    },
                ..
            }) => self.model_error(model, root_was_open, error),
            Err(error) => Err(ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::Pickup {
                    reason: error.to_string(),
                },
            )),
        }
    }
}
