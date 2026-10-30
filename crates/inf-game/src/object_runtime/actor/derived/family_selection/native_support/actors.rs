//! Complete actual constructor / supplied backing / arena preparation phase.
//! Fixture layouts admit classes and storage independently from scan policies.

use super::*;

pub(in crate::object_runtime::actor::derived) fn construct_supplied_actor(
    arena: &mut ObjectArena,
    supplied: &Value,
    rng: &mut u32,
) -> ObjectId {
    const SINE: [u16; 4096] = [0; 4096];
    let request = DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some(xyz(&supplied["position"])),
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
    // These are real portable constructors, followed by explicit supplied
    // canonical query backing. The oracle does not claim native constructors.
    match number(&supplied["vtable"]) {
        0x520f34 => arena.allocate_family428650_actor(request, rng).unwrap(),
        0x520e58 => arena.allocate_derived_actor(request, rng).unwrap(),
        0x51f080 => arena.allocate_selector11_actor(request, rng).unwrap(),
        0x52273c => arena.allocate_selector35_actor(request, rng).unwrap(),
        0x527058 => {
            let raw = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
            )
            .unwrap();
            let table = inf_formats::parse_grant_table(
                inf_formats::FrontendExecutable::parse(&raw).unwrap(),
            )
            .unwrap();
            arena
                .allocate_selector12_actor(
                    request,
                    rng,
                    Selector12GrantContext {
                        table: &table,
                        first_player_grants: None,
                    },
                )
                .unwrap()
        }
        0x527c00 => arena.allocate_actor(request.actor).unwrap(),
        0x5289c0 => arena.allocate_selector30_actor(request.actor).unwrap(),
        other => panic!("unadmitted canonical query table {other:x}"),
    }
}

pub(super) fn supply_actor_fields(
    arena: &mut ObjectArena,
    id: ObjectId,
    supplied: &Value,
    responses: &Arc<ActorConstructorPayload>,
    layout: FixtureLayout,
) {
    let mut handle = arena.actor(id).unwrap().handle;
    let wanted = ActorHandle::from_raw(number(&supplied["handle"]));
    arena
        .actor_handles
        .as_mut()
        .unwrap()
        .reassign(id, &mut handle, wanted)
        .unwrap();
    arena.owned_mut(id).unwrap().state.base.word_28 = number(&supplied["extent_word"]) as u16;
    if let Some(flags) = supplied.get("flags") {
        arena.owned_mut(id).unwrap().state.base.flags = number(flags);
    }
    let actor = arena.actor_mut(id).unwrap();
    actor.handle = wanted;
    actor.words_8e_92 = supplied.get("velocity_words").map(xyz).unwrap_or([0; 3]);
    actor.word_74 = number(&supplied["size_word"]) as u16;
    actor.word_b4 = number(&supplied["class"]);
    actor.word_c4 = number(&supplied["lifecycle"]);
    actor.damage_table = ActorDamageTable::Payload(Arc::clone(&responses));
    if layout == FixtureLayout::Selector12ActorList104 {
        actor.word_7a = number(&supplied["grant_word7a"]) as u16;
        if let crate::object_runtime::ActorExtension::Selector30(state) = &mut actor.extension {
            state.byte_18e = number(&supplied["interaction_byte18e"]) as u8;
        }
    }
    if let Ok(derived) = arena.derived_actor_mut(id) {
        derived.word_164 = number(&supplied["radius"]) as i32;
        derived.behavior_140 = supplied["behavior"].as_bool().unwrap().then(behavior);
    }
    if let Ok(family) = arena.family428650_actor_mut(id) {
        family.count_1a0 = number(&supplied["count"]);
        family.word_20c = number(&supplied["source"]);
        family.word_210 = number(&supplied["timer"]);
        family.word_214 = number(&supplied["boost"]);
        for (row, supplied) in family
            .entries_1a4
            .iter_mut()
            .zip(supplied["rows"].as_array().unwrap())
        {
            *row = crate::object_runtime::FamilyHandleEntry {
                handle: ActorHandle::from_raw(number(&supplied[0])),
                word_04: number(&supplied[1]),
                word_08: number(&supplied[2]),
            };
        }
        family.helper_218 = supplied["helper"].as_bool().unwrap().then(|| {
            Box::new(FamilyHelper482ff0 {
                words_00_04: [Some(0); 2],
                word_08: 0,
                records: [FamilyHelperHistoryRecord {
                    handle: Some(ActorHandle::from_raw(0)),
                    normal_bits: [Some(0); 3],
                    weight: Some(0),
                    padding: Some(0),
                }; 4],
            })
        });
        if layout == FixtureLayout::Selector12ActorList104 {
            if let super::super::super::Family428650Class::Selector35(state) = &mut family.class {
                state.bytes_240[2] = supplied["notification_byte242"]
                    .as_u64()
                    .map(|value| value.try_into().unwrap());
                state.word_244 = number(&supplied["notification_word244"]);
            }
        }
    }
}

