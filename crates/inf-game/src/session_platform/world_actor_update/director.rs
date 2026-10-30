//! The Alien Brain's ending director (selector 78) from the world tick: its
//! helpers (00462750) and the camera (00478E00) through the intro's factory
//! inputs, the player's take (0046AE90), the sparks (004665D0), retirements
//! (0047CE90) with the world's damage owners, and the world's end
//! (00487560), whose requested phase the caller applies once the session's
//! loan ends.

use inf_formats::native_basis::NativeBasisMath;

use super::{grenade26::spark, WorldFamilyUpdateHost};
use crate::{
    object_runtime::{
        ActorConstructorNull, ActorHandle, ArenaIntroServices, CameraSettings,
        IntroAdmissionInputs, ObjectArena, ObjectId, PlayerId, Selector78Host,
        TargetNullHandleSource, ENDING_COMPLETION_BONUS,
    },
    session::SessionPhase,
    session_platform::{
        particles::damage::world_retirement_host,
        world_actor_factory::{fresh_world_allocation, RequiredListenerEffects},
    },
    world_completion::{
        complete_world, WorldCompletionInputs, WorldCompletionOutcome, WorldCompletionRequest,
    },
};

pub(crate) struct WorldDirectorHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
    pub math: &'a NativeBasisMath,
    pub clock: u32,
    pub world_flags: u32,
    pub camera_settings: CameraSettings,
    /// Settings +84 bits for the live settings word.
    pub cheat_flags_84: &'a mut u32,
    /// The phase the world's end requests.
    pub requested_phase: Option<SessionPhase>,
    pub listener_effects: RequiredListenerEffects,
}

impl WorldDirectorHost<'_, '_> {
    /// The factory inputs of 00462750 and 00478E00, as the intro lends them.
    fn admission(&mut self) -> Result<IntroAdmissionInputs<'_>, String> {
        let owners = &mut self.family.owners;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("the director's helpers need the world's creation listeners")?;
        let process = &*owners.factory_process;
        let allocation = fresh_world_allocation(
            owners.terrain,
            self.clock,
            self.world_flags,
            self.family.sine_quarter,
            ActorConstructorNull::Process(process),
        );
        Ok(IntroAdmissionInputs {
            process: Some(process),
            runtime: owners.residency,
            math: self.math,
            helper_allocation: allocation,
            camera_allocation: allocation,
            local_owner: process.local_slot,
            camera_settings: self.camera_settings,
            network_active: process.network_flag != 0,
            network_world_active: process.network_flag != 0,
            listeners: &mut *loan.listeners,
            listener_effects: &mut self.listener_effects,
        })
    }
}

impl TargetNullHandleSource for WorldDirectorHost<'_, '_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.family.owners.factory_process.process_null_handle
    }
}

impl ArenaIntroServices for WorldDirectorHost<'_, '_> {
    type Error = String;

    fn create_helper(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, String> {
        self.admission()?
            .create_helper(arena)
            .map_err(|error| error.to_string())
    }

    fn camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, String> {
        self.admission()?
            .acquire_camera(arena)
            .map_err(|error| error.to_string())
    }

    fn retire_actor(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        let null = self.family.owners.factory_process.process_null_handle;
        let family = &mut self.family;
        let mut host = world_retirement_host(&mut family.owners, &mut *family.rng);
        arena
            .retire_actor(actor, null, &mut host)
            .map(|_| ())
            .map_err(|error| format!("0047CE90 on {actor:?}: {error:?}"))
    }

    fn world_present(&mut self) -> Result<bool, String> {
        Ok(true)
    }

    fn complete_mission(
        &mut self,
        arena: &mut ObjectArena,
        selection: u32,
        start_ordinal: u16,
        argument: u32,
    ) -> Result<(), String> {
        if argument != 0 {
            return Err("00487560 with an actor is not the director's".into());
        }
        let owners = &mut self.family.owners;
        let outcome = complete_world(
            owners.mission,
            arena,
            WorldCompletionRequest {
                next_selection: selection,
                start_ordinal,
                actor: None,
            },
            WorldCompletionInputs {
                network_active: owners.factory_process.network_flag != 0,
                process: Some(&*owners.factory_process),
            },
            &mut *owners.sounds,
        )
        .map_err(|error| error.to_string())?;
        if let WorldCompletionOutcome::Accepted { request_phase } = outcome {
            self.requested_phase = Some(request_phase);
        }
        Ok(())
    }
}

impl Selector78Host for WorldDirectorHost<'_, '_> {
    fn rng(&mut self) -> &mut u32 {
        self.family.rng
    }

    fn take_player(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        handle: ActorHandle,
    ) -> Result<(), String> {
        self.admission()?
            .take_player(arena, player, handle)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn spark(
        &mut self,
        _: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
        owner: ActorHandle,
    ) -> Result<(), String> {
        let family = &mut self.family;
        spark(&mut family.owners, point, velocity, owner, &mut *family.rng)
            .map_err(|error| format!("director spark {error}"))
    }

    fn completion_bonus(&mut self) {
        *self.cheat_flags_84 |= ENDING_COMPLETION_BONUS;
    }
}
