use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest, FamilyTerrainProbeMissHost, Selector12GrantContext,
    },
    terrain_runtime::{TerrainHeightScale, TerrainSampling},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use std::cell::RefCell;

fn assets() -> FrontendAssets {
    FrontendAssets::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"))
        .unwrap()
}
fn owners() -> (ProcessFurnitureQueryState, ProcessFurnitureRadius) {
    let a = assets();
    let e = inf_formats::FrontendExecutable::parse(a.executable_bytes()).unwrap();
    (
        ProcessFurnitureQueryState::native_startup(e).unwrap(),
        ProcessFurnitureRadius::native_startup(e).unwrap(),
    )
}
///Canonical H12 bytes supplied by this fixture; no original asset is changed.
fn terrain(sparse: bool, lookup: u8) -> TerrainRuntime {
    let mut raw = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        raw[TERRAIN_PREFIX_BYTES + index * 2] = 2;
    }
    //First cell of the five-by-five window around1024 with112+512 radius.
    if sparse {
        raw[TERRAIN_PREFIX_BYTES + (4 * 512 + 4) * 2 + 1] = 4;
    }
    for z in 0..512usize {
        if sparse && z == 4 {
            raw[TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + z * 2
                ..TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + z * 2 + 2]
                .copy_from_slice(&4u16.to_le_bytes());
            raw.extend_from_slice(&[4, 0, 0x27, lookup]);
        }
    }
    let span = ByteSpan::new(0, raw.len());
    let bank = parse_terrain_bank(
        &raw,
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
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}
fn arena(position: Option<[u16; 3]>) -> (ObjectArena, ObjectId) {
    let mut a = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let sine = assets().sine_table().values;
    let grants = inf_formats::GrantTable {
        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let id = a
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: position,
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &sine,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap();
    a.actor_mut(id).unwrap().rotating.visual.model_word = 223;
    (a, id)
}
#[derive(Clone, Copy)]
struct Model {
    slot: u16,
    radius: u16,
}
#[derive(Debug, PartialEq, Eq)]
enum Error {
    H11,
    Active(FurnitureCollisionPairState),
    Basis,
}
#[derive(Clone, Copy)]
enum PairFixture {
    Required,
    Return(CollisionContact),
}
struct Host {
    terrain: TerrainRuntime,
    captured: bool,
    actor_radius: Option<u16>,
    candidate: Option<u16>,
    candidate_slot: u16,
    fail_placement: bool,
    fail_basis: bool,
    basis_position: Option<[u16; 3]>,
    change_state: bool,
    events: RefCell<Vec<String>>,
    pair: Option<FurnitureCollisionPairState>,
    pair_fixture: PairFixture,
}
impl Host {
    fn new(sparse: bool, lookup: u8) -> Self {
        Self {
            terrain: terrain(sparse, lookup),
            captured: true,
            actor_radius: Some(112),
            candidate: Some(10),
            candidate_slot: 7,
            fail_placement: false,
            fail_basis: false,
            basis_position: None,
            change_state: false,
            events: RefCell::new(Vec::new()),
            pair: None,
            pair_fixture: PairFixture::Required,
        }
    }
    fn event(&self, s: impl Into<String>) {
        self.events.borrow_mut().push(s.into());
    }
}
impl FurnitureCollisionPrefixHost for Host {
    type Error = Error;
    type Terrain = u8;
    type Model = Model;
    fn acquire_terrain(&mut self) -> Result<Option<u8>, Error> {
        self.event("H12");
        Ok(self.captured.then_some(0))
    }
    fn terrain(&self, t: u8) -> Result<&TerrainRuntime, Error> {
        assert_eq!(t, 0);
        self.event("captured");
        Ok(&self.terrain)
    }
    fn spacing(&mut self) -> TerrainSpacing {
        self.event("F0");
        TerrainSpacing::Units256
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        self.event("F4");
        TerrainSpacing::Units256
    }
    fn height_shift(&mut self) -> u32 {
        self.event("F8");
        5
    }
    fn model(&mut self, slot: u16) -> Result<Option<Model>, Error> {
        self.event(format!("H8:{slot}"));
        Ok(if slot == 223 {
            self.actor_radius.map(|radius| Model { slot, radius })
        } else if (slot as i16) < 0 {
            None
        } else {
            self.candidate.map(|radius| Model { slot, radius })
        })
    }
    fn model_radius(&self, m: &Model) -> u16 {
        self.event(format!("radius:{}", m.slot));
        m.radius
    }
    fn placement_model(&mut self, l: TerrainResourceLookup) -> Result<u16, Error> {
        self.event(format!("H11:{}", l.raw()));
        if self.fail_placement {
            return Err(Error::H11);
        }
        if self.change_state {
            self.terrain
                .write_sparse_payload(
                    TerrainSparseRecordId {
                        bucket: 4,
                        index: 0,
                    },
                    crate::terrain_runtime::TerrainSparsePayload {
                        state: 0xa7,
                        resource_lookup: l,
                    },
                )
                .unwrap();
        }
        Ok(self.candidate_slot)
    }
    fn actor_basis(&mut self, a: &mut ObjectArena, id: ObjectId) -> Result<NativeBasis, Error> {
        self.event("actor48");
        if self.fail_basis {
            return Err(Error::Basis);
        }
        if let Some(p) = self.basis_position {
            a.owned_mut(id).unwrap().state.base.position_words = Some(p)
        }
        Ok(a.actor(id).unwrap().rotating.basis)
    }
    fn shared_basis(&mut self) -> NativeBasis {
        self.event("sharedbasis");
        let mut b = NativeBasis::INITIAL;
        b.words[0] = 17;
        b
    }
    fn furniture_basis(&mut self, state: u8) -> NativeBasis {
        self.event(format!("furniturebasis:{state}"));
        let mut b = NativeBasis::INITIAL;
        b.words[0] = u32::from(state);
        b
    }
    fn qualify_record(&self, _: u8, id: TerrainSparseRecordId) -> FurnitureQueryRecord {
        self.event(format!("record:{}:{}", id.bucket, id.index));
        FurnitureQueryRecord::NativeAddress(0x12345678)
    }
    fn active_pair(
        &mut self,
        r: FurnitureCollisionPair<'_, Model>,
        p: &mut ProcessFurnitureQueryState,
    ) -> Result<CollisionContact, Error> {
        self.event("pair");
        assert_eq!(r.actor_root.slot, 223);
        assert_eq!(r.counterpart.slot, 7);
        assert!(p.pair_alias_is_546b48());
        self.pair = Some(r.state);
        match self.pair_fixture {
            PairFixture::Required => Err(Error::Active(r.state)),
            PairFixture::Return(contact) => Ok(contact),
        }
    }
    fn respond_contact(
        &mut self,
        _: &mut ObjectArena,
        _: FurnitureCollisionPairState,
        _: CollisionContact,
    ) -> Result<u8, Error> {
        panic!("this fixture explicitly stops at ACTIVE before response")
    }
}
fn run(
    a: &mut ObjectArena,
    id: ObjectId,
    p: &mut ProcessFurnitureQueryState,
    r: &ProcessFurnitureRadius,
    h: &mut Host,
) -> Result<(), FurnitureCollisionPrefixError<Error>> {
    collide_furniture_prefix(
        a,
        FurnitureCollisionPrefixRequest {
            actor: id,
            process: p,
            radius: r,
            exit_registration: LiteralRetExitRegistrationApproximation,
        },
        h,
    )
}

#[test]
fn missing_actor_header_is_required_before_zero_process_radius_and_keeps_init() {
    let (mut a, id) = arena(None);
    let (mut p, r) = owners();
    let mut h = Host::new(false, 0);
    h.captured = false;
    h.actor_radius = None;
    assert!(
        matches!(run(&mut a,id,&mut p,&r,&mut h),Err(FurnitureCollisionPrefixError::RequiredActorModel{actor,model_word:223})if actor==id)
    );
    assert_eq!(p.query_init_mask(), 1);
    assert_eq!(*p.query_bases(), [NativeBasis::INITIAL; 5]);
    assert!(p.literal_ret_registration_attempted());
    assert_eq!(*h.events.borrow(), ["H12", "H8:223"]);
}
#[test]
fn genuine_header_or_process_zero_skips_xyz_and_null_captured_terrain() {
    for (header, process) in [(0, 512), (112, 0)] {
        let (mut a, id) = arena(None);
        let (mut p, mut r) = owners();
        *r.word_mut() = process;
        let mut h = Host::new(false, 0);
        h.captured = false;
        h.actor_radius = Some(header);
        run(&mut a, id, &mut p, &r, &mut h).unwrap();
        assert_eq!(*h.events.borrow(), ["H12", "H8:223", "radius:223"]);
        assert!(p.literal_ret_registration_attempted());
        assert_eq!(a.object(id).unwrap().base.position_words, None);
    }
}
#[test]
fn zero_lookup_window_is_real_normal_return_and_outer_reuses_last_inner_spacing() {
    let (mut a, id) = arena(Some([1024, 777, 1024]));
    let before = format!("{a:?}");
    let (mut p, mut r) = owners();
    *r.word_mut() = 512;
    let record = p.current_record().clone();
    let mut h = Host::new(false, 0);
    run(&mut a, id, &mut p, &r, &mut h).unwrap();
    let e = h.events.borrow();
    assert_eq!(e.iter().filter(|s| s.as_str() == "F0").count(), 28); //initial+two count divisors+25 cells, no extra outer reads.
    assert_eq!(e.iter().filter(|s| s.as_str() == "F4").count(), 26);
    assert!(!e
        .iter()
        .any(|s| s.starts_with("H11") || s == "actor48" || s == "pair"));
    assert_eq!(format!("{a:?}"), before);
    assert_eq!(p.current_record(), &record);
    assert!(!p.pair_alias_is_546b48());
}
#[test]
fn null_captured_h12_fault_is_late_after_model_window_and_init() {
    let (mut a, id) = arena(Some([1024, 64, 1024]));
    let (mut p, mut r) = owners();
    *r.word_mut() = 512;
    let mut h = Host::new(false, 0);
    h.captured = false;
    assert!(matches!(
        run(&mut a, id, &mut p, &r, &mut h),
        Err(FurnitureCollisionPrefixError::RequiredCapturedTerrain)
    ));
    assert_eq!(
        *h.events.borrow(),
        ["H12", "H8:223", "radius:223", "F0", "F4", "F0", "F0", "F4"]
    );
    assert_eq!(p.query_init_mask(), 1);
}
#[test]
fn negative_process_word_combines_with_unsigned_header_before_word_window() {
    let (mut a, id) = arena(Some([1024, 64, 1024]));
    let (mut p, mut r) = owners();
    *r.word_mut() = 0xffff;
    let mut h = Host::new(false, 0);
    h.actor_radius = Some(1);
    run(&mut a, id, &mut p, &r, &mut h).unwrap();
    assert_eq!(
        h.events
            .borrow()
            .iter()
            .filter(|s| s.as_str() == "F0")
            .count(),
        4
    ); //combined radius0 still scans one cell.
}
#[test]
fn missing_h11_stops_before_saved_actor_basis_and_current_record_publication() {
    let (mut a, id) = arena(Some([1024, 64, 1024]));
    let (mut p, mut r) = owners();
    *r.word_mut() = 512;
    let old = p.current_record().clone();
    let mut h = Host::new(true, 1);
    h.fail_placement = true;
    assert!(matches!(
        run(&mut a, id, &mut p, &r, &mut h),
        Err(FurnitureCollisionPrefixError::Host {
            effect: "current H11/0",
            error: Error::H11
        })
    ));
    assert_eq!(p.current_record(), &old);
    assert!(!p.pair_alias_is_546b48());
    assert!(!h.events.borrow().iter().any(|s| s == "actor48"));
}
#[test]
fn negative_null_and_zero_radius_counterparts_skip_without_query_or_basis() {
    for (slot, radius) in [(0xffff, Some(10)), (7, None), (7, Some(0))] {
        let (mut a, id) = arena(Some([1024, 64, 1024]));
        let (mut p, mut r) = owners();
        *r.word_mut() = 512;
        let mut h = Host::new(true, 1);
        h.candidate_slot = slot;
        h.candidate = radius;
        run(&mut a, id, &mut p, &r, &mut h).unwrap();
        assert!(!h
            .events
            .borrow()
            .iter()
            .any(|s| s == "actor48" || s == "pair"));
        assert!(!p.pair_alias_is_546b48());
    }
}
#[test]
fn basis_failure_retains_init_but_precedes_record_and_shared_basis_stores() {
    let (mut a, id) = arena(Some([1024, 64, 1024]));
    let (mut p, mut r) = owners();
    *r.word_mut() = 512;
    let old = p.current_record().clone();
    let mut h = Host::new(true, 1);
    h.fail_basis = true;
    assert!(matches!(
        run(&mut a, id, &mut p, &r, &mut h),
        Err(FurnitureCollisionPrefixError::Host {
            effect: "actor virtual48",
            error: Error::Basis
        })
    ));
    assert_eq!(p.current_record(), &old);
    assert_eq!(p.query_bases()[0], NativeBasis::INITIAL);
    assert!(!p.pair_alias_is_546b48());
}
#[test]
fn active_stop_retains_captured_xyz_fresh_state_and_exact_process_prefix() {
    let (mut a, id) = arena(Some([1024, 777, 1024]));
    let (mut p, mut r) = owners();
    *r.word_mut() = 512;
    let mut h = Host::new(true, 1);
    h.change_state = true;
    h.basis_position = Some([1, 2, 3]);
    assert!(matches!(
        run(&mut a, id, &mut p, &r, &mut h),
        Err(FurnitureCollisionPrefixError::Host {
            effect: "004F2BE0 active pair",
            error: Error::Active(_)
        })
    ));
    let pair = h.pair.unwrap();
    assert_eq!(pair.saved_actor_position, [1024, 777, 1024]);
    assert_eq!(pair.cell_position, [512, 64, 512]);
    assert_eq!(pair.relative_actor_origin, [512, 713, 512]);
    assert_eq!(pair.contact_flag, 0);
    assert_eq!(a.object(id).unwrap().base.position_words, Some([1, 2, 3]));
    assert_eq!(
        p.current_record(),
        &FurnitureQueryRecord::NativeAddress(0x12345678)
    );
    assert_eq!(p.query_bases()[0].words[0], 0xa7);
    assert_eq!(p.query_bases()[1..], [NativeBasis::INITIAL; 4]);
    assert!(p.pair_alias_is_546b48());
    let e = h.events.borrow();
    let record = e.iter().position(|s| s.starts_with("record:")).unwrap();
    let basis = e.iter().position(|s| s == "furniturebasis:167").unwrap();
    assert!(record < basis);
    assert!(!e[record..].iter().any(|s| s == "F0"));
}
#[test]
fn second_initialized_visit_does_not_reconstruct_basis_history() {
    let (mut a, id) = arena(Some([1024, 777, 1024]));
    let (mut p, mut r) = owners();
    *r.word_mut() = 512;
    let mut active = Host::new(true, 1);
    assert!(run(&mut a, id, &mut p, &r, &mut active).is_err());
    let retained = *p.query_bases();
    let current = p.current_record().clone();
    let mut h = Host::new(false, 0);
    run(&mut a, id, &mut p, &r, &mut h).unwrap();
    assert_eq!(*p.query_bases(), retained);
    assert_eq!(p.current_record(), &current);
    assert!(p.literal_ret_registration_attempted());
}
#[test]
fn word_windows_ignore_preserved_upper_register_residue() {
    for high in [0_u32, 0x12340000, 0x80000000, 0xffff0000] {
        for (coordinate, radius, step, mask) in [
            (0u16, -1i32, 256i32, 255u32),
            (65535, 65535, 128, 127),
            (1024, 624, 256, 255),
        ] {
            let native = (high | u32::from(coordinate))
                .wrapping_sub((high | 112).wrapping_add(radius as u32))
                .wrapping_add(step as u32)
                .wrapping_sub(1)
                & !mask;
            let low = (u32::from(coordinate)
                .wrapping_sub(112u32.wrapping_add(radius as u32))
                .wrapping_add(step as u32)
                .wrapping_sub(1))
                & !mask;
            assert_eq!(native as u16, low as u16);
        }
    }
}

struct MissHost {
    prefix: Host,
    process: ProcessFurnitureQueryState,
    radius: ProcessFurnitureRadius,
}
impl FamilyTerrainProbeMissHost for MissHost {
    type Error = FurnitureCollisionPrefixError<Error>;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        Ok(&self.prefix.terrain)
    }
    fn collide(&mut self, a: &mut ObjectArena, id: ObjectId) -> Result<(), Self::Error> {
        run(a, id, &mut self.process, &self.radius, &mut self.prefix)
    }
}
#[test]
fn actual_cache_miss_zero_lookup_restores_xyz_guard_and_completes_same_allocation() {
    let (mut a, id) = arena(Some([1024, 777, 1024]));
    let (process, mut radius) = owners();
    *radius.word_mut() = 512;
    let mut host = MissHost {
        prefix: Host::new(false, 0),
        process,
        radius,
    };
    let previous = a.object(id).unwrap().base.position_words;
    let cache = a
        .family429e20_actor(id)
        .unwrap()
        .probe_234
        .as_ref()
        .map(|allocation| {
            &**allocation as *const crate::object_runtime::TerrainProbeAllocation as usize
        })
        .unwrap();
    let answer = a
        .probe_family_terrain_cache(id, [1024, 1024], |a, id, xz| {
            a.probe_family_terrain_miss(id, xz, &mut host)
                .map(u32::from)
        })
        .unwrap();
    assert_eq!(answer, 0);
    assert_eq!(a.object(id).unwrap().base.position_words, previous);
    assert_eq!(a.family428650_actor(id).unwrap().word_22e & 0x8000, 0);
    assert_eq!(
        a.family429e20_actor(id)
            .unwrap()
            .probe_234
            .as_ref()
            .map(
                |allocation| &**allocation as *const crate::object_runtime::TerrainProbeAllocation
                    as usize
            )
            .unwrap(),
        cache
    );
    let events = host.prefix.events.borrow().len();
    assert_eq!(
        a.probe_family_terrain_cache(id, [1024, 1024], |_, _, _| Err::<u32, _>(
            "must be real cache hit"
        ))
        .unwrap(),
        0
    );
    assert_eq!(host.prefix.events.borrow().len(), events);
}
#[test]
fn actual_cache_miss_active_error_keeps_temporary_xyz_guard_and_uncompleted_result() {
    let (mut a, id) = arena(Some([1024, 777, 1024]));
    let (process, mut radius) = owners();
    *radius.word_mut() = 512;
    let mut host = MissHost {
        prefix: Host::new(true, 1),
        process,
        radius,
    };
    let error = a
        .probe_family_terrain_cache(id, [1024, 1024], |a, id, xz| {
            a.probe_family_terrain_miss(id, xz, &mut host)
                .map(u32::from)
        })
        .unwrap_err();
    assert!(matches!(
        error,
        crate::object_runtime::FamilyTerrainProbeError::Cache(
            crate::object_runtime::TerrainProbeError::Host(
                crate::object_runtime::FamilyTerrainProbeMissError::Host {
                    effect: "0047C1D0 collision",
                    ..
                }
            )
        )
    ));
    assert_eq!(
        a.object(id).unwrap().base.position_words,
        Some([1088, 64, 1088])
    );
    assert_ne!(a.family428650_actor(id).unwrap().word_22e & 0x8000, 0);
    assert!(a
        .family429e20_actor(id)
        .unwrap()
        .probe_234
        .as_ref()
        .unwrap()
        .decoded_cache()
        .words()
        .iter()
        .all(|&w| w == 0));
    assert!(host.process.pair_alias_is_546b48());
}

