//! The `+3C` of the thrown weapons another machine owns: `004559B0` for the
//! grenade, the charge and the mine (selectors 26, 27 and 38), and
//! `00457660` for the missiles (25, 28 and 68). Each runs the shared
//! `0047EBF0` (the actor moves on its velocity and spin, a burning one
//! trails, `+C8` counts down) around its own visible part.

use super::grenade26::{Grenade26Error, Grenade26Host};
use super::missile::{MissileError, MissileHost, MissileLifecycleHost};
use crate::object_runtime::actor::common_update::{
    RemoteActorError, RngRemoteHost, StatusTrailHost,
};
use crate::object_runtime::{ObjectArena, ObjectId};

type GrenadeError<H> = Grenade26Error<<H as Grenade26Host>::Error>;
type RemoteMissileError<H> = MissileError<<H as MissileLifecycleHost>::Error>;

impl ObjectArena {
    /// `004559B0`: the smoke (`004558B0`), `+138` set, then `0047EBF0`.
    pub fn tick_remote_thrown<H>(
        &mut self,
        thrown: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), GrenadeError<H>>
    where
        H: Grenade26Host + StatusTrailHost<Error = <H as Grenade26Host>::Error>,
        <H as Grenade26Host>::Error: std::fmt::Display,
    {
        self.grenade26_smoke(thrown, rng, host)?;
        self.thrown_mut(thrown)?.byte_138 = 1;
        let load = host.status_load_factor();
        self.tick_remote_actor(
            thrown,
            delta,
            sine_quarter,
            Some(load),
            &mut RngRemoteHost { host, rng },
        )
        .map_err(|error| match error {
            RemoteActorError::Object(error) => Grenade26Error::Object(error),
            RemoteActorError::Motion(error) => Grenade26Error::Motion(error),
            RemoteActorError::Status(error) => Grenade26Error::Status(error),
            RemoteActorError::Host(error) => Grenade26Error::Host(error),
        })
    }

    /// `00457660`: `0047EBF0`, then the exhaust ribbon as the local update
    /// lays it.
    pub fn tick_remote_missile<H>(
        &mut self,
        missile: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        rng: &mut u32,
        host: &mut H,
    ) -> Result<(), RemoteMissileError<H>>
    where
        H: MissileHost + StatusTrailHost<Error = <H as MissileLifecycleHost>::Error>,
        <H as MissileLifecycleHost>::Error: std::fmt::Display,
    {
        self.missile(missile)?;
        let load = host.status_load_factor();
        self.tick_remote_actor(
            missile,
            delta,
            sine_quarter,
            Some(load),
            &mut RngRemoteHost {
                host: &mut *host,
                rng,
            },
        )
        .map_err(|error| match error {
            RemoteActorError::Object(error) => MissileError::Object(error),
            RemoteActorError::Motion(error) => MissileError::Motion(error),
            RemoteActorError::Status(error) => MissileError::Status(error),
            RemoteActorError::Host(error) => MissileError::Host(error),
        })?;
        self.missile_ribbon(missile, delta, host)
    }
}
