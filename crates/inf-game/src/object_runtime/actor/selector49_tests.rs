//! The Seismic Stabiliser against check_selector49.py's receipt of the
//! unchanged executable: commands 004763F0, the fit 00476470 with its link
//! search 0047F770, the update 004765A0 with every terrain write, the
//! damage gate 00476720, the death 00476520 and the after-load 0047FE90.

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
};
use crate::terrain_runtime::{ProcessTerrainPolicy, TerrainSpacing};
use inf_formats::{ByteSpan, InstanceField};
use serde::Deserialize;

const SINE: [u16; 4096] = [0; 4096];
/// check_particle_births.py's current NULL 0054E894.
const NATIVE_NULL: u32 = 0x1234_5678;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    kind: String,
    position: [i32; 3],
    #[serde(default)]
    code: i64,
    #[serde(default)]
    value: i64,
    #[serde(default)]
    half: u16,
    #[serde(default)]
    sink: u16,
    #[serde(default)]
    seconds: Option<u32>,
    #[serde(default)]
    heights: Option<[u16; 4]>,
    #[serde(default)]
    left: u32,
    #[serde(default)]
    link: Option<i64>,
    #[serde(default)]
    held: u8,
    #[serde(default)]
    linked_byte: u8,
    #[serde(default)]
    delta: u32,
    #[serde(default)]
    grant: u16,
    #[serde(default)]
    linked: Option<Linked>,
    #[serde(default)]
    completed: bool,
    #[serde(default)]
    actors: Vec<NativeActor>,
    outcome: serde_json::Value,
    events: Vec<serde_json::Value>,
    fields: Fields,
}

#[derive(Deserialize)]
struct Linked {
    handle: u32,
    state: u32,
}

#[derive(Deserialize)]
struct NativeActor {
    placement: u16,
    handle: u32,
}

#[derive(Deserialize)]
struct Fields {
    words: [u16; 2],
    seconds: u32,
    heights: [u16; 4],
    left: u32,
    link: u32,
    y: u16,
    event_5483b0: u32,
    mission_24: u32,
    mission_17: u8,
}

fn receipt() -> Receipt {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector49_native.json"
    ))
    .unwrap()
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 40,
        null_handle: ActorHandle::from_raw(NATIVE_NULL),
    })
    .unwrap()
}

fn request() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
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
    }
}

/// A stabiliser holding the case's supplied fields, placed where no link
/// search finds it.
fn stabiliser(arena: &mut ObjectArena, case: &Case) -> ObjectId {
    let object = arena.allocate_selector49(request()).unwrap();
    let base = &mut arena.owned_mut(object).unwrap().state.base;
    base.position_words = Some(case.position.map(|value| value as u16));
    base.placement_word = 500;
    let actor = arena.actor_mut(object).unwrap();
    actor.word_7a = case.grant;
    actor.byte_78 = 5;
    let state = arena.selector49_mut(object).unwrap();
    state.word_158 = case.half;
    state.word_15a = case.sink;
    if let Some(seconds) = case.seconds {
        state.dword_15c = seconds;
    }
    state.heights_160 = case.heights;
    state.dword_168 = case.left;
    state.dword_16c = case.link.map(|value| value as u32);
    state.intermediate.byte_14c = case.held;
    state.intermediate.byte_14d = case.linked_byte;
    object
}

/// 00479FA0 put the object on the disposal list.
fn disposed(arena: &ObjectArena, object: ObjectId) -> bool {
    arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|node| node.object == object)
}

fn kinds(case: &Case) -> Vec<&str> {
    case.events
        .iter()
        .map(|event| event["kind"].as_str().unwrap())
        .collect()
}

