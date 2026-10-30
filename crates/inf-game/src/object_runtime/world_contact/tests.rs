use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainSurface, DerivedActorConstructionRequest,
};
use crate::terrain_runtime::TerrainRuntime;
use serde_json::{json, Value};

const SINE: [u16; 4096] = [0; 4096];

fn ship() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let actor = arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([10, 20, 30]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    (arena, actor)
}

struct Host<'a> {
    terrain: Option<&'a TerrainRuntime>,
    contact: Option<CollisionContact>,
    gate: NetworkImpactGate,
    damage: Vec<(ObjectId, ActorDamageRequest)>,
    terrain_calls: usize,
    furniture_calls: usize,
    process_null: Option<ActorHandle>,
    destruction: Vec<TerrainDestructionEffect>,
}

impl<'a> Host<'a> {
    fn new(terrain: Option<&'a TerrainRuntime>) -> Self {
        Self {
            terrain,
            contact: None,
            gate: NetworkImpactGate::Offline,
            damage: Vec::new(),
            terrain_calls: 0,
            furniture_calls: 0,
            process_null: None,
            destruction: Vec::new(),
        }
    }
}

impl WorldContactHost for Host<'_> {
    type Error = &'static str;
    fn terrain_contact(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
    ) -> Result<Option<CollisionContact>, Self::Error> {
        self.terrain_calls += 1;
        Ok(self.contact)
    }
    fn water_inputs(&self) -> ActorTerrainInputs<'_> {
        ActorTerrainInputs {
            terrain: self.terrain.map(|terrain| ActorTerrainSurface {
                terrain,
                plane_height_fixed: terrain.plane_base(),
            }),
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: &SINE,
        }
    }
    fn network_impact_gate(&self) -> NetworkImpactGate {
        self.gate
    }
    fn impact_damage(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error> {
        self.damage.push((actor, request));
        Ok(())
    }
    fn furniture_contact(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        self.furniture_calls += 1;
        Ok(())
    }
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.process_null
    }
    fn destruction_effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: TerrainDestructionEffect,
    ) -> Result<(), Self::Error> {
        self.destruction.push(effect);
        Ok(())
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        Err("unreached RNG")
    }
    fn boulder_particle(
        &mut self,
        _: BoulderParticle,
        _: [u16; 3],
        _: [u16; 3],
    ) -> Result<(), Self::Error> {
        Err("unreached boulder particle")
    }
}

fn words<const N: usize>(value: &Value) -> [u32; N] {
    std::array::from_fn(|i| value[i].as_u64().unwrap() as u32)
}

fn set_actor(arena: &mut ObjectArena, actor: ObjectId, flags: u32, position: [u16; 3]) {
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.flags = flags;
    base.position_words = Some(position);
}

#[test]
fn terrain_push_matches_native_0047d2f0_position_rebound_and_impact_packet() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/terrain_contact_response_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["provenance"]["fpcw"].as_u64(), Some(0x027f));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 162);
    let mut damaged = 0;
    for (index, case) in cases.iter().enumerate() {
        let (mut arena, actor) = ship();
        let flags = case["flags"].as_u64().unwrap() as u32;
        set_actor(
            &mut arena,
            actor,
            flags,
            words::<3>(&case["position"]).map(|w| w as u16),
        );
        let state = arena.actor_mut(actor).unwrap();
        state.word_8c = case["mass"].as_u64().unwrap() as u16;
        state.words_8e_92 = words::<3>(&case["velocity"]).map(|w| w as u16);
        let [x, y, z] = words::<3>(&case["normal"]);
        let displacement = case["displacement"].as_u64().unwrap() as u32;
        let contact = CollisionContact::from_native_words([
            Some(1),
            Some(x),
            Some(y),
            Some(z),
            Some(displacement),
        ]);
        let mut host = Host::new(None);
        if case["network"].as_u64() == Some(1) {
            host.gate = NetworkImpactGate::Session(case["session_34"].as_u64().unwrap() as u32);
        }
        arena
            .respond_terrain_push(actor, contact, &mut host)
            .unwrap();
        let base = &arena.object(actor).unwrap().base;
        assert_eq!(
            base.position_words.unwrap().map(u32::from),
            words::<3>(&case["final_position"]),
            "native case {index} position"
        );
        assert_eq!(base.flags, flags, "native case {index} flags");
        assert_eq!(
            arena.actor(actor).unwrap().words_8e_92.map(u32::from),
            words::<3>(&case["final_velocity"]),
            "native case {index} velocity"
        );
        let native = case["damage"].as_array().unwrap();
        assert_eq!(
            host.damage.len(),
            native.len(),
            "native case {index} damage"
        );
        if let Some((target, request)) = host.damage.first() {
            damaged += 1;
            let packet = request.packet;
            let dword = u32::from(packet.strength as u16)
                | u32::from(packet.channel) << 16
                | u32::from(packet.byte_03.unwrap()) << 24;
            assert_eq!(
                dword,
                native[0]["packet"].as_u64().unwrap() as u32,
                "native case {index} packet"
            );
            assert_eq!(*target, actor);
            assert_eq!(request.source, arena.actor(actor).unwrap().handle);
            assert_eq!(native[0]["source"], case["handle"]);
        }
    }
    assert_eq!(damaged, 52);
}

