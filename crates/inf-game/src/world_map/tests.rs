use super::*;
use crate::world_map::markers::{Link, MarkerId, MarkerLinkField, MarkerListKind};
use crate::{
    terrain_runtime::TerrainSpacing, world_placement::WorldPlacements, Installation,
    ResidencyRequest,
};

fn runtime(loaded: bool) -> ResidentRuntime {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    if loaded {
        runtime.apply(ResidencyRequest::Boot).unwrap();
        for level in [48, 32] {
            runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        }
    }
    runtime
}

fn supplied<'a>(
    runtime: &'a ResidentRuntime,
    placements: &'a WorldPlacements<'_>,
    rng: &'a mut u32,
    plane_byte: u8,
) -> WorldMapConstructionRequest<'a> {
    WorldMapConstructionRequest {
        plane_byte,
        terrain: WorldMapTerrainInputs {
            residency: runtime,
            prepared: Some(PreparedWorldMapTerrain::new(
                placements.terrain_source(),
                placements.terrain(),
                placements.plane_height_fixed(),
            )),
        },
        rng,
    }
}

fn build(
    maps: &mut ProcessWorldMapRuntime,
    state: &mut MapLoadState,
    runtime: &ResidentRuntime,
    placements: &WorldPlacements<'_>,
    rng: &mut u32,
    plane: u8,
) -> WorldMapId {
    construct_world_map(
        MapLoadContext {
            process: maps,
            state,
        },
        supplied(runtime, placements, rng, plane),
    )
    .unwrap()
}

fn after_cells(mut rng: u32) -> u32 {
    for _ in 0..65_536 {
        rng = rng.wrapping_mul(0x13d56d).wrapping_add(0x13d573);
    }
    rng
}

fn global_members(maps: &ProcessWorldMapRuntime) -> Vec<MarkerId> {
    let mut next = maps.markers.global.head.next.unwrap();
    let mut previous = Link::Head;
    let mut members = Vec::new();
    while let Link::Marker(id) = next {
        let marker = maps.markers.markers[id.0].as_ref().unwrap();
        assert_eq!(marker.global.previous, Some(previous));
        assert_eq!(marker.global.payload, Some(Some(id)));
        members.push(id);
        previous = next;
        next = marker.global.next.unwrap();
    }
    assert_eq!(next, Link::Tail);
    assert_eq!(maps.markers.global.tail.previous, Some(previous));
    members
}

#[test]
fn complete_map_uses_current_prepared_source_grid_rng_and_paired_registration() {
    let runtime = runtime(true);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let terrain_before = placements.terrain().clone();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    assert_eq!(maps.plane_byte(), None);
    assert!(maps.grid().is_none());
    let mut rng = 0;
    let id = build(&mut maps, &mut state, &runtime, &placements, &mut rng, 0x80);
    assert_eq!(rng, 0x47b1_0000);
    assert_eq!(state.current(), Some(id));
    assert_eq!(state.pending(), None);
    assert_eq!(maps.plane_byte(), Some(0x80));
    let allocation = maps.allocation(id).unwrap();
    assert_eq!(
        (
            allocation.byte_1c,
            allocation.dword_20,
            allocation.jitter.jitter_radius()
        ),
        (0, 0, 0)
    );
    let marker = global_members(&maps)[0];
    let node = maps.markers.markers[marker.0].as_ref().unwrap();
    assert_eq!(node.owner, allocation.local);
    assert_eq!(node.local.previous, Some(Link::Head));
    assert_eq!(node.local.next, Some(Link::Tail));
    assert_eq!(node.local.payload, Some(Some(marker)));
    let local = maps.markers.locals[allocation.local.0].as_ref().unwrap();
    assert_eq!(local.head.next, Some(Link::Marker(marker)));
    assert_eq!(local.tail.previous, Some(Link::Marker(marker)));
    // The complete grid is process storage and uses the supplied plane policy,
    // not an empty completed map. Existing intrinsic tests own cell arithmetic.
    let mut expected_rng = 0;
    let expected = TerrainMap::new(
        placements.terrain(),
        TerrainMapPlane::Classify {
            height_fixed: placements.plane_height_fixed(),
        },
        &mut expected_rng,
    );
    assert_eq!(maps.grid(), Some(&expected));
    assert_eq!(placements.terrain().sampling(), terrain_before.sampling());
    assert_eq!(placements.terrain().cells(), terrain_before.cells());
    assert_eq!(placements.terrain().buckets(), terrain_before.buckets());
    assert_eq!(
        placements.terrain().plane_base(),
        terrain_before.plane_base()
    );
    assert_eq!(
        placements.terrain().slope_factors(),
        terrain_before.slope_factors()
    );
}

