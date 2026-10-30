use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest,
};
use serde_json::Value;

const ZERO_SINE: [u16; 4096] = [0; 4096];

fn ships() -> (ObjectArena, ObjectId, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let allocate = |arena: &mut ObjectArena| {
        arena
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
                            sine_quarter: &ZERO_SINE,
                        },
                    },
                    world_mask_limits: [3, 3],
                },
                &mut 0,
            )
            .unwrap()
    };
    let a = allocate(&mut arena);
    let b = allocate(&mut arena);
    (arena, a, b)
}

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn words<const N: usize>(value: &Value) -> [u32; N] {
    std::array::from_fn(|i| word(&value[i]))
}

fn set(arena: &mut ObjectArena, actor: ObjectId, spec: &Value) {
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.word_28 = word(&spec["word_28"]) as u16;
    base.flags = word(&spec["flags"]);
    base.position_words = Some(words::<3>(&spec["position"]).map(|w| w as u16));
    let state = arena.actor_mut(actor).unwrap();
    state.word_8c = word(&spec["mass"]) as u16;
    state.word_b4 = word(&spec["team"]);
    state.word_c4 = word(&spec["state"]);
    let rows = words::<9>(&spec["basis"]);
    state.rotating.basis.words = [
        0, rows[0], rows[1], rows[2], rows[3], rows[4], rows[5], rows[6], rows[7], rows[8],
    ];
    let derived = arena.derived_actor_mut(actor).unwrap();
    derived.word_144 = word(&spec["word_144"]) as i32;
    derived.word_14c = Some(word(&spec["word_14c"]));
    arena.family428650_actor_mut(actor).unwrap().word_22e = word(&spec["byte_22e"]) as u16;
}

#[test]
fn family_pair_reaction_matches_native_00427c20() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_pair_reaction_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["provenance"]["fpcw"].as_u64(), Some(0x027f));
    let null = ActorHandle::from_raw(word(&fixture["provenance"]["process_null"]));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = inf_formats::parse_frontend_sine_table(executable.data(), 0x53df00)
        .unwrap()
        .values;
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 202);
    let (mut helpers, mut ahead, mut side) = (0, 0, 0);
    for (index, case) in cases.iter().enumerate() {
        let (mut arena, a, b) = ships();
        set(&mut arena, a, &case["a"]);
        set(&mut arena, b, &case["b"]);
        if word(&case["a"]["target_154"]) != 0 {
            let reference = arena
                .allocate_actor_service(ActorServiceAllocationRequest {
                    kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
                })
                .unwrap()
                .unwrap();
            arena.derived_actor_mut(a).unwrap().target_154 = Some(reference);
        }
        let existing = arena.derived_actor(a).unwrap().target_154;
        let mut rng = word(&case["seed"]);
        arena
            .react_family_pair(
                a,
                b,
                FamilyPairReactionRequest {
                    rng: &mut rng,
                    sine_quarter: &sine,
                    constructor_null: WeightedTargetConstructorNull::Supplied(Some(null)),
                },
            )
            .unwrap();
        assert_eq!(rng, word(&case["final_seed"]), "case {index} rng");
        let derived = arena.derived_actor(a).unwrap();
        assert_eq!(
            derived.word_144 as u32,
            word(&case["final_144"]),
            "case {index}"
        );
        assert_eq!(
            arena.object(a).unwrap().base.flags,
            word(&case["final_flags"])
        );
        assert_eq!(
            arena.family428650_actor(a).unwrap().word_22e as u8,
            word(&case["final_22e"]) as u8,
            "case {index}"
        );
        match case["helper"].as_object() {
            None => assert_eq!(derived.target_154, existing, "case {index}"),
            Some(helper) => {
                helpers += 1;
                assert_eq!(
                    derived.word_15c,
                    Some(word(&case["final_15c"])),
                    "case {index}"
                );
                let reference = derived.target_154.expect("allocated helper");
                let NativeTarget::FixedPosition(target) = arena.native_target(reference).unwrap()
                else {
                    panic!("case {index}: helper is not a kind-8 copied point");
                };
                assert_eq!(
                    target,
                    FixedPositionTarget::from_point(
                        null,
                        Some(words::<3>(&helper["point"]).map(|w| w as u16)),
                    ),
                    "case {index}"
                );
                let own = words::<3>(&case["a"]["position"]);
                let theirs = words::<3>(&case["b"]["position"]);
                let offset =
                    std::array::from_fn(|i| (theirs[i] as u16).wrapping_sub(own[i] as u16) as i16);
                if arena.actor(a).unwrap().rotating.basis.inverse_words(offset)[2] > 0 {
                    ahead += 1;
                } else {
                    side += 1;
                }
            }
        }
    }
    assert_eq!(helpers, 87);
    assert!(ahead > 10 && side > 10, "ahead {ahead}, side {side}");
}

