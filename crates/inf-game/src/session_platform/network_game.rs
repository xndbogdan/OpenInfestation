//! `004875A0`'s network branch over the loaded world, and the world the game
//! protocol reaches there: the header's status, the player table's changes
//! (`0045C940`), the sends (`0045C5A0`), the drain (`00492830`), the lost
//! session's return to the frontend and the recharging grenades.

use inf_formats::native_basis::NativeBasisMath;

use super::particles::damage::{world_retirement_host, BehaviorFactoryLoan, WorldDamageOwners};
use super::{PortPlatform, PortSession, PortSessionServices};
use crate::network::game::{GameHeaderHost, GameNetState};
use crate::network::game_tick::{ActorSendTimer, GameFrameHost};
use crate::network::protocol::{GameProtocol, GameSend, GameWorldHost, GAME_CHANNEL};
use crate::network::receivers::{GameCall, GameMessageHost, HoldOut};
use crate::object_runtime::actor_record::{record_head, skipped_length, ActorRecordHost};
use crate::object_runtime::{
    ActorHandle, ActorStateChangeRequest, ObjectArena, ObjectId, PlayerId, VehicleCarryContext,
    VehicleCarryEffect, VehicleCarryHost, VehicleDropHost,
};
use crate::session::{SessionPhase, SessionState};

/// `004875A0`'s recharge: grenades every 4 seconds.
const RECHARGE_MICROS: i32 = 4_000_000;

impl<P: PortPlatform> PortSession<P> {
    /// The protocol's state goes from the link to the world while it updates
    /// (the furniture's effects, the actors, the camera and the particles):
    /// a player's vehicle hit or killed tells it (`0045DA50`, `0045D8F0`).
    pub(super) fn lend_network_game(&mut self) {
        if self.factory_process.network_flag == 0 {
            return;
        }
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let Some(network) = PortSessionServices::network_platform(&mut self.platform) else {
            return;
        };
        world
            .arena
            .lend_network_game(Box::new(crate::network::notices::NetworkGameHold {
                game: std::mem::take(&mut network.link.game),
                frame: std::mem::take(&mut network.link.frame),
            }));
    }

    /// The protocol's state back to the link before the network branch.
    pub(super) fn return_network_game(&mut self) {
        let Some(hold) = self
            .world
            .as_mut()
            .and_then(|world| world.arena.take_network_game())
        else {
            return;
        };
        if let Some(network) = PortSessionServices::network_platform(&mut self.platform) {
            network.link.game = hold.game;
            network.link.frame = hold.frame;
        }
    }

    /// The network branch of the world update, after the particles.
    pub(super) fn tick_network(&mut self, delta: u32, state: &mut SessionState) {
        if self.factory_process.network_flag == 0 {
            return;
        }
        if let Err(error) = self.run_network_tick(delta, state) {
            if !self.network_stop_logged {
                eprintln!("Network game stopped after reached writes: {error}");
                self.network_stop_logged = true;
            }
        }
    }