#[test]
fn missing_h12_retains_initialized_pending_local_and_plane_before_resource_error() {
    let loaded = runtime(true);
    let placements = WorldPlacements::new(&loaded, TerrainSpacing::Units256).unwrap();
    let missing = runtime(false);
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    let mut rng = 7;
    let previous = build(&mut maps, &mut state, &loaded, &placements, &mut rng, 0);
    let grid = maps.grid().unwrap().clone();
    let globals = maps.markers.global.clone();
    let markers = maps.markers.markers.clone();
    let previous_local = maps.markers.locals[0].clone();
    let rng_before = rng;
    assert_eq!(
        construct_world_map(
            MapLoadContext {
                process: &mut maps,
                state: &mut state
            },
            supplied(&missing, &placements, &mut rng, 0xff)
        ),
        Err(WorldMapConstructionError::RequiredTerrain)
    );
    assert_eq!(state.current(), Some(previous));
    let pending = state.pending().unwrap();
    let allocation = maps.allocation(pending).unwrap();
    assert_eq!(allocation.phase, ConstructionPhase::PlaneStored);
    assert_eq!(
        (
            allocation.byte_1c,
            allocation.dword_20,
            allocation.jitter.jitter_radius()
        ),
        (0, 0, 0)
    );
    let local = maps.markers.locals[allocation.local.0].as_ref().unwrap();
    assert_eq!(local.head.next, Some(Link::Tail));
    assert_eq!(local.tail.previous, Some(Link::Head));
    assert_eq!(maps.plane_byte(), Some(0xff));
    assert_eq!(maps.grid(), Some(&grid));
    assert_eq!(rng, rng_before);
    assert_eq!(maps.markers.global, globals);
    assert_eq!(maps.markers.markers, markers);
    assert_eq!(maps.markers.locals[0], previous_local);
    assert_eq!(
        maps.jitter(pending),
        Err(WorldMapLookupError::Pending(pending))
    );
}

#[test]
fn current_allocation_identity_and_prepared_availability_are_reached_after_prefix() {
    let first = runtime(true);
    let placements = WorldPlacements::new(&first, TerrainSpacing::Units256).unwrap();
    let replacement = runtime(true);
    for available in [false, true] {
        let mut maps = ProcessWorldMapRuntime::native_startup();
        let mut state = MapLoadState::native_mission_constructor();
        let mut rng = 123;
        let prepared = available.then(|| {
            PreparedWorldMapTerrain::new(
                placements.terrain_source(),
                placements.terrain(),
                placements.plane_height_fixed(),
            )
        });
        assert_eq!(
            construct_world_map(
                MapLoadContext {
                    process: &mut maps,
                    state: &mut state
                },
                WorldMapConstructionRequest {
                    plane_byte: 1,
                    terrain: WorldMapTerrainInputs {
                        residency: &replacement,
                        prepared
                    },
                    rng: &mut rng
                }
            ),
            Err(if available {
                WorldMapConstructionError::PreparedTerrainReplaced
            } else {
                WorldMapConstructionError::RequiredPreparedTerrain
            })
        );
        assert!(state.pending().is_some());
        assert_eq!(state.current(), None);
        assert_eq!(maps.plane_byte(), Some(1));
        assert!(maps.grid().is_none());
        assert_eq!(rng, 123);
        assert!(maps.markers.markers.is_empty());
    }
}

