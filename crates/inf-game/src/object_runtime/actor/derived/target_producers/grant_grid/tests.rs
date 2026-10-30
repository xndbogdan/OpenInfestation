use super::*;
use crate::object_runtime::*;
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use std::collections::VecDeque;

const SINE: [u16; 4096] = [0; 4096];
fn fixture(position: [u16; 3]) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x12345678),
    })
    .unwrap();
    let actor = arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some(position),
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
            },
            &mut 0,
            Selector12GrantContext {
                table: &inf_formats::GrantTable {
                    virtual_address: 0,
                    records: vec![],
                },
                first_player_grants: None,
            },
        )
        .unwrap();
    (arena, actor)
}
fn request() -> Selector12GrantGridRequest {
    let profile = FamilyBehaviorProfile::Selector12 {
        behavior_present: false,
    };
    Selector12GrantGridRequest::from_periodic(FamilyPeriodicProbeRequest {
        profile,
        index: 4,
        record: profile.periodic_record(4).unwrap(),
    })
    .unwrap()
}

/// Canonical synthetic H12 bytes, parsed by the real intrinsic decoder.
fn terrain(height: i8, records: &[(u16, u16, u8)]) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for i in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + i * 2] = height as u8;
    }
    for &(x, z, _) in records {
        let at = TERRAIN_PREFIX_BYTES + (usize::from(x) * 512 + usize::from(z)) * 2;
        bytes[at + 1] |= 4;
    }
    for z in 0..512_u16 {
        let mut row = records
            .iter()
            .filter(|r| r.1 == z)
            .copied()
            .collect::<Vec<_>>();
        row.sort_by_key(|r| r.0);
        for (x, _, lookup) in &row {
            bytes.extend_from_slice(&x.to_le_bytes());
            bytes.extend_from_slice(&[0, *lookup]);
        }
        let at = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + usize::from(z) * 2;
        bytes[at..at + 2].copy_from_slice(&((row.len() * 4) as u16).to_le_bytes());
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        crate::terrain_runtime::TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    )
}

