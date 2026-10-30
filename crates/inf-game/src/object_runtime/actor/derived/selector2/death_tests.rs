//! The vehicle's death against Infest02, where the Level 1 vehicle drowned
//! and its wreck took 81 dying updates before it disposed of itself.

use serde_json::Value;

use super::super::test_support::{loaded, DamageServices};
use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorStateChangeEffect, ActorStateChangeHost, ActorStateChangeQuery,
    ActorStateChangeRequest, ActorTerrainInputs, CameraFactoryInputs, CameraSettings,
    CreationListener, CreationListenerEffects, CreationListenerError, CreationListeners,
    HeldCameraHost, ObjectDestructionRequest, ObjectList, PlayerCommandOperation,
    PlayerCommandRequest, PlayerId, SoundTeardownService,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::terrain_runtime::TerrainSpacing;
use inf_formats::FrontendExecutable;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_dying_native.json"
    ))
    .unwrap()
}

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn array<const N: usize>(value: &Value) -> [u32; N] {
    let values = value.as_array().unwrap();
    assert_eq!(values.len(), N);
    std::array::from_fn(|index| number(&values[index]))
}

fn words<const N: usize>(value: &Value) -> [u16; N] {
    array::<N>(value).map(|word| word as u16)
}

/// Replays one tick's boundaries: each host-visible boundary must be the
/// next recorded one, entered at its recorded RNG; the effects' own draws
/// are taken from its recorded return. The deep-water hit runs the port's
/// `0047C7C0` and `004212F0`.
struct Host<'a> {
    rng: u32,
    events: Vec<&'a Value>,
    null: ActorHandle,
    source: ActorHandle,
    fixture_null: u32,
    damage: DamageServices,
    tick: usize,
}

impl<'a> Host<'a> {
    fn new(tick: usize, record: &'a Value, null: ActorHandle, source: ActorHandle) -> Self {
        let events = record["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| {
                matches!(
                    event["name"].as_str().unwrap(),
                    "trail" | "burst" | "radius" | "timed"
                )
            })
            .collect::<Vec<_>>();
        Self {
            rng: number(&record["in"]["rng"]),
            events,
            null,
            source,
            fixture_null: 0,
            damage: DamageServices::new(null),
            tick,
        }
    }

    fn next(&mut self, name: &str) -> &'a Value {
        assert!(
            !self.events.is_empty(),
            "tick {}: unexpected {name}",
            self.tick
        );
        let event = self.events.remove(0);
        assert_eq!(event["name"], name, "tick {}", self.tick);
        let [start, stop] = array::<2>(&event["rng"]);
        assert_eq!(self.rng, start, "tick {}: rng entering {name}", self.tick);
        self.rng = stop;
        event
    }
}

impl VehicleDyingHost for Host<'_> {
    type Error = String;
    fn random(&mut self) -> Result<u32, String> {
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }
    fn status_effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), String> {
        assert!(
            matches!(effect, FamilyUpdateEffect::StatusTrail { .. }),
            "tick {}: {effect:?}",
            self.tick
        );
        self.next("trail");
        Ok(())
    }
    fn wreck_effect(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        effect: TerrainDestructionEffect,
    ) -> Result<(), String> {
        let position = arena.object(vehicle).unwrap().base.position_words.unwrap();
        match effect {
            TerrainDestructionEffect::Burst(burst) => {
                let block = array::<8>(&self.next("burst")["block"]);
                // The block's WORDs: count, X, Y, Z, radius, then an unwritten
                // pad (zero here) and the DWORD types, speed and flags.
                let expected = ParticleBurst {
                    debris_count: block[0] as u16,
                    position: [
                        (block[0] >> 16) as u16,
                        block[1] as u16,
                        (block[1] >> 16) as u16,
                    ],
                    placement_radius: block[2] as u16,
                    central_type: block[3],
                    debris_types: [block[4], block[5]],
                    speed: block[6],
                    flags: block[7],
                };
                assert_eq!(burst, expected, "tick {}", self.tick);
                assert_eq!(burst.position, position, "tick {}", self.tick);
            }
            TerrainDestructionEffect::Radius {
                block_address,
                source,
            } => {
                let event = self.next("radius");
                assert_eq!(block_address, number(&event["block"]));
                assert_eq!(position, words(&event["position"]), "tick {}", self.tick);
                assert_eq!(source, self.source);
                assert_eq!(number(&event["replicate"]), 1);
            }
            other => panic!("tick {}: {other:?}", self.tick),
        }
        Ok(())
    }
    fn timed_damage(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), String> {
        let event = self.next("timed");
        let packet = array::<4>(&event["packet"]);
        assert_eq!(
            request.packet,
            ActorDamagePacket {
                strength: (packet[0] | packet[1] << 8) as i16,
                channel: packet[2] as u8,
                byte_03: Some(packet[3] as u8),
            },
            "tick {}",
            self.tick
        );
        assert_eq!(number(&event["source"]), self.fixture_null);
        assert_eq!(request.source, self.null);
        assert_eq!(request.microseconds as u32, number(&event["delta"]));
        assert_eq!(request.period_microseconds as u32, number(&event["period"]));
        arena
            .apply_actor_timed_damage(vehicle, request, &mut self.damage)
            .map_err(|error| error.to_string())
    }

    fn release_sound(&mut self, _: crate::process_sound::ProcessSoundRef) -> Result<(), String> {
        Ok(())
    }
}

