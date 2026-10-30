//! Strict native constructors plus explicit supplied inputs, never loaded actor policy.
use super::*;
use crate::object_runtime::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing};
use inf_formats::{
    parse_terrain_bank, ByteSpan, FrontendExecutable, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
const SINE: [u16; 4096] = [0; 4096];
fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}
fn subject(arena: &mut ObjectArena) -> ObjectId {
    arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([11, 22, 33]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
            Selector12GrantContext {
                table: &inf_formats::GrantTable {
                    virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
                    records: vec![],
                },
                first_player_grants: None,
            },
        )
        .unwrap()
}
fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = arena();
    let actor = subject(&mut arena);
    (arena, actor)
}
fn absent() -> LiveGrantContext<'static> {
    LiveGrantContext {
        process: None,
        executable: None,
        resident: None,
        text_data: None,
    }
}
fn process(network_flag: u32) -> crate::process_particles::factories::ParticleFactoryProcessState {
    crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    }
}
fn follow(arena: &mut ObjectArena, actor: ObjectId, grant: u16) -> NativeHeapRef {
    arena.actor_mut(actor).unwrap().word_7a = grant;
    let handle = arena.actor(actor).unwrap().handle;
    let target = arena.import_fixture_target(NativeTarget::ActorFollow(ActorFollowTarget {
        handle_04: handle,
        focus_08: None,
        word_10: 2,
        focus_14: None,
        velocity_1a: None,
    }));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(target);
    target
}
fn fixed(arena: &mut ObjectArena, actor: ObjectId, point: Option<[u16; 3]>) -> NativeHeapRef {
    let target = arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(0),
        focus_08: point,
        word_10: 8,
    }));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(target);
    target
}
fn carries(arena: &mut ObjectArena, actor: ObjectId, distance: u32, threshold: u32, timer: u32) {
    //Declared supplied backing for arithmetic tests, not constructor/native writer claims.
    arena.derived_actor_mut(actor).unwrap().word_14c = Some(distance);
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.words_188_19c[4] = Some(threshold);
    state.words_188_19c[2] = Some(timer);
}
fn terrain(records: &[(u16, u16, u8)]) -> TerrainRuntime {
    //Canonical explicitly supplied H12 bytes decoded by the actual parser.
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for &(x, z, _) in records {
        bytes[TERRAIN_PREFIX_BYTES + (usize::from(x) * 512 + usize::from(z)) * 2 + 1] |= 4;
    }
    for z in 0..512_u16 {
        let mut row: Vec<_> = records.iter().filter(|r| r.1 == z).copied().collect();
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
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    )
}
struct Host {
    terrain: TerrainRuntime,
    events: Vec<&'static str>,
    grant: u16,
    acquisitions: usize,
    null_refresh: bool,
    fail_acquire: bool,
    mutate_carries: Option<(u32, u32, u32)>,
}
impl Host {
    fn empty() -> Self {
        Self::new(&[], 0)
    }
    fn new(records: &[(u16, u16, u8)], grant: u16) -> Self {
        Self {
            terrain: terrain(records),
            events: vec![],
            grant,
            acquisitions: 0,
            null_refresh: false,
            fail_acquire: false,
            mutate_carries: None,
        }
    }
}
impl FamilyGrantGridHost for Host {
    type Error = &'static str;
    type Terrain = ();
    fn acquire_terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<()>, Self::Error> {
        self.events.push("h12");
        self.acquisitions += 1;
        if self.fail_acquire {
            return Err("actual H12 fixture error");
        }
        if self.acquisitions == 1 {
            if let Some((d, t, r)) = self.mutate_carries {
                carries(arena, actor, d, t, r);
            }
        }
        Ok(if self.null_refresh && self.acquisitions == 2 {
            None
        } else {
            Some(())
        })
    }
    fn terrain(&self, _: ()) -> Result<&TerrainRuntime, Self::Error> {
        Ok(&self.terrain)
    }
    fn spacing(&mut self) -> TerrainSpacing {
        panic!("grant query does not read spacing")
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        self.events.push("mask");
        TerrainSpacing::Units256
    }
    fn height_scale(&mut self) -> TerrainHeightScale {
        panic!("grant query does not read height")
    }
    fn placement_kind(&mut self, lookup: u8) -> Result<u8, Self::Error> {
        assert_eq!(lookup, 7);
        self.events.push("h11");
        Ok(3)
    }
    fn furniture_grant_id(&mut self, kind: u8) -> Result<u16, Self::Error> {
        assert_eq!(kind, 3);
        self.events.push("grant");
        Ok(self.grant)
    }
    fn publish_grid_point(
        &mut self,
        _: &mut ObjectArena,
        _: FamilyGrantGridPointRequest,
    ) -> Result<bool, Self::Error> {
        panic!("update cannot allocate Target284")
    }
}
fn update(
    arena: &mut ObjectArena,
    actor: ObjectId,
    microseconds: u32,
    context: &LiveGrantContext<'_>,
    host: &mut Host,
) -> Result<(), Selector12PreferredUpdateError<&'static str>> {
    arena.update_selector12_preferred_target(
        actor,
        Selector12PreferredUpdateRequest {
            microseconds,
            live_grants: context,
        },
        host,
    )
}

