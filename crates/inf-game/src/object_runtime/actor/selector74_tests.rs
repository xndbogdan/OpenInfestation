//! The zone against its original bodies under Unicorn
//! ([selector74_native.json](../../../tests/fixtures/selector74_native.json),
//! `check_selector74.py`).

use inf_formats::{ByteSpan, ConstructorField, InstanceField};
use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorConstructorNull, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn cases(mode: &str) -> Vec<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector74_native.json"
    ))
    .unwrap();
    fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["input"]["mode"] == mode)
        .cloned()
        .collect()
}

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

fn request(sine: &[u16; 4096]) -> ActorConstructionRequest<'_> {
    ActorConstructionRequest {
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0, 0, 0]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: sine,
        },
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

/// A zone as the case's input left it, its basis the case's turn.
fn zone(arena: &mut ObjectArena, input: &Value) -> ObjectId {
    let zone = arena.allocate_selector74(request(&UNUSED_SINE)).unwrap();
    arena.owned_mut(zone).unwrap().state.base.position_words = Some(words(&input["position"]));
    let basis = &mut arena.actor_mut(zone).unwrap().rotating.basis;
    basis.words[0] = 1;
    for (word, bits) in basis.words[1..]
        .iter_mut()
        .zip(input["basis_bits"].as_array().unwrap())
    {
        *word = number(bits);
    }
    let state = arena.selector74_mut(zone).unwrap();
    state.radius2_138 = Some(number(&input["radius2"]));
    state.timer_13c = Some(number(&input["timer"]));
    state.circle_140 = Some(number(&input["circle"]));
    state.count_144 = Some(number(&input["count"]));
    state.linked_148 = number(&input["linked"]) as u8;
    state.links_14c = number(&input["links"]);
    state.extents_150 = Some(words(&input["extents"]));
    zone
}

/// The fields the fixture reports, as the port holds them.
fn observed(arena: &ObjectArena, zone: ObjectId) -> Value {
    let state = arena.selector74(zone).unwrap();
    serde_json::json!({
        "radius2": state.radius2_138.unwrap(),
        "timer": state.timer_13c.unwrap(),
        "circle": state.circle_140.unwrap(),
        "count": state.count_144.unwrap(),
        "linked": state.linked_148,
        "links": state.links_14c,
        "extents": state.extents_150.unwrap(),
        "extent_28": arena.object(zone).unwrap().base.word_28,
    })
}

/// The fixture's output without the call's answer.
fn expected(case: &Value) -> Value {
    let mut output = case["output"].clone();
    output.as_object_mut().unwrap().remove("eax");
    output
}

fn view(code: i32, payload: InstanceCommandPayload) -> InstanceCommandView {
    InstanceCommandView {
        target: InstanceCommandTarget::Constructor(74),
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: code,
        },
        proven_method: Some(0x0042_d6f0),
        payload_consumer: Some(0x0042_d6f0),
        payload,
    }
}

/// 0042D6F0: codes through 15 (signed) reach the shared commands; every
/// other code reads its WORD, and 16 and 17 halve it toward zero into the
/// half width and depth, 18 stores the half height, and the rest refuse.
#[test]
fn the_commands_match_the_original() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let runtime = crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    let context = || ActorCommandContext {
        runtime: &runtime,
        first_player_grants: None,
        terrain: None,
    };
    let all = cases("command");
    assert_eq!(all.len(), 30);
    for case in all {
        let input = &case["input"];
        let mut arena = arena();
        let zone = zone(&mut arena, input);
        let code = number(&input["args"][0]) as i32;
        let field = InstanceField {
            span: ByteSpan { start: 4, end: 6 },
            value: number(&input["payload"][0]) as u16,
        };
        let shared = case["events"]
            .as_array()
            .unwrap()
            .first()
            .is_some_and(|event| event["kind"] == "shared");
        assert_eq!(shared, code <= 15, "{input}");
        if shared {
            if code == 15 {
                continue; // 0047ADA0 reads a WORD operand the receipt records.
            }
            let accepted = arena
                .apply_selector74_command(
                    zone,
                    &view(code, InstanceCommandPayload::NoPayloadRead),
                    context(),
                )
                .unwrap();
            assert_eq!(accepted, (0..=15).contains(&code), "{input}");
            continue;
        }
        let payload = match code {
            16 => InstanceCommandPayload::Selector74Width(field),
            17 => InstanceCommandPayload::Selector74Depth(field),
            18 => InstanceCommandPayload::Selector74Height(field),
            _ => InstanceCommandPayload::Selector74Refused(field),
        };
        let accepted = arena
            .apply_selector74_command(zone, &view(code, payload), context())
            .unwrap();
        assert_eq!(
            u64::from(accepted),
            case["output"]["eax"].as_u64().unwrap(),
            "{input}"
        );
        assert_eq!(observed(&arena, zone), expected(&case), "{input}");
    }
}