fn set_state(arena: &mut ObjectArena, vehicle: ObjectId, state: &Value, null: ActorHandle) {
    {
        let base = &mut arena.owned_mut(vehicle).unwrap().state.base;
        base.position_words = Some(words(&state["position"]));
        base.flags = number(&state["flags"]);
        base.dirty_word = number(&state["dirty"]) as u16;
        base.word_28 = number(&state["size"]) as u16;
    }
    {
        let actor = arena.actor_mut(vehicle).unwrap();
        actor.rotating.basis.words = array(&state["basis"]);
        actor.words_8e_92 = words(&state["velocity"]);
        actor.words_94_9c = array(&state["angular"]);
        actor.word_c4 = number(&state["c4"]);
        actor.health = number(&state["health"]) as u16;
        actor.maximum_health = Some(number(&state["maximum"]) as u16);
        let status = words::<4>(&state["status"]);
        actor.word_c8 = status[0];
        actor.word_ca = Some(status[1]);
        actor.word_cc = status[2];
        actor.word_ce = status[3];
        actor.word_d8 = number(&state["d8"]);
    }
    let selector = arena.selector2_actor_mut(vehicle).unwrap();
    selector.dwords_200_204 = array(&state["timers_200"]);
    selector.dwords_210_218[2] = number(&state["dword_218"]);
    selector.dword_268 = Some(number(&state["dword_268"]));
    selector.dwords_580_584[0] = number(&state["dword_580"]);
    selector.update.byte_594 = Some(number(&state["byte_594"]) as u8);
    assert_eq!(number(&state["handle_598"]), 0);
    selector.update.handle_598 = Some(null);
    selector.dwords_59c_5a0 = array(&state["timers_59c"]);
    selector.byte_1c0 = number(&state["byte_1c0"]) as u8;
}