#[test]
fn actual_null148_stops_before_any_host_or_timer_reads() {
    let (mut arena, actor) = fixture();
    let mut host = Host::empty();
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    let before = format!("{arena:?}");
    assert!(matches!(
        update(&mut arena, actor, 1, &absent(), &mut host),
        Err(Selector12PreferredUpdateError::RequiredField { offset: 0x148, .. })
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert!(host.events.is_empty());
}

#[test]
fn actor_zero_grant_and_null_referent_bypass_all_unknown_carries() {
    for null in [false, true] {
        let (mut arena, actor) = fixture();
        let reference = follow(&mut arena, actor, 0);
        if null {
            let mut target = arena.native_target(reference).unwrap();
            if let NativeTarget::ActorFollow(value) = &mut target {
                value.handle_04 = ActorHandle::from_raw(0);
            }
            arena.write_native_target(reference, target).unwrap();
        }
        arena.family428650_actor_mut(actor).unwrap().word_22e = 0xab80;
        let timers = arena.family428650_actor(actor).unwrap().words_188_19c;
        let mut host = Host::empty();
        update(&mut arena, actor, u32::MAX, &absent(), &mut host).unwrap();
        assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0xab8c);
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c,
            timers
        );
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
        assert!(host.events.is_empty());
    }
}

#[test]
fn actual_target_query_custody_error_is_not_a_zero_grant_return() {
    let (mut arena, actor) = fixture();
    follow(&mut arena, actor, 4);
    arena.actor_handles = None;
    let before = format!("{arena:?}");
    let mut host = Host::empty();
    assert!(matches!(
        update(&mut arena, actor, 1, &absent(), &mut host),
        Err(Selector12PreferredUpdateError::Object(
            ObjectError::MissingActorRegistry
        ))
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert!(host.events.is_empty());
}

#[test]
fn raw_target_focus_is_required_before_real_terrain_zero_grant_bypass() {
    let (mut arena, actor) = fixture();
    let reference = fixed(&mut arena, actor, None);
    let mut host = Host::empty();
    assert!(matches!(update(&mut arena, actor, 1, &absent(), &mut host),
        Err(Selector12PreferredUpdateError::RequiredTargetField { target, offset: 0x0c }) if target == reference));
    assert!(host.events.is_empty());
    let target = NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(0),
        focus_08: Some([128, 99, 256]),
        word_10: 8,
    });
    arena.write_native_target(reference, target).unwrap();
    update(&mut arena, actor, 1, &absent(), &mut host).unwrap();
    assert_eq!(host.events, ["h12", "mask"]);
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
}

#[test]
fn terrain_lookup_keeps_capture_and_reads_carries_fresh_after_callback() {
    let (mut arena, actor) = fixture();
    fixed(&mut arena, actor, Some([128, 999, 256]));
    let mut host = Host::new(&[(0, 2, 7)], 4);
    host.null_refresh = true;
    host.mutate_carries = Some((0, 0, 10));
    update(&mut arena, actor, 1, &absent(), &mut host).unwrap();
    assert_eq!(host.events, ["h12", "mask", "h12", "h11", "grant"]);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[2],
        Some(9)
    );
    assert_eq!(
        arena.selector12_actor(actor).unwrap().grants_24c.mask_04,
        [0; 6]
    );
}

