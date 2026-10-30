use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, ObjectClassKind, ObjectList, ObjectNodeKind,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/simple_object_updates_native.json"
    ))
    .unwrap()
}
fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

#[test]
fn simple_update_countdown_matches_54_native_unsigned_cases() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        })
        .unwrap();
    let fixture = fixture();
    assert_eq!(fixture["countdown"].as_array().unwrap().len(), 54);
    for case in fixture["countdown"].as_array().unwrap() {
        arena.actor_mut(actor).unwrap().word_c8 = number(&case["remaining"]) as u16;
        let mut expected = arena.object(actor).unwrap().clone();
        let crate::object_runtime::ObjectClass::Actor(state) = &mut expected.class else {
            unreachable!()
        };
        state.word_c8 = number(&case["after"]) as u16;
        arena
            .tick_actor_common_timer(actor, number(&case["delta"]))
            .unwrap();
        assert_eq!(arena.object(actor).unwrap(), &expected, "{case}");
    }
}

#[test]
fn simple_update_forwarding_matches_native_class_tables_and_reappend_order() {
    let fixture = fixture();
    for case in fixture["forwarding"].as_array().unwrap() {
        let kind = match case["kind"].as_str().unwrap() {
            "base" => ObjectClassKind::Base,
            "visual" => ObjectClassKind::VisualBase,
            "rotating" => ObjectClassKind::RotatingVisualBase,
            "terrain_light" => ObjectClassKind::TerrainLight,
            "route" => ObjectClassKind::RouteNode,
            "map" => ObjectClassKind::MapAnnotation,
            "emitter" => ObjectClassKind::Selector86,
            "pickup" => ObjectClassKind::TimedPickup,
            other => panic!("{other}"),
        };
        let mut arena = ObjectArena::new();
        let objects = std::array::from_fn::<_, 3, _>(|_| arena.allocate(kind));
        for object in objects {
            let state = &mut arena.owned_mut(object).unwrap().state;
            state.base.flags = number(&case["flags"]);
            state.base.dirty_word = 0xa580;
        }
        let classes = objects.map(|object| arena.object(object).unwrap().class.clone());
        for step in case["steps"].as_array().unwrap() {
            let id = objects[number(&step["actor"]) as usize];
            arena.set_flag1(id, false).unwrap();
            assert_eq!(
                json!(objects.map(|id| arena.object(id).unwrap().base.flags)),
                step["flags"]
            );
            assert_eq!(
                json!(objects.map(|id| arena.object(id).unwrap().base.dirty_word)),
                step["dirty"]
            );
            assert_eq!(
                objects.map(|id| arena.object(id).unwrap().class.clone()),
                classes
            );
            for (name, list) in [
                ("primary", ObjectList::Flag1Primary),
                ("secondary", ObjectList::Flag1Secondary),
                ("parking", ObjectList::Parking),
            ] {
                let actual: Vec<_> = arena
                    .list(list)
                    .iter()
                    .map(|node| {
                        [
                            objects.iter().position(|id| *id == node.object).unwrap(),
                            match node.kind {
                                ObjectNodeKind::Flag0 => 4,
                                ObjectNodeKind::Flag1 => 16,
                                ObjectNodeKind::Lifecycle => 28,
                            },
                        ]
                    })
                    .collect();
                assert_eq!(json!(actual), step["lists"][name], "{case}");
            }
        }
    }
}

#[test]
fn simple_update_full_native_bodies_and_selected_vtables_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    let fixture = fixture();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        fixture["retail_sha256"].as_str().unwrap()
    );
    for guard in fixture["guards"].as_array().unwrap() {
        let begin = number(&guard["address"]) as usize - 0x400000;
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[begin..begin + number(&guard["length"]) as usize])
            ),
            guard["sha256"].as_str().unwrap()
        );
    }
    let exe = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    assert_eq!(exe.mapped(0x0054_69e8, 6).unwrap().bytes, &[0; 6]);
    assert_eq!(
        crate::object_runtime::CameraGlobalState::INITIAL.cached_position,
        Some([0; 3])
    );
    for table in fixture["tables"].as_object().unwrap().values() {
        assert_eq!(
            exe.word(number(&table["table"]) + 0x24).unwrap(),
            number(&table["update"])
        );
        assert_eq!(
            exe.word(number(&table["table"]) + 0x3c).unwrap(),
            number(&table["alternate"])
        );
    }
    assert_eq!(
        exe.word(number(&fixture["camera"]["table"]) + 0x24)
            .unwrap(),
        0x47ad30
    );
}

/// Selector 6's plain shared actors (the flags) keep table 00527C00, whose
/// +24 is the countdown 0047AD30; their +3C (0047EBF0) stays unowned.
#[test]
fn simple_update_ticks_the_shared_countdown_of_plain_actors() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let flag = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        })
        .unwrap();
    arena.actor_mut(flag).unwrap().word_c8 = 100;
    assert!(
        crate::session_platform::world_actor_update::tick_simple_object_callback(
            &mut arena, flag, false, 320
        )
        .unwrap()
    );
    assert_eq!(arena.actor(flag).unwrap().word_c8, 90);
    assert!(
        !crate::session_platform::world_actor_update::tick_simple_object_callback(
            &mut arena, flag, true, 320
        )
        .unwrap()
    );
    assert_eq!(arena.actor(flag).unwrap().word_c8, 90);
}