fn assert_state(arena: &ObjectArena, vehicle: ObjectId, state: &Value, rng: u32, tick: usize) {
    let base = &arena.object(vehicle).unwrap().base;
    assert_eq!(
        base.position_words,
        Some(words(&state["position"])),
        "tick {tick} position"
    );
    assert_eq!(base.flags, number(&state["flags"]), "tick {tick} flags");
    assert_eq!(
        u32::from(base.dirty_word),
        number(&state["dirty"]),
        "tick {tick} dirty"
    );
    let actor = arena.actor(vehicle).unwrap();
    assert_eq!(
        actor.rotating.basis.words,
        array(&state["basis"]),
        "tick {tick} basis"
    );
    assert_eq!(
        actor.words_8e_92,
        words(&state["velocity"]),
        "tick {tick} velocity"
    );
    assert_eq!(
        actor.words_94_9c,
        array(&state["angular"]),
        "tick {tick} angular"
    );
    assert_eq!(actor.word_c4, number(&state["c4"]), "tick {tick}");
    assert_eq!(
        u32::from(actor.health),
        number(&state["health"]),
        "tick {tick} health"
    );
    let status = words::<4>(&state["status"]);
    assert_eq!(
        [
            actor.word_c8,
            actor.word_ca.unwrap(),
            actor.word_cc,
            actor.word_ce
        ],
        status,
        "tick {tick} status"
    );
    assert_eq!(actor.word_d8, number(&state["d8"]), "tick {tick}");
    let selector = arena.selector2_actor(vehicle).unwrap();
    assert_eq!(
        selector.dwords_200_204,
        array(&state["timers_200"]),
        "tick {tick}"
    );
    assert_eq!(
        selector.dwords_210_218[2],
        number(&state["dword_218"]),
        "tick {tick}"
    );
    assert_eq!(
        selector.dword_268,
        Some(number(&state["dword_268"])),
        "tick {tick} +268"
    );
    assert_eq!(
        selector.dwords_580_584[0],
        number(&state["dword_580"]),
        "tick {tick}"
    );
    assert_eq!(
        selector.update.byte_594,
        Some(number(&state["byte_594"]) as u8),
        "tick {tick}"
    );
    assert_eq!(
        selector.dwords_59c_5a0,
        array(&state["timers_59c"]),
        "tick {tick} +59C"
    );
    assert_eq!(
        u32::from(selector.byte_1c0),
        number(&state["byte_1c0"]),
        "tick {tick}"
    );
    assert_eq!(rng, number(&state["rng"]), "tick {tick} rng");
}