#[test]
fn nonzero_grant_requires14c_then198_then190_without_seeding() {
    let (mut arena, actor) = fixture();
    let reference = follow(&mut arena, actor, 1);
    //The reached virtual10 owns a real actor-registry cache refresh. Prime it
    //through that canonical query before comparing the deeper timer prefixes.
    let target = arena.native_target(reference).unwrap();
    assert_eq!(arena.resolve_target_actor(&target).unwrap(), Some(actor));
    let mut host = Host::empty();
    for offset in [0x14c, 0x198, 0x190] {
        let before = format!("{arena:?}");
        assert!(matches!(update(&mut arena, actor, 1, &absent(), &mut host),
            Err(Selector12PreferredUpdateError::RequiredField { offset: actual, .. }) if actual == offset));
        assert_eq!(format!("{arena:?}"), before);
        if offset == 0x14c {
            arena.derived_actor_mut(actor).unwrap().word_14c = Some(0);
        }
        if offset == 0x198 {
            arena.family428650_actor_mut(actor).unwrap().words_188_19c[4] = Some(0);
        }
    }
    assert!(host.events.is_empty());
}

#[test]
fn signed_wrapping_countdown_and920_cap_leave_flags_masks_notification_unchanged() {
    for (distance, threshold, timer, delta, expected) in [
        (0_u32, 0_u32, 100_u32, 10_u32, 90_u32),
        (920, 0, 200, 1024, 96),
        (921, 0, 200, 1024, 96),
        (0x7fff_ffff, 0xffff_ffff, 2_097_154, 1, 1),
        (0, 0, 4, 0x8000_0000, 0x8000_0004),
    ] {
        let (mut arena, actor) = fixture();
        follow(&mut arena, actor, 1);
        carries(&mut arena, actor, distance, threshold, timer);
        arena.family428650_actor_mut(actor).unwrap().word_22e = 0xa500;
        arena.selector12_actor_mut(actor).unwrap().byte_29d = 0x55;
        let mut host = Host::empty();
        update(&mut arena, actor, delta, &absent(), &mut host).unwrap();
        let state = arena.family428650_actor(actor).unwrap();
        assert_eq!(state.words_188_19c[2], Some(expected));
        assert_eq!(state.word_22e, 0xa500);
        assert_eq!(
            arena.selector12_actor(actor).unwrap().grants_24c.mask_04,
            [0; 6]
        );
        assert_eq!(arena.selector12_actor(actor).unwrap().byte_29d, 0x55);
    }
}

#[test]
fn expiry_flags_timers_and_mask_bit_survive_missing_process_before_notification() {
    let (mut arena, actor) = fixture();
    follow(&mut arena, actor, 1);
    carries(&mut arena, actor, 0, 0, 1);
    arena.add_world_player([0; 6]);
    arena.selector12_actor_mut(actor).unwrap().byte_29d = 0x55;
    arena.family428650_actor_mut(actor).unwrap().word_22e = 0xa500;
    let mut host = Host::empty();
    assert!(matches!(update(&mut arena, actor, 1, &absent(), &mut host),
        Err(Selector12PreferredUpdateError::Grant(LiveGrantError::MissingProcess {
            owner: GrantOwner::Selector12Actor24c(actual) })) if actual == actor));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0xa50c);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[2..4],
        [Some(0), Some(1)]
    );
    assert_eq!(
        arena.selector12_actor(actor).unwrap().grants_24c.mask_04[0],
        2
    );
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29d, 0x55);
}

#[test]
fn new_grant_requires_dependency_exe_even_with_real_null_notification_binding() {
    let (mut arena, actor) = fixture();
    follow(&mut arena, actor, 1);
    carries(&mut arena, actor, 0, 0, 0);
    let process = process(0);
    let context = LiveGrantContext {
        process: Some(&process),
        ..absent()
    };
    let mut host = Host::empty();
    assert_eq!(
        arena
            .selector12_actor(actor)
            .unwrap()
            .grants_24c
            .notification_owner_1c,
        0
    );
    assert!(matches!(
        update(&mut arena, actor, 0, &context, &mut host),
        Err(Selector12PreferredUpdateError::Grant(
            LiveGrantError::MissingExecutable { id: 149, .. }
        ))
    ));
    assert_eq!(
        arena.selector12_actor(actor).unwrap().grants_24c.mask_04[0],
        2
    );
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[2..4],
        [Some(0), Some(1)]
    );
}