// Two explicit known active records exercise once-per-query preparation; the
// supplied contact is a host test mode, never a production clear result.
fn two_active_cells() -> TerrainRuntime {
    let mut raw = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        raw[TERRAIN_PREFIX_BYTES + index * 2] = 2;
    }
    for z in [4usize, 6] {
        raw[TERRAIN_PREFIX_BYTES + (4 * 512 + z) * 2 + 1] = 4;
        raw[TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + z * 2
            ..TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2 + z * 2 + 2]
            .copy_from_slice(&4u16.to_le_bytes());
        raw.extend_from_slice(&[4, 0, 0x27, 1]);
    }
    let span = ByteSpan::new(0, raw.len());
    let bank = parse_terrain_bank(
        &raw,
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
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}
#[test]
fn multiple_active_cells_prepare_actor_once_and_normal_miss_restores_original_xyz() {
    let (mut a, id) = arena(Some([1024, 777, 1024]));
    let previous = a.object(id).unwrap().base.position_words;
    let (process, mut radius) = owners();
    *radius.word_mut() = 512;
    let mut prefix = Host::new(true, 1);
    prefix.terrain = two_active_cells();
    prefix.pair_fixture = PairFixture::Return(CollisionContact::from_native_words([
        Some(0),
        None,
        None,
        None,
        None,
    ]));
    prefix.basis_position = Some([4096, 900, 8192]);
    let mut host = MissHost {
        prefix,
        process,
        radius,
    };
    let answer = a
        .probe_family_terrain_cache(id, [1024, 1024], |a, id, xz| {
            a.probe_family_terrain_miss(id, xz, &mut host)
                .map(u32::from)
        })
        .unwrap();
    assert_eq!(answer, 0);
    let events = host.prefix.events.borrow();
    assert_eq!(events.iter().filter(|e| e.as_str() == "pair").count(), 2);
    assert_eq!(events.iter().filter(|e| e.as_str() == "actor48").count(), 1);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.as_str() == "sharedbasis")
            .count(),
        1
    );
    assert_eq!(
        host.prefix.pair.unwrap().saved_actor_position,
        [1088, 64, 1088]
    );
    assert_eq!(a.object(id).unwrap().base.position_words, previous);
    assert_eq!(a.family428650_actor(id).unwrap().word_22e & 0x8000, 0);
    assert!(host.process.pair_alias_is_546b48());
}
