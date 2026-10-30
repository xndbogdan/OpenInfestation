//! The unchanged score wrapper 00425D30 (over +12C, 00423B40) and change
//! 00423A80 under Unicorn ([check_follow_player.py], fixture
//! `follow_player_native.json`) against the port on the same supplied
//! fields: 27 scores and 14 changes.

use std::sync::Arc;

use inf_formats::ByteSpan;
use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorFollowTarget, ActorHandle,
    ActorRegistryConfig, ActorTerrainInputs, DerivedActorConstructionRequest,
    FamilyDescriptorScoreError, FamilyDescriptorScoreKind, FamilyDescriptorScoreRequest,
    FamilyDescriptorScoreRng, FixtureActorServiceAllocation, NativeTarget, RuntimeAllocation,
};

const SINE: [u16; 4096] = [0; 4096];
const SENTINEL: u32 = 0xdead_beef;

fn request() -> DerivedActorConstructionRequest<'static> {
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
    }
}

fn u(value: &Value) -> u32 {
    value
        .as_u64()
        .map(|value| value as u32)
        .unwrap_or_else(|| value.as_i64().unwrap() as u32)
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| u(&value[axis]) as u16)
}

struct Case {
    arena: ObjectArena,
    actor: ObjectId,
    player: Option<ObjectId>,
}

/// The fixture's actor and first player's unit as port state.
fn case(input: &Value) -> Case {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap();
    let actor = arena
        .allocate_family4236d0_actor(request(), &mut 0)
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some(words(&input["position"]));
    arena.derived_actor_mut(actor).unwrap().word_164 = u(&input["radius"]) as i32;
    for (index, key) in [(0, "target_1d4"), (4, "target_1f4")] {
        if u(&input[key]) != 0 {
            let held = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
                handle_04: ActorHandle::from_raw(0),
                focus_08: None,
                word_10: 2,
                focus_14: None,
                velocity_1a: None,
            }));
            arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[index] = Some(held);
        }
    }
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.word_22e = (u(&input["bytes_22c"]) >> 16) as u16;
    family.word_228 = SENTINEL;
    family.words_204_208[1] = Some(SENTINEL);
    let owner = Arc::new(RuntimeAllocation::new(vec![0; 16].into(), ByteSpan::new(0, 16)).unwrap());
    owner
        .write_word(0, u(&input["coefficient"]) as u16)
        .unwrap();
    arena.set_family4236d0_retained(actor, &owner, 0).unwrap();
    let player = input["player"].as_bool().unwrap().then(|| {
        let unit = arena.allocate_derived_actor(request(), &mut 0).unwrap();
        arena.owned_mut(unit).unwrap().state.base.position_words =
            Some(words(&input["player_position"]));
        arena.owned_mut(unit).unwrap().state.base.word_28 = u(&input["player_size"]) as u16;
        arena.actor_mut(unit).unwrap().word_c4 = u(&input["player_state"]);
        let player = arena.add_world_player([0; 6]);
        let handle = arena.actor(unit).unwrap().handle;
        arena.world_player_mut(player).unwrap().handle_d4 = handle;
        unit
    });
    if !input["allocation"].as_bool().unwrap() {
        arena
            .actor_service_heap_mut()
            .script([FixtureActorServiceAllocation::Null]);
    }
    Case {
        arena,
        actor,
        player,
    }
}

fn receipt() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/follow_player_native.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn every_native_score_matches_the_port() {
    let receipt = receipt();
    let mut seen = 0;
    for native in receipt["cases"].as_array().unwrap() {
        if native["kind"] != "score" {
            continue;
        }
        seen += 1;
        let input = &native["input"];
        let Case {
            mut arena, actor, ..
        } = case(input);
        let score = arena.score_family_descriptor(
            actor,
            FamilyDescriptorScoreRequest {
                kind: FamilyDescriptorScoreKind::Virtual12c,
                current: u(&input["current"]) != 0,
                bias: u(&input["bias"]) as i32,
            },
            FamilyDescriptorScoreRng::RequiredAtReach,
        );
        assert_eq!(
            score.map(|score| score as u32),
            Ok(u(&native["output"]["score"])),
            "{input}"
        );
        // The score writes nothing.
        assert_eq!(arena.family428650_actor(actor).unwrap().word_228, SENTINEL);
    }
    assert_eq!(seen, 27);
}