fn water_terrain(plane: i32) -> TerrainRuntime {
    crate::terrain_runtime::native_probe_fixture_terrain(&json!({
        "spacing": 128,
        "height_shift": 5,
        "plane_base": plane << 8,
        "plane_shift": 8,
        "xz": [0, 0],
        "heights": [0, 0, 0, 0],
        "alternate": false,
    }))
}

/// The vehicle's +94 (00520860) is 0047D2F0 itself, as for selectors
/// 10/12/36 (Infest01 reaches it 4 times): pushed up 5, falling at 400 it
/// rebounds by 600 and, closing faster than 0x96, takes the self-sourced
/// impact packet, (600 * mass 100) / 192 on channel 1.
#[test]
fn the_vehicle_responds_to_terrain_through_0047d2f0() {
    let terrain = water_terrain(-1000);
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let vehicle = arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([100, 200, 300]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: Some(ActorTerrainSurface {
                            terrain: &terrain,
                            plane_height_fixed: 0,
                        }),
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: Some(0),
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [0, 0],
            },
            &mut 0,
        )
        .unwrap();
    set_actor(&mut arena, vehicle, 0, [100, 200, 300]);
    let state = arena.actor_mut(vehicle).unwrap();
    state.word_8c = 100;
    state.words_8e_92 = [0, (-400i16) as u16, 0];
    let contact = CollisionContact::from_native_words([
        Some(1),
        Some(0.0f32.to_bits()),
        Some(1.0f32.to_bits()),
        Some(0.0f32.to_bits()),
        Some(5),
    ]);
    let mut host = Host::new(None);
    arena
        .respond_world_terrain(vehicle, contact, &mut host)
        .unwrap();
    let base = &arena.object(vehicle).unwrap().base;
    assert_eq!(base.position_words, Some([100, 205, 300]));
    assert_eq!(arena.actor(vehicle).unwrap().words_8e_92, [0, 200, 0]);
    let [(target, request)] = host.damage[..] else {
        panic!("one impact");
    };
    assert_eq!(target, vehicle);
    assert_eq!(
        request.packet,
        ActorDamagePacket {
            strength: 312,
            channel: 1,
            byte_03: Some(0),
        }
    );
    assert_eq!(request.source, arena.actor(vehicle).unwrap().handle);
}

#[derive(serde::Deserialize)]
struct BallContacts {
    contacts: Vec<BallContact>,
}

#[derive(serde::Deserialize)]
struct BallContact {
    recording: String,
    at: String,
    /// The five DWORDs 0047D2F0 reads.
    block: String,
    /// The 0x138-byte Powerball.
    before: String,
    after: String,
}

/// The Powerball's +94 (00526464) is 0047D2F0 itself. World 11's first
/// three contacts in Infest03 and Infest04 (`powerball.js`) push it out
/// along the ground's normal (25, then 8 up a slope, then 13) and send it
/// back up at 3/2 of its closing speed, as both recordings do.
#[test]
fn the_powerball_responds_to_terrain_through_0047d2f0() {
    let receipts: BallContacts = serde_json::from_str(include_str!(
        "../../../tests/fixtures/powerball_infest.json"
    ))
    .unwrap();
    assert_eq!(receipts.contacts.len(), 6);
    let bytes = |hex: &str| -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    };
    let word = |bytes: &[u8], at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let motion = |bytes: &[u8]| {
        (
            [word(bytes, 0x32), word(bytes, 0x34), word(bytes, 0x36)],
            [word(bytes, 0x8e), word(bytes, 0x90), word(bytes, 0x92)],
        )
    };
    for contact in &receipts.contacts {
        let block = bytes(&contact.block);
        let words = std::array::from_fn(|index| {
            Some(u32::from_le_bytes(
                block[index * 4..index * 4 + 4].try_into().unwrap(),
            ))
        });
        let before = bytes(&contact.before);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x1234),
        })
        .unwrap();
        let ball = arena
            .allocate_selector57(ActorConstructionRequest {
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: None,
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &SINE,
                },
            })
            .unwrap();
        let (position, velocity) = motion(&before);
        let flags = u32::from_le_bytes(before[0x2c..0x30].try_into().unwrap());
        set_actor(&mut arena, ball, flags, position);
        let state = arena.actor_mut(ball).unwrap();
        state.word_8c = word(&before, 0x8c);
        state.words_8e_92 = velocity;
        let mut host = Host::new(None);
        arena
            .respond_world_terrain(ball, CollisionContact::from_native_words(words), &mut host)
            .unwrap();
        assert_eq!(
            (
                arena.object(ball).unwrap().base.position_words.unwrap(),
                arena.actor(ball).unwrap().words_8e_92
            ),
            motion(&bytes(&contact.after)),
            "{} {}",
            contact.recording,
            contact.at
        );
        assert!(host.damage.is_empty());
    }
}

