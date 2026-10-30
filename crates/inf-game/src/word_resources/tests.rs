use super::*;
use crate::{
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    world_start::{WorldStartContext, WorldStartError, WorldStartState},
    Installation, ResidencyRequest,
};
use inf_formats::HANDLER_COUNT;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Independent valid Base0 word-bank domain; original OVLs are never changed.
struct Words {
    root: PathBuf,
    runtime: ResidentRuntime,
}

impl Words {
    fn new(capacity: u16, global_delta: i16, words: &[u32]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-shared-h1-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut preload = vec![0, 0];
        for handler in 0..HANDLER_COUNT {
            preload.extend_from_slice(&(if handler == 1 { capacity } else { 0 }).to_le_bytes());
        }
        preload.extend_from_slice(&0_u32.to_le_bytes());
        preload.extend_from_slice(&[1, 1, 0, 0]);
        for table in 0..4 {
            for handler in 0..HANDLER_COUNT {
                let populated = table == 0 && handler == 1 && !words.is_empty();
                preload.extend_from_slice(
                    &(if populated { words.len() as i16 } else { 0 }).to_le_bytes(),
                );
                preload.extend_from_slice(&u16::from(populated).to_le_bytes());
                if populated {
                    // ResourceSegment LOWWORD is local_start=0; signed
                    // HIGHWORD is the delta added to each local index.
                    let segment = u32::from(global_delta as u16) << 16;
                    preload.extend_from_slice(&segment.to_le_bytes());
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), preload).unwrap();
        let mut overlay = Vec::new();
        if !words.is_empty() {
            overlay.extend_from_slice(&((words.len() * 4) as u32).to_le_bytes());
            for word in words {
                overlay.extend_from_slice(&word.to_le_bytes());
            }
        }
        fs::write(root.join("Overlay/0.ovl"), overlay).unwrap();
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        Self { root, runtime }
    }
}

impl Drop for Words {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn terrain() -> TerrainRuntime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 28 })
        .unwrap();
    let source = runtime.resource(12, 0).unwrap();
    let HandlerResources::Terrain(bank) =
        &source.overlay.handlers()[source.registration.region_index]
    else {
        panic!("authored mission18 must provide its actual H12 terrain")
    };
    TerrainRuntime::new(
        &bank.entries[usize::from(source.registration.local_index)],
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

#[test]
fn raw_h1_follows_registered_local_index_without_coordinate_or_heading_policy() {
    let source = Words::new(6, 2, &[0xabcd_1234, 0xbeef_ffa6]);
    let revision = source.runtime.revision();
    assert_eq!(resolve_h1_word(&source.runtime, 2).unwrap(), 0xabcd_1234);
    assert_eq!(resolve_h1_word(&source.runtime, 3).unwrap(), 0xbeef_ffa6);
    assert_eq!(
        source
            .runtime
            .resource(1, 2)
            .unwrap()
            .registration
            .local_index,
        0
    );
    assert_eq!(
        source
            .runtime
            .resource(1, 3)
            .unwrap()
            .registration
            .local_index,
        1
    );
    assert_eq!(
        resolve_h1_word(&source.runtime, 0),
        Err(H1WordError::NotResident { slot: 0 })
    );
    assert_eq!(
        resolve_h1_word(&source.runtime, 6),
        Err(H1WordError::CapacityBoundary {
            slot: 6,
            capacity: 6
        })
    );
    assert_eq!(source.runtime.revision(), revision);
}

#[test]
fn world_start_retains_signed_wrapped_slots_and_existing_error_identity() {
    let source = Words::new(4, 0, &[]);
    let terrain = terrain();
    let mut state = WorldStartState {
        pair_index: 0,
        override_position: None,
        vehicle_selection: 7,
        harness_spawn: None,
        port_spawn: None,
    };
    let context = WorldStartContext {
        state: &mut state,
        runtime: &source.runtime,
        terrain: &terrain,
    };
    assert_eq!(
        context.resolve_position(),
        Err(WorldStartError::PositionNotResident { slot: 0 })
    );
    assert_eq!(
        context.resolve_heading(),
        Err(WorldStartError::PositionNotResident { slot: 1 })
    );
    context.state.pair_index = 2;
    assert_eq!(
        context.resolve_position(),
        Err(WorldStartError::PositionSlotBoundary {
            slot: 4,
            capacity: 4
        })
    );
    assert_eq!(
        context.resolve_heading(),
        Err(WorldStartError::PositionSlotBoundary {
            slot: 5,
            capacity: 4
        })
    );
    context.state.pair_index = 0x4000;
    assert_eq!(
        context.resolve_position(),
        Err(WorldStartError::NegativePositionSlot {
            pair_index: 0x4000,
            slot: i16::MIN
        })
    );
    assert_eq!(
        context.resolve_heading(),
        Err(WorldStartError::NegativePositionSlot {
            pair_index: 0x4000,
            slot: i16::MIN + 1
        })
    );
    context.state.pair_index = 0x8000;
    assert_eq!(
        context.resolve_position(),
        Err(WorldStartError::PositionNotResident { slot: 0 })
    );
    assert_eq!(
        context.resolve_heading(),
        Err(WorldStartError::PositionNotResident { slot: 1 })
    );
}

#[test]
fn override_skips_h1_position_but_retains_independent_heading_acquisition() {
    let source = Words::new(0, 0, &[]);
    let terrain = terrain();
    let mut state = WorldStartState {
        pair_index: 0x4000,
        override_position: Some([0x8021, 0xffff, 0x00ff]),
        vehicle_selection: 7,
        harness_spawn: None,
        port_spawn: None,
    };
    let context = WorldStartContext {
        state: &mut state,
        runtime: &source.runtime,
        terrain: &terrain,
    };
    assert_eq!(
        context.resolve_position().unwrap(),
        [
            0x8021,
            (terrain.raw_height_at([0x8021, 0x00ff]) as u16).wrapping_add(60),
            0x00ff
        ]
    );
    assert_eq!(
        context.resolve_heading(),
        Err(WorldStartError::NegativePositionSlot {
            pair_index: 0x4000,
            slot: i16::MIN + 1
        })
    );
    assert_eq!(
        context.state.override_position,
        Some([0x8021, 0xffff, 0x00ff])
    );
    assert_eq!(context.state.vehicle_selection, 7);
}

#[test]
fn odd_word_is_signed_heading_even_when_the_position_is_overridden() {
    let source = Words::new(2, 0, &[0x1234_abcd, 0xdead_ffa6]);
    let terrain = terrain();
    let mut state = WorldStartState {
        pair_index: 0,
        override_position: Some([1, 2, 3]),
        vehicle_selection: 7,
        harness_spawn: None,
        port_spawn: None,
    };
    let context = WorldStartContext {
        state: &mut state,
        runtime: &source.runtime,
        terrain: &terrain,
    };
    assert_eq!(context.resolve_heading().unwrap(), [0, 0xc000, 0]);
    assert_eq!(resolve_h1_word(&source.runtime, 1).unwrap(), 0xdead_ffa6);
    assert_eq!(context.state.override_position, Some([1, 2, 3]));
}