    fn run_network_tick(&mut self, delta: u32, state: &mut SessionState) -> Result<(), String> {
        let Some(pass) = self.run_network_phase(delta, state, NetworkPhase::Receive)? else {
            return Ok(());
        };
        // 004878D2: the departed players' objects go at once.
        if pass.disposed {
            self.drain_network_disposal()?;
        }
        self.run_network_phase(delta, state, NetworkPhase::Send)?;
        self.run_network_phase(delta, state, NetworkPhase::Drain)?;
        // 0048790C: recharging grenades, every 4 seconds of G+0.
        if pass.recharge {
            let countdown = &mut self.network_recharge;
            if *countdown > delta as i32 {
                *countdown -= delta as i32;
            } else {
                *countdown += RECHARGE_MICROS;
                if let Some(player) = pass.player {
                    let executable = inf_formats::FrontendExecutable::parse(
                        self.catalog.assets().executable_bytes(),
                    )
                    .map_err(|error| error.to_string())?;
                    let clock = crate::frontend_camera::AnimationClockSource::animation_value(
                        &self.process,
                    );
                    let text = super::cheats::pickup_text(&self.profile_name_classification);
                    let tables = super::cheats::GrantTables::parse(executable)?;
                    let loan = |clock| super::cheats::GrantLoan {
                        executable,
                        residency: &self.residency,
                        process: &self.factory_process,
                        text: &text,
                        clock,
                    };
                    let arena = &mut self.world.as_mut().expect("loaded world").arena;
                    if pass.game_mode != 3 {
                        super::cheats::pickup(arena, player, 0x1a, 1, &tables, loan(clock))?;
                    }
                    for (upgrade, grenade) in [(0xa3, 0x1d), (0xa0, 0x1c)] {
                        if player_has(arena, player, upgrade)? {
                            super::cheats::pickup(arena, player, grenade, 1, &tables, loan(clock))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 00479840 over the world's owners, after `0045C940` disposed of a
    /// departed player's objects.
    fn drain_network_disposal(&mut self) -> Result<(), String> {
        let Some(world) = self.world.as_mut() else {
            return Ok(());
        };
        let executable =
            inf_formats::FrontendExecutable::parse(self.catalog.assets().executable_bytes())
                .map_err(|error| error.to_string())?;
        let grant_text = super::world_owners::current_grant_text(&self.profile_name_classification);
        let grants = crate::object_runtime::LiveGrantContext {
            process: Some(&self.factory_process),
            executable: Some(executable),
            resident: Some(&self.residency),
            text_data: Some(&grant_text),
        };
        let mut voices = self.platform.sound_voice_teardown();
        let report = world
            .arena
            .drain_object_disposal(crate::object_runtime::ObjectDisposalRequest {
                mission: &mut world.mission,
                grants: &grants,
                sound: crate::object_runtime::SoundTeardownService::Available {
                    sounds: &mut self.sounds,
                    voices: &mut voices,
                },
                state: &mut world.actor_updates.disposal,
                failure_policy:
                    crate::object_runtime::ObjectDisposalFailurePolicy::RetainStoppedObjectApproximation,
                listeners: Some(&mut world.listeners),
            })
            .map_err(|error| format!("00479840: {error}"))?;
        for stop in report.newly_stopped {
            eprintln!(
                "World object disposal stopped: {:?} {:?}: {} (RetainStoppedObjectApproximation)",
                stop.object, stop.phase, stop.reason,
            );
        }
        if let Some(ship) = report.exit_pending {
            return Err(format!(
                "00479840 paused at {ship:?}'s exit 0043E120 in a network game"
            ));
        }
        Ok(())
    }

    /// One part of `004875A0`'s network work, each after the queued sends.
    fn run_network_phase(
        &mut self,
        delta: u32,
        state: &mut SessionState,
        phase: NetworkPhase,
    ) -> Result<Option<NetworkPass>, String> {
        let Some(world) = self.world.as_mut() else {
            return Ok(None);
        };
        let executable =
            inf_formats::FrontendExecutable::parse(self.catalog.assets().executable_bytes())
                .map_err(|error| error.to_string())?;
        let sine = self.catalog.assets().sine_table().values;
        let math = NativeBasisMath::new(sine);
        let inverse_sine = inverse_sine_table(executable)?;
        let grant_text = super::world_owners::current_grant_text(&self.profile_name_classification);
        let pickup_text = super::cheats::pickup_text(&self.profile_name_classification);
        let camera_settings = self.platform.world_tick_inputs().camera_settings;
        let clock = crate::frontend_camera::AnimationClockSource::animation_value(&self.process);
        let null = self
            .factory_process
            .process_null_handle
            .ok_or("the network game requires the process NULL 0054E894")?;
        let local_slot = self.factory_process.local_slot;
        // 0046B030: the first player's unit (+D4).
        let player = world.arena.first_player();
        let unit = match player {
            Some(player) => {
                let handle = world
                    .arena
                    .world_player(player)
                    .map_err(|error| error.to_string())?
                    .handle_d4;
                world
                    .arena
                    .resolve_actor_handle(handle)
                    .map_err(|error| error.to_string())?
            }
            None => None,
        };
        let world_word = world.mission.current_world_a0 as u16;
        let mut network = PortSessionServices::network_platform(&mut self.platform)
            .ok_or("the network game requires the platform's network link")?;
        let identity = network.net_identity();
        let words = network.settings.words_mut();
        let link = &mut *network.link;
        // 004875A0's status: this slot's state (settings +14, or 0x1F
        // without a unit), the arena +1C, catchup +4C, the mode +10 with the
        // host's bit 23, the detail +70 and the five flags.
        let state_bits = if unit.is_some() {
            words[0x14 / 4] & 0x1f
        } else {
            0x1f
        };
        let mut mode = words[0x10 / 4];
        if link.game.host_flag_23 != 0 {
            mode |= 0x80;
        }
        let flags = (((((words[0x34 / 4] & 1) << 1 | words[0x2c / 4] & 1) << 1
            | words[0x28 / 4] & 1)
            << 1
            | words[0x24 / 4] & 1)
            << 1)
            | words[0x20 / 4] & 1;
        let status = state_bits
            | ((words[0x1c / 4] & 0x1f) << 5 | words[0x4c / 4] & 0x1f) << 5
            | ((flags << 2 | words[0x70 / 4] & 3) << 8 | mode) << 16;
        let recharge = words[0x28 / 4] != 0;
        let game_mode = words[0x10 / 4];
        let queued = crate::network::merge_sends(
            world.arena.take_ordered_network_sends(),
            self.particles.take_ordered_network_sends(),
        );
        let mut owners = WorldDamageOwners {
            grant_text: Some(&grant_text),
            camera_settings: Some(camera_settings),
            flash: Some(&mut world.flash),
            mission: &mut world.mission,
            tasks: &mut self.tasks,
            terrain: &mut world.terrain,
            particles: &mut self.particles,
            sounds: &mut self.sounds,
            furniture: &mut self.furniture,
            residency: &self.residency,
            executable,
            factory_process: &mut self.factory_process,
            behavior_tables: &mut self.actor_behavior_tables,
            animation_clock: &self.process,
            session: state,
            particle_load_factor: world.fade.performance.particle_load_factor,
            behavior_factory: Some(BehaviorFactoryLoan {
                listeners: &mut world.listeners,
                sine: &sine,
                world_flags: world.world_flags,
            }),
            map: None,
        };
        // The frame's state is the sends' in their phase, the received
        // frames' notices' in the drain.
        let (drain_frame, send_frame) = if phase == NetworkPhase::Drain {
            (Some(&mut link.frame), None)
        } else {
            (None, Some(&mut link.frame))
        };
        let mut host = WorldGameHost {
            arena: &mut world.arena,
            owners: &mut owners,
            rng: &mut world.rng,
            settings: words,
            identity,
            math: &math,
            inverse_sine: &inverse_sine,
            clock,
            null,
            local_slot,
            instances: &world.report.instances,
            frame: drain_frame,
            pickup_text: &pickup_text,
            world_start: &mut world.world_start,
            terrain_policy: &mut self.terrain_policy,
            world_flags: world.world_flags,
        };
        let mut protocol = GameProtocol::new(&mut link.game, &mut host);
        let transport = &mut link.transport;
        // World code's sends went into the open frame as it ran.
        for (id, send) in queued {
            protocol
                .send(transport, id, send)
                .map_err(|error| error.to_string())?;
        }
        let mut disposed = false;
        self.network_initials = protocol.state.details;
        self.network_team_flag = protocol.state.host_flag_23;
        if phase == NetworkPhase::Receive {
            if status != protocol.state.header_out[0] {
                transport.state.slot_assigned = 1;
            }
            if transport.state.slot_assigned != 0
                || world_word != protocol.state.header_out[3] as u16
            {
                transport.state.slot_assigned = 0;
                transport.finish_frame(GAME_CHANNEL, &mut protocol);
                let header = &mut protocol.state.header_out;
                header[3] = header[3] & 0xffff_0000 | u32::from(world_word);
                header[0] = status;
                protocol.state.publish_header(&transport.state);
                transport.flush_frame(GAME_CHANNEL, &mut protocol);
            }
            disposed = protocol
                .receive_slots(&transport.state)
                .map_err(|error| error.to_string())?;
        } else if phase == NetworkPhase::Send {
            let frame = send_frame.expect("the send phase keeps the frame");
            protocol
                .send_frame(transport, frame, delta)
                .map_err(|error| error.to_string())?;
        } else {
            protocol
                .drain(transport, &mut link.table)
                .map_err(|error| error.to_string())?;
            drop(protocol);
            if transport.state.host_changed || transport.state.session_lost {
                // 004391B0(2) and 00438380: back to the frontend.
                let _ = state.request_phase(SessionPhase::Frontend);
            }
        }
        Ok(Some(NetworkPass {
            disposed,
            player,
            recharge,
            game_mode,
        }))
    }
}

/// `004875A0`'s network work in order: the status header and the player
/// table's changes (`0045C940`), then (after a departed player's objects
/// drain) the frame's sends (`0045C5A0`), then the frames received
/// (`00492830`) and a lost session's return to the frontend.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NetworkPhase {
    Receive,
    Send,
    Drain,
}

/// What the first part of the network work leaves the rest.
struct NetworkPass {
    disposed: bool,
    player: Option<PlayerId>,
    recharge: bool,
    game_mode: u32,
}

/// `0046BDC0`: the player's `+100` bit.
fn player_has(arena: &ObjectArena, player: PlayerId, id: u32) -> Result<bool, String> {
    let bits = arena
        .world_player(player)
        .map_err(|error| error.to_string())?
        .bits_100;
    Ok(bits
        .get(id as usize / 32)
        .is_some_and(|word| word & 1 << (id % 32) != 0))
}

pub(super) fn inverse_sine_table(
    executable: inf_formats::FrontendExecutable<'_>,
) -> Result<[i16; 1024], String> {
    let bytes = executable
        .mapped(0x0053_ff00, 2048)
        .map_err(|error| error.to_string())?
        .bytes;
    Ok(std::array::from_fn(|i| {
        i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]])
    }))
}

/// The loaded world the game protocol reaches. Object ids are the actors'
/// handles.
struct WorldGameHost<'a, 'o, 'exe> {
    arena: &'a mut ObjectArena,
    owners: &'a mut WorldDamageOwners<'o, 'exe>,
    rng: &'a mut u32,
    settings: &'a mut [u32],
    identity: ([u8; 16], [u8; 6]),
    math: &'a NativeBasisMath,
    inverse_sine: &'a [i16; 1024],
    clock: u32,
    null: ActorHandle,
    local_slot: u8,
    /// The world's placements, which `0048E170` builds from.
    instances: &'a crate::world_loader::WorldInstanceAllocation,
    /// The frame's state while the frames received run (their notices'
    /// lines and attacker).
    frame: Option<&'a mut crate::network::game_tick::GameFrameState>,
    /// The banner's capture pickup's text.
    pickup_text: &'a crate::grant_text::PickupTextData<'a>,
    /// The world's starts, which a remote gate's construction reads.
    world_start: &'a mut crate::world_start::WorldStartState,
    /// The spacing a host's detail change reselects (`004502C0`), and the
    /// world's H14 flags it reads.
    terrain_policy: &'a mut crate::terrain_runtime::ProcessTerrainPolicy,
    world_flags: u32,
}

impl WorldGameHost<'_, '_, '_> {
    /// A routine that can reach a notice (`0045DA50`, `0045D8F0`) runs with
    /// the protocol's state lent to the world, as while it updates.
    fn with_network<R>(
        &mut self,
        state: &mut GameNetState,
        run: impl FnOnce(&mut Self) -> Result<R, String>,
    ) -> Result<R, String> {
        let frame = self
            .frame
            .as_deref_mut()
            .ok_or("a notice outside the frames received has no frame state")?;
        self.arena
            .lend_network_game(Box::new(crate::network::notices::NetworkGameHold {
                game: std::mem::take(state),
                frame: std::mem::take(frame),
            }));
        let result = run(self);
        let hold = self
            .arena
            .take_network_game()
            .ok_or("the world gave the protocol's state away")?;
        *state = hold.game;
        **self.frame.as_mut().expect("lent above") = hold.frame;
        result
    }