fn rescue_actor(
    arena: &mut ObjectArena,
    class: &str,
    grants: &inf_formats::GrantTable,
) -> ObjectId {
    let request = DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([10, 20, 30]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &ZERO_SINE,
            },
        },
        world_mask_limits: [3, 3],
    };
    match class {
        "selector10" => arena.allocate_selector10_actor(request, &mut 0),
        "selector36" => arena.allocate_selector36_actor(request, &mut 0),
        "selector12" => arena.allocate_selector12_actor(
            request,
            &mut 0,
            crate::object_runtime::Selector12GrantContext {
                table: grants,
                first_player_grants: None,
            },
        ),
        other => panic!("{other}"),
    }
    .unwrap()
}

/// 00425B90 over 64 supplied pairs: both states, the partner's +5C, the
/// ship's team and its flag 10000000. The four boarded cases then run the
/// boarding (00425BE0): the partner's team, dirty 2 and +1A0 emptied, as the
/// native suffix left them; the mode follows the team (00425AB0).
#[test]
fn family_rescue_accept_matches_native_00425b90() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_rescue_native.json"
    ))
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    let grants =
        inf_formats::parse_grant_table(inf_formats::FrontendExecutable::parse(&raw).unwrap())
            .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x1234),
        })
        .unwrap();
        let ship = rescue_actor(&mut arena, case["ship"].as_str().unwrap(), &grants);
        let partner = rescue_actor(&mut arena, case["partner"].as_str().unwrap(), &grants);
        let base = &mut arena.owned_mut(ship).unwrap().state.base;
        base.flags = word(&case["flags"]);
        base.dirty_word = 0;
        let state = arena.actor_mut(ship).unwrap();
        state.word_b4 = word(&case["team"]);
        state.word_c4 = word(&case["ship_state"]);
        let state = arena.actor_mut(partner).unwrap();
        state.word_b4 = word(&case["partner_team"]);
        state.word_c4 = word(&case["partner_state"]);
        let verdict = arena.accept_family_rescue(ship, partner).unwrap();
        let after = &case["after"];
        let boarded = word(&after["accepted"]) != 0;
        assert_eq!(verdict == FamilyRescueAccept::Boarding, boarded, "{name}");
        assert_eq!(
            arena.object(ship).unwrap().base.flags,
            word(&after["flags"]),
            "{name}"
        );
        if boarded {
            arena.board_family_unit(ship, partner).unwrap();
            assert_eq!(
                arena.actor(ship).unwrap().word_b4,
                word(&after["team"]),
                "{name}"
            );
            assert_eq!(
                u32::from(arena.object(ship).unwrap().base.dirty_word),
                word(&after["dirty"]),
                "{name}"
            );
            assert_eq!(
                arena.family428650_actor(ship).unwrap().count_1a0,
                word(&after["word_1a0"]),
                "{name}"
            );
            let armed = arena.derived_actor(ship).unwrap().behavior_140.is_some();
            let mode = match word(&after["team"]) {
                0 => 0,
                18 => 1,
                _ => 1 + u16::from(armed),
            };
            assert_eq!(
                arena.family4236d0_actor(ship).unwrap().word_266,
                mode,
                "{name}"
            );
        } else {
            // The boarding suffix owns the team copy and its dirty bit.
            assert!(case["events"].as_array().unwrap().is_empty(), "{name}");
            assert_eq!(
                u32::from(arena.object(ship).unwrap().base.dirty_word),
                word(&after["dirty"]),
                "{name}"
            );
            assert_eq!(
                arena.actor(ship).unwrap().word_b4,
                word(&after["team"]),
                "{name}"
            );
        }
    }
    assert_eq!(cases.len(), 64);
}
