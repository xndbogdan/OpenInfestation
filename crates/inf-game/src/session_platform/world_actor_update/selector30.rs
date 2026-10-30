//! Production services for the gate update 0048B3F0 and its pass 0048BE70:
//! the persistent record +154 in the process sound list (00487F60/FC0/FE0),
//! the player messages, the world's end (00487560 through
//! [`complete_world`]), and a linked gate's flash, camera snap and creation
//! listeners.

use inf_formats::{native_basis::NativeBasisMath, FrontendExecutable};

use crate::{
    grant_text::GrantTextData,
    object_runtime::{
        ActorMessage, CameraSettings, CreationListeners, LiveGrantContext, ObjectArena,
        ObjectError, ObjectId, PlayerActorLookupContext, PlayerActorLookupRequest, PlayerId,
        Selector30PassHost, Selector30UpdateHost, VehicleMessageEffect,
    },
    process_particles::factories::ParticleFactoryProcessState,
    process_sound::{ProcessSoundRef, ProcessSoundRuntime, ProcessSoundVoiceTeardown},
    session::SessionState,
    session_platform::world_actor_factory::{
        RequiredListenerEffects, WorldCameraFactoryContext, WorldCameraInputs,
    },
    terrain_runtime::TerrainRuntime,
    world_completion::{
        complete_world, WorldCompletionInputs, WorldCompletionOutcome, WorldCompletionRequest,
    },
    world_flash::{WorldFlashHost, WorldFlashPosition, WorldFlashRequest, WorldFlashState},
    world_mission::WorldMission,
    ResidentRuntime,
};

/// H0 strings of the gate's notices: 0x15F "Warp to %s", 0x160
/// "Surface-bound warpgate", 0x161 the aliens notice, 0x164 the missing
/// information, and 0xE0 + world each world's name.
const WARP_FORMAT: u16 = 0x15f;
const SURFACE_GATE: u16 = 0x160;
const ALIENS_REMAIN: u32 = 0x161;
const MISSING_GRANT: u16 = 0x164;
const WORLD_NAMES: u32 = 0xe0;
/// 00487D70's radius and style at 0048C29D.
const FLASH_RADIUS: u32 = 0xf00;
/// 0048BEF3: the resource whose held items a gate drops.
const DROPPED_RESOURCE: u16 = 0x31;

pub(crate) struct WorldSelector30Host<'a, 'exe> {
    pub sounds: &'a mut ProcessSoundRuntime,
    pub residency: &'a ResidentRuntime,
    pub voices: &'a mut dyn ProcessSoundVoiceTeardown,
    pub executable: FrontendExecutable<'exe>,
    pub factory_process: &'a ParticleFactoryProcessState,
    pub mission: &'a mut WorldMission,
    /// The phase 00487560's completion requests.
    pub state: &'a mut SessionState,
    pub grant_text: Option<&'a GrantTextData<'a>>,
    pub terrain: &'a TerrainRuntime,
    pub flash: &'a mut WorldFlashState,
    pub listeners: &'a mut CreationListeners,
    pub camera_settings: CameraSettings,
    pub sine: &'a [u16; 4096],
    pub math: &'a NativeBasisMath,
    pub animation_clock: u32,
    pub world_flags: u32,
    pub rng: &'a mut u32,
    /// Settings +84 bits a pass grants; the session applies them after the
    /// tick (00463AB0()+84).
    pub cheat_flags: &'a mut u32,
}

/// `0046B090` on the actor's handle: the player whose unit it is.
pub(crate) fn gate_actor_player(
    arena: &mut ObjectArena,
    actor: ObjectId,
    process: &ParticleFactoryProcessState,
) -> Result<Option<PlayerId>, String> {
    let handle = arena.actor(actor).map_err(|e| e.to_string())?.handle;
    arena
        .find_player_for_actor_handle(
            PlayerActorLookupRequest { handle },
            PlayerActorLookupContext {
                process: Some(process),
            },
        )
        .map_err(|error| format!("0046B090: {error}"))
}