    /// A banner routine with the protocol's state lent (a capture scores
    /// its teams).
    fn banner_call(
        &mut self,
        state: &mut GameNetState,
        banner: u32,
        run: impl FnOnce(
            &mut ObjectArena,
            ObjectId,
            &crate::object_runtime::BannerRequest<'_>,
            &mut super::banner::WorldBannerHost<'_>,
        ) -> Result<(), crate::object_runtime::BannerError<String>>,
    ) -> Result<u32, String> {
        let banner = self.actor(banner)?;
        self.with_network(state, |host| {
            let request = crate::object_runtime::BannerRequest {
                delta: 0,
                local_slot: host.local_slot,
                null: host.null,
                math: host.math,
                terrain: &*host.owners.terrain,
            };
            let mut banner_host = super::banner::WorldBannerHost {
                residency: host.owners.residency,
                executable: host.owners.executable,
                process: &*host.owners.factory_process,
                text: Some(host.pickup_text),
                clock: host.clock,
            };
            run(host.arena, banner, &request, &mut banner_host).map_err(|error| error.to_string())
        })?;
        Ok(0)
    }

    /// Sonic Shock's pulse or the flamethrower another machine fired.
    fn remote_weapon(&mut self, object: u32, flame: bool) -> Result<u32, String> {
        use super::world_actor_factory::{RequiredListenerEffects, WorldActorChildHost};
        let actor = self.actor(object)?;
        let loan = self
            .owners
            .behavior_factory
            .as_mut()
            .ok_or("a remote weapon needs the world's factory")?;
        let mut children = WorldActorChildHost {
            residency: self.owners.residency,
            executable: self.owners.executable,
            process: &*self.owners.factory_process,
            terrain: &*self.owners.terrain,
            world_flags: loan.world_flags,
            clock: self.clock,
            sine: loan.sine,
            math: self.math,
            camera_settings: self.owners.camera_settings,
            world_start: Some(&mut *self.world_start),
            listeners: &mut *loan.listeners,
            listener_effects: &mut RequiredListenerEffects,
        };
        let mut pulse = super::DrawPulseHost {
            flash: self
                .owners
                .flash
                .as_deref_mut()
                .ok_or("a remote pulse needs the world's flash")?,
            map: self.owners.map.as_deref_mut(),
            residency: self.owners.residency,
        };
        let mut runtime = crate::object_runtime::WeaponFiringRuntime {
            executable: self.owners.executable,
            factory_process: &*self.owners.factory_process,
            particles: &mut *self.owners.particles,
            statistics: self.owners.mission.record_0c.as_deref_mut(),
            sounds: &mut *self.owners.sounds,
            residency: self.owners.residency,
            phase: Some(u32::from(self.owners.session.current_phase().id())),
            children: Some(&mut children),
            pulse: Some(&mut pulse),
        };
        if flame {
            runtime.throw_flame(self.arena, actor)
        } else {
            let lookup = crate::object_runtime::PlayerActorLookupContext {
                process: Some(&*self.owners.factory_process),
            };
            runtime.sonic_shock(self.arena, actor, lookup, self.rng)
        }
        .map_err(|error| error.to_string())?;
        Ok(0)
    }