#[test]
fn pass_merges_pre_pass_flags_and_replaces_only_the_water_bits() {
    let terrain = water_terrain(100);
    // No contact: 0x80000 is cleared, then 0047EBB0 ORs the pre-pass copy
    // back; the old 0x20000000 is replaced by the above-water bit.
    let (mut arena, actor) = ship();
    set_actor(&mut arena, actor, 0x2008_0100, [10, 300, 30]);
    arena.actor_mut(actor).unwrap().rotating.visual.radius_word = 20;
    let mut host = Host::new(Some(&terrain));
    let phases = WorldContactPhases {
        terrain: true,
        furniture: true,
    };
    arena.run_world_contact(actor, phases, &mut host).unwrap();
    assert_eq!(arena.object(actor).unwrap().base.flags, 0x4008_0100);
    assert_eq!((host.terrain_calls, host.furniture_calls), (1, 0));

    // A contact sets 0x80000 after the push; deep water selects 0x20000000.
    let (mut arena, actor) = ship();
    set_actor(&mut arena, actor, 0x4000_0104, [10, (-1000_i16) as u16, 30]);
    let mut host = Host::new(Some(&terrain));
    host.contact = Some(CollisionContact::from_native_words([
        Some(1),
        Some(0f32.to_bits()),
        Some(1f32.to_bits()),
        Some(0f32.to_bits()),
        Some(5),
    ]));
    arena.run_world_contact(actor, phases, &mut host).unwrap();
    let base = &arena.object(actor).unwrap().base;
    assert_eq!(base.flags, 0x2008_0104);
    assert_eq!(base.position_words, Some([10, (-995_i16) as u16, 30]));
    assert_eq!((host.terrain_calls, host.furniture_calls), (1, 1));
}

#[test]
fn pass_gates_on_the_pre_pass_flags_and_admitted_phases() {
    let terrain = water_terrain(100);
    let all = WorldContactPhases {
        terrain: true,
        furniture: true,
    };
    for (flags, phases, calls) in [
        (0x0000_010c, all, (0, 0)),
        (0x0400_0104, all, (0, 0)),
        (0x0000_0104, all, (1, 1)),
        (
            0x0000_0104,
            WorldContactPhases {
                furniture: false,
                ..all
            },
            (1, 0),
        ),
        (
            0x0000_0104,
            WorldContactPhases {
                terrain: false,
                ..all
            },
            (0, 1),
        ),
        (0x0000_0004, all, (0, 1)),
    ] {
        let (mut arena, actor) = ship();
        set_actor(&mut arena, actor, flags, [10, 300, 30]);
        let mut host = Host::new(Some(&terrain));
        arena.run_world_contact(actor, phases, &mut host).unwrap();
        assert_eq!(
            (host.terrain_calls, host.furniture_calls),
            calls,
            "flags {flags:08X}"
        );
    }
}

#[derive(Debug, PartialEq)]
enum FurnitureCall {
    Damage(ActorDamageRequest),
    Furniture([u16; 2], ActorDamagePacket),
}

struct FurnitureHost {
    gate: NetworkImpactGate,
    null: ActorHandle,
    calls: Vec<FurnitureCall>,
}

impl FurnitureResponseHost for FurnitureHost {
    type Error = &'static str;
    fn network_impact_gate(&self) -> NetworkImpactGate {
        self.gate
    }
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(self.null)
    }
    fn actor_damage(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error> {
        self.calls.push(FurnitureCall::Damage(request));
        Ok(())
    }
    fn furniture_damage(
        &mut self,
        xz: [u16; 2],
        packet: ActorDamagePacket,
    ) -> Result<(), Self::Error> {
        self.calls.push(FurnitureCall::Furniture(xz, packet));
        Ok(())
    }
}

fn packet_low24(packet: ActorDamagePacket) -> u32 {
    u32::from(packet.strength as u16) | u32::from(packet.channel) << 16
}

