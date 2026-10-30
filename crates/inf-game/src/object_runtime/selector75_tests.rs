//! The furniture marker's finish and cleanup against 00435310 and 00435360
//! under Unicorn ([selector75_native.json](../../tests/fixtures/selector75_native.json),
//! `check_selector75.py`).

use inf_formats::{
    parse_terrain_bank, ByteSpan, FrontendExecutable, HandlerFraming, HandlerId, HandlerRegion,
    MissionSaveState, TerrainResourceLookup, TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES,
    TERRAIN_PREFIX_BYTES,
};
use serde_json::Value;

use super::*;
use crate::object_runtime::{ObjectClassKind, ObjectList};
use crate::terrain_runtime::{
    ProcessTerrainPolicy, TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing,
};
use crate::world_furniture_effect::{
    FurnitureChainLoan, FurnitureEffectProgram, ProcessFurnitureQueryState,
};
use crate::world_mission::WorldMissionInputs;

fn cases(mode: &str) -> Vec<Value> {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/selector75_native.json")).unwrap();
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
    std::array::from_fn(|axis| number(&value[axis]) as u16)
}

fn completion(value: &Value) -> [u32; 4] {
    std::array::from_fn(|word| number(&value[word]))
}

/// A marker as the case's input left it, with the loader's serial and
/// completion words.
fn marker(input: &Value) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::new();
    let marker = arena.allocate(ObjectClassKind::Selector75);
    {
        let base = &mut arena.owned_mut(marker).unwrap().state.base;
        base.placement_word = number(&input["placement"]) as u16;
        base.position_words = Some(words(&input["position"]));
    }
    let state = arena.selector75_mut(marker).unwrap();
    assert_eq!((state.kind_40, state.index_44), (0x0e, 0xff));
    state.kind_40 = number(&input["kind"]);
    state.index_44 = number(&input["index"]) as u8;
    arena.set_world_completion(completion(&input["completion"]));
    arena.world_load.actor_serial_3d0 = number(&input["serial"]);
    (arena, marker)
}

/// 00435310: -1 disposes it; otherwise the serial's low byte, and the chain
/// when that bit is set.
#[test]
fn the_finish_matches_the_original() {
    let all = cases("finish");
    assert_eq!(all.len(), 10);
    for case in all {
        let input = &case["input"];
        let output = &case["output"];
        let (mut arena, marker) = marker(input);
        arena.finish_selector75(marker).unwrap();
        let disposed = arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|node| node.object == marker);
        let chain = arena.take_selector75_chain(marker).unwrap();
        let mut events = Vec::new();
        if disposed {
            events.push(serde_json::json!({"kind": "dispose", "self": true}));
        }
        if let Some(([x, z], kind)) = chain {
            let [px, py, pz] = words(&input["position"]);
            assert_eq!([x, z], [px, pz]);
            events.push(serde_json::json!({
                "kind": "chain", "point": [px, py, pz], "kind_dl": kind, "self_position": true,
            }));
        }
        assert_eq!(Value::from(events), output["events"], "{input}");
        let state = arena.selector75(marker).unwrap();
        assert_eq!(
            u32::from(state.index_44),
            number(&output["index"]),
            "{input}"
        );
        assert_eq!(
            arena.world_load.actor_serial_3d0,
            number(&output["serial"]),
            "{input}"
        );
        // Taken once.
        assert_eq!(arena.take_selector75_chain(marker).unwrap(), None);
    }
}

/// H11 lookup 1 has the fence's kind.
struct Kinds;

impl FurnitureChainResources for Kinds {
    type Error = String;
    fn kind(&mut self, lookup: TerrainResourceLookup) -> Result<u8, String> {
        Ok(if lookup.raw() == 1 { 0x0e } else { 0x07 })
    }
    fn program(&mut self, _: u8) -> Result<Option<FurnitureEffectProgram>, String> {
        Err("the removal reads no program".into())
    }
}

/// A terrain whose fence runs `count` cells along Z from the marker's cell,
/// so the removal there takes exactly `count`.
fn fence_terrain(position: [u16; 3], count: usize) -> TerrainRuntime {
    let [x, _, z] = position.map(|word| usize::from(word >> 7));
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for row in 0..512 {
        let size: u16 = if (z..z + count).contains(&row) { 4 } else { 0 };
        let offset = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + row * 2;
        bytes[offset..offset + 2].copy_from_slice(&size.to_le_bytes());
    }
    for row in z..z + count {
        let offset = TERRAIN_PREFIX_BYTES + (x * 512 + row) * 2;
        bytes[offset..offset + 2].copy_from_slice(&0x0400_u16.to_le_bytes());
        bytes.extend_from_slice(&(x as u16).to_le_bytes());
        bytes.extend_from_slice(&[0x40, 1]);
    }
    let span = ByteSpan::new(0, bytes.len());
    let entry = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap()
    .entries
    .remove(0);
    TerrainRuntime::new(
        &entry,
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

/// 00435360 over a fence of the receipt's count: none left marks +44
/// through 0048EAD0 with force 1, whatever the cleanup's inhibit.
#[test]
fn the_cleanup_matches_the_original() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let all = cases("cleanup");
    assert_eq!(all.len(), 6);
    for case in all {
        let input = &case["input"];
        let output = &case["output"];
        let (mut arena, marker) = marker(input);
        let position = words(&input["position"]);
        let mut terrain = fence_terrain(position, number(&input["removed"]) as usize);
        let mut policy = ProcessTerrainPolicy::supplied(TerrainSpacing::Units256);
        let mut query = ProcessFurnitureQueryState::native_startup(executable).unwrap();
        let mut furniture = MarkerFurniture {
            terrain: &mut terrain,
            chain: FurnitureChainLoan {
                policy: &mut policy,
                detail: 0,
                world_flags: None,
                network_active: false,
            },
            resources: Kinds,
            query: &mut query,
        };
        let mut save = MissionSaveState::new_game();
        let world = number(&input["world"]) as usize;
        save.owner_240[world * 4..world * 4 + 4].copy_from_slice(&completion(&input["completion"]));
        let mut mission = WorldMission::new(
            save,
            WorldMissionInputs {
                current_world_a0: world as u32,
                dword_1c: 0,
                byte_13: None,
                record_0c: None,
            },
        );
        mission.inhibit_byte_08 = number(&input["inhibit"]) as u8;
        let cleanup = arena
            .cleanup_selector75(marker, &mut furniture, &mut mission)
            .unwrap();
        let kinds: Vec<_> = output["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["kind"].as_str().unwrap())
            .collect();
        let removed = number(&input["removed"]);
        if removed == 0 {
            assert_eq!(kinds, ["chain", "mark"], "{input}");
            assert_eq!(cleanup, Selector75Cleanup::Marked);
        } else {
            assert_eq!(kinds, ["chain"], "{input}");
            assert_eq!(cleanup, Selector75Cleanup::Stood(removed));
        }
        // The removal ran at the fine spacing and gave the detail's back.
        assert_eq!(terrain.sampling().spacing, TerrainSpacing::Units256);
        assert_eq!(
            mission.current_world_completion().unwrap(),
            completion(&output["completion"]),
            "{input}"
        );
    }
}