    /// What world code sent while a receiver ran, the world's and the
    /// particles' in order.
    fn world_sends(&mut self) -> Vec<(u32, GameSend)> {
        crate::network::merge_sends(
            self.arena.take_ordered_network_sends(),
            self.owners.particles.take_ordered_network_sends(),
        )
    }

    fn resolve(&mut self, handle: u32) -> Result<Option<ObjectId>, String> {
        self.arena
            .resolve_actor_handle(ActorHandle::from_raw(handle))
            .map_err(|error| error.to_string())
    }

    fn actor(&mut self, object: u32) -> Result<ObjectId, String> {
        self.resolve(object)?
            .ok_or_else(|| format!("handle {object:#010x} names no live actor"))
    }

    /// `0047E600` for a handle this machine lacks: below `0x4000` the type
    /// is a placement of the current world (`0048E170`), else a resource
    /// less `0x4000` at no position (`00460310(0, 0, -1, 2, handle)`).
    /// False when the factory refuses it.
    fn build_remote_actor(&mut self, handle: u32, kind: u16) -> Result<bool, String> {
        use super::world_actor_factory::{
            RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
            WorldFactoryInitialization, WorldFactoryPosition,
        };
        let loan = self
            .owners
            .behavior_factory
            .as_mut()
            .ok_or("a remote actor needs the world's factory")?;
        let mut factory = WorldActorChildHost {
            residency: self.owners.residency,
            executable: self.owners.executable,
            process: &*self.owners.factory_process,
            terrain: &*self.owners.terrain,
            world_flags: loan.world_flags,
            clock: self.clock,
            sine: loan.sine,
            math: self.math,
            camera_settings: self.owners.camera_settings,
            world_start: Some(&mut *self.world_start),
            listeners: &mut *loan.listeners,
            listener_effects: &mut RequiredListenerEffects,
        };
        let request = if kind < 0x4000 {
            // 0048E170: past the count it builds nothing and answers the
            // index. A record naming the process NULL builds in mode 0.
            let index = u32::from(kind);
            let network_handle =
                (handle != self.null.raw()).then_some(ActorHandle::from_raw(handle));
            match super::world_actor_factory::placement_request(
                self.instances,
                &*self.owners.terrain,
                index,
                network_handle,
            )? {
                Some(request) => request,
                None => return Ok(index != 0),
            }
        } else {
            WorldActorFactoryRequest {
                resource: crate::object_runtime::ActorChildResource::Full(u32::from(kind) - 0x4000),
                position: WorldFactoryPosition::Residue,
                spawn_mode_1: false,
                network_handle: Some(ActorHandle::from_raw(handle)),
                initialization: WorldFactoryInitialization::Bare,
            }
        };
        let created = factory
            .create_object(self.arena, request, self.rng)
            .map_err(|error| format!("00460310: {error}"))?;
        Ok(created.is_some())
    }
}

/// `actor`'s +8C(0, the process NULL, its +D4): a held actor let go by a
/// record whose holder has gone, or by message 9.
fn release_held(
    arena: &mut ObjectArena,
    actor: ObjectId,
    owners: &mut WorldDamageOwners<'_, '_>,
    rng: &mut u32,
) -> Result<(), String> {
    let null = owners
        .factory_process
        .process_null_handle
        .ok_or("a release needs the process NULL 0054E894")?;
    // The release reads the process words only; the host lends the rest.
    let process = *owners.factory_process;
    let context = VehicleCarryContext {
        process: &process,
        executable: owners.executable,
        resident: owners.residency,
        text_data: owners.grant_text,
    };
    let mut host = NetworkDropHost {
        owners: owners.reborrow(),
    };
    arena
        .network_hold_release(actor, null, context, &mut host, rng)
        .map_err(|error| format!("{actor:?}'s release (+8C): {error}"))
}

/// A hold's flash and set-down, and a release's searches, over the world's
/// owners: the vehicle's drop's services without its attachment, which no
/// release reaches.
struct NetworkDropHost<'o, 'exe> {
    owners: WorldDamageOwners<'o, 'exe>,
}

impl VehicleCarryHost for NetworkDropHost<'_, '_> {
    type Error = String;
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: VehicleCarryEffect,
        rng: &mut u32,
    ) -> Result<(), String> {
        let owners = &mut self.owners;
        super::world_actor_update::CarryEffects {
            particles: &mut *owners.particles,
            sounds: &mut *owners.sounds,
            residency: owners.residency,
            executable: owners.executable,
            process: &*owners.factory_process,
            phase: u32::from(owners.session.current_phase().id()),
        }
        .effect(arena, effect, rng)
    }
}

