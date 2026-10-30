//! Real load handoff and explicitly supplied resident-producer failure domains.
//! These tests do not claim the complete native loader or avoidance collision.
use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::terrain_runtime::{
    produce_resident_furniture_radius, ProcessFurnitureRadius, TerrainFurnitureRadiusError,
    TerrainFurnitureRadiusRequest, TerrainRuntime, TerrainSparsePayload, TerrainSparseRecordId,
};
use crate::world_furniture_effect::resident::{
    resident_furniture_model_radius, resident_placement_table, ResidentFurnitureResourceError,
};
use inf_formats::{FrontendExecutable, TerrainRecord, TerrainResourceLookup};
use std::path::{Path, PathBuf};

fn retail_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

struct RadiusLoadFixture {
    assets: FrontendAssets,
    resident: ResidentRuntime,
    radius: ProcessFurnitureRadius,
    process: ParticleFactoryProcessState,
    maps: crate::world_map::ProcessWorldMapRuntime,
    map_load: crate::world_map::MapLoadState,
    rng: u32,
}

impl RadiusLoadFixture {
    fn new() -> Self {
        let root = retail_root();
        let assets = FrontendAssets::open(&root).unwrap();
        let radius = ProcessFurnitureRadius::native_startup(
            FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        resident
            .apply(ResidencyRequest::Select {
                selection: crate::residency::ResourceSelection {
                    difficulty: 2,
                    language: 0,
                },
            })
            .unwrap();
        resident.apply(ResidencyRequest::Boot).unwrap();
        resident
            .apply(ResidencyRequest::Activate { level: 47 })
            .unwrap();
        Self {
            assets,
            resident,
            radius,
            process: ParticleFactoryProcessState::native_startup(),
            maps: crate::world_map::ProcessWorldMapRuntime::native_startup(),
            map_load: crate::world_map::MapLoadState::native_mission_constructor(),
            rng: 0x1234_5678,
        }
    }

    fn load(&mut self, selection: u32) -> (WorldLoadOwner, WorldLoadMetadata) {
        let executable = FrontendExecutable::parse(self.assets.executable_bytes()).unwrap();
        let grants = inf_formats::parse_grant_table(executable).unwrap();
        let mut offline = crate::offline_prepare::offline_game(&grants).unwrap();
        offline.mission.mission_selection = selection;
        // Actual saved-import constructor inputs, matching the existing corpus
        // loader. This is not a campaign transition or a captured actor pose.
        let mut owner = WorldLoadOwner::import_saved(OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        })
        .unwrap();
        let mut effects = EmptyListenerEffects;
        let metadata = load_offline_world(
            &mut self.resident,
            OfflineWorldLoadInputs {
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                    crate::terrain_runtime::TerrainSpacing::Units256,
                ),
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                owner: &mut owner,
                assets: &self.assets,
                process: Some(&self.process),
                furniture_radius: &mut self.radius,
                map: crate::world_map::MapLoadContext {
                    process: &mut self.maps,
                    state: &mut self.map_load,
                },
                rng: &mut self.rng,
                reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
                listener_effects: &mut effects,
                policy: OfflineWorldLoadPolicy::CleanImport,
                particles: None,
                phase: None,
                furniture: None,
                settings_84: 0,
                ultra: None,
            },
        )
        .unwrap();
        (owner, metadata)
    }
}

fn records(terrain: &TerrainRuntime) -> Vec<(TerrainSparseRecordId, TerrainRecord)> {
    terrain
        .buckets()
        .iter()
        .enumerate()
        .flat_map(|(bucket, rows)| {
            rows.records.iter().enumerate().map(move |(index, record)| {
                (
                    TerrainSparseRecordId {
                        bucket: bucket as u16,
                        index,
                    },
                    *record,
                )
            })
        })
        .collect()
}

#[test]
fn real_intro_and_level1_loads_publish_from_current_h12_h11_h8_into_one_process_word() {
    let mut fixture = RadiusLoadFixture::new();
    assert_eq!(
        fixture.radius.word(),
        0,
        "strict PE loader proof, once at startup"
    );
    let identity = fixture.radius.word_mut() as *mut u16;
    let mut terrain_allocations = Vec::new();
    for (selection, leaf) in [(22, 32), (18, 28)] {
        // A supplied retained WORD distinguishes an omitted producer from a
        // correct fresh 4508E0 scan. This is test history, not native startup.
        *fixture.radius.word_mut() = 0xa55a;
        let (owner, metadata) = fixture.load(selection);
        assert_eq!(fixture.radius.word_mut() as *mut u16, identity);
        assert!(matches!(owner.load_status, WorldLoadStatus::Complete));
        assert!(!metadata.report.placements.is_empty());
        assert_eq!(fixture.map_load.current(), Some(metadata.map));
        let prepared = owner.prepared.as_ref().unwrap();
        let h12 = fixture.resident.resource(12, 0).unwrap();
        let h11 = fixture.resident.resource(11, 0).unwrap();
        assert!(prepared.terrain_source.is_current(&h12));
        assert_eq!(h12.registration.level, leaf);
        terrain_allocations.push(h12.overlay.path().to_path_buf());
        let table = resident_placement_table(&fixture.resident).unwrap();
        let mut reached_authored_model = false;
        for (_, record) in records(&prepared.terrain) {
            assert_ne!(record.resource_lookup.raw(), 0);
            let slot = &table.slots[usize::from(record.resource_lookup.raw())];
            if let Some(resource) = fixture.resident.resource(8, slot.resource_word) {
                let crate::HandlerResources::Models(bank) =
                    &resource.overlay.handlers()[resource.registration.region_index]
                else {
                    panic!("current H8 registration must select its model bank")
                };
                let model = &bank.entries[usize::from(resource.registration.local_index)];
                assert_eq!(
                    resident_furniture_model_radius(&fixture.resident, record.resource_lookup,)
                        .unwrap(),
                    Some(model.header.raw_0e)
                );
                reached_authored_model = true;
            }
        }
        assert!(
            reached_authored_model,
            "real sparse lookups reached current H11/H8"
        );
        assert!(matches!(
            &h11.overlay.handlers()[h11.registration.region_index],
            crate::HandlerResources::MissionPlacements(_)
        ));
        let mut same_h12 = prepared.terrain.clone();
        let before = format!("{same_h12:?}");
        let mut reference = 0xbeef;
        produce_resident_furniture_radius(
            TerrainFurnitureRadiusRequest::new(&mut same_h12, &mut reference),
            &fixture.resident,
        )
        .unwrap();
        assert_eq!(fixture.radius.word(), reference);
        assert_eq!(
            format!("{same_h12:?}"),
            before,
            "radius does not alter prepared H12"
        );
        assert_eq!(format!("{:?}", prepared.terrain), before);
    }
    assert_ne!(terrain_allocations[0], terrain_allocations[1]);
    // The same process owner was lent to two genuine loads. New saved-import
    // managers do not test retained-manager cleanup or a campaign transition.
}

