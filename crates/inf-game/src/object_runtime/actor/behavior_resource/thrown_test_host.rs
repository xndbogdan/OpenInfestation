//! The recording host the grenade's and the mine's update tests share.

use super::grenade26::Grenade26Host;
use crate::object_runtime::{
    ActorHandle, ActorStateChangeEffect, ActorStateChangeHost, ActorStateChangeQuery,
    ActorStateChangeRequest, ObjectArena, ObjectId,
};
use crate::process_particles::births::ParticleBurst;

/// Records each service a grenade or mine reaches, by the native
/// function's name; a detonation runs the real lifecycle 0047D670 back into
/// its +AC and +B0.
pub(super) struct Host<'t> {
    pub calls: Vec<&'static str>,
    pub water: i16,
    pub terrain: &'t crate::terrain_runtime::TerrainRuntime,
}

impl crate::object_runtime::StatusTrailHost for Host<'_> {
    type Error = std::convert::Infallible;
    fn status_load_factor(&self) -> i32 {
        0x10000
    }
    fn status_trail(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::FamilyUpdateEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("489d90 trail");
        Ok(())
    }
}

impl Grenade26Host for Host<'_> {
    type Error = std::convert::Infallible;
    fn load_factor(&self) -> u32 {
        0x10000
    }
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
    ) -> Result<&crate::terrain_runtime::TerrainRuntime, Self::Error> {
        Ok(self.terrain)
    }
    fn water_level(&mut self, _: &mut ObjectArena) -> Result<i16, Self::Error> {
        Ok(self.water)
    }
    fn smoke(
        &mut self,
        _: &mut ObjectArena,
        _: [u16; 3],
        _: [u16; 3],
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("489d90");
        Ok(())
    }
    fn alert(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: u32,
        _: u32,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("47dbf0");
        Ok(())
    }
    fn burst(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: [u8; 10],
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("4887d0");
        Ok(())
    }
    fn spark(
        &mut self,
        _: &mut ObjectArena,
        _: [u16; 3],
        _: [u16; 3],
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("456350");
        Ok(())
    }
    fn spawn(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u16,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("458d20");
        Ok(())
    }
    fn debris(
        &mut self,
        _: &mut ObjectArena,
        _: ParticleBurst,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn broadcast(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: u32,
        _: u32,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("47dd20");
        Ok(())
    }
    fn explosion(
        &mut self,
        _: &mut ObjectArena,
        _: [u16; 3],
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("48a0f0");
        Ok(())
    }
    fn detonate(
        &mut self,
        arena: &mut ObjectArena,
        grenade: ObjectId,
        source: ActorHandle,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("47d670");
        arena
            .change_actor_state(
                grenade,
                ActorStateChangeRequest {
                    target_state: 0,
                    argument: 0,
                    source,
                },
                &mut Lifecycle { host: self, rng },
            )
            .unwrap();
        Ok(())
    }
    fn status_visual(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("47dfd0");
        Ok(())
    }
    fn settings_7c(&self) -> u32 {
        0
    }
    fn burst_at(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: [u16; 3],
        _: [u8; 10],
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("4887d0");
        Ok(())
    }
    fn owner_message(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.calls.push("47dd40");
        Ok(())
    }
}

struct Lifecycle<'a, 't> {
    host: &'a mut Host<'t>,
    rng: &'a mut u32,
}

impl ActorStateChangeHost for Lifecycle<'_, '_> {
    type Error = String;
    type AttributionOwner = ();
    fn query(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, String> {
        // The grenade's +68 is 00404610 (itself); it has no +58 component.
        Ok(matches!(
            query,
            ActorStateChangeQuery::ExcludesAttribution68
        ))
    }
    fn attribute(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
    ) -> Result<(), String> {
        Err("the grenade skips attribution".into())
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), String> {
        match effect {
            ActorStateChangeEffect::StateOne { actor, .. } => {
                self.host.calls.push("458f80");
                arena
                    .grenade26_state_one(actor)
                    .map_err(|error| error.to_string())
            }
            ActorStateChangeEffect::StateZero { actor, .. } if arena.is_mine38(actor) => {
                self.host.calls.push("4566a0");
                arena
                    .detonate_mine38(actor, self.rng, self.host)
                    .map_err(|error| error.to_string())
            }
            ActorStateChangeEffect::StateZero { actor, .. } => {
                self.host.calls.push("4564b0");
                arena
                    .detonate_grenade26(actor, self.rng, self.host)
                    .map_err(|error| error.to_string())
            }
            other => Err(format!("unexpected {other:?}")),
        }
    }
}