impl VehicleDropHost for NetworkDropHost<'_, '_> {
    fn call_mines(
        &mut self,
        arena: &mut ObjectArena,
        rig: ObjectId,
        radius: u32,
    ) -> Result<u32, String> {
        super::world_actor_update::call_world_mines(arena, rig, radius, self.owners.factory_process)
    }
    fn select_targets(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), String> {
        let owners = &mut self.owners;
        let loan = owners
            .behavior_factory
            .as_mut()
            .ok_or("a dropped actor's target search needs the world's factory")?;
        super::world_actor_update::DroppedTargetSearch {
            tables: &mut *owners.behavior_tables,
            process: &*owners.factory_process,
            residency: owners.residency,
            terrain: &*owners.terrain,
            world_flags: loan.world_flags,
            animation: owners.animation_clock,
            sine: loan.sine,
            listeners: &mut *loan.listeners,
            tasks: &mut *owners.tasks,
            session: owners.session,
            particles: &mut *owners.particles,
            executable: owners.executable,
        }
        .run(arena, actor, rng)
    }
    fn attach_effect(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        _: &mut u32,
    ) -> Result<(), String> {
        Err(format!(
            "{actor:?}: 00405420's attachment belongs to a vehicle's own drop, which no network release reaches"
        ))
    }
}

/// The record's reach into the world: state changes and model refreshes.
struct RecordHost<'h, 'a, 'o, 'exe> {
    owners: &'h mut WorldDamageOwners<'o, 'exe>,
    rng: &'h mut u32,
    sends: &'h mut Vec<(u32, GameSend)>,
    _arena: std::marker::PhantomData<&'a ()>,
}

impl ActorRecordHost for RecordHost<'_, '_, '_, '_> {
    type Error = String;
    /// `0047E35B`: onto the ground (00479DD0), then its +8C(0, NULL, +D4).
    fn drop_held(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        arena
            .ground_actor(actor, &*self.owners.terrain)
            .map_err(|error| error.to_string())?;
        release_held(arena, actor, self.owners, self.rng)
    }
    fn change_state(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        state: u32,
    ) -> Result<(), String> {
        let source = self
            .owners
            .factory_process
            .process_null_handle
            .unwrap_or(ActorHandle::from_raw(0));
        let mut host = world_retirement_host(self.owners, self.rng);
        arena
            .change_actor_state(
                actor,
                ActorStateChangeRequest {
                    target_state: state as i32,
                    argument: 0,
                    source,
                },
                &mut host,
            )
            .map_err(|error| format!("0047D670: {error:?}"))
    }
    fn refresh_model(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        arena
            .refresh_visual_model(actor, self.owners.residency)
            .map_err(|error| error.to_string())
    }
    /// The holder's +BC (its hold, or none: false), then `004120B0`.
    fn holder_holds(
        &mut self,
        arena: &ObjectArena,
        holder: ObjectId,
        handle: ActorHandle,
    ) -> Result<bool, String> {
        if !arena.has_hold(holder) {
            return Ok(false);
        }
        arena
            .hold_has(holder, handle)
            .map_err(|error| error.to_string())
    }
    fn send_release(&mut self, pair: [u32; 2]) -> Result<(), String> {
        self.sends.push((9, GameSend::Pair(pair)));
        Ok(())
    }
    fn ball_sound(&mut self, sound: u32, position: [u16; 3]) -> Result<(), String> {
        let owners = &mut *self.owners;
        owners
            .sounds
            .enqueue(
                owners.residency,
                crate::process_particles::audio::ParticleSoundRequest {
                    sound,
                    position,
                    volume: 0x10000,
                    pitch: 0x10000,
                    flags: 0,
                },
            )
            .map(|_| ())
            .map_err(|error| format!("00487F10 ball sound {sound:#x}: {error:?}"))
    }
}