#[test]
fn furniture_response_matches_native_0047bfd0_rebound_kinds_and_damage() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/furniture_contact_response_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["provenance"]["fpcw"].as_u64(), Some(0x027f));
    // Packet byte 3 is the top byte of the H11 entry pointer; the port keeps it unknown.
    let pointer_byte = (fixture["provenance"]["entry_pointer"].as_u64().unwrap() >> 24) as u32;
    let null = fixture["provenance"]["process_null"].as_u64().unwrap() as u32;
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 180);
    let mut damaged = 0;
    for (index, case) in cases.iter().enumerate() {
        let (mut arena, actor) = ship();
        let flags = case["flags"].as_u64().unwrap() as u32;
        set_actor(
            &mut arena,
            actor,
            flags,
            words::<3>(&case["position"]).map(|w| w as u16),
        );
        let state = arena.actor_mut(actor).unwrap();
        state.word_8c = case["mass"].as_u64().unwrap() as u16;
        state.words_8e_92 = words::<3>(&case["velocity"]).map(|w| w as u16);
        let [x, y, z] = words::<3>(&case["normal"]);
        let displacement = case["displacement"].as_u64().unwrap() as u32;
        let cell = words::<3>(&case["cell"]);
        let mut host = FurnitureHost {
            gate: if case["network"].as_u64() == Some(1) {
                NetworkImpactGate::Session(case["session_34"].as_u64().unwrap() as u32)
            } else {
                NetworkImpactGate::Offline
            },
            null: ActorHandle::from_raw(null),
            calls: Vec::new(),
        };
        let al = arena
            .respond_furniture_contact(
                actor,
                FurnitureContactResponse {
                    contact: CollisionContact::from_native_words([
                        Some(1),
                        Some(x),
                        Some(y),
                        Some(z),
                        Some(displacement),
                    ]),
                    kind: case["kind"].as_u64().unwrap() as u8,
                    cell_xz: [cell[0] as u16, cell[2] as u16],
                },
                &mut host,
            )
            .unwrap();
        assert_eq!(u64::from(al), case["al"].as_u64().unwrap());
        let base = &arena.object(actor).unwrap().base;
        assert_eq!(
            base.position_words.unwrap().map(u32::from),
            words::<3>(&case["final_position"]),
            "native case {index} position"
        );
        assert_eq!(base.flags, flags);
        assert_eq!(
            arena.actor(actor).unwrap().words_8e_92.map(u32::from),
            words::<3>(&case["final_velocity"]),
            "native case {index} velocity"
        );
        let native = case["calls"].as_array().unwrap();
        assert_eq!(host.calls.len(), native.len(), "native case {index} calls");
        for (port, native) in host.calls.iter().zip(native) {
            let dword = native["packet"].as_u64().unwrap() as u32;
            assert_eq!(dword >> 24, pointer_byte);
            match port {
                FurnitureCall::Damage(request) => {
                    assert_eq!(native["call"], "0047C6E0");
                    assert_eq!(
                        packet_low24(request.packet),
                        dword & 0xff_ffff,
                        "case {index}"
                    );
                    assert_eq!(request.packet.byte_03, None);
                    let expected = if case["kind"].as_u64() == Some(0x0e) {
                        ActorHandle::from_raw(null)
                    } else {
                        assert_eq!(native["source"], case["handle"]);
                        arena.actor(actor).unwrap().handle
                    };
                    assert_eq!(request.source, expected);
                    damaged += 1;
                }
                FurnitureCall::Furniture(xz, packet) => {
                    assert_eq!(native["call"], "00436020");
                    assert_eq!(packet_low24(*packet), dword & 0xff_ffff, "case {index}");
                    assert_eq!(xz.map(u32::from), words::<2>(&native["xz"]));
                }
            }
        }
    }
    assert_eq!(damaged, 72);
}

#[test]
fn furniture_response_reads_the_network_gate_only_below_minus_50() {
    // dot -43 decrements to -44; 9/8 scaling gives exactly -50 (no read),
    // dot -44 gives -51 (read). The fixed kind never reaches the gate.
    for (velocity_y, reads) in [(-43_i16, false), (-44, true)] {
        let (mut arena, actor) = ship();
        set_actor(&mut arena, actor, 0x104, [0, 0, 0]);
        arena.actor_mut(actor).unwrap().words_8e_92 = [0, velocity_y as u16, 0];
        let mut host = FurnitureHost {
            gate: NetworkImpactGate::Unowned,
            null: ActorHandle::from_raw(1),
            calls: Vec::new(),
        };
        let result = arena.respond_furniture_contact(
            actor,
            FurnitureContactResponse {
                contact: CollisionContact::from_native_words([
                    Some(1),
                    Some(0),
                    Some(1f32.to_bits()),
                    Some(0),
                    Some(0),
                ]),
                kind: 1,
                cell_xz: [0, 0],
            },
            &mut host,
        );
        assert_eq!(
            matches!(
                result,
                Err(FurnitureResponseError::RequiredNetworkGate { .. })
            ),
            reads,
            "velocity {velocity_y}"
        );
    }
}