/// Codes 21..24 and the refusal above them against 004763F0. Codes up to
/// 20 go to 0040D9C0, which the receipt records and the family owns.
#[test]
fn stabiliser_commands_match_004763f0() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let runtime = crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "command") {
        let code = case.code as i32;
        if code <= 20 {
            assert_eq!(kinds(case), ["family_command"], "{}", case.name);
            continue;
        }
        let mut arena = arena();
        let object = stabiliser(&mut arena, case);
        let span = ByteSpan { start: 0, end: 4 };
        let payload = match code {
            21 => InstanceCommandPayload::Selector49Word158(InstanceField {
                span,
                value: case.value as u16,
            }),
            22 => InstanceCommandPayload::Selector49Word15a(InstanceField {
                span,
                value: case.value as u16,
            }),
            23 => InstanceCommandPayload::Selector49Seconds15c(InstanceField {
                span,
                value: case.value as u32,
            }),
            24 => InstanceCommandPayload::Selector49Reference16c(InstanceField {
                span,
                value: case.value as u32,
            }),
            _ => InstanceCommandPayload::Opaque,
        };
        let view = InstanceCommandView {
            target: InstanceCommandTarget::Constructor(49),
            command_offset: 0,
            code: InstanceField { span, value: code },
            proven_method: Some(0x0047_63f0),
            payload_consumer: Some(0x0047_63f0),
            payload,
        };
        let accepted = arena
            .apply_selector49_command(
                object,
                &view,
                ActorCommandContext {
                    runtime: &runtime,
                    first_player_grants: None,
                    terrain: None,
                },
            )
            .unwrap();
        // The native result is AL; the rest of EAX is a leftover.
        assert_eq!(
            u64::from(accepted),
            case.outcome["result"].as_u64().unwrap() & 0xff,
            "{}",
            case.name
        );
        let state = arena.selector49(object).unwrap();
        assert_eq!(
            [state.word_158, state.word_15a],
            case.fields.words,
            "{}",
            case.name
        );
        assert_eq!(state.dword_15c, case.fields.seconds, "{}", case.name);
        assert_eq!(
            state.dword_16c.unwrap_or(0x5a5a_5a5a),
            case.fields.link,
            "{}",
            case.name
        );
        checked += 1;
    }
    assert_eq!(checked, 8);
}

/// Every fit: the ground read, the rounded half-width, the four heights and
/// the link each search finds, including placement -1 for reference 0 and
/// the process NULL for none.
#[test]
fn stabiliser_fits_match_00476470() {
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "fit") {
        let mut arena = arena();
        let object = stabiliser(&mut arena, case);
        let mut handles = std::collections::BTreeMap::new();
        handles.insert(NATIVE_NULL, ActorHandle::from_raw(NATIVE_NULL));
        for native in &case.actors {
            let actor = arena.allocate_actor(request()).unwrap();
            arena.owned_mut(actor).unwrap().state.base.placement_word = native.placement;
            handles.insert(native.handle, arena.actor(actor).unwrap().handle);
        }
        let height = &case.events[0];
        assert_eq!(height["kind"], "height", "{}", case.name);
        arena
            .fit_selector49::<()>(object, |[x, z]| {
                assert_eq!(
                    [u64::from(x), u64::from(z)],
                    [height["x"].as_u64().unwrap(), height["z"].as_u64().unwrap()],
                    "{}",
                    case.name
                );
                Ok(height["height"].as_u64().unwrap() as u16 as i16)
            })
            .unwrap();
        assert_eq!(kinds(case), ["height", "family_fit"], "{}", case.name);
        let state = arena.selector49(object).unwrap();
        assert_eq!(state.word_158, case.fields.words[0], "{}", case.name);
        assert_eq!(
            state.heights_160,
            Some(case.fields.heights),
            "{}",
            case.name
        );
        assert_eq!(
            state.dword_16c,
            Some(handles[&case.fields.link].raw()),
            "{}",
            case.name
        );
        checked += 1;
    }
    assert_eq!(checked, 12);
}

struct Update<'a> {
    terrain: &'a mut TerrainRuntime,
    policy: ProcessTerrainPolicy,
    events: Vec<serde_json::Value>,
}

impl Selector49UpdateHost for Update<'_> {
    type Error = ();
    fn terrain(&mut self) -> BridgeTerrain<'_> {
        BridgeTerrain {
            terrain: self.terrain,
            policy: &mut self.policy,
            detail: 0,
            world_flags: Some(0),
        }
    }
    fn status_visual(&mut self, _: &mut ObjectArena, _: ObjectId, delta: u32) -> Result<(), ()> {
        self.events
            .push(serde_json::json!({"kind": "status", "delta": delta}));
        Ok(())
    }
    fn flash(&mut self, _: &mut ObjectArena, position: [u16; 3]) -> Result<(), ()> {
        self.events.push(
            serde_json::json!({"kind": "flash", "radius": 0xf00, "style": 0, "position": position}),
        );
        Ok(())
    }
}