/// Each of the wreck's 81 dying updates, from its recorded entry state:
/// full physics, the status visual's trail, the one-in-sixteen debris
/// burst with its radius effect and random spin, the +268 countdown to the
/// disposal, and the deep-water hit, all at retail's RNG and state.
#[test]
fn the_wreck_dies_as_retail_does() {
    let fixture = fixture();
    let ticks = fixture["ticks"].as_array().unwrap();
    assert_eq!(ticks.len(), 81);
    let (_resident, assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let null = arena.actor_handles().unwrap().null_handle();
    let source = arena.actor(vehicle).unwrap().handle;
    let sine = assets.sine_table().values;
    let plane = (number(&fixture["plane"]) as i32 >> number(&fixture["shift"])) as i16;
    assert_eq!(
        plane,
        (world.terrain.plane_base() >> world.terrain.plane_shift()) as i16
    );
    let mut bursts = 0;
    for (index, record) in ticks.iter().enumerate() {
        set_state(arena, vehicle, &record["in"], null);
        let mut host = Host::new(index, record, null, source);
        host.fixture_null = number(&fixture["null"]);
        let request = VehicleDyingRequest {
            delta: number(&record["delta"]),
            attract: false,
            process_null: null,
            status_load_factor: Some(number(&record["load"]) as i32),
            sine_quarter: &sine,
            water_plane: plane,
            world_flags: world.world_flags,
        };
        arena
            .tick_vehicle_dying(vehicle, &request, &mut host)
            .unwrap();
        assert!(host.events.is_empty(), "tick {index}: {:?}", host.events);
        assert!(host.damage.events.is_empty(), "tick {index}");
        bursts += record["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["name"] == "burst")
            .count();
        assert_state(arena, vehicle, &record["out"], host.rng, index);
        let disposed = record["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["name"] == "dispose");
        assert_eq!(disposed, index == 80, "tick {index}");
    }
    assert_eq!(bursts, 11);
}

/// The services `0047D670` reaches when the vehicle drowns: neither +68
/// nor the attribution is reached for a vehicle whose +68 is null, its
/// +58 is itself, and state one is the vehicle's `00420B90`.
struct Lifecycle<'a> {
    context: PlayerCommandContext<'a>,
    effects: Vec<String>,
}

impl ActorStateChangeHost for Lifecycle<'_> {
    type Error = String;
    type AttributionOwner = ();
    fn query(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, String> {
        Ok(match query {
            ActorStateChangeQuery::ExcludesAttribution68 => false,
            ActorStateChangeQuery::HasDerived58 => true,
        })
    }
    fn attribute(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
    ) -> Result<(), String> {
        self.effects.push("attribute".into());
        Ok(())
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), String> {
        self.effects.push(format!("{effect:?}"));
        if let ActorStateChangeEffect::StateOne { actor, .. } = effect {
            assert_eq!(arena.actor(actor).unwrap().health, 0);
            arena
                .retire_vehicle(actor, &self.context)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn command_owned(arena: &mut ObjectArena, player: PlayerId) -> bool {
    matches!(
        arena.apply_player_command(
            PlayerCommandRequest {
                player,
                operation: PlayerCommandOperation::Query { raw_id: 0x37 },
            },
            &PlayerCommandContext {
                process: None,
                executable: None,
                resident: None,
            },
        ),
        Ok(crate::object_runtime::PlayerCommandOutcome::Owned(true))
    )
}

/// Infest02's death: from lifecycle 3 the lethal hit lowers the vehicle to
/// state 1; its +AC sets the player's command 0x37 with its notice, counts
/// the lost vehicle in +11C, snapshots the weapons into the player and gives
/// the wreck 10 s before the event-10 broadcast.
#[test]
fn the_vehicle_retires_into_a_wreck() {
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let player = arena.first_player().unwrap();
    let null = arena.actor_handles().unwrap().null_handle();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut process = ParticleFactoryProcessState::native_startup();
    process.process_null_handle = Some(null);
    let lost = arena.world_player(player).unwrap().word_11c;
    assert!(!command_owned(arena, player));
    assert_eq!(arena.actor(vehicle).unwrap().word_c4, 3);
    arena.selector2_actor_mut(vehicle).unwrap().dwords_1e8_1f4[0] = 1;
    let mut host = Lifecycle {
        context: PlayerCommandContext {
            process: Some(&process),
            executable: Some(executable),
            resident: Some(&resident),
        },
        effects: Vec::new(),
    };
    arena
        .change_actor_state(
            vehicle,
            ActorStateChangeRequest {
                target_state: 1,
                argument: 0,
                source: null,
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(
        host.effects,
        [
            "attribute".to_string(),
            format!(
                "RecordPlacementDeath {{ index: {} }}",
                arena.actor(vehicle).unwrap().byte_78 as i8
            ),
            format!("StateOne {{ actor: {vehicle:?}, source: {null:?} }}"),
        ]
    );
    assert_eq!(arena.actor(vehicle).unwrap().word_c4, 1);
    assert_eq!(arena.object(vehicle).unwrap().base.flags & 0x8000, 0x8000);
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().dword_268,
        Some(10_000_000)
    );
    assert!(command_owned(arena, player));
    let state = arena.world_player(player).unwrap();
    assert_eq!(state.word_11c, lost + 1);
    assert_eq!(state.dword_144, 1);
}

/// The wreck's destructor `00415B10` disposes every actor its +1A4 slots
/// still hold (`00411DD0`) before the derived teardown; an empty slot's
/// null handle resolves to nothing.
#[test]
fn destroying_the_wreck_disposes_its_hold() {
    let (_resident, _assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let carried = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            actor != vehicle
                && arena.object(actor).unwrap().base.flags & 3 == 3
                && arena.selector16(actor).is_err()
        })
        .unwrap();
    let handle = arena.actor(carried).unwrap().handle;
    arena
        .selector2_actor_mut(vehicle)
        .unwrap()
        .handles_1a4
        .handles[1] = handle;
    arena.dispose(vehicle).unwrap();
    arena
        .destroy(ObjectDestructionRequest {
            object: vehicle,
            sound: SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.object(vehicle).is_err());
    assert_eq!(arena.object(carried).unwrap().base.flags & 3, 0);
    assert!(arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == carried));
}

struct NoListeners;
impl CreationListenerEffects for NoListeners {
    fn invoke(
        &mut self,
        _: CreationListener,
        _: ObjectId,
        _: &mut CreationListeners,
        _: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        panic!("the loaded world's camera already exists")
    }
}

/// Stands in for `0046A890`: the wreck becomes the player's vehicle again
/// at lifecycle 3, as a fresh vehicle would be, and the player's block takes
/// the weapon and form requests (+28, +2C, +34). The real respawn's tests
/// are the actor factory's.
struct Respawn {
    vehicle: ObjectId,
    calls: usize,
}

impl HeldCameraHost for Respawn {
    type Error = String;
    fn respawn(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        _: &mut CameraFactoryInputs<'_>,
    ) -> Result<(), String> {
        // 0046AE90 detached the camera first.
        assert_eq!(arena.first_player_remote_actor().unwrap(), None);
        self.calls += 1;
        arena.actor_mut(self.vehicle).unwrap().word_c4 = 3;
        let handle = arena.actor(self.vehicle).unwrap().handle;
        arena.bind_player_vehicle(player, handle).unwrap();
        let controls = &mut arena.world_player_mut(player).unwrap().controls;
        (controls.dword_28, controls.dword_2c, controls.dword_34) = RESPAWN_REQUESTS;
        Ok(())
    }
}

/// The requests the stand-in respawn writes: weapons 1 and 10, form 2.
const RESPAWN_REQUESTS: (u32, u32, u32) = (3, 12, 2);

fn follows(arena: &ObjectArena, camera: ObjectId) -> ActorHandle {
    let target = arena.selector16(camera).unwrap().target_18c.unwrap();
    arena.actor_follow_target(target).unwrap().handle_04
}

fn camera_factory<'a>(
    process: &'a ParticleFactoryProcessState,
    resident: &'a crate::ResidentRuntime,
    math: &'a inf_formats::native_basis::NativeBasisMath,
    sine: &'a [u16; 4096],
    listeners: &'a mut CreationListeners,
    effects: &'a mut NoListeners,
) -> CameraFactoryInputs<'a> {
    CameraFactoryInputs {
        process: Some(process),
        runtime: resident,
        math,
        allocation: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: sine,
            },
        },
        local_owner: 1,
        settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        listeners,
        listener_effects: effects,
        rng: None,
    }
}

/// Infest02's death camera (`deathcamera` query). After the death the camera
/// holds the player (+D8). Each tick `0046A0E0` re-attaches it and points it
/// at the wreck again, and its +44 `0040F890` clears the controls while +180
/// rises from the 0 its +88 left. With no input, the 41st update, past 5 s,
/// respawns: +D8 returns to the process NULL, the camera stops (`+8E..+92`),
/// follows the vehicle and snaps (+187); +180 ends at 0x4E3388. With Thrust
/// held, the first update past the 2 s wait respawns.
#[test]
fn the_death_camera_holds_the_player_until_the_respawn() {
    for input in [false, true] {
        let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units256);
        let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
        let arena = &mut world.arena;
        let player = arena.first_player().unwrap();
        let null = arena.actor_handles().unwrap().null_handle();
        let mut process = ParticleFactoryProcessState::native_startup();
        process.process_null_handle = Some(null);
        let sine = assets.sine_table().values;
        let math = inf_formats::native_basis::NativeBasisMath::new(sine);
        let mut listeners = CreationListeners::new();
        let mut effects = NoListeners;
        let mut factory = camera_factory(
            &process,
            &resident,
            &math,
            &sine,
            &mut listeners,
            &mut effects,
        );
        let camera = arena.world_camera(&mut factory).unwrap().unwrap();
        let wreck = arena.actor(vehicle).unwrap().handle;
        // The death, then 0040FF66: the camera takes the player.
        arena.actor_mut(vehicle).unwrap().word_c4 = 1;
        let handle = arena.actor(camera).unwrap().handle;
        arena.attach_player(player, handle, &mut factory).unwrap();
        assert_eq!(arena.selector16(camera).unwrap().dword_180, Some(0));
        arena.actor_mut(camera).unwrap().words_8e_92 = [0x33, 0x84, 0xff52];
        let mut host = Respawn { vehicle, calls: 0 };
        let last: u32 = if input { 15 } else { 40 };
        for index in 0..=last {
            assert_eq!(
                arena.player_pre_update(player, null, &mut factory).unwrap(),
                Some(camera),
                "update {index}"
            );
            assert_eq!(follows(arena, camera), wreck);
            assert_eq!(arena.world_player(player).unwrap().handle_d8, handle);
            let mut controls = arena.world_player(player).unwrap().controls;
            controls.thrust_1c = if input { 125_000 } else { 0 };
            assert_eq!(
                arena.selector16(camera).unwrap().dword_180,
                Some(index * 125_000)
            );
            arena
                .tick_held_camera(camera, 125_000, &mut controls, &mut factory, &mut host)
                .unwrap();
            // The respawn writes the block in place after the camera clears
            // it, so its requests survive the caller's store.
            let mut expected = crate::object_runtime::PlayerControlBlock::default();
            if index == last {
                (expected.dword_28, expected.dword_2c, expected.dword_34) = RESPAWN_REQUESTS;
            }
            assert_eq!(controls, expected, "update {index}");
            arena.set_player_controls(player, controls).unwrap();
            assert_eq!(host.calls, usize::from(index == last), "update {index}");
        }
        let state = arena.selector16(camera).unwrap();
        assert_eq!(state.dword_180, Some((last + 1) * 125_000));
        assert_eq!(state.byte_187, 1);
        assert_eq!(arena.actor(camera).unwrap().words_8e_92, [0; 3]);
        assert_eq!(arena.world_player(player).unwrap().handle_d8, null);
        assert_eq!(follows(arena, camera), wreck);
        // The player block takes the vehicle again.
        assert_eq!(
            arena.player_pre_update(player, null, &mut factory).unwrap(),
            Some(vehicle)
        );
    }
}