#[test]
fn actual_live_setter_then_single_grant_queries_players_mask_not_actor_mask() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for (network, player_owns, expected) in [(0, false, 1), (0x100, true, 0)] {
        let (mut arena, actor) = fixture();
        follow(&mut arena, actor, 1);
        carries(&mut arena, actor, 0, 0, u32::MAX);
        let mut mask = [0; 6];
        if player_owns {
            mask[0] = 2;
        }
        arena.add_world_player(mask);
        let process = process(network);
        let context = LiveGrantContext {
            process: Some(&process),
            executable: Some(executable),
            resident: None,
            text_data: None,
        };
        let mut host = Host::empty();
        update(&mut arena, actor, 0, &context, &mut host).unwrap();
        assert_eq!(arena.selector12_actor(actor).unwrap().byte_29d, expected);
        assert!(arena
            .live_grant_has(GrantOwner::Selector12Actor24c(actor), 1)
            .unwrap());
        assert_eq!(
            arena
                .first_player()
                .and_then(|id| arena.world_player(id).ok())
                .unwrap()
                .grants_9c,
            mask
        );
    }
}

#[test]
fn already_owned_setter_bypasses_sources_but_notification_reads_current_player_mask() {
    let (mut arena, actor) = fixture();
    follow(&mut arena, actor, 1);
    carries(&mut arena, actor, 0, 0, 0);
    arena
        .selector12_actor_mut(actor)
        .unwrap()
        .grants_24c
        .mask_04[0] = 2;
    let player = arena.add_world_player([0; 6]);
    let mut host = Host::empty();
    update(&mut arena, actor, 0, &absent(), &mut host).unwrap();
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29d, 1);
    arena.world_player_mut(player).unwrap().grants_9c[0] = 2;
    update(&mut arena, actor, 0, &absent(), &mut host).unwrap();
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29d, 0);
}

#[test]
fn single_grant_no_player_bypasses_receiver_and_adjacent_query_stops_before29d() {
    let mut arena = arena();
    let base = arena.allocate(ObjectClassKind::Base);
    let before = format!("{arena:?}");
    arena
        .notify_selector12_single_grant(base, u32::MAX)
        .unwrap();
    assert_eq!(format!("{arena:?}"), before);
    let actor = subject(&mut arena);
    let player = arena.add_world_player([0; 6]);
    arena.selector12_actor_mut(actor).unwrap().byte_29d = 0x55;
    assert!(matches!(arena.notify_selector12_single_grant(actor, 192),
        Err(LiveGrantError::MaskBoundary { owner: GrantOwner::Player9c(actual), word_index: 6 }) if actual == player));
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29d, 0x55);
    arena
        .notify_selector12_single_grant(actor, 0xdead_0000)
        .unwrap();
    assert_eq!(arena.selector12_actor(actor).unwrap().byte_29d, 0);
}

#[test]
fn preferred_change_then_immediate_nonzero_update_retains_real14c_absence() {
    let (mut arena, actor) = fixture();
    let preferred = follow(&mut arena, actor, 1);
    arena.derived_actor_mut(actor).unwrap().target_148 = None;
    arena.selector12_actor_mut(actor).unwrap().targets_26c_284[3] = Some(preferred);
    arena
        .change_selector12_descriptor_target(
            actor,
            Selector12DescriptorChangeRequest {
                kind: Selector12DescriptorChangeKind::PreferredTarget27c284,
                entering: true,
            },
            WeightedTargetConstructorNull::ActorRegistry,
        )
        .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
    let before = format!("{arena:?}");
    let mut host = Host::empty();
    assert!(matches!(
        update(&mut arena, actor, 1, &absent(), &mut host),
        Err(Selector12PreferredUpdateError::RequiredField { offset: 0x14c, .. })
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[2..5],
        [
            Some(4_000_000),
            Some(0),
            Some(u32::from(arena.object(actor).unwrap().base.word_28) + 288)
        ]
    );
}

#[test]
fn actual_terrain_service_error_retains_prefix_without_zero_grant_flags() {
    let (mut arena, actor) = fixture();
    fixed(&mut arena, actor, Some([128, 99, 256]));
    let mut host = Host::empty();
    host.fail_acquire = true;
    let before = format!("{arena:?}");
    assert!(matches!(
        update(&mut arena, actor, 1, &absent(), &mut host),
        Err(Selector12PreferredUpdateError::Terrain(
            FamilyGrantGridError::Host {
                phase: "436BC0 H12 acquisition",
                error: "actual H12 fixture error"
            }
        ))
    ));
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(host.events, ["h12"]);
}