#[derive(Debug, PartialEq)]
enum PairCall {
    Accept(ObjectId, ObjectId),
    React {
        actor: ObjectId,
        other: ObjectId,
        own_saved: [u16; 3],
        other_saved: [u16; 3],
        own_velocity: [u16; 3],
        own_position: [u16; 3],
    },
    Effect([u16; 3], i32),
    Damage(ObjectId, u32, ActorHandle),
}

struct PairHost {
    accept: [bool; 2],
    first: ObjectId,
    calls: Vec<PairCall>,
}

impl ActorPairHost for PairHost {
    type Error = &'static str;
    fn accept_pair(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        other: ObjectId,
    ) -> Result<bool, Self::Error> {
        self.calls.push(PairCall::Accept(actor, other));
        Ok(self.accept[usize::from(actor != self.first)])
    }
    fn react_pair(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        other: ObjectId,
        own_saved: [u16; 3],
        other_saved: [u16; 3],
    ) -> Result<(), Self::Error> {
        self.calls.push(PairCall::React {
            actor,
            other,
            own_saved,
            other_saved,
            own_velocity: arena.actor(actor).unwrap().words_8e_92,
            own_position: arena.object(actor).unwrap().base.position_words.unwrap(),
        });
        Ok(())
    }
    fn impact_effect(&mut self, position: [u16; 3], strength: i32) -> Result<(), Self::Error> {
        self.calls.push(PairCall::Effect(position, strength));
        Ok(())
    }
    fn actor_damage(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error> {
        assert_eq!(request.packet.byte_03, None);
        self.calls.push(PairCall::Damage(
            actor,
            packet_low24(request.packet),
            request.source,
        ));
        Ok(())
    }
}

fn pair_actors() -> (ObjectArena, ObjectId, ObjectId) {
    let (mut arena, a) = ship();
    let b = arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([1, 2, 3]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    (arena, a, b)
}

fn set_pair_actor(arena: &mut ObjectArena, actor: ObjectId, spec: &Value) {
    set_actor(
        arena,
        actor,
        spec["flags"].as_u64().unwrap() as u32,
        words::<3>(&spec["position"]).map(|w| w as u16),
    );
    let state = arena.actor_mut(actor).unwrap();
    state.word_8c = spec["mass"].as_u64().unwrap() as u16;
    state.word_c4 = spec["state"].as_u64().unwrap() as u32;
    state.words_8e_92 = words::<3>(&spec["velocity"]).map(|w| w as u16);
    state.rotating.visual.radius_word = spec["radius"].as_u64().unwrap() as u16;
}

#[test]
fn actor_pair_response_matches_native_0047b610_arms_effect_and_damage() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/actor_pair_response_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["provenance"]["fpcw"].as_u64(), Some(0x027f));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 262);
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        let native = case["calls"].as_array().unwrap();
        // Model and query gates belong to the host; the kernel starts after them.
        let Some(query) = native.first().filter(|call| call["call"] == "004F2BE0") else {
            assert_eq!(case["model_words"][0].as_u64(), Some(0), "case {index}");
            continue;
        };
        // 004F2BE0 measures the zero B origin from the A origin, the delta.
        let delta: Vec<i64> = (0..3)
            .map(|axis| {
                i64::from(
                    (case["a"]["position"][axis].as_u64().unwrap() as u16)
                        .wrapping_sub(case["b"]["position"][axis].as_u64().unwrap() as u16)
                        as i16,
                )
            })
            .collect();
        let origin: Vec<i64> = query["a_origin"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        assert_eq!(origin, delta, "case {index}");
        if case["contact_flag"].as_u64() == Some(0) {
            assert_eq!(native.len(), 1, "case {index}");
            continue;
        }
        let (mut arena, a, b) = pair_actors();
        set_pair_actor(&mut arena, a, &case["a"]);
        set_pair_actor(&mut arena, b, &case["b"]);
        let [x, y, z] = words::<3>(&case["normal"]);
        let contact = CollisionContact::from_native_words([
            Some(1),
            Some(x),
            Some(y),
            Some(z),
            Some(case["displacement"].as_u64().unwrap() as u32),
        ]);
        let mut host = PairHost {
            accept: [case["a0"][0] == 1, case["a0"][1] == 1],
            first: a,
            calls: Vec::new(),
        };
        let al = arena.respond_actor_pair(a, b, contact, &mut host).unwrap();
        assert_eq!(u64::from(al), case["al"].as_u64().unwrap(), "case {index}");
        let id = |name: &Value| if name == "a" { a } else { b };
        assert_eq!(host.calls.len(), native.len() - 1, "case {index} calls");
        for (port, native) in host.calls.iter().zip(&native[1..]) {
            let expected = match native["call"].as_str().unwrap() {
                "A0" => PairCall::Accept(id(&native["actor"]), id(&native["other"])),
                "A4" => PairCall::React {
                    actor: id(&native["actor"]),
                    other: id(&native["other"]),
                    own_saved: words::<3>(&native["own_saved"]).map(|w| w as u16),
                    other_saved: words::<3>(&native["other_saved"]).map(|w| w as u16),
                    own_velocity: words::<3>(&native["own_velocity"]).map(|w| w as u16),
                    own_position: words::<3>(&native["own_position"]).map(|w| w as u16),
                },
                "0048AB10" => {
                    assert_eq!(native["ecx"].as_u64(), Some(0));
                    PairCall::Effect(
                        words::<3>(&native["position"]).map(|w| w as u16),
                        native["strength"].as_i64().unwrap() as i32,
                    )
                }
                "0047C6E0" => {
                    let target = id(&native["target"]);
                    let source = if target == a { b } else { a };
                    let source_name = if target == a { "b" } else { "a" };
                    assert_eq!(native["source"], case[source_name]["handle"]);
                    PairCall::Damage(
                        target,
                        native["packet"].as_u64().unwrap() as u32 & 0xff_ffff,
                        arena.actor(source).unwrap().handle,
                    )
                }
                other => panic!("unexpected native call {other}"),
            };
            assert_eq!(*port, expected, "case {index}");
        }
        for (actor, name) in [(a, "final_a"), (b, "final_b")] {
            let object = arena.object(actor).unwrap();
            assert_eq!(
                object.base.position_words.unwrap().map(u32::from),
                words::<3>(&case[name]["position"]),
                "case {index} {name} position"
            );
            assert_eq!(
                arena.actor(actor).unwrap().words_8e_92.map(u32::from),
                words::<3>(&case[name]["velocity"]),
                "case {index} {name} velocity"
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 238);
}

#[derive(Clone, Copy)]
enum CrashClass {
    Selector34,
    Selector35,
}

fn crash_ship(class: CrashClass, position: [u16; 3]) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let request = DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some(position),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3, 3],
    };
    let actor = match class {
        CrashClass::Selector34 => arena.allocate_selector34_actor(request, &mut 0),
        CrashClass::Selector35 => arena.allocate_selector35_actor(request, &mut 0),
    }
    .unwrap();
    (arena, actor)
}