/// Every update: holding, stepping up, rising and capping, the terrain
/// square under the fine spacing, the height it stands at, and the
/// countdown's smoke, flash and disposal.
#[test]
fn stabiliser_updates_match_004765a0() {
    let (_resident, _assets, world) =
        crate::test_worlds::offline_world(18, TerrainSpacing::Units256);
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "update") {
        let mut arena = arena();
        let object = stabiliser(&mut arena, case);
        let mut terrain = world.terrain.clone();
        let mut host = Update {
            terrain: &mut terrain,
            policy: ProcessTerrainPolicy::supplied(TerrainSpacing::Units256),
            events: Vec::new(),
        };
        arena
            .tick_selector49(object, case.delta, &mut host)
            .unwrap();
        let state = arena.selector49(object).unwrap();
        assert_eq!(
            state.heights_160,
            Some(case.fields.heights),
            "{}",
            case.name
        );
        assert_eq!(state.dword_168, case.fields.left, "{}", case.name);
        assert_eq!(
            arena.object(object).unwrap().base.position_words.unwrap()[1],
            case.fields.y,
            "{}",
            case.name
        );
        // Smoke and flash in order; the disposal is the arena's.
        let native: Vec<_> = case
            .events
            .iter()
            .filter(|event| matches!(event["kind"].as_str(), Some("status" | "flash")))
            .cloned()
            .collect();
        assert_eq!(host.events, native, "{}", case.name);
        let native_disposed = kinds(case).contains(&"dispose");
        assert_eq!(disposed(&arena, object), native_disposed, "{}", case.name);
        // The writes all run under the fine spacing and the spacing comes
        // back; the cells match a copy given the native writes.
        let writes: Vec<_> = case
            .events
            .iter()
            .filter(|event| event["kind"] == "write")
            .collect();
        let scopes: Vec<_> = kinds(case)
            .into_iter()
            .filter(|kind| matches!(*kind, "fine" | "restore"))
            .collect();
        assert_eq!(
            scopes,
            if writes.is_empty() && !kinds(case).contains(&"fine") {
                vec![]
            } else {
                vec!["fine", "restore"]
            },
            "{}",
            case.name
        );
        let mut expected = world.terrain.clone();
        let mut policy = ProcessTerrainPolicy::supplied(TerrainSpacing::Units256);
        policy.temporary_fine(&mut expected);
        for write in &writes {
            assert_eq!(
                write["spacing"],
                serde_json::json!([0x80, 0x7f]),
                "{}",
                case.name
            );
            expected.write_raw_height(
                [
                    write["x"].as_u64().unwrap() as u16,
                    write["z"].as_u64().unwrap() as u16,
                ],
                write["height"].as_u64().unwrap() as u32 as i32,
            );
        }
        policy.temporary_fine(&mut terrain);
        let [x, _, z] = case.position.map(|value| value as u16);
        let reach = i32::from(case.half) + 0x100;
        for across in (-reach..reach).step_by(0x80) {
            for along in (-reach..reach).step_by(0x80) {
                let cell = [x.wrapping_add(across as u16), z.wrapping_add(along as u16)];
                assert_eq!(
                    terrain.raw_height_at(cell),
                    expected.raw_height_at(cell),
                    "{} cell {cell:?}",
                    case.name
                );
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 19);
}

/// The damage gate: the shared reaction only from full height.
#[test]
fn stabiliser_damage_gate_matches_00476720() {
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "damage") {
        let mut arena = arena();
        let object = stabiliser(&mut arena, case);
        assert_eq!(
            arena.selector49_admits_damage(object).unwrap(),
            kinds(case) == ["damage"],
            "{}",
            case.name
        );
        checked += 1;
    }
    assert_eq!(checked, 4);
}

#[derive(Default)]
struct Death {
    order: Vec<String>,
    micros: Option<u32>,
    event: bool,
}

impl Selector49DeathHost for Death {
    type Error = ();
    fn grant(&mut self, _: &mut ObjectArena, _: PlayerId, id: u16) -> Result<(), ()> {
        self.order.push(format!("grant {id:#x}"));
        Ok(())
    }
    fn terrain_event(&mut self) {
        self.event = true;
    }
    fn start_doom(&mut self, micros: u32) {
        self.micros = Some(micros);
    }
    fn retire(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), ()> {
        self.order.push("retire".into());
        Ok(())
    }
}