/// A held camera whose target no longer resolves (a detonated Cranial
/// Implanter's) inside the wait, the first player's vehicle alive (0.5 s):
/// `0040F91F` scans from the camera for the nearest actor with flag 0x20
/// and not 0x80 within 512 and follows it, where the port used to stop the
/// player's update. Past the wait, Fire hands the player back to the
/// vehicle.
#[test]
fn a_held_camera_that_loses_its_target_follows_the_nearest_actor() {
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let player = arena.first_player().unwrap();
    let null = arena.actor_handles().unwrap().null_handle();
    let mut process = ParticleFactoryProcessState::native_startup();
    process.process_null_handle = Some(null);
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut listeners = CreationListeners::new();
    let mut effects = NoListeners;
    let mut factory = camera_factory(
        &process,
        &resident,
        &math,
        &sine,
        &mut listeners,
        &mut effects,
    );
    let camera = arena.world_camera(&mut factory).unwrap().unwrap();
    assert_eq!(arena.actor(vehicle).unwrap().word_c4, 3);
    // The first two the scan admits: the camera stands on the first and
    // follows the second, whose handle then goes.
    let admitted: Vec<_> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            let base = &arena.object(actor).unwrap().base;
            actor != camera && base.flags & 0xa0 == 0x20 && base.position_words.is_some()
        })
        .collect();
    let (near, lost) = (admitted[0], admitted[1]);
    let at = arena.object(near).unwrap().base.position_words;
    arena.owned_mut(camera).unwrap().state.base.position_words = at;
    let handle = arena.actor(camera).unwrap().handle;
    arena.attach_player(player, handle, &mut factory).unwrap();
    let lost_handle = arena.actor(lost).unwrap().handle;
    let target = arena
        .follow_target_resource(crate::object_runtime::FollowTargetRequest::resolve_actor(
            lost_handle,
        ))
        .unwrap();
    arena
        .replace_camera_target(camera, target, factory.settings)
        .unwrap();
    assert!(arena.actor_handles.as_mut().unwrap().release(lost_handle));
    let mut host = Respawn { vehicle, calls: 0 };
    let mut controls = arena.world_player(player).unwrap().controls;
    controls.fire_20 = 1;
    arena
        .tick_held_camera(camera, 125_000, &mut controls, &mut factory, &mut host)
        .unwrap();
    assert_eq!(controls, Default::default());
    assert_eq!(follows(arena, camera), arena.actor(near).unwrap().handle);
    assert_eq!(arena.world_player(player).unwrap().handle_d8, handle);
    // 0.625 s held: Fire hands the player back, and the camera follows the
    // vehicle.
    controls.fire_20 = 1;
    arena
        .tick_held_camera(camera, 500_000, &mut controls, &mut factory, &mut host)
        .unwrap();
    assert_eq!(arena.world_player(player).unwrap().handle_d8, null);
    assert_eq!(follows(arena, camera), arena.actor(vehicle).unwrap().handle);
    assert_eq!(host.calls, 0);
}

