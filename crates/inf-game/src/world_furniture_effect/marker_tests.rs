//! The marker's chain removal against 00437150 under Unicorn
//! ([selector75_native.json](../../tests/fixtures/selector75_native.json),
//! `check_selector75.py`).

use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TerrainEntry,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde_json::Value;

use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};

const GRID: usize = 512;

/// The receipt's H11 entries: lookup to kind (byte +2).
struct Kinds;

impl FurnitureChainResources for Kinds {
    type Error = String;
    fn kind(&mut self, lookup: TerrainResourceLookup) -> Result<u8, String> {
        match lookup.raw() {
            1 | 2 | 5 => Ok(0x0e),
            3 => Ok(0x07),
            4 => Ok(0x10),
            other => Err(format!("lookup {other} has no receipt entry")),
        }
    }
    fn program(&mut self, _: u8) -> Result<Option<super::super::FurnitureEffectProgram>, String> {
        Err("the removal reads no program".into())
    }
}

fn number(value: &Value) -> u64 {
    value.as_u64().unwrap()
}

/// A canonical H12 entry with the case's records, each flagging its cell
/// 0x400 | 0x23 as the receipt's does.
fn source(records: &[Value]) -> TerrainEntry {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let mut buckets = vec![Vec::new(); GRID];
    for record in records {
        let [x, z, state, lookup] = [0, 1, 2, 3].map(|field| number(&record[field]));
        buckets[z as usize].push((x as u16, state as u8, lookup as u8));
        let offset = TERRAIN_PREFIX_BYTES + (x as usize * GRID + z as usize) * 2;
        bytes[offset..offset + 2].copy_from_slice(&0x0423_u16.to_le_bytes());
    }
    for (z, bucket) in buckets.iter_mut().enumerate() {
        bucket.sort();
        let offset = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + z * 2;
        bytes[offset..offset + 2].copy_from_slice(&((bucket.len() * 4) as u16).to_le_bytes());
    }
    for bucket in &buckets {
        for &(x, state, lookup) in bucket {
            bytes.extend_from_slice(&x.to_le_bytes());
            bytes.extend_from_slice(&[state, lookup]);
        }
    }
    let span = ByteSpan::new(0, bytes.len());
    parse_terrain_bank(
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
    .remove(0)
}

/// Every chain case: the count, each record's bytes after the walk and the
/// fallback's, at the fine spacing 00437110 selects.
#[test]
fn the_chain_removal_matches_the_original() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/selector75_native.json")).unwrap();
    let cases: Vec<_> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["input"]["mode"] == "chain")
        .collect();
    assert_eq!(cases.len(), 13);
    let mut removed = 0;
    for case in cases {
        let input = &case["input"];
        let records = input["records"].as_array().unwrap();
        let mut terrain = TerrainRuntime::new(
            &source(records),
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale: TerrainHeightScale::Units32,
            },
        );
        let fallback_bytes = input.get("fallback").map_or([0; 4], |bytes| {
            [0, 1, 2, 3].map(|at| number(&bytes[at]) as u8)
        });
        let mut fallback = TerrainSparsePayload {
            state: fallback_bytes[2],
            resource_lookup: TerrainResourceLookup::from_raw(fallback_bytes[3]),
        };
        let point = [0, 1].map(|axis| number(&input["point"][axis]) as u16);
        let count = remove_furniture_chain(
            &mut terrain,
            point,
            number(&input["kind"]) as u8,
            &mut fallback,
            &mut Kinds,
        )
        .unwrap();
        let output = &case["output"];
        assert_eq!(u64::from(count), number(&output["removed"]), "{input}");
        removed += count;
        let after: Vec<Value> = terrain
            .buckets()
            .iter()
            .enumerate()
            .flat_map(|(z, bucket)| {
                bucket.records.iter().map(move |record| {
                    serde_json::json!([record.x, z, record.state, record.resource_lookup.raw()])
                })
            })
            .collect();
        assert_eq!(Value::from(after), output["records"], "{input}");
        assert_eq!(
            [fallback.state, fallback.resource_lookup.raw()],
            [2, 3].map(|at| number(&output["fallback"][at]) as u8),
            "{input}"
        );
    }
    assert_eq!(removed, 76);
}

/// Helniazine's one marker, from the manager's lists.
fn helniazine_marker(
    world: &crate::world_backend::WorldSession,
) -> crate::object_runtime::ObjectId {
    use crate::object_runtime::ObjectList;
    let mut found: Vec<_> = [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
        ObjectList::Parking,
        ObjectList::Disposal,
    ]
    .into_iter()
    .flat_map(|role| world.arena.list(role).iter().map(|node| node.object))
    .filter(|&object| world.arena.selector75(object).is_ok())
    .collect();
    found.dedup();
    assert_eq!(found.len(), 1);
    found[0]
}