/// Replays one crash receipt: each case sets +2C, +32, +84 and +C4, runs the
/// terrain phase with an inert contact, and compares the recorded services.
fn assert_crash_receipt(fixture: &str, class: CrashClass) {
    let fixture: Value = serde_json::from_str(fixture).unwrap();
    let terrain = water_terrain(100);
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let position = std::array::from_fn(|axis| case["position"][axis].as_u64().unwrap() as u16);
        let (mut arena, actor) = crash_ship(class, position);
        let flags = case["flags"].as_u64().unwrap() as u32;
        set_actor(&mut arena, actor, 0x100 | flags, position);
        arena.actor_mut(actor).unwrap().word_c4 = case["state"].as_u64().unwrap() as u32;
        let own = arena.actor(actor).unwrap().handle;
        let mut host = Host::new(Some(&terrain));
        host.contact = Some(CollisionContact::from_native_words([Some(0); 5]));
        host.process_null = Some(ActorHandle::from_raw(
            case["process_null"].as_u64().unwrap() as u32,
        ));
        arena
            .run_world_contact(
                actor,
                WorldContactPhases {
                    terrain: true,
                    furniture: false,
                },
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let native_handle = |handle: ActorHandle| {
            if handle == own {
                case["handle"].as_u64().unwrap() as u32
            } else {
                handle.raw()
            }
        };
        let mut events: Vec<Value> = host
            .destruction
            .iter()
            .map(|effect| match *effect {
                TerrainDestructionEffect::Burst(burst) => json!({"kind": "burst",
                    "count": burst.debris_count, "position": burst.position,
                    "radius": burst.placement_radius, "central": burst.central_type,
                    "debris": burst.debris_types, "speed": burst.speed, "flags": burst.flags}),
                TerrainDestructionEffect::Radius {
                    block_address,
                    source,
                } => json!({"kind": "radius", "block": block_address,
                    "position_is_actor": true, "source": native_handle(source), "replicate": 1}),
                TerrainDestructionEffect::Pickup { kind, value } => json!({"kind": "pickup",
                    "position_is_actor": true, "pickup_kind": kind, "value": value}),
                TerrainDestructionEffect::ChangeState(request) => json!({"kind": "state",
                    "state": request.target_state as u32, "source": native_handle(request.source),
                    "argument": request.argument}),
                TerrainDestructionEffect::Retire => json!({"kind": "retire"}),
            })
            .collect();
        // The shared response 0047D2F0 ran after any crash arm.
        events.push(json!({"kind": "terrain_response", "contact": true}));
        assert_eq!(json!(events), case["events"], "{name}");
    }
    assert_eq!(cases.len(), 13);
}