#[test]
fn every_native_change_matches_the_port() {
    let receipt = receipt();
    let mut seen = 0;
    for native in receipt["cases"].as_array().unwrap() {
        if native["kind"] != "change" {
            continue;
        }
        seen += 1;
        let input = &native["input"];
        let output = &native["output"]["actor"];
        let Case {
            mut arena,
            actor,
            player,
        } = case(input);
        arena
            .change_family_follow_player(actor, u(&input["entering"]) & 0xff != 0)
            .unwrap();
        let family = arena.family428650_actor(actor).unwrap();
        assert_eq!(family.word_228, u(&output["status_228"]), "{input}");
        assert_eq!(
            family.words_204_208[1],
            Some(u(&output["distance_208"])),
            "{input}"
        );
        assert_eq!(
            family.word_22e,
            (u(&output["bytes_22c"]) >> 16) as u16,
            "{input}"
        );
        let constructed = native["output"]["calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["call"] == "004825D0");
        let target = arena.derived_actor(actor).unwrap().target_148;
        match constructed {
            Some(call) => {
                // 004825D0 resolved the unit's own handle, with no point.
                assert_eq!(u(&call["handle"]), u(&input["player_handle"]));
                assert_eq!(u(&call["point"]), 0);
                let unit = player.unwrap();
                let NativeTarget::ActorFollow(follow) =
                    arena.native_target(target.unwrap()).unwrap()
                else {
                    panic!("a follow target");
                };
                assert_eq!(follow.handle_04, arena.actor(unit).unwrap().handle);
                assert_eq!(follow.word_10, 2);
                let [x, y, z] = words(&input["player_position"]);
                assert_eq!(follow.focus_08, Some([x, y, z]));
            }
            None => assert_eq!(target, None, "{input}"),
        }
    }
    assert_eq!(seen, 14);
}

/// Another class's +12C (selector 35's 0043F4A0, selector 12's 004672F0)
/// is not 00423B40: it stays required after the coefficient read.
#[test]
fn other_classes_still_require_their_own_virtual() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap();
    let actor = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
    let owner = Arc::new(RuntimeAllocation::new(vec![0; 16].into(), ByteSpan::new(0, 16)).unwrap());
    owner.write_word(0, 1).unwrap();
    arena.set_selector35_retained(actor, &owner, 0).unwrap();
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            FamilyDescriptorScoreRequest {
                kind: FamilyDescriptorScoreKind::Virtual12c,
                current: false,
                bias: 0,
            },
            FamilyDescriptorScoreRng::RequiredAtReach,
        ),
        Err(FamilyDescriptorScoreError::RequiredActorVirtual {
            actor,
            callback: 0x0042_5d30,
            offset: 0x12c,
        })
    );
    assert_eq!(
        arena.change_family_follow_player(actor, true),
        Err(FamilyFollowPlayerError::RequiredActorVirtual(actor))
    );
}

/// Terra Nova's assault craft (placement 83) on its loaded world: team 2
/// with a behavior, so the family's mode 2 and its nine descriptors, and
/// its payload's weights (Attack and Follow Player 500, Run Away 0, Wander,
/// Going Home and Herding 10, the trail 100). With nothing held and the
/// player 1000 away, Follow Player (500 * (2500 - 1000)) outscores Wander
/// (10 * 1250); an enemy held at slot 0 makes Attack win and zeroes Follow
/// Player; within 512 the player is no longer followed.
#[test]
fn terra_novas_craft_attacks_first_and_follows_a_nearby_player_once_clear() {
    let (_, _, world) = crate::test_worlds::offline_world_retained(
        2,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut arena = world.arena;
    let craft = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            arena.family4236d0_actor(actor).is_ok()
                && arena
                    .object(actor)
                    .ok()
                    .and_then(|object| object.base.position_words)
                    .is_some_and(|[x, _, z]| [x, z] == [26154, 23827])
        })
        .expect("placement 83");
    assert_eq!(arena.actor(craft).unwrap().word_b4, 2);
    assert_eq!(arena.family4236d0_actor(craft).unwrap().word_266, 2);
    assert_eq!(arena.derived_actor(craft).unwrap().word_164, 2500);
    let profile = crate::object_runtime::FamilyBehaviorProfile::Family4236d0 { mode: 2 };
    let craft_at = arena.object(craft).unwrap().base.position_words.unwrap();
    let scores = |arena: &mut ObjectArena, player_offset: u16| {
        arena.owned_mut(vehicle).unwrap().state.base.position_words = Some([
            craft_at[0].wrapping_add(player_offset),
            craft_at[1],
            craft_at[2],
        ]);
        (2..9)
            .map(|index| {
                let descriptor = profile.descriptor(index).unwrap();
                let request =
                    FamilyDescriptorScoreRequest::from_descriptor(descriptor, false, 0).unwrap();
                arena
                    .score_family_descriptor(
                        craft,
                        request,
                        FamilyDescriptorScoreRng::RequiredAtReach,
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>()
    };
    // Attack, Run Away, Wander, Going Home, Herding, trail, Follow Player.
    assert_eq!(scores(&mut arena, 1000), [0, 0, 12_500, 0, 0, 0, 750_000]);
    assert_eq!(scores(&mut arena, 400), [0, 0, 12_500, 0, 0, 0, 0]);
    let enemy = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
        handle_04: ActorHandle::from_raw(0),
        focus_08: None,
        word_10: 2,
        focus_14: None,
        velocity_1a: None,
    }));
    let family = arena.family428650_actor_mut(craft).unwrap();
    family.targets_1d4_1fc[0] = Some(enemy);
    family.words_1d8_200[0] = 300;
    assert_eq!(scores(&mut arena, 1000), [150_000, 0, 12_500, 0, 0, 0, 0]);
}