/// 0042D780: the half width and depth from the payload's DWORDs halved
/// toward zero, the half height from its WORD, and +28 from the width.
#[test]
fn the_payload_reads_match_the_original() {
    let all = cases("payload");
    assert_eq!(all.len(), 4);
    for case in all {
        let input = &case["input"];
        let kinds: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["payload_reads"]);
        let mut arena = arena();
        let zone = zone(&mut arena, input);
        let field = |at: usize, value: u32| ConstructorField {
            span: ByteSpan::new(at, at + 4),
            value,
        };
        let payload = Selector74ConstructorPayload {
            word_44: ConstructorField {
                span: ByteSpan::new(0x44, 0x46),
                value: number(&input["payload"][0]) as u16,
            },
            dword_48: field(0x48, number(&input["payload"][1])),
            dword_4c: field(0x4c, number(&input["payload"][2])),
        };
        arena
            .initialize_selector74_payload(zone, Some(&payload))
            .unwrap();
        assert_eq!(observed(&arena, zone), expected(&case), "{input}");
    }
    let mut arena = arena();
    let zone = arena.allocate_selector74(request(&UNUSED_SINE)).unwrap();
    assert_eq!(
        arena.initialize_selector74_payload(zone, None),
        Err(ObjectError::MissingSelector74Payload(zone))
    );
}

/// 0042D950: the sphere through the corners, its root at +28, the circle
/// of the width and depth, and their sum over 800.
#[test]
fn the_finish_matches_the_original() {
    let all = cases("finish");
    assert_eq!(all.len(), 8);
    for case in all {
        let input = &case["input"];
        let mut arena = arena();
        let zone = zone(&mut arena, input);
        arena.finish_selector74_initialization(zone).unwrap();
        assert_eq!(observed(&arena, zone), expected(&case), "{input}");
    }
}

/// 0042D6B0: 7 marks the zone linked and counts a link, 8 takes one.
#[test]
fn the_messages_match_the_original() {
    let all = cases("message");
    assert_eq!(all.len(), 6);
    for case in all {
        let input = &case["input"];
        assert_eq!(case["output"]["eax"], 0);
        let mut arena = arena();
        let zone = zone(&mut arena, input);
        arena
            .selector74_message(zone, number(&input["args"][2]) as i32)
            .unwrap();
        assert_eq!(observed(&arena, zone), expected(&case), "{input}");
    }
}

/// Records the retirements without running them.
#[derive(Default)]
struct Retirements(Vec<ObjectId>);

impl Selector74Host for Retirements {
    type Error = std::convert::Infallible;
    fn retire(&mut self, _: &mut ObjectArena, actor: ObjectId) -> Result<(), Self::Error> {
        self.0.push(actor);
        Ok(())
    }
}

/// 0042D570 over the case's actors: a grenade (00404610 at +68) for an
/// answering candidate, a shared actor (00401300) for the others.
#[test]
fn the_update_matches_the_original() {
    let all = cases("update");
    assert_eq!(all.len(), 8);
    let mut retired = 0;
    for case in all {
        let input = &case["input"];
        let mut arena = arena();
        let zone = zone(&mut arena, input);
        let candidates: Vec<_> = input["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|candidate| {
                let actor = if candidate["answers"] == true {
                    arena
                        .allocate_behavior_resource(request(&UNUSED_SINE), 26)
                        .unwrap()
                } else {
                    arena.allocate_actor(request(&UNUSED_SINE)).unwrap()
                };
                arena.owned_mut(actor).unwrap().state.base.position_words =
                    Some(words(&candidate["position"]));
                actor
            })
            .collect();
        let mut host = Retirements::default();
        arena
            .tick_selector74(zone, number(&input["args"][0]), &mut host)
            .unwrap();
        let events: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| {
                assert_eq!(event["kind"], "retire");
                if event["self"] == true {
                    zone
                } else {
                    candidates[event["candidate"].as_u64().unwrap() as usize]
                }
            })
            .collect();
        assert_eq!(host.0, events, "{input}");
        retired += events.len();
        assert_eq!(observed(&arena, zone), expected(&case), "{input}");
    }
    assert_eq!(retired, 23);
}