/// Every death: the grant (even of id 0), the terrain event, the wrapping
/// countdown on itself and the mission, and the linked actor's retirement
/// only alive.
#[test]
fn stabiliser_deaths_match_00476520() {
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "death") {
        let mut arena = arena();
        let object = stabiliser(&mut arena, case);
        arena.add_world_player([0; 6]);
        if let Some(linked) = &case.linked {
            let actor = arena.allocate_actor(request()).unwrap();
            arena.actor_mut(actor).unwrap().word_c4 = linked.state;
            assert_eq!(case.link, Some(i64::from(linked.handle)));
            let handle = arena.actor(actor).unwrap().handle.raw();
            arena.selector49_mut(object).unwrap().dword_16c = Some(handle);
        }
        let mut host = Death::default();
        arena.retire_selector49(object, &mut host).unwrap();
        let mut expected = Vec::new();
        for event in &case.events {
            match event["kind"].as_str().unwrap() {
                "grant" => expected.push(format!("grant {:#x}", event["id"].as_u64().unwrap())),
                "retire" => expected.push("retire".into()),
                _ => {}
            }
        }
        assert_eq!(host.order, expected, "{}", case.name);
        assert_eq!(host.event, case.fields.event_5483b0 == 1, "{}", case.name);
        assert_eq!(host.micros, Some(case.fields.mission_24), "{}", case.name);
        assert_eq!(case.fields.mission_17, 1, "{}", case.name);
        assert_eq!(
            arena.selector49(object).unwrap().dword_168,
            case.fields.left,
            "{}",
            case.name
        );
        checked += 1;
    }
    assert_eq!(checked, 5);
}

/// The after-load: a completed placement disposes the stabiliser.
#[test]
fn stabiliser_after_load_matches_0047fe90() {
    let mut checked = 0;
    for case in receipt()
        .cases
        .iter()
        .filter(|case| case.kind == "after_load")
    {
        let mut arena = arena();
        let object = stabiliser(&mut arena, case);
        let completion = if case.completed {
            [1 << 5, 0, 0, 0]
        } else {
            [0; 4]
        };
        arena.selector49_after_load(object, &completion).unwrap();
        assert_eq!(
            disposed(&arena, object),
            kinds(case).contains(&"dispose"),
            "{}",
            case.name
        );
        checked += 1;
    }
    assert_eq!(checked, 2);
}

/// The three authored stabilisers build in their worlds: leaf 17's links to
/// placement 31, leaf 23's reference names placement 39, built after it, so
/// its link is the process NULL, and leaf 27's has no command 24, so its
/// link stays unknown. Each half-width rounds down to 0x80 and each starts
/// sunk under the ground at its position.
#[test]
fn the_three_stabilisers_fit_their_worlds() {
    for (selection, placement, half, sink, seconds, grant, link) in [
        (7, 35, 512, 700, 60, 33, Some(31)),
        (13, 3, 384, 1000, 100, 49, None),
        (17, 50, 768, 1000, 25, 66, Some(usize::MAX)),
    ] {
        let (_resident, _assets, world) =
            crate::test_worlds::offline_world(selection, TerrainSpacing::Units256);
        let arena = &world.arena;
        let object = world.report.placements[placement].unwrap();
        let state = arena.selector49(object).unwrap();
        assert_eq!(
            (state.word_158, state.word_15a, state.dword_15c),
            (half, sink, seconds),
            "world {selection}"
        );
        assert_eq!(
            arena.actor(object).unwrap().word_7a,
            grant,
            "world {selection}"
        );
        let [x, _, z] = arena.object(object).unwrap().base.position_words.unwrap();
        let ground = world.terrain.height_at([x, z]) as u16;
        let sunk = ground.wrapping_sub(sink);
        assert_eq!(
            state.heights_160,
            Some([sunk, ground, sunk, sunk.wrapping_add(1)]),
            "world {selection}"
        );
        let null = arena.actor_handles().unwrap().null_handle().raw();
        match link {
            Some(usize::MAX) => assert_eq!(state.dword_16c, None, "world {selection}"),
            Some(linked) => {
                let expected = world.report.placements[linked]
                    .map(|actor| arena.actor(actor).unwrap().handle.raw())
                    .unwrap_or(null);
                assert_eq!(state.dword_16c, Some(expected), "world {selection}");
            }
            None => assert_eq!(state.dword_16c, Some(null), "world {selection}"),
        }
    }
}