/// `004750C0` on the player's grants `+9C`: the gate's grant `+7A`.
pub(crate) fn gate_grant(
    arena: &mut ObjectArena,
    player: PlayerId,
    id: u16,
    context: &LiveGrantContext<'_>,
) -> Result<(), String> {
    use crate::object_runtime::{GrantOwner, LiveGrantOperation, LiveGrantRequest};
    arena
        .mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Player9c(player),
                operation: LiveGrantOperation::SetRaw { id: u32::from(id) },
            },
            context,
        )
        .map_err(|error| format!("004750C0({id:#x}): {error}"))
}

/// `00478E00`'s world camera, created if there is none, as the pass's
/// flash and camera snap reach it.
pub(crate) struct GateCamera<'a> {
    pub residency: &'a ResidentRuntime,
    pub sine: &'a [u16; 4096],
    pub camera_settings: CameraSettings,
    pub factory_process: &'a ParticleFactoryProcessState,
    pub animation_clock: u32,
    pub terrain: &'a TerrainRuntime,
    pub world_flags: u32,
    pub listeners: &'a mut CreationListeners,
}

impl GateCamera<'_> {
    /// `0048C2A7`: `00487D70(position, 0xF00, 0)`, the world flash.
    pub(crate) fn flash(
        self,
        flash: &mut WorldFlashState,
        arena: &mut ObjectArena,
        position: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        let mut camera = WorldCameraFactoryContext::new(WorldCameraInputs {
            residency: self.residency,
            sine_quarter: self.sine,
            settings: self.camera_settings,
            process: Some(self.factory_process),
            animation_clock: self.animation_clock,
        });
        let mut host = super::super::world_flash_host::WorldFlashCameraHost {
            camera: camera.factory(self.terrain, self.world_flags, self.listeners),
            residency: self.residency,
        };
        flash
            .trigger(
                arena,
                WorldFlashRequest {
                    position: WorldFlashPosition::Words(position),
                    radius: FLASH_RADIUS,
                    style_argument: 0,
                },
                &mut host,
                rng,
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// `0048C2AF`: the camera.
    pub(crate) fn camera(self, arena: &mut ObjectArena, rng: &mut u32) -> Result<ObjectId, String> {
        let mut camera = WorldCameraFactoryContext::new(WorldCameraInputs {
            residency: self.residency,
            sine_quarter: self.sine,
            settings: self.camera_settings,
            process: Some(self.factory_process),
            animation_clock: self.animation_clock,
        });
        let mut host = super::super::world_flash_host::WorldFlashCameraHost {
            camera: camera.factory(self.terrain, self.world_flags, self.listeners),
            residency: self.residency,
        };
        host.acquire_camera(arena, rng)?
            .ok_or_else(|| "00478E00 found no camera".into())
    }
}

impl WorldSelector30Host<'_, '_> {
    /// The camera's loans, the flash and the RNG, borrowed apart.
    fn gate_services(&mut self) -> (GateCamera<'_>, &mut WorldFlashState, &mut u32) {
        (
            GateCamera {
                residency: self.residency,
                sine: self.sine,
                camera_settings: self.camera_settings,
                factory_process: self.factory_process,
                animation_clock: self.animation_clock,
                terrain: self.terrain,
                world_flags: self.world_flags,
                listeners: &mut *self.listeners,
            },
            &mut *self.flash,
            &mut *self.rng,
        )
    }
}

fn position(arena: &ObjectArena, actor: ObjectId) -> Result<[u16; 3], ObjectError> {
    arena
        .object(actor)?
        .base
        .position_words
        .ok_or(ObjectError::ActorPositionUnknown(actor))
}

impl Selector30UpdateHost for WorldSelector30Host<'_, '_> {
    type Error = String;

    fn create_sound(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
        volume: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, String> {
        // 0048BB99: sound 0x1A, flag 0.
        self.sounds
            .create_persistent(self.residency, 0x1a, volume, pitch, 0, || {
                position(arena, actor)
            })
            .map_err(|error| format!("{error:?}"))
    }

    fn update_sound(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
        reference: ProcessSoundRef,
        volume: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.sounds
            .records_mut()
            .update(reference, volume, pitch, Some(|| position(arena, actor)))
            .map_err(|error| format!("{error:?}"))
    }

    fn release_sound(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        self.sounds
            .delete_record(reference, &mut *self.voices)
            .map_err(|error| format!("{error:?}"))
    }

    fn find_player(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<PlayerId>, String> {
        Selector30PassHost::find_player(self, arena, actor)
    }

    fn aliens_remain(
        &mut self,
        arena: &mut ObjectArena,
        gate: ObjectId,
        actor: ObjectId,
    ) -> Result<(), String> {
        // 0047DD80 with no radius delivers whatever the distance.
        let (own, theirs) = (
            position(arena, gate).map_err(|e| e.to_string())?,
            position(arena, actor).map_err(|e| e.to_string())?,
        );
        let squared = (0..3).fold(0_u32, |sum, axis| {
            let word = i32::from(own[axis].wrapping_sub(theirs[axis]) as i16);
            sum.wrapping_add(word.wrapping_mul(word) as u32)
        });
        if arena.selector2_actor(actor).is_err() {
            return Err(format!(
                "{actor:?}'s +98 for the gate's notice is not ported"
            ));
        }
        let effect = arena
            .vehicle_message(
                actor,
                ActorMessage {
                    sender: Some(gate),
                    distance_squared: squared,
                    code: 4,
                    argument: ALIENS_REMAIN,
                },
                PlayerActorLookupContext {
                    process: Some(self.factory_process),
                },
                self.factory_process.network_flag != 0,
            )
            .map_err(|error| format!("00420BE0: {error}"))?;
        match effect {
            VehicleMessageEffect::None => Ok(()),
            VehicleMessageEffect::PlayerText { player, string } => arena
                .deliver_player_text(player, string, self.residency)
                .map_err(|error| format!("004135D0 text {string:#x}: {error}")),
            other => Err(format!("00420BE0 code 4 left {other:?}")),
        }
    }

    fn publish_gate_word(&mut self, word_164: u16) -> Result<(), String> {
        self.mission.word_28 = Some(word_164);
        self.mission.word_2a = Some(0x119);
        Ok(())
    }
    fn aliens_cleared(&mut self) -> Result<(), String> {
        let bytes = self
            .mission
            .bytes_14_16
            .as_mut()
            .ok_or("G+1C4..1C6 are unknown")?;
        bytes[2] = 1;
        Ok(())
    }

    fn missing_grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        grant: u16,
    ) -> Result<(), String> {
        let name = crate::object_runtime::grant_record_text(
            &LiveGrantContext {
                process: Some(self.factory_process),
                executable: Some(self.executable),
                resident: Some(self.residency),
                text_data: self.grant_text,
            },
            grant,
        )
        .map_err(|error| format!("00475320({grant:#x}): {error}"))?;
        arena
            .deliver_player_pair_text(player, MISSING_GRANT, name, self.residency, self.grant_text)
            .map_err(|error| format!("004135D0 missing grant: {error}"))
    }

    fn destination(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        world: u32,
    ) -> Result<(), String> {
        // Only Pelagia's woken gate leads to world 1, whose H0 name is the
        // campaign end's "Completed" ([`crate::pelagia`]).
        if let Some(name) = crate::pelagia::world_name(world, true) {
            return arena
                .deliver_player_warp_name(
                    player,
                    WARP_FORMAT,
                    name,
                    self.residency,
                    self.grant_text,
                )
                .map_err(|error| format!("004135D0 destination: {error}"));
        }
        // 0048B941: the name's index is a WORD, 0xE0 + world.
        let (format, argument) = if world != 0 {
            (WARP_FORMAT, Some(world.wrapping_add(WORLD_NAMES) as u16))
        } else {
            (SURFACE_GATE, None)
        };
        arena
            .deliver_player_warp_text(player, format, argument, self.residency, self.grant_text)
            .map_err(|error| format!("004135D0 destination: {error}"))
    }

    fn pass(
        &mut self,
        arena: &mut ObjectArena,
        gate: ObjectId,
        actor: ObjectId,
    ) -> Result<(), String> {
        let math = self.math;
        let network_active = self.factory_process.network_flag != 0;
        arena
            .pass_selector30(gate, actor, network_active, math, self)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

impl Selector30PassHost for WorldSelector30Host<'_, '_> {
    type Error = String;

    fn find_player(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<PlayerId>, String> {
        gate_actor_player(arena, actor, self.factory_process)
    }

    fn grant(&mut self, arena: &mut ObjectArena, player: PlayerId, id: u16) -> Result<(), String> {
        let context = LiveGrantContext {
            process: Some(self.factory_process),
            executable: Some(self.executable),
            resident: Some(self.residency),
            text_data: self.grant_text,
        };
        gate_grant(arena, player, id, &context)
    }

    fn drop_item(&mut self, _: &mut ObjectArena, _: ObjectId, index: usize) -> Result<(), String> {
        // No recorded pass held one: every vehicle carried scientists.
        Err(format!(
            "the hold's +0 for slot {index}'s resource-{DROPPED_RESOURCE:#x} item needs the vehicle update's drop services"
        ))
    }

    fn world_status(&mut self) -> Result<(u32, u32), String> {
        let current = self.mission.current_world_a0;
        let status = *self
            .mission
            .save
            .owner_3c
            .get(current as usize)
            .ok_or_else(|| format!("world {current} has no G+1EC status"))?;
        Ok((current, status))
    }

    fn set_world_status(&mut self, status: u32) -> Result<(), String> {
        let current = self.mission.current_world_a0;
        *self
            .mission
            .save
            .owner_3c
            .get_mut(current as usize)
            .ok_or_else(|| format!("world {current} has no G+1EC status"))? = status;
        Ok(())
    }

    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), String> {
        *self.cheat_flags |= bit;
        Ok(())
    }

    /// As the Teleport loads a world (`session_platform::cheats`): world 1
    /// at its first start, its briefing first; nothing completes, no
    /// results page shows and nothing is saved, and the vehicle keeps all it
    /// has.
    fn enter_pelagia(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), String> {
        self.mission.save.mission_selection = crate::pelagia::WORLD;
        self.mission.save.owner_a4 = 0;
        eprintln!("Pelagia: the Command Centre's gate leads to world 1");
        let _ = self
            .state
            .request_phase(crate::session::SessionPhase::Phase3);
        Ok(())
    }

    fn request_world(
        &mut self,
        arena: &mut ObjectArena,
        world: u32,
        start: u16,
        actor: ObjectId,
    ) -> Result<(), String> {
        let outcome = complete_world(
            self.mission,
            arena,
            WorldCompletionRequest {
                next_selection: world,
                start_ordinal: start,
                actor: Some(actor),
            },
            WorldCompletionInputs {
                network_active: self.factory_process.network_flag != 0,
                process: Some(self.factory_process),
            },
            &mut *self.sounds,
        )
        .map_err(|error| error.to_string())?;
        eprintln!("World gate: world {world}, start {start}: {outcome:?}");
        if let WorldCompletionOutcome::Accepted { request_phase } = outcome {
            let _ = self.state.request_phase(request_phase);
        }
        Ok(())
    }

    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        let (camera, flash, rng) = self.gate_services();
        camera.flash(flash, arena, position, rng)
    }

    fn camera(&mut self, arena: &mut ObjectArena) -> Result<ObjectId, String> {
        let (camera, _, rng) = self.gate_services();
        camera.camera(arena, rng)
    }

    fn notify_listeners(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        self.listeners
            .notify(actor, arena, &mut RequiredListenerEffects)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[path = "selector30_tests.rs"]
mod tests;