impl GameMessageHost for WorldGameHost<'_, '_, '_> {
    type Error = String;
    fn call(&mut self, call: GameCall, state: &mut GameNetState) -> Result<u32, String> {
        match call {
            GameCall::Resolve { handle } => Ok(if self.resolve(handle)?.is_some() {
                handle
            } else {
                0
            }),
            GameCall::Dispose { object } => {
                let actor = self.actor(object)?;
                self.arena
                    .dispose(actor)
                    .map_err(|error| error.to_string())?;
                Ok(0)
            }
            GameCall::SetState {
                object,
                null,
                state,
            } => {
                let actor = self.actor(object)?;
                let mut host = world_retirement_host(self.owners, self.rng);
                self.arena
                    .change_actor_state(
                        actor,
                        ActorStateChangeRequest {
                            target_state: state as i32,
                            argument: 0,
                            source: ActorHandle::from_raw(null),
                        },
                        &mut host,
                    )
                    .map_err(|error| format!("0047D670: {error:?}"))?;
                Ok(0)
            }
            GameCall::FirstPlayer => Ok(u32::from(self.arena.first_player().is_some())),
            // 0045D1B0's 004135D0(text, 0) on the first player's +88.
            GameCall::PlayerText { mut text } => {
                let player = self
                    .arena
                    .first_player()
                    .ok_or("0045D1B0 reaches no first player")?;
                text.push(0);
                let recipient = self
                    .arena
                    .player_message(player)
                    .map_err(|error| error.to_string())?
                    .identity();
                self.arena
                    .apply_player_message(crate::object_runtime::PlayerMessageRequest {
                        recipient,
                        operation: crate::object_runtime::PlayerMessageOperation::Send {
                            text: Some(&text),
                            flag: 0,
                        },
                    })
                    .map_err(|error| error.to_string())?;
                Ok(0)
            }
            // 0045DB20's 00487EC0(id, &position, volume, pitch, 0); the
            // queue releases its answer as 004923C0 does.
            GameCall::Sound {
                id,
                position,
                values: [volume, pitch],
            } => {
                self.owners
                    .sounds
                    .enqueue(
                        self.owners.residency,
                        crate::process_particles::audio::ParticleSoundRequest {
                            sound: id,
                            position,
                            volume,
                            pitch,
                            flags: 0,
                        },
                    )
                    .map_err(|error| format!("00487EC0 sound {id:#x}: {error:?}"))?;
                Ok(0)
            }
            GameCall::ErrorDispose { .. } => Ok(0),
            // 0045DEB0's slot routines: 00462480, 00462510 and 00462560.
            GameCall::SlotUnknown { handle } => Ok(u32::from(
                self.arena
                    .actor_handle_mode2_admissible(ActorHandle::from_raw(handle))
                    .map_err(|error| error.to_string())?,
            )),
            GameCall::SlotObject { handle } => {
                match self
                    .arena
                    .actor_handle_slot_object(ActorHandle::from_raw(handle))
                {
                    Some(object) => Ok(self
                        .arena
                        .actor(object)
                        .map_err(|error| error.to_string())?
                        .handle
                        .raw()),
                    None => Ok(0),
                }
            }
            GameCall::SlotFree { handle } => Ok(u32::from(
                self.arena
                    .forget_actor_handle_slot(ActorHandle::from_raw(handle)),
            )),
            // 0045EDE0's 0048D980 and 0045EE00's 00431EF0 on the object: the
            // pulse or the flame here, told to no one again.
            GameCall::SonicPulse { object } => self.remote_weapon(object, false),
            GameCall::Effect431ef0 { object } => self.remote_weapon(object, true),
            // 0045EA20's 004665D0 (kind 0) or 00466710 (kind 1): the
            // particle here, flag 1 keeping it from being told again.
            GameCall::PointEffect {
                kind,
                argument,
                points,
                extra,
            } => {
                use crate::process_particles::factories::{
                    ParticleFactoryContext, ParticleFactoryKind, ParticleFactoryRequest,
                    ParticleFactorySources, ParticleVelocitySource,
                };
                let vector =
                    |bytes: [u8; 6]| crate::process_particles::ParticleVectorBytes(bytes.map(Some));
                self.owners
                    .particles
                    .create_factory_particle(
                        ParticleFactoryRequest {
                            kind: if kind == 0 {
                                ParticleFactoryKind::Factory4665D0
                            } else {
                                ParticleFactoryKind::Factory466710
                            },
                            selector: argument,
                            source_handle: ActorHandle::from_raw(extra.unwrap_or(self.null.raw())),
                            flags: 1,
                            velocity_source: ParticleVelocitySource::Separate,
                        },
                        &mut ParticleFactoryContext {
                            process: &mut *self.owners.factory_process,
                            rng: &mut *self.rng,
                            executable: self.owners.executable,
                        },
                        &mut ParticleFactorySources {
                            position: vector(points[0]),
                            separate_velocity: Some(vector(points[1])),
                        },
                        &mut super::particles::factory::WorldParticleFactoryHost {
                            session: self.owners.session,
                        },
                    )
                    .map_err(|error| format!("{error:?}"))?;
                Ok(0)
            }
            // 0045E050's 004887D0(&record, &point, value, 0) on G: the blast
            // here, not sent again.
            GameCall::WorldEffect {
                record,
                point,
                value,
            } => {
                let position = std::array::from_fn(|axis| {
                    u16::from_le_bytes([point[axis * 2], point[axis * 2 + 1]])
                });
                super::particles::radius::apply_radius_effect(
                    self.arena,
                    crate::world_radius_effect::WorldRadiusEffectRequest {
                        origin: super::particles::radius::WorldRadiusOrigin::PointBlock {
                            position,
                            block: record,
                        },
                        damage_source: ActorHandle::from_raw(value),
                        replicate: false,
                    },
                    self.owners,
                    self.rng,
                )?;
                Ok(0)
            }
            // Virtual +B8: the banner an object is.
            GameCall::BannerOf { object } => {
                let actor = self.actor(object)?;
                Ok(match self.arena.banner_of(actor) {
                    Some(banner) => self
                        .arena
                        .actor(banner)
                        .map_err(|error| error.to_string())?
                        .handle
                        .raw(),
                    None => 0,
                })
            }
            // 0045B9E0, 0045BA50 and 0045BA90 on this machine's banner.
            GameCall::BannerTaken { banner, by } => {
                self.banner_call(state, banner, |arena, banner, request, host| {
                    arena.banner_taken(banner, ActorHandle::from_raw(by), request, host)
                })
            }
            GameCall::BannerDropped { banner, by } => {
                self.banner_call(state, banner, |arena, banner, request, host| {
                    arena.banner_dropped(banner, ActorHandle::from_raw(by), request, host)
                })
            }
            GameCall::BannerCaptured {
                banner,
                values: [carrier, team],
            } => self.banner_call(state, banner, |arena, banner, request, host| {
                arena.capture_banner(banner, ActorHandle::from_raw(carrier), team, request, host)
            }),
            // 0045BBD0's 0046CC50 on the first player.
            GameCall::BannerEvent {
                banner,
                event,
                value,
            } => self.banner_call(state, banner, |arena, banner, _, host| {
                use crate::object_runtime::BannerHost;
                host.announce(arena, banner, event as u8, ActorHandle::from_raw(value))
                    .map_err(crate::object_runtime::BannerError::Host)
            }),
            // 0045DFB0's 0047C6E0: the packet's DWORD and the source.
            GameCall::Damage {
                object,
                record,
                amount,
            } => {
                let actor = self.actor(object)?;
                let packet = crate::object_runtime::ActorDamagePacket {
                    strength: record as u16 as i16,
                    channel: (record >> 16) as u8,
                    byte_03: Some((record >> 24) as u8),
                };
                self.with_network(state, |host| {
                    super::particles::damage::apply_world_actor_damage(
                        host.arena,
                        actor,
                        packet,
                        ActorHandle::from_raw(amount),
                        host.owners,
                        host.rng,
                    )
                    .map_err(|error| error.to_string())
                })?;
                Ok(0)
            }
            // 0048EA30: the current world's four DWORDs of W+B0.
            GameCall::WorldWords { words } => {
                let mission = &mut *self.owners.mission;
                let at = mission.current_world_a0 as usize * 4;
                for (index, word) in words.into_iter().enumerate() {
                    *mission_b0_word(mission, at + index)? = word;
                }
                Ok(0)
            }
            // 0048E980: a bit of them.
            GameCall::WorldBit { bit } => {
                let mission = &mut *self.owners.mission;
                let at = mission.current_world_a0 as usize * 4 + (bit as usize / 32);
                *mission_b0_word(mission, at)? |= 1 << (bit % 32);
                Ok(0)
            }
            // 0045EB30's take (into a stack slot) and 0045EBA0's release
            // (from +D4): the actor's +8C.
            GameCall::Hold {
                object,
                take,
                other,
                out,
            } => {
                let actor = self.actor(object)?;
                match (take, out) {
                    (true, HoldOut::Stack) => {
                        let owners = &*self.owners;
                        let context = VehicleCarryContext {
                            process: &*owners.factory_process,
                            executable: owners.executable,
                            resident: owners.residency,
                            text_data: owners.grant_text,
                        };
                        self.arena
                            .network_hold_take::<String>(
                                actor,
                                ActorHandle::from_raw(other),
                                context,
                            )
                            .map(u32::from)
                            .map_err(|error| format!("{actor:?}'s take (+8C): {error}"))
                    }
                    (false, HoldOut::FieldD4) => {
                        release_held(self.arena, actor, self.owners, self.rng)?;
                        Ok(0)
                    }
                    _ => Err(format!("{actor:?}: no receiver makes this hold call")),
                }
            }
            // Virtual +BC: the holder itself names its hold.
            GameCall::Component { object } => {
                let actor = self.actor(object)?;
                Ok(if self.arena.has_hold(actor) {
                    object
                } else {
                    0
                })
            }
            // 004122A0, the hold's +C: the slot flash at the held actor.
            GameCall::ComponentTake { held, .. } => {
                let held = self.actor(held)?;
                let owners = &mut *self.owners;
                let mut effects = super::world_actor_update::CarryEffects {
                    particles: &mut *owners.particles,
                    sounds: &mut *owners.sounds,
                    residency: owners.residency,
                    executable: owners.executable,
                    process: &*owners.factory_process,
                    phase: u32::from(owners.session.current_phase().id()),
                };
                self.arena
                    .hold_flash(held, &mut effects, self.rng)
                    .map_err(|error| format!("004122A0: {error}"))?;
                Ok(0)
            }
            // 004121D0, the hold's +4: set down before its holder.
            GameCall::ComponentDrop { holder, held, .. } => {
                let (holder, held) = (self.actor(holder)?, self.actor(held)?);
                let owners = &mut *self.owners;
                let mut effects = super::world_actor_update::CarryEffects {
                    particles: &mut *owners.particles,
                    sounds: &mut *owners.sounds,
                    residency: owners.residency,
                    executable: owners.executable,
                    process: &*owners.factory_process,
                    phase: u32::from(owners.session.current_phase().id()),
                };
                self.arena
                    .hold_set_down(holder, held, &*owners.terrain, &mut effects, self.rng)
                    .map_err(|error| format!("004121D0: {error}"))?;
                Ok(0)
            }
            // 00479DD0.
            GameCall::Ground { object } => {
                let actor = self.actor(object)?;
                self.arena
                    .ground_actor(actor, &*self.owners.terrain)
                    .map_err(|error| error.to_string())?;
                Ok(0)
            }
            other => Err(format!("game message call {other:?} is not ported")),
        }
    }
    fn flags(&mut self, object: u32) -> Result<u32, String> {
        let actor = self.actor(object)?;
        Ok(self
            .arena
            .object(actor)
            .map_err(|e| e.to_string())?
            .base
            .flags)
    }
    fn word_70(&mut self, object: u32) -> Result<u32, String> {
        let actor = self.actor(object)?;
        let state = self.arena.actor(actor).map_err(|e| e.to_string())?;
        state
            .capture_handle_70
            .map(ActorHandle::raw)
            .ok_or_else(|| format!("{actor:?}'s +70 is unwritten"))
    }
    fn handle_84(&mut self, object: u32) -> Result<u32, String> {
        let actor = self.actor(object)?;
        Ok(self
            .arena
            .actor(actor)
            .map_err(|e| e.to_string())?
            .handle
            .raw())
    }
}