#[test]
fn late_global_storage_error_preserves_complete_grid_rng_and_local_insertion() {
    let runtime = runtime(true);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    // Explicit unavailable global predecessor backing: no native NULL/OOM claim.
    maps.markers.global.tail.previous = None;
    let globals = maps.markers.global.clone();
    let mut rng = 0x1234_5678;
    assert_eq!(
        construct_world_map(
            MapLoadContext {
                process: &mut maps,
                state: &mut state
            },
            supplied(&runtime, &placements, &mut rng, 0)
        ),
        Err(WorldMapConstructionError::Marker(
            MarkerStorageError::Unavailable {
                list: MarkerListKind::Global,
                field: MarkerLinkField::Previous
            }
        ))
    );
    assert_eq!(rng, after_cells(0x1234_5678));
    assert_eq!(maps.grid().unwrap().indices().len(), 65_536);
    assert_eq!(state.current(), None);
    let allocation = maps.allocation(state.pending().unwrap()).unwrap();
    assert_eq!(allocation.phase, ConstructionPhase::GridStored);
    let local = maps.markers.locals[allocation.local.0].as_ref().unwrap();
    assert_eq!(local.head.next, Some(Link::Marker(MarkerId(0))));
    assert_eq!(local.tail.previous, Some(Link::Marker(MarkerId(0))));
    let marker = maps.markers.markers[0].as_ref().unwrap();
    assert_eq!(marker.local.next, Some(Link::Tail));
    assert_eq!(marker.local.previous, Some(Link::Head));
    assert_eq!(marker.global.next, None);
    assert_eq!(marker.global.previous, None);
    assert_eq!(maps.markers.global, globals);
}

#[test]
fn stopped_pending_constructor_cannot_implicitly_allocate_or_restart_rng() {
    let runtime = runtime(false);
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    let mut rng = 456;
    assert_eq!(
        construct_world_map(
            MapLoadContext {
                process: &mut maps,
                state: &mut state
            },
            WorldMapConstructionRequest {
                plane_byte: 1,
                terrain: WorldMapTerrainInputs {
                    residency: &runtime,
                    prepared: None
                },
                rng: &mut rng
            }
        ),
        Err(WorldMapConstructionError::RequiredTerrain)
    );
    let pending = state.pending().unwrap();
    let before = format!("{maps:?} {state:?}");
    assert_eq!(
        construct_world_map(
            MapLoadContext {
                process: &mut maps,
                state: &mut state
            },
            WorldMapConstructionRequest {
                plane_byte: 0,
                terrain: WorldMapTerrainInputs {
                    residency: &runtime,
                    prepared: None
                },
                rng: &mut rng
            }
        ),
        Err(WorldMapConstructionError::PendingConstruction(pending))
    );
    assert_eq!(format!("{maps:?} {state:?}"), before);
    assert_eq!(rng, 456);
}

#[test]
fn overwrite_and_local_retirement_preserve_other_map_and_compatible_foreign_locals() {
    let runtime = runtime(true);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    // Genuine435660-compatible distinct header, shared437A90 successful phase.
    let foreign_before = maps.markers.allocate_local();
    let first_foreign = maps.markers.register(foreign_before).unwrap();
    let mut rng = 91;
    let old = build(&mut maps, &mut state, &runtime, &placements, &mut rng, 0);
    maps.jitter_mut(old)
        .unwrap()
        .apply(crate::world_map::WorldMapJitterWrite::MissionEvent48D980)
        .unwrap();
    let old_local = maps.allocation(old).unwrap().local;
    let old_header = maps.markers.locals[old_local.0].clone();
    let new = build(&mut maps, &mut state, &runtime, &placements, &mut rng, 1);
    let current_marker = global_members(&maps)[2];
    let foreign_after = maps.markers.allocate_local();
    let last_foreign = maps.markers.register(foreign_after).unwrap();
    let before_foreign_headers = [
        maps.markers.locals[foreign_before.0].clone(),
        maps.markers.locals[foreign_after.0].clone(),
    ];
    assert_eq!(state.current(), Some(new));
    assert_eq!(global_members(&maps).len(), 4);
    let grid = maps.grid().unwrap().clone();
    let plane = maps.plane_byte();
    retire_current_world_map(MapLoadContext {
        process: &mut maps,
        state: &mut state,
    })
    .unwrap();
    assert_eq!(state.current(), None);
    assert_eq!(state.pending(), None);
    let members = global_members(&maps);
    assert_eq!(members[0], first_foreign);
    assert_eq!(members[2], last_foreign);
    assert!(!members.contains(&current_marker));
    assert!(maps.markers.markers[current_marker.0].is_none());
    assert_eq!(maps.jitter(new), Err(WorldMapLookupError::Unavailable(new)));
    assert_eq!(maps.jitter(old).unwrap().jitter_radius(), 0x1fff);
    assert_eq!(maps.markers.locals[old_local.0], old_header);
    assert_eq!(
        [
            maps.markers.locals[foreign_before.0].clone(),
            maps.markers.locals[foreign_after.0].clone()
        ],
        before_foreign_headers
    );
    assert_eq!(maps.grid(), Some(&grid));
    assert_eq!(maps.plane_byte(), plane);
}