struct Host {
    surfaces: Vec<TerrainRuntime>,
    acquisitions: VecDeque<Option<usize>>,
    current: Option<usize>,
    events: Vec<String>,
    acquire_count: usize,
    spacing: TerrainSpacing,
    grant: u16,
    h11_count: usize,
    fail_acquire: Option<usize>,
    null_acquire: Option<usize>,
    fail_h11: Option<usize>,
    mutation: Option<(usize, [u16; 3], i32)>,
    own_on_acquire: Option<usize>,
    published: Vec<(FamilyGrantGridPointRequest, bool)>,
}
impl Host {
    fn new(records: &[(u16, u16, u8)]) -> Self {
        Self {
            surfaces: vec![terrain(-2, &[]), terrain(99, records)],
            acquisitions: [Some(0)].into(),
            current: Some(1),
            events: vec![],
            acquire_count: 0,
            spacing: TerrainSpacing::Units256,
            grant: 1,
            h11_count: 0,
            fail_acquire: None,
            null_acquire: None,
            fail_h11: None,
            mutation: None,
            own_on_acquire: None,
            published: vec![],
        }
    }
}
impl FamilyGrantGridHost for Host {
    type Error = &'static str;
    type Terrain = usize;
    fn acquire_terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<usize>, Self::Error> {
        self.acquire_count += 1;
        self.events.push(format!("h12:{}", self.acquire_count));
        if self.fail_acquire == Some(self.acquire_count) {
            return Err("H12 custody");
        }
        if let Some((call, position, radius)) = self.mutation {
            if call == self.acquire_count {
                arena.owned_mut(actor).unwrap().state.base.position_words = Some(position);
                arena.derived_actor_mut(actor).unwrap().word_164 = radius;
            }
        }
        if self.own_on_acquire == Some(self.acquire_count) {
            let Family429e20Class::Selector12(state) =
                &mut arena.family429e20_actor_mut(actor).unwrap().class
            else {
                panic!("actual selector12 fixture");
            };
            state.grants_24c.mask_04[0] |= 2;
        }
        if self.null_acquire == Some(self.acquire_count) {
            return Ok(None);
        }
        Ok(self.acquisitions.pop_front().unwrap_or(self.current))
    }
    fn terrain(&self, source: usize) -> Result<&TerrainRuntime, Self::Error> {
        self.surfaces.get(source).ok_or("retained terrain token")
    }
    fn spacing(&mut self) -> TerrainSpacing {
        self.events.push("f0".into());
        self.spacing
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        self.events.push("f4".into());
        self.spacing
    }
    fn height_scale(&mut self) -> TerrainHeightScale {
        self.events.push("f8".into());
        TerrainHeightScale::Units16
    }
    fn placement_kind(&mut self, lookup: u8) -> Result<u8, Self::Error> {
        self.h11_count += 1;
        self.events.push(format!("h11:{lookup}"));
        if self.fail_h11 == Some(self.h11_count) {
            return Err("H11 custody");
        }
        // Actual H11 consumer uses only LOWBYTE of the row's second WORD.
        Ok((0xab03_u16) as u8)
    }
    fn furniture_grant_id(&mut self, kind: u8) -> Result<u16, Self::Error> {
        assert_eq!(kind, 3);
        self.events.push("grant:3".into());
        Ok(self.grant)
    }
    fn publish_grid_point(
        &mut self,
        arena: &mut ObjectArena,
        request: FamilyGrantGridPointRequest,
    ) -> Result<bool, Self::Error> {
        self.events.push("publish".into());
        // Real shared publisher and retained heap; ActorRegistry is the explicit
        // supplied fixture NULL source. Production uses its live process source.
        let result = arena
            .publish_family_target(
                request.actor,
                FamilyTargetSlot::Target284,
                WeightedTargetRequest {
                    referent: WeightedTargetReferent::Point(request.position_words),
                    score: request.score,
                    constructor_null: WeightedTargetConstructorNull::ActorRegistry,
                },
            )
            .map_err(|_| "target publication")?;
        self.published.push((request, result));
        Ok(result)
    }
}
fn stored_point(arena: &ObjectArena, actor: ObjectId) -> Option<[u16; 3]> {
    let pair = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target284)
        .unwrap();
    let Some(reference) = pair.target else {
        return None;
    };
    let NativeTarget::FixedPosition(target) = arena.native_target(reference).unwrap() else {
        panic!("actual point publisher");
    };
    target.focus_08
}

#[test]
fn only_literal_selector12_fifth_no_factory_record_is_admitted() {
    let actual = request().periodic();
    assert_eq!(actual.record.actor_offset, 0x284);
    assert_eq!(actual.record.callback, 0x468e30);
    assert_eq!(actual.record.input_factory, None);
    for record in [
        FamilyPeriodicRecord {
            callback: 0x468e30,
            actor_offset: 0x27c,
            input_factory: None,
        },
        FamilyPeriodicRecord {
            callback: 0x468e30,
            actor_offset: 0x284,
            input_factory: Some(0x427640),
        },
    ] {
        assert!(
            Selector12GrantGridRequest::from_periodic(FamilyPeriodicProbeRequest {
                record,
                ..actual
            })
            .is_none()
        );
    }
    let profile = FamilyBehaviorProfile::Family4236d0 { mode: 0 };
    assert!(
        Selector12GrantGridRequest::from_periodic(FamilyPeriodicProbeRequest { profile, ..actual })
            .is_none()
    );
}