fn mission_b0_word(
    mission: &mut crate::world_mission::WorldMission,
    index: usize,
) -> Result<&mut u32, String> {
    mission
        .b0_span_word_mut(index)
        .map_err(|error| error.to_string())
}

impl GameHeaderHost for WorldGameHost<'_, '_, '_> {
    type Error = String;
    fn settings_word(&mut self, offset: usize) -> u32 {
        self.settings[offset / 4]
    }
    fn set_settings_word(&mut self, offset: usize, value: u32) {
        self.settings[offset / 4] = value;
    }
    fn mission(&self) -> (i32, u8) {
        let mission = &*self.owners.mission;
        (
            mission.current_world_a0 as i32,
            mission.byte_12.unwrap_or(0),
        )
    }
    /// `0045C506`: in a world H12's first slot is resident, so `004502C0`
    /// reselects the spacing and prepares the terrain, as Alt+F3 does.
    fn rebuild_terrain(&mut self, detail: u32) -> Result<bool, String> {
        crate::terrain_runtime::TerrainDetailContext {
            process: &mut *self.terrain_policy,
            world_flags: Some(self.world_flags),
            terrain: Some(&mut *self.owners.terrain),
        }
        .configure(detail)
        .map_err(|error| format!("004502C0({detail}): {error}"))?;
        Ok(true)
    }
}

