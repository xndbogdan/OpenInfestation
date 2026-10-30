//! The flamethrower's services from the world tick: the process RNG, H12's
//! height, its flames (00489D90 type 9, flags 0x800) and its burns
//! (004887D0 with the block at 00532768, or 00532778 online, whose radii it
//! writes, from the point along the flame, with its thrower as the source
//! and no replication).

use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{ActorHandle, ObjectArena, Selector76Host},
    process_particles::births::{ParticleBirthInputs, ParticleBirthRequest},
    session_platform::particles::{
        damage::WorldDamageOwners,
        radius::{apply_radius_effect, WorldRadiusOrigin},
    },
    world_radius_effect::WorldRadiusEffectRequest,
};

/// 00432240/00432374: a flame is a type-9 trail with flags 0x800.
const FLAME: u32 = 9;
const FLAME_FLAGS: u32 = 0x800;
/// 0043225D/00432268: the burn blocks, online and offline.
const NETWORK_BLOCK: u32 = 0x0053_2778;
const OFFLINE_BLOCK: u32 = 0x0053_2768;

pub(crate) struct WorldFlameHost<'a, 'exe> {
    pub owners: WorldDamageOwners<'a, 'exe>,
    pub rng: &'a mut u32,
}

impl Selector76Host for WorldFlameHost<'_, '_> {
    type Error = String;

    #[track_caller]
    fn random(&mut self) -> u32 {
        name_entry_rng_next(self.rng)
    }

    fn terrain_height(&mut self, _: &mut ObjectArena, xz: [u16; 2]) -> Result<i16, String> {
        Ok(self.owners.terrain.height_at(xz))
    }

    fn flame(
        &mut self,
        _: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), String> {
        let owners = &mut self.owners;
        let request = ParticleBirthRequest {
            particle_type: FLAME,
            position: point,
            velocity: Some(velocity),
            flags: FLAME_FLAGS,
            inputs: ParticleBirthInputs {
                network_active: owners.factory_process.network_flag != 0,
                phase: Some(u32::from(owners.session.current_phase().id())),
                process: owners.factory_process,
            },
        };
        owners
            .particles
            .birth_trail(request, owners.executable, self.rng)
            .map(|_| ())
            .map_err(|error| format!("00489D90 flame: {error:?}"))
    }

    fn burn(
        &mut self,
        arena: &mut ObjectArena,
        point: [u16; 3],
        radii: [u16; 2],
        thrower: ActorHandle,
    ) -> Result<(), String> {
        let address = if self.owners.factory_process.network_flag != 0 {
            NETWORK_BLOCK
        } else {
            OFFLINE_BLOCK
        };
        let bytes = self
            .owners
            .executable
            .mapped(address, 10)
            .map_err(|error| error.to_string())?
            .bytes;
        let mut block: [u8; 10] = bytes.try_into().expect("ten mapped bytes");
        block[4..6].copy_from_slice(&radii[0].to_le_bytes());
        block[6..8].copy_from_slice(&radii[1].to_le_bytes());
        apply_radius_effect(
            arena,
            WorldRadiusEffectRequest {
                origin: WorldRadiusOrigin::PointBlock {
                    position: point,
                    block,
                },
                damage_source: thrower,
                replicate: false,
            },
            &mut self.owners,
            self.rng,
        )
        .map_err(|error| format!("004887D0 burn: {error}"))
    }
}

#[cfg(test)]
#[path = "selector76_tests.rs"]
mod tests;