#[test]
fn captured_height_and_fresh_lookup_resource_are_distinct_and_refresh_null_is_ignored() {
    let (mut arena, actor) = fixture([0; 3]);
    let mut host = Host::new(&[(0, 0, 0xfd)]);
    host.null_acquire = Some(43);
    assert!(arena
        .produce_selector12_grant_grid(actor, request(), &mut host)
        .unwrap());
    assert_eq!(stored_point(&arena, actor), Some([0, (-32_i16) as u16, 0]));
    assert_eq!(host.published[0].0.score, 6234);
    assert_eq!(host.h11_count, 1);
    assert!(host
        .events
        .windows(3)
        .any(|w| w == ["h12:43", "h11:253", "grant:3"]));
    assert_eq!(
        host.events.iter().filter(|e| e.as_str() == "f0").count(),
        84
    );
    assert_eq!(host.events.iter().filter(|e| e.as_str() == "f8").count(), 1);
    // Same-class update publisher returns false, but producer AL remains true.
    let original = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target284)
        .unwrap()
        .target;
    host.acquisitions = [Some(0)].into();
    host.null_acquire = None;
    host.published.clear();
    assert!(arena
        .produce_selector12_grant_grid(actor, request(), &mut host)
        .unwrap());
    assert!(!host.published[0].1);
    assert_eq!(
        arena
            .family_weighted_target(actor, FamilyTargetSlot::Target284)
            .unwrap()
            .target,
        original
    );
}

#[test]
fn zero_and_owned_grants_bypass_height_and_publish_null_after_whole_scan() {
    for owned in [false, true] {
        let (mut arena, actor) = fixture([0; 3]);
        let mut host = Host::new(&[(0, 0, 1)]);
        if owned {
            host.own_on_acquire = Some(2);
        } else {
            host.grant = 0;
        }
        assert!(!arena
            .produce_selector12_grant_grid(actor, request(), &mut host)
            .unwrap());
        assert_eq!(host.published[0].0.position_words, None);
        assert_eq!(host.published[0].0.score, 0);
        assert!(!host.events.iter().any(|e| e == "f8"));
        assert_eq!(host.events.last().map(String::as_str), Some("publish"));
    }
}

#[test]
fn null_target_allocation_does_not_change_positive_scan_return_or_score_prefix() {
    let (mut arena, actor) = fixture([0; 3]);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    let mut host = Host::new(&[(0, 0, 1)]);
    assert!(arena
        .produce_selector12_grant_grid(actor, request(), &mut host)
        .unwrap());
    let pair = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target284)
        .unwrap();
    assert_eq!(pair.target, None);
    assert_eq!(pair.score, 6234);
}

#[test]
fn allocation_failure_retains_reached_actual_publisher_score() {
    let (mut arena, actor) = fixture([0; 3]);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Error(71)]);
    let mut host = Host::new(&[(0, 0, 1)]);
    assert!(matches!(
        arena.produce_selector12_grant_grid(actor, request(), &mut host),
        Err(FamilyGrantGridError::Host {
            phase: "427390 Target284 publisher",
            error: "target publication"
        })
    ));
    let pair = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target284)
        .unwrap();
    assert_eq!(pair.target, None);
    assert_eq!(pair.score, 6234);
}

#[test]
fn h12_precedes_position_and_required_marker_null_precedes_h11() {
    let (mut arena, actor) = fixture([0; 3]);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let mut host = Host::new(&[]);
    assert!(matches!(
        arena.produce_selector12_grant_grid(actor, request(), &mut host),
        Err(FamilyGrantGridError::MissingPosition { offset: 0x32, .. })
    ));
    assert_eq!(host.events, ["h12:1"]);
    let mut host = Host::new(&[]);
    host.acquisitions = [None].into();
    assert!(matches!(
        terrain_furniture_grant(
            &mut arena,
            FamilyTerrainGrantRequest { actor, xz: [0; 2] },
            &mut host
        ),
        Err(FamilyGrantGridError::RequiredTerrain {
            phase: "436BC0 sparse marker"
        })
    ));
    assert_eq!(host.events, ["h12:1", "f4"]);
}

#[test]
fn sparse_marker_absence_and_zero_lookup_never_acquire_h11_or_refresh_h12() {
    for records in [vec![], vec![(0, 0, 0)]] {
        let (mut arena, actor) = fixture([0; 3]);
        let mut host = Host::new(&records);
        host.acquisitions = [Some(1)].into();
        assert_eq!(
            terrain_furniture_grant(
                &mut arena,
                FamilyTerrainGrantRequest { actor, xz: [0; 2] },
                &mut host
            )
            .unwrap(),
            0
        );
        assert_eq!(host.events, ["h12:1", "f4"]);
    }
}