#[test]
fn required_current_h11_failure_retains_the_real_process_entry_reset() {
    let mut fixture = RadiusLoadFixture::new();
    let (owner, _) = fixture.load(22);
    let mut terrain = owner.prepared.unwrap().terrain;
    let first = records(&terrain).first().unwrap().0;
    let before = format!("{terrain:?}");
    // Explicit supplied missing-residency domain after a real H12 load. No
    // original OVL is changed and this is not a backend failure injection.
    let missing = ResidentRuntime::new(Installation::open(&retail_root()).unwrap()).unwrap();
    assert!(missing.resource(11, 0).is_none());
    *fixture.radius.word_mut() = 600;
    let error = produce_resident_furniture_radius(
        TerrainFurnitureRadiusRequest::new(&mut terrain, fixture.radius.word_mut()),
        &missing,
    )
    .unwrap_err();
    assert!(matches!(error, TerrainFurnitureRadiusError::Resolver {
        record, error: ResidentFurnitureResourceError::RequiredH11,
    } if record == first));
    assert_eq!(fixture.radius.word(), 0);
    assert_eq!(format!("{terrain:?}"), before);
}

#[test]
fn supplied_later_zero_payload_preserves_actual_prior_model_max_before_normal_completion() {
    let mut fixture = RadiusLoadFixture::new();
    let (owner, _) = fixture.load(22);
    let mut terrain = owner.prepared.unwrap().terrain;
    let ordered = records(&terrain);
    let stop_index = ordered
        .iter()
        .enumerate()
        .find_map(|(index, (_, record))| {
            let radius =
                resident_furniture_model_radius(&fixture.resident, record.resource_lookup).unwrap();
            (radius.is_some_and(|word| word != 0) && index + 1 < ordered.len()).then_some(index + 1)
        })
        .expect("real Intro H12 has a positive resolved model before another record");
    let mut uncapped_prefix = 0_u16;
    for (_, record) in &ordered[..stop_index] {
        if let Some(radius) =
            resident_furniture_model_radius(&fixture.resident, record.resource_lookup).unwrap()
        {
            if i32::from(radius) > i32::from(uncapped_prefix as i16) {
                uncapped_prefix = radius;
            }
        }
    }
    assert_ne!(uncapped_prefix, 0);
    let (stop, original) = ordered[stop_index];
    // Only the existing mutable runtime BYTE3 is supplied as zero; loader-fixed
    // bucket counts, X words, coordinates and original OVL bytes are retained.
    terrain
        .write_sparse_payload(
            stop,
            TerrainSparsePayload {
                state: original.state,
                resource_lookup: TerrainResourceLookup::from_raw(0),
            },
        )
        .unwrap();
    let before = terrain.buckets().to_vec();
    *fixture.radius.word_mut() = 0xa55a;
    let error = produce_resident_furniture_radius(
        TerrainFurnitureRadiusRequest::new(&mut terrain, fixture.radius.word_mut()),
        &fixture.resident,
    )
    .unwrap_err();
    assert!(matches!(error, TerrainFurnitureRadiusError::ZeroLookup { record } if record == stop));
    assert_eq!(fixture.radius.word(), uncapped_prefix);
    assert_eq!(terrain.buckets(), before.as_slice());
    // This concrete resource scan proves reset/earlier writes. The dedicated
    // intrinsic fixtures discriminate >512 error prefixes from the normal cap.
    // No load error/map/placement nonexecution witness is inferred here.
    terrain
        .write_sparse_payload(
            stop,
            TerrainSparsePayload {
                state: original.state,
                resource_lookup: original.resource_lookup,
            },
        )
        .unwrap();
    produce_resident_furniture_radius(
        TerrainFurnitureRadiusRequest::new(&mut terrain, fixture.radius.word_mut()),
        &fixture.resident,
    )
    .unwrap();
    let mut reference = 0;
    let mut same_h12 = terrain.clone();
    produce_resident_furniture_radius(
        TerrainFurnitureRadiusRequest::new(&mut same_h12, &mut reference),
        &fixture.resident,
    )
    .unwrap();
    assert_eq!(fixture.radius.word(), reference);
}