impl GameWorldHost for WorldGameHost<'_, '_, '_> {
    fn null_handle(&self) -> u32 {
        self.null.raw()
    }
    fn encode_actor(&mut self, handle: u32, out: &mut [u8]) -> Result<i32, String> {
        let actor = self.actor(handle)?;
        let mut sends = Vec::new();
        let mut host = RecordHost {
            owners: self.owners,
            rng: self.rng,
            sends: &mut sends,
            _arena: std::marker::PhantomData,
        };
        let room = out.len() as i32;
        let resource = self
            .arena
            .object(actor)
            .map(|object| object.base.resource_word)
            .ok();
        self.arena
            .encode_actor_record(
                actor,
                out,
                room,
                self.null,
                self.math,
                self.inverse_sine,
                &mut host,
            )
            .map_err(|error| format!("0047E330 (resource {resource:?}): {error}"))
    }
    fn receive_actor(
        &mut self,
        payload: &[u8],
        sends: &mut Vec<(u32, GameSend)>,
    ) -> Result<i32, String> {
        let (handle, kind, record, _, start) =
            record_head::<String>(payload).map_err(|error| error.to_string())?;
        let Some(actor) = self.resolve(handle)? else {
            // The new actor takes nothing from this record; a refused one
            // names its handle in message 7.
            if !self.build_remote_actor(handle, kind)? {
                sends.push((7, GameSend::Dword(handle)));
            }
            let skipped = skipped_length::<String>(record, &payload[start..])
                .map_err(|error| error.to_string())?;
            return Ok((start + skipped) as i32);
        };
        let mut host = RecordHost {
            owners: self.owners,
            rng: self.rng,
            sends,
            _arena: std::marker::PhantomData,
        };
        let used = self
            .arena
            .apply_actor_record(
                actor,
                payload,
                self.null,
                self.clock,
                self.local_slot,
                self.math,
                &mut host,
            )
            .map_err(|error| format!("0047E600: {error}"))?;
        // A class's sync (the banner's 0045B860) sends from the world.
        sends.extend(self.world_sends());
        Ok(used)
    }
    fn world_values(&mut self) -> Result<[u32; 4], String> {
        let mission = &mut *self.owners.mission;
        let at = mission.current_world_a0 as usize * 4;
        Ok([
            *mission_b0_word(mission, at)?,
            *mission_b0_word(mission, at + 1)?,
            *mission_b0_word(mission, at + 2)?,
            *mission_b0_word(mission, at + 3)?,
        ])
    }
    fn take_world_sends(&mut self) -> Vec<(u32, GameSend)> {
        self.world_sends()
    }
    /// `00475B00`'s bytes.
    fn territory(&mut self) -> Result<Vec<u8>, String> {
        self.arena
            .territory_holders()
            .map_err(|error| error.to_string())
    }
    fn encode_race(&mut self, out: &mut [u8]) -> Result<i32, String> {
        self.arena
            .encode_race(out)
            .map_err(|error| error.to_string())
    }
    fn receive_race(&mut self, payload: &[u8]) -> Result<i32, String> {
        self.arena
            .receive_race(payload)
            .map_err(|error| error.to_string())
    }
    fn receive_ball(&mut self, id: u32, payload: &[u8]) -> Result<(), String> {
        let holder = ActorHandle::from_raw(u32::from_le_bytes(
            payload[..4].try_into().expect("a six-byte payload"),
        ));
        let sequence = u16::from_le_bytes([payload[4], payload[5]]);
        let throw = (id == 44).then(|| {
            std::array::from_fn(|axis| {
                i16::from_le_bytes([payload[6 + 2 * axis], payload[7 + 2 * axis]])
            })
        });
        let sine = self
            .owners
            .behavior_factory
            .as_ref()
            .ok_or("the ball's message needs the world's sine table")?
            .sine;
        let mut host = crate::session_platform::world_actor_update::WorldFamilyUpdateHost {
            owners: self.owners.reborrow(),
            rng: &mut *self.rng,
            sine_quarter: sine,
        };
        self.arena
            .receive_ball_message(holder, sequence, throw, &mut host)
            .map_err(|error| error.to_string())
    }
    fn receive_territory(
        &mut self,
        id: u32,
        payload: &[u8],
        state: &mut GameNetState,
        sends: &mut Vec<(u32, GameSend)>,
    ) -> Result<(), String> {
        let request = crate::object_runtime::TerritoryRequest {
            delta: 0,
            local_slot: self.local_slot,
            host: self.owners.factory_process.host_flag != 0,
            resident: self.owners.residency,
        };
        let arena = &mut *self.arena;
        match id {
            38 => arena
                .receive_territory_holders(&payload[1..])
                .map_err(|error| error.to_string())?,
            39 => arena
                .receive_territory_claim([payload[0], payload[1]], &request, state)
                .map_err(|error| error.to_string())?,
            40 => arena
                .receive_territory_grant([payload[0], payload[1]], self.local_slot)
                .map_err(|error| error.to_string())?,
            _ => arena
                .territory_line(
                    [payload[0], payload[1], payload[2], payload[3]],
                    &request,
                    state,
                )
                .map_err(|error| error.to_string())?,
        }
        // Their 00492500 sends wait for the drain's end like the rest.
        sends.extend(self.world_sends());
        Ok(())
    }
}

impl GameFrameHost for WorldGameHost<'_, '_, '_> {
    fn slot_handles(&mut self, slot: u8) -> Result<Option<Vec<u32>>, String> {
        let registry = self
            .arena
            .actor_handles()
            .ok_or("the world has no actor registry")?;
        Ok(registry.owner(slot).map(|owner| {
            owner
                .slots()
                .iter()
                .skip(1)
                .filter(|entry| entry.object.is_some())
                .map(|entry| entry.handle.raw())
                .collect()
        }))
    }
    fn send_timer(&mut self, handle: u32) -> Result<ActorSendTimer, String> {
        let actor = self.actor(handle)?;
        let held = self
            .arena
            .object(actor)
            .map_err(|e| e.to_string())?
            .base
            .dirty_word
            & 0x8000
            != 0;
        let state = self.arena.actor(actor).map_err(|e| e.to_string())?;
        Ok(ActorSendTimer {
            period_d8: state.word_d8,
            countdown_dc: state.word_dc,
            count_e0: state.word_e0,
            held,
        })
    }
    fn set_send_timer(
        &mut self,
        handle: u32,
        countdown_dc: u32,
        count_e0: u32,
    ) -> Result<(), String> {
        let actor = self.actor(handle)?;
        self.arena
            .set_actor_send_timer(actor, countdown_dc, count_e0)
            .map_err(|error| error.to_string())
    }
    fn deadline(&mut self, handle: u32) -> Result<i32, String> {
        let actor = self.actor(handle)?;
        Ok(self
            .arena
            .object(actor)
            .map_err(|e| e.to_string())?
            .base
            .word_3c as i32)
    }
    /// The shared actor's `0047ECF0`; selector 41's `00401310` does
    /// nothing, and that class is not built.
    fn expire(&mut self, handle: u32) -> Result<bool, String> {
        let actor = self.actor(handle)?;
        self.arena
            .set_actor_deadline(actor, self.clock.wrapping_add(200))
            .map_err(|error| error.to_string())?;
        Ok(true)
    }
    fn race_actor(&mut self) -> bool {
        self.arena.race().is_some()
    }
    fn random(&mut self) -> Result<u32, String> {
        Ok(crate::name_entry::name_entry_rng_next(self.rng))
    }
    fn clock(&self) -> u32 {
        self.clock
    }
    fn identity(&mut self) -> Result<([u8; 16], [u8; 6]), String> {
        Ok(self.identity)
    }
}