/// An unknown timer stays unknown, and the walk still runs.
#[test]
fn an_unknown_timer_stays_unknown() {
    let input = &cases("update")[0]["input"];
    let mut arena = arena();
    let zone = zone(&mut arena, input);
    arena.selector74_mut(zone).unwrap().timer_13c = None;
    let grenade = arena
        .allocate_behavior_resource(request(&UNUSED_SINE), 26)
        .unwrap();
    arena.owned_mut(grenade).unwrap().state.base.position_words = Some(words(&input["position"]));
    let mut host = Retirements::default();
    arena.tick_selector74(zone, 125_000, &mut host).unwrap();
    assert_eq!(host.0, [grenade]);
    assert_eq!(arena.selector74(zone).unwrap().timer_13c, None);
    // With no points the draw reads no timer; with one it holds.
    assert!(arena
        .selector74_draw_points(zone, &UNUSED_SINE)
        .unwrap()
        .is_empty());
    arena.selector74_mut(zone).unwrap().count_144 = Some(1);
    assert!(matches!(
        arena.selector74_draw_points(zone, &UNUSED_SINE),
        Err(Selector74Error::TimerResidue { zone: held }) if held == zone
    ));
}

/// Only the fresh-heap carry policy starts the timer, at zero.
#[test]
fn the_fresh_heap_policy_starts_the_timer_at_zero() {
    use crate::object_runtime::ActorAllocationPolicy::*;
    for (policy, timer) in [
        (Unowned, None),
        (FreshHeapZeroCarryApproximation, None),
        (FreshHeapZeroExtendedCarryApproximation, Some(0)),
    ] {
        let mut arena = arena();
        arena.select_actor_allocation_policy(policy).unwrap();
        let zone = arena.allocate_selector74(request(&UNUSED_SINE)).unwrap();
        assert_eq!(
            arena.selector74(zone).unwrap().timer_13c,
            timer,
            "{policy:?}"
        );
    }
}

fn sine_table() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    crate::frontend_assets::FrontendAssets::open(&root)
        .unwrap()
        .sine_table()
        .values
}

/// 0042D7D0 with the real sine table and H12 height: each point round the
/// circle, clamped to the box, turned into the world, on the ground plus
/// 0x32, births particle type 2 with no flags.
#[test]
fn the_draw_matches_the_original() {
    let sine = sine_table();
    let all = cases("draw");
    assert_eq!(all.len(), 10);
    let mut births = 0;
    for case in all {
        let input = &case["input"];
        let height_scale = match number(&input["height_shift"]) {
            5 => TerrainHeightScale::Units32,
            4 => TerrainHeightScale::Units16,
            other => panic!("{other}"),
        };
        assert_eq!(number(&input["spacing_mask"]), 0x7f);
        let terrain = TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale,
            },
            0,
            std::iter::repeat(number(&input["cell"]) as u16 & 0xff).take(512 * 512),
        );
        let mut arena = arena();
        let zone = zone(&mut arena, input);
        let points: Vec<_> = arena
            .selector74_draw_points(zone, &sine)
            .unwrap()
            .into_iter()
            .map(|[x, z]| {
                serde_json::json!({
                    "kind": "birth",
                    "type": SELECTOR74_PARTICLE,
                    "point": [x, selector74_point_height(terrain.height_at([x, z])), z],
                    "arg": 0,
                })
            })
            .collect();
        assert_eq!(Value::from(points.clone()), case["events"], "{input}");
        births += points.len();
        assert_eq!(observed(&arena, zone), expected(&case), "{input}");
    }
    assert_eq!(births, 28);
}

/// Each WORD the box clamps, by the signed comparisons of 0042D880..8B7.
#[test]
fn the_clamp_keeps_the_originals_signed_edges() {
    assert_eq!(clamp(500, 300), 300);
    assert_eq!(clamp(300, 300), 300);
    assert_eq!(clamp((-300_i16) as u16, 300), (-300_i16) as u16);
    assert_eq!(clamp((-301_i16) as u16, 300), (-300_i16) as u16);
    // A negative limit sends a value above it to the limit and every other
    // one to the limit's negation.
    assert_eq!(clamp(0, (-5_i16) as u16), (-5_i16) as u16);
    assert_eq!(clamp((-9_i16) as u16, (-5_i16) as u16), 5);
    // Under the limit 0x8000 only 0x8000 itself is not above it, and it
    // fails the sign-extended test too: it takes the negation, itself.
    assert_eq!(clamp(0x7fff, 0x8000), 0x8000);
    assert_eq!(clamp(0x8000, 0x8000), 0x8000);
}