#[test]
fn mutable_acquisition_keeps_scan_origin_but_rereads_xyz_radius_and_ownership() {
    let (mut arena, actor) = fixture([0; 3]);
    let mut host = Host::new(&[(0, 0, 1)]);
    host.mutation = Some((2, [100, 50, 200], 3000));
    assert!(arena
        .produce_selector12_grant_grid(actor, request(), &mut host)
        .unwrap());
    assert_eq!(
        host.published[0].0.position_words,
        Some([0, (-32_i16) as u16, 0])
    );
    assert_eq!(host.published[0].0.score, 7381);
    assert_eq!(
        host.events.iter().filter(|e| e.as_str() == "f0").count(),
        84
    );
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([100, 50, 200])
    );
}

#[test]
fn lookup_failure_keeps_old_target_and_reached_resource_prefix_without_publication() {
    let (mut arena, actor) = fixture([0; 3]);
    let mut host = Host::new(&[(0, 0, 1)]);
    arena
        .produce_selector12_grant_grid(actor, request(), &mut host)
        .unwrap();
    let before = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target284)
        .unwrap();
    let mut host = Host::new(&[(0, 0, 1), (0, 2, 2)]);
    host.fail_h11 = Some(2);
    assert!(matches!(
        arena.produce_selector12_grant_grid(actor, request(), &mut host),
        Err(FamilyGrantGridError::Host {
            phase: "436BC0 H11 row",
            error: "H11 custody"
        })
    ));
    assert_eq!(
        arena
            .family_weighted_target(actor, FamilyTargetSlot::Target284)
            .unwrap(),
        before
    );
    assert!(host.published.is_empty());
    assert_eq!(host.h11_count, 2);
    assert_eq!(host.events.last().unwrap(), "h11:2");
}

#[test]
fn missing_captured_height_and_outside_mask_grant_fail_at_the_reached_owner() {
    let (mut arena, actor) = fixture([0; 3]);
    let mut host = Host::new(&[(0, 0, 1)]);
    host.acquisitions = [None].into();
    assert!(matches!(
        arena.produce_selector12_grant_grid(actor, request(), &mut host),
        Err(FamilyGrantGridError::RequiredTerrain {
            phase: "468E30 captured height"
        })
    ));
    assert_eq!(host.h11_count, 1);
    assert!(host.published.is_empty());
    let mut host = Host::new(&[(0, 0, 1)]);
    host.grant = 192;
    assert!(matches!(
        arena.produce_selector12_grant_grid(actor, request(), &mut host),
        Err(FamilyGrantGridError::Grant(GrantClosureError::Id(192)))
    ));
    assert!(!host.events.iter().any(|e| e == "f8"));
}

#[test]
fn signed_nonpositive_score_and_equal_positive_scores_preserve_native_selection() {
    let (mut arena, actor) = fixture([0, 0, 128]);
    let mut host = Host::new(&[(0, 0, 1), (0, 2, 2)]);
    host.surfaces[0] = terrain(0, &[]);
    assert!(arena
        .produce_selector12_grant_grid(actor, request(), &mut host)
        .unwrap());
    assert_eq!(host.published[0].0.position_words, Some([0, 0, 0]));
    arena.derived_actor_mut(actor).unwrap().word_164 = -1;
    let mut host = Host::new(&[(0, 0, 1)]);
    assert!(!arena
        .produce_selector12_grant_grid(actor, request(), &mut host)
        .unwrap());
    assert_eq!(host.published[0].0.position_words, None);
}

#[test]
fn supported_fine_scan_preserves_wrapping_world_and_live_tail_spacing_reads() {
    let (mut arena, actor) = fixture([0; 3]);
    let mut host = Host::new(&[(0, 0, 1)]);
    host.spacing = TerrainSpacing::Units128;
    assert!(arena
        .produce_selector12_grant_grid(actor, request(), &mut host)
        .unwrap());
    assert_eq!(
        host.events.iter().filter(|e| e.as_str() == "f0").count(),
        364
    );
    assert_eq!(host.h11_count, 1);
    assert_eq!(stored_point(&arena, actor), Some([0, (-32_i16) as u16, 0]));
}