#[test]
fn cleanup_keeps_current_allocation_and_global_write_prefix_on_late_receiver_boundary() {
    let runtime = runtime(true);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    let id = build(&mut maps, &mut state, &runtime, &placements, &mut 0, 1);
    let marker = global_members(&maps)[0];
    let local = maps.allocation(id).unwrap().local;
    let local_before = maps.markers.locals[local.0].clone();
    let links_before = maps.markers.markers[marker.0]
        .as_ref()
        .unwrap()
        .local
        .clone();
    // A declared unavailable receiver allocation, not a native pointer fault
    // replay: the first global successor.prev store must remain reached.
    let unavailable = MarkerId(maps.markers.markers.len() + 10);
    maps.markers.markers[marker.0]
        .as_mut()
        .unwrap()
        .global
        .previous = Some(Link::Marker(unavailable));
    assert_eq!(
        retire_current_world_map(MapLoadContext {
            process: &mut maps,
            state: &mut state
        }),
        Err(WorldMapCleanupError::Marker(
            MarkerStorageError::MarkerUnavailable
        ))
    );
    assert_eq!(state.current(), Some(id));
    assert_eq!(
        maps.markers.global.tail.previous,
        Some(Link::Marker(unavailable))
    );
    assert_eq!(maps.markers.global.head.next, Some(Link::Marker(marker)));
    assert_eq!(maps.markers.locals[local.0], local_before);
    assert_eq!(
        maps.markers.markers[marker.0].as_ref().unwrap().local,
        links_before
    );
    assert!(maps.allocation(id).is_ok());
}

#[test]
fn cleanup_reads_payload_before_qualifying_successor_and_before_null_payload_gate() {
    let runtime = runtime(true);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    for payload in [None, Some(None)] {
        let mut maps = ProcessWorldMapRuntime::native_startup();
        let mut state = MapLoadState::native_mission_constructor();
        let id = build(&mut maps, &mut state, &runtime, &placements, &mut 0, 0);
        let local = maps.allocation(id).unwrap().local;
        let second = maps.markers.register(local).unwrap();
        maps.markers.markers[0].as_mut().unwrap().local.payload = payload;
        maps.markers.markers[second.0].as_mut().unwrap().local.next = None;
        let before = format!("{maps:?} {state:?}");
        let field = if payload.is_none() {
            MarkerLinkField::Payload
        } else {
            MarkerLinkField::Next
        };
        assert_eq!(
            retire_current_world_map(MapLoadContext {
                process: &mut maps,
                state: &mut state
            }),
            Err(WorldMapCleanupError::Marker(
                MarkerStorageError::Unavailable {
                    list: MarkerListKind::Local,
                    field
                }
            ))
        );
        assert_eq!(format!("{maps:?} {state:?}"), before);
    }
}