/// Stands in for the respawn host at the world's loss limit
/// (`0046A8C6..0046A8D7`): refused, the mission's doom countdown starts at
/// 1 µs and nothing of the player changes.
struct LossLimitRespawn<'m> {
    mission: &'m mut crate::world_mission::WorldMission,
    limit: u32,
    calls: &'m mut usize,
}

impl HeldCameraHost for LossLimitRespawn<'_> {
    type Error = String;
    fn respawn(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        _: &mut CameraFactoryInputs<'_>,
    ) -> Result<(), String> {
        *self.calls += 1;
        assert!(arena
            .respawn_loss_limit_reached(player, self.limit)
            .unwrap());
        self.mission.start_doom_countdown(1);
        Ok(())
    }
}

/// The last life. With no input the camera calls the respawn on the 41st
/// update, past 5 s; at the limit it is refused, and the camera's hold
/// branch clears the player's +D8 and +D4. From then on `0046A0E0` resolves
/// neither and returns null, so the player block reaches no actor and
/// nothing calls the respawn again. The mission tick (`0048E2E0`, before
/// the player block in `004875A0`) finds no unit on the 42nd update and takes
/// the 3 s stage; 24 updates later it ends the world into itself with +15 set
/// ("Mission Failed").
#[test]
fn the_last_life_ends_the_world_three_seconds_after_the_refused_respawn() {
    use crate::world_mission::MissionCountdownTick;
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let current = world.mission.current_world_a0;
    let player = world.arena.first_player().unwrap();
    let null = world.arena.actor_handles().unwrap().null_handle();
    let mut process = ParticleFactoryProcessState::native_startup();
    process.process_null_handle = Some(null);
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut listeners = CreationListeners::new();
    let mut effects = NoListeners;
    let mut factory = camera_factory(
        &process,
        &resident,
        &math,
        &sine,
        &mut listeners,
        &mut effects,
    );
    let camera = world.arena.world_camera(&mut factory).unwrap().unwrap();
    // The third loss of three: the vehicle dies and the camera takes the
    // player.
    world.arena.actor_mut(vehicle).unwrap().word_c4 = 1;
    world.arena.world_player_mut(player).unwrap().word_11c = 3;
    let handle = world.arena.actor(camera).unwrap().handle;
    world
        .arena
        .attach_player(player, handle, &mut factory)
        .unwrap();
    let mut calls = 0;
    let mut ticks = Vec::new();
    for index in 0..=65 {
        let unit = world.arena.player_actor(player).unwrap().is_some();
        let tick = world.mission.tick_countdown(125_000, unit).unwrap();
        ticks.push(tick);
        if let MissionCountdownTick::MissionFailed { .. } = tick {
            assert_eq!(index, 65);
            break;
        }
        let actor = world
            .arena
            .player_pre_update(player, null, &mut factory)
            .unwrap();
        if index > 40 {
            assert_eq!(actor, None, "update {index}");
            continue;
        }
        assert_eq!(actor, Some(camera), "update {index}");
        let mut controls = world.arena.world_player(player).unwrap().controls;
        let mut host = LossLimitRespawn {
            mission: &mut world.mission,
            limit: 3,
            calls: &mut calls,
        };
        world
            .arena
            .tick_held_camera(camera, 125_000, &mut controls, &mut factory, &mut host)
            .unwrap();
        world.arena.set_player_controls(player, controls).unwrap();
        assert_eq!(calls, usize::from(index == 40), "update {index}");
    }
    assert_eq!(calls, 1);
    assert_eq!(ticks[40], MissionCountdownTick::Idle);
    assert_eq!(ticks[41], MissionCountdownTick::LastSeconds);
    assert!(ticks[42..65]
        .iter()
        .all(|tick| *tick == MissionCountdownTick::Counted));
    assert_eq!(
        ticks[65],
        MissionCountdownTick::MissionFailed { world: current }
    );
    assert_eq!(world.mission.bytes_14_16.unwrap()[1], 1);
    assert_eq!(
        (world.mission.dword_24, world.mission.byte_18),
        (Some(0), Some(0))
    );
    // Nothing of the player changed: the losses stay at the limit.
    assert_eq!(world.arena.world_player(player).unwrap().word_11c, 3);
}

