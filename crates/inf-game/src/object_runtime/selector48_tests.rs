//! The altitude ceiling against the original update 0043C8F0 under Unicorn
//! ([selector48_update_native.json](../../tests/fixtures/selector48_update_native.json),
//! `check_selector48_update.py`), and its commands.

use inf_formats::{InstanceCommandPayload, InstanceField, GRANT_MASK_WORDS};
use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, ObjectClassKind, ObjectList,
};

const SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn ceiling(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    let object = arena.allocate(ObjectClassKind::Selector48);
    arena.owned_mut(object).unwrap().state.base.position_words = Some(position);
    object
}

fn unit(arena: &mut ObjectArena, position: [u16; 3], velocity: [u16; 3]) -> ObjectId {
    let unit = arena
        .allocate_selector37(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0, 0, 0]),
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
    arena.owned_mut(unit).unwrap().state.base.position_words = Some(position);
    arena.actor_mut(unit).unwrap().words_8e_92 = velocity;
    unit
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

#[derive(Default)]
struct Host {
    /// Kind, position, velocity and the RNG word at each shot.
    shots: Vec<(u32, [u16; 3], [u16; 3], u32)>,
    hints: Vec<PlayerId>,
}

impl Selector48Host for Host {
    type Error = String;
    fn shot(
        &mut self,
        _: &mut ObjectArena,
        kind: u32,
        position: [u16; 3],
        velocity: [u16; 3],
        rng: &mut u32,
    ) -> Result<(), String> {
        self.shots.push((kind, position, velocity, *rng));
        Ok(())
    }
    fn hint(&mut self, _: &mut ObjectArena, player: PlayerId) -> Result<(), String> {
        self.hints.push(player);
        Ok(())
    }
}

/// Every case's shots (kind, start, velocity and the RNG at the factory
/// call), its hint and the final RNG word. The original passes the process
/// NULL as the owner and flags 0, which the production host does.
#[test]
fn ceiling_update_matches_the_original() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/selector48_update_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    let mut fired = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut arena = arena();
        let object = ceiling(&mut arena, words(&case["position"]));
        let state = arena.selector48_mut(object).unwrap();
        state.ceiling_44 = case["ceiling"].as_i64().unwrap() as i16;
        state.radius_46 = case["radius"].as_u64().unwrap() as u16;
        state.relative_48 = case["relative"].as_u64().unwrap() as u8;
        let mut player = None;
        if case["player"].as_bool().unwrap() {
            let id = arena.add_world_player([0; GRANT_MASK_WORDS]);
            if !case["unit"].is_null() {
                let unit = unit(
                    &mut arena,
                    words(&case["unit"]["position"]),
                    words(&case["unit"]["velocity"]),
                );
                let handle = arena.actor(unit).unwrap().handle;
                arena.world_player_mut(id).unwrap().handle_d4 = handle;
            }
            player = Some(id);
        }
        let mut rng = case["rng"].as_u64().unwrap() as u32;
        let mut host = Host::default();
        arena
            .tick_selector48(
                object,
                case["delta"].as_u64().unwrap() as u32,
                &mut rng,
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let expected: Vec<(u32, [u16; 3], [u16; 3], u32)> = case["shots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|shot| {
                assert_eq!(shot["owner"], case["null_handle"], "{name}");
                assert_eq!(shot["flags"], 0, "{name}");
                (
                    shot["kind"].as_u64().unwrap() as u32,
                    words(&shot["position"]),
                    words(&shot["velocity"]),
                    shot["rng"].as_u64().unwrap() as u32,
                )
            })
            .collect();
        assert_eq!(host.shots, expected, "{name}");
        fired += expected.len();
        let hints: Vec<PlayerId> = case["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|command| {
                assert_eq!(command, &serde_json::json!([CEILING_HINT, 0]), "{name}");
                player.unwrap()
            })
            .collect();
        assert_eq!(host.hints, hints, "{name}");
        assert_eq!(
            u64::from(rng),
            case["final_rng"].as_u64().unwrap(),
            "{name}"
        );
    }
    assert!(fired > 0);
}

fn field<T>(value: T) -> InstanceField<T> {
    InstanceField {
        span: inf_formats::ByteSpan::new(0, 0),
        value,
    }
}

/// 0043C840: codes 3..5 store the ceiling, the radius and the relative
/// byte; code 6 disposes the ceiling only when the first player owns the
/// grant, and accepts with no player too.
#[test]
fn commands_set_the_ceiling_and_a_owned_grant_removes_it() {
    let mut arena = arena();
    let object = ceiling(&mut arena, [0; 3]);
    assert_eq!(*arena.selector48(object).unwrap(), Selector48State::new());
    for payload in [
        InstanceCommandPayload::Selector48Ceiling(field(0xfc18)),
        InstanceCommandPayload::Selector48Radius(field(900)),
        InstanceCommandPayload::Selector48Relative(field(2)),
    ] {
        assert!(arena.apply_selector48_command(object, &payload).unwrap());
    }
    let state = arena.selector48(object).unwrap();
    assert_eq!(
        (state.ceiling_44, state.radius_46, state.relative_48),
        (-1000, 900, 2)
    );
    let disposed = |arena: &ObjectArena| {
        arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|node| node.object == object)
    };
    let grant = InstanceCommandPayload::Selector48Grant(field(0x30));
    assert!(arena.apply_selector48_command(object, &grant).unwrap());
    assert!(!disposed(&arena));
    let mut mask = [0; GRANT_MASK_WORDS];
    arena.add_world_player(mask);
    assert!(arena.apply_selector48_command(object, &grant).unwrap());
    assert!(!disposed(&arena));
    mask[1] |= 1 << 16;
    let player = arena.first_player().unwrap();
    arena.world_player_mut(player).unwrap().grants_9c = mask;
    assert!(arena.apply_selector48_command(object, &grant).unwrap());
    assert!(disposed(&arena));
    assert!(!arena
        .apply_selector48_command(object, &InstanceCommandPayload::NoPayloadRead)
        .unwrap());
}
