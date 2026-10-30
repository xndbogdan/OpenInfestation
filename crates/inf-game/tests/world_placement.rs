use std::path::{Path, PathBuf};

use inf_game::{
    terrain_runtime::{TerrainHeightScale, TerrainSpacing},
    world_placement::{WorldPlacementError, WorldPlacements},
    HandlerResources, Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};
use serde::Deserialize;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn runtime() -> ResidentRuntime {
    let mut runtime = ResidentRuntime::new(Installation::open(&root()).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
}

#[test]
fn every_world_preserves_ordered_constructor_inputs_and_original_resource_bytes() {
    let mut runtime = runtime();
    let mut placement_count = 0;
    let mut scale_counts = [0; 2];
    let mut forced_coarse = 0;
    let mut prepared_diagonals = 0;
    for level in 11..=43 {
        // Retain the loader's empty-leaf release boundary for every entry.
        runtime
            .apply(ResidencyRequest::Activate { level: 48 })
            .unwrap();
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        let summary = runtime.summary();
        let revision = runtime.revision();
        let original_world = runtime.resource(14, 0).unwrap().overlay.bytes().to_vec();
        let original_terrain = runtime.resource(12, 0).unwrap().overlay.bytes().to_vec();
        for spacing in [TerrainSpacing::Units128, TerrainSpacing::Units256] {
            let prepared = WorldPlacements::new(&runtime, spacing).unwrap();
            let world = prepared.world();
            let source = prepared.world_source();
            let terrain_source = prepared.terrain_source();
            assert_eq!(
                source.registration,
                runtime.resource(14, 0).unwrap().registration
            );
            assert_eq!(
                terrain_source.registration,
                runtime.resource(12, 0).unwrap().registration
            );
            assert_eq!(source.overlay.key().level, level);
            assert_eq!(terrain_source.overlay.key().level, level);
            assert_eq!(
                world.local_index,
                u32::from(source.registration.local_index)
            );
            assert_eq!(prepared.inputs().len(), world.placements.len());
            assert!(prepared.input(world.placements.len()).is_none());
            assert!(prepared.input(usize::MAX).is_none());
            let sampling = prepared.terrain().sampling();
            assert_eq!(sampling.spacing, spacing.for_world(world.flags));
            assert_eq!(
                sampling.height_scale,
                TerrainHeightScale::from_world_flags(world.flags)
            );

            let local_index = usize::from(source.registration.local_index);
            for (authored, input) in world.placements.iter().zip(prepared.inputs()) {
                let index = authored.index as usize;
                assert_eq!(input.placement_index, authored.index);
                assert_eq!(input.resource_argument, authored.resource_argument);
                // All retail arguments preserve FFFF in their upper WORD.
                assert_eq!(input.resource_argument >> 16, 0xffff);
                assert_eq!(input.position_words[0], authored.position_words[0]);
                assert_eq!(input.position_words[2], authored.position_words[2]);
                assert_eq!(input.rotation_words, authored.constructor_rotation_words());
                assert_eq!(input.instance.placement_index, authored.index);
                assert_eq!(input.instance.suffix_offset, authored.payload_offset);
                assert!(std::ptr::eq(
                    input.instance,
                    &prepared.assets().instances[local_index].placements[index]
                ));
                assert_eq!(
                    input.position_words,
                    prepared.input(index).unwrap().position_words
                );
                // Raw prefix and ordered command selectors still address the
                // original H14 allocation, not rewritten host records.
                let prefix = authored.prefix_span.get(source.overlay.bytes()).unwrap();
                assert_eq!(
                    input.resource_argument,
                    u32::from_le_bytes(prefix[..4].try_into().unwrap())
                );
                for command in &input.instance.commands {
                    assert_eq!(
                        command.code.value,
                        i32::from_le_bytes(
                            command
                                .code
                                .span
                                .get(source.overlay.bytes())
                                .unwrap()
                                .try_into()
                                .unwrap()
                        )
                    );
                }
            }

            let HandlerResources::Terrain(bank) =
                &terrain_source.overlay.handlers()[terrain_source.registration.region_index]
            else {
                panic!("H12 registration must remain typed terrain")
            };
            let original = &bank.entries[usize::from(terrain_source.registration.local_index)];
            assert!(original
                .cells()
                .iter()
                .all(|cell| !cell.stored_alternate_diagonal()));
            for (before, after) in original.cells().iter().zip(prepared.terrain().cells()) {
                assert_eq!(
                    before.raw() & !0x800,
                    after.raw() & !0x800,
                    "preparation can only change the runtime triangle bit"
                );
                prepared_diagonals += usize::from(after.stored_alternate_diagonal());
            }
            assert_eq!(source.overlay.bytes(), original_world);
            assert_eq!(terrain_source.overlay.bytes(), original_terrain);
            if spacing == TerrainSpacing::Units128 {
                placement_count += world.placements.len();
                scale_counts[usize::from(world.flags & 0x80 != 0)] += 1;
                forced_coarse += usize::from(world.flags & 0x2000 != 0);
            }
        }
        assert_eq!(runtime.summary(), summary);
        assert_eq!(runtime.revision(), revision);
        assert_eq!(
            runtime.resource(14, 0).unwrap().overlay.bytes(),
            original_world
        );
        assert_eq!(
            runtime.resource(12, 0).unwrap().overlay.bytes(),
            original_terrain
        );
    }
    assert_eq!(placement_count, 4486);
    assert_eq!(scale_counts, [22, 11]);
    assert_eq!(forced_coarse, 2);
    assert!(prepared_diagonals > 0);
}

#[test]
fn absent_world_is_not_loaded_and_reentry_uses_the_current_registration() {
    let mut runtime = runtime();
    let before = runtime.summary();
    assert!(matches!(
        WorldPlacements::new(&runtime, TerrainSpacing::Units128),
        Err(WorldPlacementError::NotResident { handler: 14 })
    ));
    assert_eq!(runtime.summary(), before);
    for level in [32, 11, 43, 32] {
        runtime
            .apply(ResidencyRequest::Activate { level: 48 })
            .unwrap();
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        let prepared = WorldPlacements::new(&runtime, TerrainSpacing::Units128).unwrap();
        assert_eq!(prepared.world_source().registration.level, level);
        assert_eq!(prepared.terrain_source().registration.level, level);
        assert!(!prepared.world().placements.is_empty());
    }
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    assert!(matches!(
        WorldPlacements::new(&runtime, TerrainSpacing::Units256),
        Err(WorldPlacementError::NotResident { handler: 14 })
    ));
}

#[derive(Deserialize)]
struct NativeFixture {
    corpus: Vec<NativeWorld>,
}

#[derive(Deserialize)]
struct NativeWorld {
    leaf: u8,
    flags: u32,
    initial_spacing: u16,
    spacing: u16,
    count: usize,
    input_cells_fnv1a64: String,
    prepared_cells_fnv1a64: String,
    adjusted_y_words_fnv1a64: String,
}

fn word_hash(words: impl IntoIterator<Item = u16>) -> String {
    let hash = words
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{hash:016x}")
}

#[test]
fn all_initial_constructor_y_words_match_unmodified_retail_code() {
    // The receipt runs unmodified 0044B5C0 -> 004506A0 -> 00450A30
    // over the canonical raw H12 cells and H14 positions. It contains no
    // expected geometry generated by this Rust placement implementation.
    let fixture: NativeFixture =
        serde_json::from_str(include_str!("fixtures/terrain_runtime_native.json")).unwrap();
    assert_eq!(fixture.corpus.len(), 66);
    let mut runtime = runtime();
    let mut queries = 0;
    for case in fixture.corpus {
        runtime
            .apply(ResidencyRequest::Activate { level: 48 })
            .unwrap();
        runtime
            .apply(ResidencyRequest::Activate { level: case.leaf })
            .unwrap();
        let spacing = match case.initial_spacing {
            128 => TerrainSpacing::Units128,
            256 => TerrainSpacing::Units256,
            other => panic!("unsupported native fixture spacing {other}"),
        };
        let prepared = WorldPlacements::new(&runtime, spacing).unwrap();
        assert_eq!(prepared.world().flags, case.flags);
        assert_eq!(prepared.inputs().len(), case.count);
        assert_eq!(prepared.terrain().sampling().spacing.units(), case.spacing);
        let source = prepared.terrain_source();
        let HandlerResources::Terrain(bank) =
            &source.overlay.handlers()[source.registration.region_index]
        else {
            panic!("registered H12 terrain")
        };
        let raw = &bank.entries[usize::from(source.registration.local_index)];
        assert_eq!(
            word_hash(raw.cells().iter().map(|cell| cell.raw())),
            case.input_cells_fnv1a64,
            "world{} source identity",
            case.leaf
        );
        assert_eq!(
            word_hash(prepared.terrain().cells().iter().map(|cell| cell.raw())),
            case.prepared_cells_fnv1a64,
            "world{} initial spacing{} prepared terrain",
            case.leaf,
            case.initial_spacing
        );
        assert_eq!(
            word_hash(prepared.inputs().map(|input| input.position_words[1])),
            case.adjusted_y_words_fnv1a64,
            "world{} initial spacing{} adjusted constructor Y",
            case.leaf,
            case.initial_spacing
        );
        queries += case.count;
    }
    assert_eq!(queries, 4486 * 2);
}