pub(in crate::object_runtime::actor::derived) fn supplied_arena(
    input: &Value,
    layout: FixtureLayout,
) -> (ObjectArena, [Option<ObjectId>; 4]) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        // The new callback may bypass an unavailable current process NULL.
        // Registry construction has its independent explicit setup operand.
        null_handle: ActorHandle::from_raw(number(
            &input[if layout == FixtureLayout::Selector12ActorList104 {
                "registry_null"
            } else {
                "process_null"
            }],
        )),
    })
    .unwrap();
    let responses: Vec<_> = input["responses"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|pair| {
            pair.as_array()
                .unwrap()
                .iter()
                .flat_map(|v| (number(v) as u16).to_le_bytes().map(Some))
        })
        .collect();
    let responses = payload(&responses);
    let mut ids = [None; 4];
    for index in 0..4 {
        let supplied = &input["actors"][NAMES[index]];
        if layout == FixtureLayout::InlineTable100 {
            assert_eq!(supplied["vtable"], 0x520f34);
        } else if layout == FixtureLayout::Selector12Inline100 {
            assert_eq!(
                supplied["vtable"],
                if index == 0 { 0x527058 } else { 0x520f34 }
            );
        } else if layout == FixtureLayout::ActorList104 {
            assert!(matches!(
                number(&supplied["vtable"]),
                0x520f34 | 0x520e58 | 0x51f080 | 0x527058 | 0x527c00
            ));
        } else {
            assert_eq!(layout, FixtureLayout::Selector12ActorList104);
            if index == 0 {
                assert_eq!(supplied["vtable"], 0x527058);
            } else {
                assert!(matches!(
                    number(&supplied["vtable"]),
                    0x520f34 | 0x520e58 | 0x51f080 | 0x527c00 | 0x5289c0 | 0x52273c
                ));
            }
        }
        if supplied.get("initially_live").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let id = construct_supplied_actor(&mut arena, supplied, &mut 0);
        supply_actor_fields(&mut arena, id, supplied, &responses, layout);
        ids[index] = Some(id);
    }
    for index in 0..4 {
        let Some(id) = ids[index] else { continue };
        if arena.family428650_actor(id).is_ok() {
            let supplied = &input["actors"][NAMES[index]];
            let fields: &[(usize, &str)] = match layout {
                FixtureLayout::InlineTable100 | FixtureLayout::Selector12Inline100 => {
                    &[(0, "primary"), (1, "secondary")]
                }
                FixtureLayout::ActorList104 if index == 0 => {
                    &[(4, "primary"), (3, "reverse"), (2, "group")]
                }
                FixtureLayout::ActorList104 => &[],
                FixtureLayout::Selector12ActorList104 if index == 0 => {
                    &[(3, "secondary"), (4, "primary")]
                }
                FixtureLayout::Selector12ActorList104 => &[],
            };
            for &(slot, key) in fields {
                let old = match layout {
                    FixtureLayout::InlineTable100 | FixtureLayout::Selector12Inline100 => {
                        &supplied["targets"][key]
                    }
                    FixtureLayout::ActorList104 | FixtureLayout::Selector12ActorList104 => {
                        &input["list_targets"][key]
                    }
                };
                let target = supplied_target(old, ids[3].unwrap())
                    .map(|data| arena.import_fixture_target(data));
                let family = arena.family428650_actor_mut(id).unwrap();
                family.targets_1d4_1fc[slot] = target;
                family.words_1d8_200[slot] = if old.is_null() {
                    0
                } else {
                    number(&old["score"])
                };
            }
        }
    }
    if layout == FixtureLayout::Selector12ActorList104 {
        let self_id = ids[0].unwrap();
        let mask: [u32; 6] = input["actors"]["self"]["grant_mask"]
            .as_array()
            .unwrap()
            .iter()
            .map(number)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let super::super::super::Family429e20Class::Selector12(state) =
            &mut arena.family429e20_actor_mut(self_id).unwrap().class
        else {
            panic!("actual selector12 target and grant owner")
        };
        state.grants_24c.mask_04 = mask;
        for (slot, key) in [(1, "resource"), (2, "grant")] {
            let old = &input["list_targets"][key];
            let target =
                supplied_target(old, ids[3].unwrap()).map(|data| arena.import_fixture_target(data));
            let super::super::super::Family429e20Class::Selector12(state) =
                &mut arena.family429e20_actor_mut(self_id).unwrap().class
            else {
                unreachable!()
            };
            state.targets_26c_284[slot] = target;
            let score = if old.is_null() {
                0
            } else {
                number(&old["score"])
            };
            match slot {
                1 => state.word_278 = score,
                2 => state.word_280 = score,
                _ => unreachable!(),
            }
        }
    }
    if layout.is_actor_list() {
        arena.actors = input["actor_list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| {
                ids[NAMES
                    .iter()
                    .position(|v| Some(*v) == name.as_str())
                    .unwrap()]
                .unwrap()
            })
            .collect();
        // Native supplied absent actor has a known handle word but no live
        // allocation. Bind/release establishes that actual registry custody.
        for index in 0..4 {
            if ids[index].is_none() {
                let handle =
                    ActorHandle::from_raw(number(&input["actors"][NAMES[index]]["handle"]));
                let registry = arena.actor_handles.as_mut().unwrap();
                registry.bind(handle, ids[0].unwrap()).unwrap();
                registry.release(handle);
            }
        }
    }
    let registry = arena.actor_handles.as_mut().unwrap();
    let empty = registry
        .owner(1)
        .unwrap()
        .slots()
        .iter()
        .skip(1)
        .find(|entry| entry.object.is_none())
        .unwrap()
        .handle;
    assert_eq!(registry.resolve(empty), None);
    registry.release(empty);
    (arena, ids)
}