/// 00434ED0 over 13 supplied ships: every +C4 but 3 runs the burst, the
/// flag-gated radius effect, the pickup and state 0 before 0047D2F0.
#[test]
fn selector34_crash_matches_native_00434ed0() {
    assert_crash_receipt(
        include_str!("../../../tests/fixtures/selector34_crash_native.json"),
        CrashClass::Selector34,
    );
}

/// 0043F3E0 over 13 supplied ships: only +C4 == 1 runs the flag-gated
/// radius effect, the burst and state 0 before 0047D2F0.
#[test]
fn selector35_crash_matches_native_0043f3e0() {
    assert_crash_receipt(
        include_str!("../../../tests/fixtures/selector35_crash_native.json"),
        CrashClass::Selector35,
    );
}

/// `00469030` against the 75 knocks of `world_interactions_native.json`,
/// run on the original instructions with the partner's real +64
/// (`00404610` or `00401300`) and `0048EFC0`: whether the scientist dies.
#[test]
fn the_knock_matches_the_native_receipt() {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/world_interactions_native.json"
    ))
    .unwrap();
    let cases = fixture["scientist_knock"].as_array().unwrap();
    assert_eq!(cases.len(), 76);
    let words = |value: &Value| -> [u16; 3] {
        let list = value.as_array().unwrap();
        std::array::from_fn(|i| list[i].as_u64().unwrap() as u16)
    };
    let mut killed = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let (mut arena, scientist) = ship();
        let actor = arena.actor_mut(scientist).unwrap();
        actor.word_b4 = case["team"].as_u64().unwrap() as u32;
        actor.word_c4 = case["state"].as_u64().unwrap() as u32;
        actor.words_8e_92 = words(&case["velocity"]);
        let kills = arena
            .knock_kills_scientist(scientist, case["partner"] == "self", words(&case["saved"]))
            .unwrap();
        assert_eq!(kills, case["retired"].as_bool().unwrap(), "{name}");
        killed += usize::from(kills);
    }
    assert!(killed >= 4, "{killed}");
}

/// World 20's terrain under the Boulders' contact, the process RNG and the
/// particle births that draw from it.
struct BoulderHost<'a> {
    terrain: &'a TerrainRuntime,
    world_flags: u32,
    sine: &'a [u16; 4096],
    clock: u32,
    rng: u32,
    particles: crate::process_particles::ProcessParticleRuntime,
    executable: inf_formats::FrontendExecutable<'a>,
    process: &'a crate::process_particles::factories::ParticleFactoryProcessState,
    births: Vec<(BoulderParticle, [u16; 3], [u16; 3])>,
    damage: Vec<ActorDamageRequest>,
}

impl WorldContactHost for BoulderHost<'_> {
    type Error = String;
    fn terrain_contact(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
    ) -> Result<Option<CollisionContact>, Self::Error> {
        Err("unreached terrain contact".into())
    }
    fn water_inputs(&self) -> ActorTerrainInputs<'_> {
        ActorTerrainInputs {
            terrain: Some(ActorTerrainSurface {
                terrain: self.terrain,
                plane_height_fixed: self.terrain.plane_base(),
            }),
            coordinate_shift: self.terrain.plane_shift(),
            clock: self.clock,
            world_flags: Some(self.world_flags),
            sine_quarter: self.sine,
        }
    }
    fn network_impact_gate(&self) -> NetworkImpactGate {
        NetworkImpactGate::Offline
    }
    fn impact_damage(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error> {
        self.damage.push(request);
        Ok(())
    }
    fn furniture_contact(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        Err("unreached furniture contact".into())
    }
    fn process_null_handle(&self) -> Option<ActorHandle> {
        None
    }
    fn destruction_effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: TerrainDestructionEffect,
    ) -> Result<(), Self::Error> {
        Err("unreached destruction".into())
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }
    fn boulder_particle(
        &mut self,
        kind: BoulderParticle,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), Self::Error> {
        self.births.push((kind, position, velocity));
        let request = crate::process_particles::births::ParticleBirthRequest {
            particle_type: 0,
            position,
            velocity: Some(velocity),
            flags: 0,
            inputs: crate::process_particles::births::ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: self.process,
            },
        };
        match kind {
            BoulderParticle::Splash => self
                .particles
                .birth_impact(request, self.executable, &mut self.rng)
                .map(|_| ()),
            BoulderParticle::Dust => self
                .particles
                .birth_material_sprite(request, false, self.executable, &mut self.rng)
                .map(|_| ()),
        }
        .map_err(|error| format!("{error:?}"))
    }
}

