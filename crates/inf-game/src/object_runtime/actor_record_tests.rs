use serde_json::{json, Value};

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
};

static SINE: [u16; 4096] = [0; 4096];

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/net_record_native.json")).unwrap()
}

fn int(value: &Value) -> u32 {
    value.as_i64().unwrap() as u32
}

fn words<const N: usize>(value: &Value) -> [u32; N] {
    std::array::from_fn(|index| int(&value[index]))
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn tables() -> (NativeBasisMath, [i16; 1024]) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let bytes = executable.mapped(0x0053_ff00, 2048).unwrap().bytes;
    let inverse = std::array::from_fn(|i| i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]));
    (NativeBasisMath::new(assets.sine_table().values), inverse)
}

fn allocate(arena: &mut ObjectArena) -> ObjectId {
    arena
        .allocate_actor(ActorConstructionRequest {
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
        .unwrap()
}

/// The receipt's actor, with its holder bound when the case resolves it.
fn world(case: &Value, null: u32) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 256,
        null_handle: ActorHandle::from_raw(null),
    })
    .unwrap();
    let actor = allocate(&mut arena);
    let holder = allocate(&mut arena);
    let fields = &case["actor"];
    {
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.flags = int(&fields["flags"]);
        base.placement_word = int(&fields["placement"]) as u16;
        base.resource_word = int(&fields["resource"]) as u16;
        base.position_words = Some(words::<3>(&fields["position"]).map(|w| w as u16));
        base.dirty_word = int(&fields["dirty"]) as u16;
        base.word_3c = int(&fields["deadline"]);
    }
    {
        let state = arena.actor_mut(actor).unwrap();
        state.rotating.visual.model_word = int(&fields["model"]) as u16;
        state.rotating.basis.words = words::<10>(&fields["basis"]);
        state.capture_handle_70 = Some(ActorHandle::from_raw(int(&fields["holder"])));
        state.handle = ActorHandle::from_raw(int(&fields["handle"]));
        state.owner_handle = ActorHandle::from_raw(int(&fields["owner"]));
        state.words_8e_92 = words::<3>(&fields["spin"]).map(|w| w as u16);
        state.words_94_9c = words::<3>(&fields["velocity"]);
        state.health = int(&fields["health"]) as u16;
        state.maximum_health = Some(int(&fields["maximum"]) as u16);
        state.word_b4 = int(&fields["klass"]);
        state.word_c4 = int(&fields["state"]);
        state.word_e0 = int(&fields["count"]);
        state.word_e4 = int(&fields["length"]);
    }
    arena.owned_mut(holder).unwrap().state.base.flags =
        fields.get("holder_flags").map_or(0x8000_0000, int);
    if let Some(handle) = case["resolve"].as_i64() {
        if handle as u32 != null {
            let handle = ActorHandle::from_raw(handle as u32);
            let own = arena.actor(holder).unwrap().handle;
            let registry = arena.actor_handles.as_mut().unwrap();
            registry.release(own);
            registry.bind(handle, holder).unwrap();
        }
    }
    (arena, actor)
}

fn after(arena: &ObjectArena, actor: ObjectId) -> Value {
    let base = &arena.object(actor).unwrap().base;
    let state = arena.actor(actor).unwrap();
    json!({
        "flags": base.flags,
        "placement": base.placement_word,
        "resource": base.resource_word,
        "position": base.position_words.unwrap(),
        "dirty": base.dirty_word,
        "deadline": base.word_3c,
        "model": state.rotating.visual.model_word,
        "holder": state.capture_handle_70.unwrap().raw(),
        "handle": state.handle.raw(),
        "owner": state.owner_handle.raw(),
        "spin": state.words_8e_92,
        "velocity": state.words_94_9c,
        "health": state.health,
        "maximum": state.maximum_health.unwrap(),
        "klass": state.word_b4,
        "state": state.word_c4,
        "count": state.word_e0,
        "length": state.word_e4,
        "basis": state.rotating.basis.words,
    })
}