#[test]
fn map_lookup_authenticates_process_and_retired_identity_without_mutation() {
    let runtime = runtime(true);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let mut first = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    let id = build(&mut first, &mut state, &runtime, &placements, &mut 0, 0);
    let mut second = ProcessWorldMapRuntime::native_startup();
    let before = format!("{second:?}");
    assert_eq!(
        second.jitter(id),
        Err(WorldMapLookupError::WrongProcess(id))
    );
    assert_eq!(
        second.jitter_mut(id),
        Err(WorldMapLookupError::WrongProcess(id))
    );
    assert_eq!(format!("{second:?}"), before);
    retire_current_world_map(MapLoadContext {
        process: &mut first,
        state: &mut state,
    })
    .unwrap();
    assert_eq!(first.jitter(id), Err(WorldMapLookupError::Unavailable(id)));
    assert_eq!(
        first.jitter_mut(id),
        Err(WorldMapLookupError::Unavailable(id))
    );
}

#[test]
fn null_current_retirement_bypasses_unavailable_headers_and_pending_carrier() {
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    maps.markers.global.head.next = None;
    maps.markers.global.tail.previous = None;
    let before = format!("{maps:?} {state:?}");
    retire_current_world_map(MapLoadContext {
        process: &mut maps,
        state: &mut state,
    })
    .unwrap();
    assert_eq!(format!("{maps:?} {state:?}"), before);
    let missing = runtime(false);
    assert_eq!(
        construct_world_map(
            MapLoadContext {
                process: &mut maps,
                state: &mut state
            },
            WorldMapConstructionRequest {
                plane_byte: 1,
                terrain: WorldMapTerrainInputs {
                    residency: &missing,
                    prepared: None
                },
                rng: &mut 0
            }
        ),
        Err(WorldMapConstructionError::RequiredTerrain)
    );
    let before = format!("{maps:?} {state:?}");
    retire_current_world_map(MapLoadContext {
        process: &mut maps,
        state: &mut state,
    })
    .unwrap();
    assert_eq!(format!("{maps:?} {state:?}"), before);
}

#[test]
fn successful_local_cleanup_follows_and_frees_all_three_captured_successors() {
    let runtime = runtime(true);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    let id = build(&mut maps, &mut state, &runtime, &placements, &mut 0, 0);
    let local = maps.allocation(id).unwrap().local;
    maps.markers.register(local).unwrap();
    maps.markers.register(local).unwrap();
    let actual_members = global_members(&maps);
    assert_eq!(actual_members.len(), 3);
    retire_current_world_map(MapLoadContext {
        process: &mut maps,
        state: &mut state,
    })
    .unwrap();
    assert!(global_members(&maps).is_empty());
    assert!(actual_members
        .iter()
        .all(|id| maps.markers.markers[id.0].is_none()));
    assert!(maps.markers.locals[local.0].is_none());
    assert_eq!(state.current(), None);
    assert_eq!(maps.jitter(id), Err(WorldMapLookupError::Unavailable(id)));
}

#[test]
fn known_null_local_payload_advances_without_unlinking_it_and_retires_after_later_marker() {
    let runtime = runtime(true);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut state = MapLoadState::native_mission_constructor();
    let id = build(&mut maps, &mut state, &runtime, &placements, &mut 0, 0);
    let local = maps.allocation(id).unwrap().local;
    let null_payload_marker = global_members(&maps)[0];
    let later = maps.markers.register(local).unwrap();
    // Explicit current NULL payload domain of471C02, not a constructor
    // malloc-NULL policy or proof of a normal writer to marker+14.
    maps.markers.markers[null_payload_marker.0]
        .as_mut()
        .unwrap()
        .local
        .payload = Some(None);
    retire_current_world_map(MapLoadContext {
        process: &mut maps,
        state: &mut state,
    })
    .unwrap();
    assert_eq!(global_members(&maps), [null_payload_marker]);
    assert!(maps.markers.markers[null_payload_marker.0].is_some());
    assert!(maps.markers.markers[later.0].is_none());
    assert!(maps.markers.locals[local.0].is_none());
    assert_eq!(state.current(), None);
    // Typed logical retirement does not execute native free/alias faults or
    // attach process-global44FD30 to this surviving marker's old local links.
    assert_eq!(maps.jitter(id), Err(WorldMapLookupError::Unavailable(id)));
}