#[derive(serde::Deserialize)]
struct BoulderContacts {
    contacts: Vec<BoulderContactReceipt>,
}

#[derive(serde::Deserialize)]
struct BoulderContactReceipt {
    recording: String,
    at: String,
    /// The five DWORDs 0047D2F0 and 00405A00 read.
    block: String,
    rng: u32,
    clock: u32,
    /// The 0x148-byte boulder.
    before: String,
    after: String,
    rng_after: u32,
    births: Vec<BoulderBirth>,
}

#[derive(serde::Deserialize)]
struct BoulderBirth {
    kind: String,
    position: [u16; 3],
    velocity: [u16; 3],
}

/// The Boulders' +94 (00405A00): the shared 0047D2F0, then dust. Every one
/// of Infest04's first sixteen contacts that threw any up (one or two
/// sprites at the contact point with half the velocity, the second jittered
/// by up to half the radius), a hard landing that throws nothing (it is not
/// drawn near) and a drawn-near roll whose draw passes it by: positions,
/// velocities, births and the RNG after each match world 20's recording.
#[test]
fn the_boulders_throw_up_dust_as_infest04s_do() {
    let receipts: BoulderContacts =
        serde_json::from_str(include_str!("../../../tests/fixtures/boulders_infest.json")).unwrap();
    assert_eq!(receipts.contacts.len(), 18);
    let (_, assets, world) = crate::test_worlds::offline_world_retained(
        20,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let bytes = |hex: &str| -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    };
    let word = |bytes: &[u8], at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let dword = |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let motion = |bytes: &[u8]| {
        (
            [word(bytes, 0x32), word(bytes, 0x34), word(bytes, 0x36)],
            [word(bytes, 0x8e), word(bytes, 0x90), word(bytes, 0x92)],
        )
    };
    let mut births = 0;
    for contact in &receipts.contacts {
        let block = bytes(&contact.block);
        let words = std::array::from_fn(|index| Some(dword(&block, index * 4)));
        let before = bytes(&contact.before);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x1234),
        })
        .unwrap();
        let boulder = arena
            .allocate_selector59(ActorConstructionRequest {
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: None,
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &SINE,
                },
            })
            .unwrap();
        let (position, velocity) = motion(&before);
        set_actor(&mut arena, boulder, dword(&before, 0x2c), position);
        arena.owned_mut(boulder).unwrap().state.base.word_28 = word(&before, 0x28);
        let state = arena.actor_mut(boulder).unwrap();
        state.word_8c = word(&before, 0x8c);
        state.words_8e_92 = velocity;
        let mut host = BoulderHost {
            terrain: &world.terrain,
            world_flags: world.world_flags,
            sine: &sine,
            clock: contact.clock,
            rng: contact.rng,
            particles: crate::process_particles::ProcessParticleRuntime::default(),
            executable,
            process: &process,
            births: Vec::new(),
            damage: Vec::new(),
        };
        arena
            .respond_world_terrain(
                boulder,
                CollisionContact::from_native_words(words),
                &mut host,
            )
            .unwrap();
        let context = format!("{} {}", contact.recording, contact.at);
        assert_eq!(
            (
                arena.object(boulder).unwrap().base.position_words.unwrap(),
                arena.actor(boulder).unwrap().words_8e_92
            ),
            motion(&bytes(&contact.after)),
            "{context}"
        );
        let recorded: Vec<_> = contact
            .births
            .iter()
            .map(|birth| {
                let kind = match birth.kind.as_str() {
                    "splash" => BoulderParticle::Splash,
                    _ => BoulderParticle::Dust,
                };
                (kind, birth.position, birth.velocity)
            })
            .collect();
        assert_eq!(host.births, recorded, "{context}");
        assert_eq!(host.rng, contact.rng_after, "{context}");
        births += recorded.len();
    }
    assert_eq!(births, 23);
}