/// `0046A0E0` while the player holds the camera over a vehicle at
/// lifecycle 3: nothing changes until a mode request (+34), which detaches
/// the camera, points it at the vehicle and lapses. The camera is still
/// this tick's actor.
#[test]
fn a_mode_request_returns_the_player_to_a_live_vehicle() {
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let player = arena.first_player().unwrap();
    let null = arena.actor_handles().unwrap().null_handle();
    let mut process = ParticleFactoryProcessState::native_startup();
    process.process_null_handle = Some(null);
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut listeners = CreationListeners::new();
    let mut effects = NoListeners;
    let mut factory = camera_factory(
        &process,
        &resident,
        &math,
        &sine,
        &mut listeners,
        &mut effects,
    );
    let camera = arena.world_camera(&mut factory).unwrap().unwrap();
    let handle = arena.actor(camera).unwrap().handle;
    arena.attach_player(player, handle, &mut factory).unwrap();
    // The load's respawn left its mode request, which the first vehicle
    // update would consume.
    arena.world_player_mut(player).unwrap().controls.dword_34 = 0;
    let target = arena.selector16(camera).unwrap().target_18c;
    assert_eq!(
        arena.player_pre_update(player, null, &mut factory).unwrap(),
        Some(camera)
    );
    assert_eq!(arena.world_player(player).unwrap().handle_d8, handle);
    assert_eq!(arena.selector16(camera).unwrap().target_18c, target);
    arena.world_player_mut(player).unwrap().controls.dword_34 = 2;
    assert_eq!(
        arena.player_pre_update(player, null, &mut factory).unwrap(),
        Some(camera)
    );
    let state = arena.world_player(player).unwrap();
    assert_eq!((state.handle_d8, state.controls.dword_34), (null, 0));
    assert_eq!(follows(arena, camera), arena.actor(vehicle).unwrap().handle);
    assert_eq!(
        arena.player_pre_update(player, null, &mut factory).unwrap(),
        Some(vehicle)
    );
}