/// The fence's kind-0x0E records within 0xC00 of the marker, at the fine
/// spacing, that still hold their lookup.
fn standing_fence(
    world: &mut crate::world_backend::WorldSession,
    centre: [u16; 2],
    resident: &crate::ResidentRuntime,
    executable: inf_formats::FrontendExecutable<'_>,
) -> usize {
    let mut resources = ResidentFurnitureChainResources {
        resident,
        executable,
    };
    let saved = world.terrain.sampling().spacing;
    let mut policy = crate::terrain_runtime::ProcessTerrainPolicy::supplied(saved);
    policy.temporary_fine(&mut world.terrain);
    let mut standing = 0;
    let mut dx = -0xc00_i32;
    while dx <= 0xc00 {
        let mut dz = -0xc00_i32;
        while dz <= 0xc00 {
            let xz = [
                centre[0].wrapping_add(dx as u16),
                centre[1].wrapping_add(dz as u16),
            ];
            if world.terrain.cell_at(xz).has_sparse_record() {
                let (_, record) = world.terrain.sparse_record_at(xz).unwrap();
                if record.resource_lookup.raw() != 0
                    && resources.kind(record.resource_lookup).unwrap() == 0x0e
                {
                    standing += 1;
                }
            }
            dz += 0x80;
        }
        dx += 0x80;
    }
    policy.restore_detail(0, None, &mut world.terrain);
    assert_eq!(world.terrain.sampling().spacing, saved);
    standing
}

/// Infest03 and Infest04 build the marker at (39350, -1216, 19947) with the
/// fence's kind, and its finish takes serial 41 (`marker-life.js`); a fresh
/// save leaves the pad's 30 fence segments standing.
#[test]
fn helniazines_marker_takes_the_recordings_serial() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world_with_completion(6, [0; 4], TerrainSpacing::Units256);
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let marker = helniazine_marker(&world);
    assert_eq!(
        world.arena.object(marker).unwrap().base.position_words,
        Some([39350, 64320, 19947])
    );
    let state = *world.arena.selector75(marker).unwrap();
    assert_eq!(
        (state.kind_40, state.index_44, state.chain_due),
        (0x0e, 41, false)
    );
    assert_eq!(
        standing_fence(&mut world, [39350, 19947], &resident, executable),
        30
    );
}

/// A save with bit 41 (the fence gone when the world was last left) loads
/// with the marker's 00437110 having taken the whole fence down, before
/// the next placement.
#[test]
fn a_completed_marker_takes_the_fence_down_at_load() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_with_completion(
        6,
        [0, 1 << 9, 0, 0],
        TerrainSpacing::Units256,
    );
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let marker = helniazine_marker(&world);
    let state = *world.arena.selector75(marker).unwrap();
    assert_eq!((state.index_44, state.chain_due), (41, false));
    assert_eq!(
        standing_fence(&mut world, [39350, 19947], &resident, executable),
        0
    );
    assert_eq!(world.terrain.sampling().spacing, TerrainSpacing::Units256);
}

/// The cleanup leaving Helniazine: with the fence standing the removal
/// takes its 30 segments and marks nothing; with it gone, as both
/// recordings left it, the cleanup marks bit 41 with force 1 (0048EAD0(41,
/// 1) at 34BA7C in Infest03, 882436 in Infest04), past the inhibit.
#[test]
fn leaving_without_the_fence_marks_the_marker() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world_with_completion(6, [0; 4], TerrainSpacing::Units256);
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let marker = helniazine_marker(&world);
    let mut query = ProcessFurnitureQueryState::native_startup(executable).unwrap();
    let mut policy =
        crate::terrain_runtime::ProcessTerrainPolicy::supplied(TerrainSpacing::Units256);
    world.mission.inhibit_byte_08 = 1;
    let mut before = world.mission.current_world_completion().unwrap();
    for expected in [
        crate::object_runtime::Selector75Cleanup::Stood(30),
        crate::object_runtime::Selector75Cleanup::Marked,
    ] {
        let mut furniture = MarkerFurniture {
            terrain: &mut world.terrain,
            chain: FurnitureChainLoan {
                policy: &mut policy,
                detail: 0,
                world_flags: Some(world.world_flags),
                network_active: false,
            },
            resources: ResidentFurnitureChainResources {
                resident: &resident,
                executable,
            },
            query: &mut query,
        };
        assert_eq!(
            world
                .arena
                .cleanup_selector75(marker, &mut furniture, &mut world.mission)
                .unwrap(),
            expected
        );
    }
    assert_eq!(world.mission.current_world_completion().unwrap(), {
        // Only the marker's bit 41 joins the load's own records.
        assert_eq!(before[1] & 1 << 9, 0);
        before[1] |= 1 << 9;
        before
    });
}