/// Records what the receipt's boundaries record, in its terms.
struct Host {
    null: u32,
    calls: Vec<Value>,
}

impl ActorRecordHost for Host {
    type Error = ();
    fn drop_held(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), ()> {
        self.calls.push(json!(["ground"]));
        self.calls.push(json!(["drop", 0, self.null, 0xd4]));
        Ok(())
    }
    fn change_state(&mut self, _: &mut ObjectArena, _: ObjectId, state: u32) -> Result<(), ()> {
        self.calls.push(json!(["state", 0, self.null, state]));
        Ok(())
    }
    fn refresh_model(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), ()> {
        self.calls.push(json!(["model"]));
        Ok(())
    }
    /// The shared actor's +BC (00401300) has no holding component.
    fn holder_holds(&mut self, _: &ObjectArena, _: ObjectId, _: ActorHandle) -> Result<bool, ()> {
        Ok(false)
    }
    fn send_release(&mut self, pair: [u32; 2]) -> Result<(), ()> {
        self.calls.push(json!(["send", 9, pair]));
        Ok(())
    }
    fn ball_sound(&mut self, sound: u32, position: [u16; 3]) -> Result<(), ()> {
        self.calls.push(json!(["sound", sound, position]));
        Ok(())
    }
}

/// The receipt's calls without its resolves and the bad_lode.txt fopen,
/// which the port makes inside the arena or logs.
fn boundary_calls(case: &Value) -> Vec<Value> {
    case["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|call| !matches!(call[0].as_str(), Some("resolve" | "fopen")))
        .cloned()
        .collect()
}

/// `check_net_record.py`: 0045DBD0/0047E330 over 140 actors and rooms.
#[test]
fn records_match_0047e330() {
    let fixture = fixture();
    let null = int(&fixture["null"]);
    let (math, inverse) = tables();
    let mut checked = 0;
    for case in fixture["cases"].as_array().unwrap() {
        if case["kind"] != "encode" {
            continue;
        }
        let (mut arena, actor) = world(case, null);
        let mut host = Host {
            null,
            calls: Vec::new(),
        };
        let room = int(&case["room"]) as i32;
        let mut out = vec![0xcc; 0x200];
        let length = arena
            .encode_actor_record(
                actor,
                &mut out,
                room,
                ActorHandle::from_raw(null),
                &math,
                &inverse,
                &mut host,
            )
            .unwrap();
        assert_eq!(
            i64::from(length),
            case["length"].as_i64().unwrap(),
            "{case}"
        );
        if length > 0 {
            assert_eq!(
                out[..length as usize],
                hex(case["out"].as_str().unwrap()),
                "{case}"
            );
        }
        assert_eq!(host.calls, boundary_calls(case), "{case}");
        assert_eq!(after(&arena, actor), case["after"], "{case}");
        checked += 1;
    }
    assert_eq!(checked, 140);
}

/// `check_net_record.py`: 0047E600 over the records, into remote and local
/// actors.
#[test]
fn received_records_match_0047e600() {
    let fixture = fixture();
    let null = int(&fixture["null"]);
    let clock = int(&fixture["clock"]);
    let (math, _) = tables();
    let mut checked = 0;
    for case in fixture["cases"].as_array().unwrap() {
        if case["kind"] != "receive" {
            continue;
        }
        let (mut arena, actor) = world(case, null);
        let mut host = Host {
            null,
            calls: Vec::new(),
        };
        let payload = hex(case["payload"].as_str().unwrap());
        let used = arena
            .apply_actor_record(
                actor,
                &payload,
                ActorHandle::from_raw(null),
                clock,
                1,
                &math,
                &mut host,
            )
            .unwrap();
        assert_eq!(i64::from(used), case["used"].as_i64().unwrap(), "{case}");
        assert_eq!(host.calls, boundary_calls(case), "{case}");
        assert_eq!(after(&arena, actor), case["after"], "{case}");
        checked += 1;
    }
    assert_eq!(checked, 146);
}

/// What the turret's, the charge's and the factory's syncs carry.
type Synced = (
    [u32; 2],
    (ActorHandle, Option<[u16; 3]>),
    (u32, [u16; 6], ActorHandle),
);

fn synced(arena: &ObjectArena, [turret, charge, factory]: [ObjectId; 3]) -> Synced {
    let aim = arena.selector11_actor(turret).unwrap();
    let Family455560Tail::Flags14c {
        handle_154: Some(handle),
        offset_158,
        ..
    } = &arena.thrown(charge).unwrap().tail
    else {
        panic!("selector 27's tail");
    };
    let shop = arena.selector15(factory).unwrap();
    (
        [aim.word_188, aim.word_18c],
        (*handle, *offset_158),
        (shop.word_160, shop.words_178_182.unwrap(), shop.handle_188),
    )
}

fn set_synced(
    arena: &mut ObjectArena,
    [turret, charge, factory]: [ObjectId; 3],
    (aim, (handle, offset), (state, words, docked)): Synced,
) {
    let turret = arena.selector11_mut(turret).unwrap();
    [turret.word_188, turret.word_18c] = aim;
    let Family455560Tail::Flags14c {
        handle_154,
        offset_158,
        ..
    } = &mut arena.thrown_mut(charge).unwrap().tail
    else {
        panic!("selector 27's tail");
    };
    (*handle_154, *offset_158) = (Some(handle), offset);
    let factory = arena.selector15_mut(factory).unwrap();
    (factory.word_160, factory.words_178_182, factory.handle_188) = (state, Some(words), docked);
}

/// The turret's (`004025C0`/`00402600`), the charge's (`004560D0`/
/// `00456130`) and the factory's (`00431AD0`/`00431C00`) syncs carry their
/// fields from an actor to another machine's copy of it.
#[test]
fn records_carry_the_turret_charge_and_factory_syncs() {
    let null = ActorHandle::from_raw(0x55);
    let (math, inverse) = tables();
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: null,
    })
    .unwrap();
    let request = |position| ActorConstructionRequest {
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
    };
    let turret = arena
        .allocate_selector11_actor(
            crate::object_runtime::DerivedActorConstructionRequest {
                actor: request([1, 2, 3]),
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    let charge = arena
        .allocate_behavior_resource(request([4, 5, 6]), 27)
        .unwrap();
    let factory = arena.allocate_selector15(request([7, 8, 9])).unwrap();
    let actors = [turret, charge, factory];
    let stuck = arena.actor(turret).unwrap().handle;
    let sent: Synced = (
        [0x1234, 0xffff_fff0],
        (stuck, Some([7, 8, 9])),
        (4, [0, 0, 0, 0x2a, 0x10, 0], stuck),
    );
    set_synced(&mut arena, actors, sent);
    let mut records = Vec::new();
    for actor in actors {
        arena.owned_mut(actor).unwrap().state.base.dirty_word = flags::SYNC;
        let mut host = Host {
            null: null.raw(),
            calls: Vec::new(),
        };
        let mut out = vec![0xcc; 0x200];
        let length = arena
            .encode_actor_record(actor, &mut out, 0x200, null, &math, &inverse, &mut host)
            .unwrap();
        out.truncate(length as usize);
        records.push(out);
    }
    // The receiving machine's copies: remote, with other fields.
    set_synced(
        &mut arena,
        actors,
        ([0; 2], (null, None), (0, [0, 0, 0, 0xffff, 0, 5], null)),
    );
    for (actor, record) in actors.into_iter().zip(&records) {
        arena.owned_mut(actor).unwrap().state.base.flags |= 0x8000_0000;
        let mut host = Host {
            null: null.raw(),
            calls: Vec::new(),
        };
        let used = arena
            .apply_actor_record(actor, record, null, 0, 1, &math, &mut host)
            .unwrap();
        assert_eq!(used as usize, record.len());
    }
    // 00431C00 keeps +182 when the record has none.
    let mut expected = sent;
    expected.2 .1[5] = 5;
    assert_eq!(synced(&arena, actors), expected);
}