/// Infest02's visit to the save pad (`savespawn` query): 0046AF80 saves the
/// vehicle's position (4ECA, D7, 81A6), its buggy mode 1 and the angles
/// 3FB049DDh of its basis at clock 6C6h, then refuses another save until
/// 50 clock ticks have passed.
#[test]
fn the_pad_saves_the_vehicles_spawn_as_retail_does() {
    let (_resident, assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let player = arena.first_player().unwrap();
    let math = inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        executable.data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap()
    .values;
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some([0x4eca, 0xd7, 0x81a6]);
    arena.actor_mut(vehicle).unwrap().rotating.basis.words = [
        0xc, 0xbf7a659f, 0xbce2fe82, 0xbe52d4a6, 0xbb59b702, 0x3f7e3113, 0xbdf18ce1, 0x3e54cc92,
        0xbdeaf714, 0xbf78a822,
    ];
    arena.selector2_actor_mut(vehicle).unwrap().dwords_1e8_1f4[0] = 1;
    arena
        .save_player_spawn(player, 0x6c6, &math, &inverse)
        .unwrap();
    let state = arena.world_player(player).unwrap();
    assert_eq!(
        (
            state.byte_ed,
            state.saved_e0,
            state.dword_e8,
            state.byte_ec,
            state.dword_f0
        ),
        (1, Some([0x4eca, 0xd7, 0x81a6]), 0x3fb049dd, Some(1), 0x6c6)
    );
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some([0, 0, 0]);
    arena
        .save_player_spawn(player, 0x6c6 + 49, &math, &inverse)
        .unwrap();
    assert_eq!(
        arena.world_player(player).unwrap().saved_e0,
        Some([0x4eca, 0xd7, 0x81a6])
    );
    arena
        .save_player_spawn(player, 0x6c6 + 50, &math, &inverse)
        .unwrap();
    assert_eq!(
        arena.world_player(player).unwrap().saved_e0,
        Some([0, 0, 0])
    );
}
