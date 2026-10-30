//! Supplied allocation modes and actual constructors/writers. These cases do
//! not adopt a captured distance/header or prove native allocator reuse.
use super::*;
use crate::object_runtime::{
    FamilyAttackError, FamilyAttackHost, FamilyHelperHistoryRecord, FamilyObjectTargetChangeError,
    FamilyObjectTargetChangeRequest, FollowTargetRequest, NativeTarget, RuntimeAllocation,
    TargetNullHandleSource, WeightedTargetConstructorNull,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing};
use inf_formats::ByteSpan;
use std::sync::Arc;

const EXTENDED: ActorAllocationPolicy =
    ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation;
const OLD: ActorAllocationPolicy = ActorAllocationPolicy::FreshHeapZeroCarryApproximation;
const POLICIES: [ActorAllocationPolicy; 3] = [ActorAllocationPolicy::Unowned, OLD, EXTENDED];

fn selected(policy: ActorAllocationPolicy) -> ObjectArena {
    let mut arena = arena();
    arena.select_actor_allocation_policy(policy).unwrap();
    arena
}
fn grants() -> GrantTable {
    let raw = executable();
    parse_grant_table(FrontendExecutable::parse(&raw).unwrap()).unwrap()
}
fn flat_terrain() -> TerrainRuntime {
    // Explicit fixed-layout H12 fixture, not a retail terrain/capture value.
    let bytes = vec![0; inf_formats::TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, bytes.len());
    let bank = inf_formats::parse_terrain_bank(
        &bytes,
        &inf_formats::HandlerRegion {
            handler: inf_formats::HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: inf_formats::HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}
fn retained(bytes: Vec<u8>) -> Arc<RuntimeAllocation> {
    let size = bytes.len();
    Arc::new(RuntimeAllocation::new(bytes.into(), ByteSpan::new(0, size)).unwrap())
}

#[test]
fn extended_constructor_matrix_changes_only_the_old_carries_distance_and_family_timers() {
    let grants = grants();
    for class in [
        Constructor::Shared,
        Constructor::Derived,
        Constructor::Family428650,
        Constructor::Family429e20,
        Constructor::Family4236d0,
        Constructor::Selector10,
        Constructor::Selector11,
        Constructor::Selector12,
        Constructor::Selector34,
        Constructor::Selector35,
        Constructor::Selector36,
        Constructor::Selector77,
        Constructor::Selector30,
    ] {
        let mut strict = arena();
        let mut extended = selected(EXTENDED);
        let mut strict_rng = 0x1234_5678;
        let mut extended_rng = strict_rng;
        let expected = construct(&mut strict, class, &mut strict_rng, &grants);
        let actual = construct(&mut extended, class, &mut extended_rng, &grants);
        normalize_expected_carries(&mut strict, expected, class);
        if !matches!(class, Constructor::Shared | Constructor::Selector30) {
            let state = extended.derived_actor(actual).unwrap();
            assert_eq!(
                (state.word_14c, state.word_150),
                (Some(0), Some(0)),
                "{class:?}"
            );
            assert_eq!((state.word_158, state.word_15c), (None, None));
            assert_eq!((state.word_16c, state.word_170), (None, None));
            strict.derived_actor_mut(expected).unwrap().word_14c = Some(0);
            strict.derived_actor_mut(expected).unwrap().word_150 = Some(0);
        }
        if let Ok(derived) = strict.derived_actor_mut(expected) {
            if let DerivedActorClass::Selector11(state) = &mut derived.class {
                // Selector11 +19C, the turret's aim time, is its own extended carry.
                let word = extended.selector11_actor(actual).unwrap().word_19c;
                assert_eq!((state.word_19c, word), (None, Some(0)), "{class:?}");
                state.word_19c = Some(0);
            }
        }
        if let ActorExtension::Selector30(state) =
            &mut strict.actor_mut(expected).unwrap().extension
        {
            // Selector30 +158, the gate's held handle, is its only extended carry.
            let word = extended
                .selector30_actor(actual)
                .unwrap()
                .intermediate
                .word_158;
            assert_eq!(
                (state.intermediate.word_158, word),
                (None, Some(0)),
                "{class:?}"
            );
            state.intermediate.word_158 = Some(0);
        }
        if strict.selector35_actor(expected).is_ok() {
            // Selector35 +260, the route distance, is its own extended carry.
            let strict_word = strict.selector35_actor(expected).unwrap().word_260;
            let word = extended.selector35_actor(actual).unwrap().word_260;
            assert_eq!((strict_word, word), (None, Some(0)), "{class:?}");
            strict.selector35_actor_mut(expected).unwrap().word_260 = Some(0);
        }
        if strict.family428650_actor(expected).is_ok() {
            // Family +18C/+19C are the only extended family-shell carries;
            // strict leaves both unknown.
            for index in [1, 5] {
                let strict_word = strict.family428650_actor(expected).unwrap().words_188_19c[index];
                let word = extended.family428650_actor(actual).unwrap().words_188_19c[index];
                assert_eq!((strict_word, word), (None, Some(0)), "{class:?} +{index}");
                strict
                    .family428650_actor_mut(expected)
                    .unwrap()
                    .words_188_19c[index] = Some(0);
            }
        }
        assert_eq!(
            strict.object(expected).unwrap(),
            extended.object(actual).unwrap(),
            "{class:?}"
        );
        assert_eq!(strict_rng, extended_rng, "{class:?}");
        assert_eq!(
            registry_shape(&strict),
            registry_shape(&extended),
            "{class:?}"
        );
        assert_eq!(strict.len(), extended.len());
        assert_eq!(strict.actor_list().len(), extended.actor_list().len());
        assert_eq!(
            extended.actor_allocation,
            ActorAllocationService::Started(EXTENDED)
        );
    }
    // The existing full four-carry matrix remains unchanged and owns old mode.
    let mut old = selected(OLD);
    let actor = construct(&mut old, Constructor::Selector12, &mut 0, &grants);
    assert_eq!(old.derived_actor(actor).unwrap().word_14c, None);
}

fn finish(
    arena: &mut ObjectArena,
    actor: ObjectId,
    class: Constructor,
    terrain: Option<&TerrainRuntime>,
    grants: &GrantTable,
) -> Result<(), crate::object_runtime::DerivedActorError> {
    match class {
        Constructor::Selector12 => arena.finish_selector12_initialization(
            actor,
            terrain,
            Selector12GrantContext {
                table: grants,
                first_player_grants: None,
            },
        ),
        Constructor::Selector10 | Constructor::Selector36 => {
            arena.finish_family4236d0_initialization(actor)
        }
        _ => unreachable!("actual two post-init gate owners"),
    }
}
fn prepare_finish(
    arena: &mut ObjectArena,
    actor: ObjectId,
    class: Constructor,
    owner: &Arc<RuntimeAllocation>,
) {
    match class {
        Constructor::Selector12 => arena.set_selector12_retained(actor, owner, 0).unwrap(),
        _ => arena.set_family4236d0_retained(actor, owner, 0).unwrap(),
    }
    // Declared post-context fixture backing; no actor or scene ID policy.
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, 2, 0]);
}

#[test]
fn both_fresh_helper_gates_are_explicit_and_preserve_existing_header_and_rows() {
    let grants = grants();
    let terrain = flat_terrain();
    for policy in POLICIES {
        for class in [
            Constructor::Selector10,
            Constructor::Selector36,
            Constructor::Selector12,
        ] {
            let mut arena = selected(policy);
            let actor = construct(&mut arena, class, &mut 0, &grants);
            let allocation = retained(vec![0; 18]);
            prepare_finish(&mut arena, actor, class, &allocation);
            assert!(arena.derived_actor(actor).unwrap().behavior_140.is_none());
            finish(&mut arena, actor, class, Some(&terrain), &grants).unwrap();
            assert!(arena
                .family428650_actor(actor)
                .unwrap()
                .helper_218
                .is_none());
            arena.derived_actor_mut(actor).unwrap().behavior_140 =
                crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 3);
            finish(&mut arena, actor, class, Some(&terrain), &grants).unwrap();
            let helper = arena
                .family428650_actor_mut(actor)
                .unwrap()
                .helper_218
                .as_mut()
                .unwrap();
            assert_eq!(
                helper.words_00_04,
                [if policy == EXTENDED { Some(0) } else { None }, None]
            );
            assert_eq!(helper.word_08, 0);
            assert_eq!(helper.records, [FamilyHelperHistoryRecord::UNKNOWN; 4]);
            helper.words_00_04 = [Some(91), Some(0xffff_fffb)];
            helper.word_08 = 1;
            helper.records[0].weight = Some(17);
            helper.records[0].padding = Some(0x789a);
            let before = helper.as_ref().clone();
            finish(&mut arena, actor, class, Some(&terrain), &grants).unwrap();
            assert_eq!(
                arena
                    .family428650_actor(actor)
                    .unwrap()
                    .helper_218
                    .as_deref(),
                Some(&before)
            );
        }
    }
}

#[test]
fn helper_creation_waits_for_earlier_postinit_reads_and_keeps_clamp_prefix_on_error() {
    let grants = grants();
    let mut arena = selected(EXTENDED);
    let actor = construct(&mut arena, Constructor::Selector12, &mut 0, &grants);
    let mut bytes = vec![0; 18];
    bytes[16..18].copy_from_slice(&1500_u16.to_le_bytes());
    let allocation = retained(bytes);
    prepare_finish(&mut arena, actor, Constructor::Selector12, &allocation);
    arena.derived_actor_mut(actor).unwrap().behavior_140 =
        crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 3);
    assert!(matches!(
        finish(&mut arena, actor, Constructor::Selector12, None, &grants),
        Err(crate::object_runtime::DerivedActorError::MissingSelector12Terrain)
    ));
    assert_eq!(allocation.word(16).unwrap(), 1000);
    assert!(arena
        .family428650_actor(actor)
        .unwrap()
        .helper_218
        .is_none());
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(0));
    let common = construct(&mut arena, Constructor::Selector10, &mut 0, &grants);
    arena.derived_actor_mut(common).unwrap().behavior_140 =
        crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 3);
    assert!(matches!(
        finish(&mut arena, common, Constructor::Selector10, None, &grants),
        Err(crate::object_runtime::DerivedActorError::MissingFamily4236d0Payload)
    ));
    assert!(arena
        .family428650_actor(common)
        .unwrap()
        .helper_218
        .is_none());
}

#[test]
fn helper_initialize_writes_only_count_and_keeps_supplied_header_and_padding() {
    for policy in POLICIES {
        let (mut arena, actor, _) = attack_fixture(policy);
        let helper = arena
            .family428650_actor_mut(actor)
            .unwrap()
            .helper_218
            .as_mut()
            .unwrap();
        helper.words_00_04[0] = Some(0x1234_5678);
        helper.words_00_04[1] = Some(0xffff_fffb);
        helper.word_08 = 3;
        helper.records[0].normal_bits = [Some(1), None, Some(2)];
        helper.records[0].padding = Some(0x4321);
        let mut expected = helper.as_ref().clone();
        expected.word_08 = 0;
        helper.initialize();
        assert_eq!(helper.as_ref(), &expected);
    }
}

struct AttackHost {
    rng: u32,
    events: Vec<&'static str>,
    fail_sound: bool,
}
impl TargetNullHandleSource for AttackHost {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
}
impl FamilyAttackHost for AttackHost {
    type Error = &'static str;
    fn random(&mut self) -> Result<u32, Self::Error> {
        self.events.push("random");
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }
    fn attack_sound(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u16,
    ) -> Result<(), Self::Error> {
        self.events.push("sound");
        if self.fail_sound {
            Err("sound stopped")
        } else {
            Ok(())
        }
    }
}
fn attack_fixture(policy: ActorAllocationPolicy) -> (ObjectArena, ObjectId, ObjectId) {
    let mut arena = selected(policy);
    let actor = construct(&mut arena, Constructor::Selector10, &mut 0, &grants());
    let target_actor = arena.allocate_actor(request().actor).unwrap();
    arena.owned_mut(actor).unwrap().state.base.word_28 = 30;
    arena.owned_mut(target_actor).unwrap().state.base.word_28 = 70;
    arena
        .owned_mut(target_actor)
        .unwrap()
        .state
        .base
        .position_words = Some([10, 20, 30]);
    let handle = arena.actor(target_actor).unwrap().handle;
    let data = arena
        .follow_target(FollowTargetRequest::resolve_actor(handle))
        .unwrap();
    let reference = arena.import_fixture_target(NativeTarget::ActorFollow(data));
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(reference);
    let allocation = retained(vec![0; 18]);
    prepare_finish(&mut arena, actor, Constructor::Selector10, &allocation);
    arena.derived_actor_mut(actor).unwrap().behavior_140 =
        crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 3);
    finish(&mut arena, actor, Constructor::Selector10, None, &grants()).unwrap();
    arena.derived_actor_mut(actor).unwrap().sound_words_17c_182 = Some([0; 4]);
    (arena, actor, target_actor)
}

#[test]
fn actual_attack_mismatch_writes_helper_timer_but_strict_and_old_stop_at_header() {
    for policy in POLICIES {
        let (mut arena, actor, target_actor) = attack_fixture(policy);
        let handle = arena.actor(target_actor).unwrap().handle.raw();
        assert_ne!(
            handle, 0,
            "real allocated registry handle is the declared mismatch domain"
        );
        let mut host = AttackHost {
            rng: 7,
            events: vec![],
            fail_sound: false,
        };
        let mut expected_rng = host.rng;
        let random = crate::name_entry::name_entry_rng_next(&mut expected_rng);
        let result = arena.change_family_attack(actor, true, &mut host);
        if policy == EXTENDED {
            assert_eq!(result, Ok(()));
        } else {
            assert_eq!(
                result,
                Err(FamilyAttackError::UninitializedHelper { actor, offset: 0 })
            );
        }
        assert_eq!(host.events, ["random"]);
        assert_eq!(host.rng, expected_rng);
        assert!(arena.derived_actor(actor).unwrap().target_148.is_some());
        let family = arena.family428650_actor(actor).unwrap();
        assert_eq!(family.word_228, 0x0053_1244);
        assert_eq!(
            family.words_188_19c[1],
            Some(2_000_000 + ((random & 0xffff) << 4))
        );
        assert_eq!(family.words_204_208[1], Some(450));
        assert_eq!(family.byte_22c, 1);
        let helper = family.helper_218.as_ref().unwrap();
        assert_eq!(
            helper.words_00_04,
            if policy == EXTENDED {
                [Some(handle), Some(0)]
            } else {
                [None, None]
            }
        );
        assert_eq!(helper.records, [FamilyHelperHistoryRecord::UNKNOWN; 4]);
    }
}

#[test]
fn actual_attack_equality_keeps_unknown_timer_required_and_retains_signed_half_on_host_error() {
    let (mut arena, actor, target_actor) = attack_fixture(EXTENDED);
    let handle = arena.actor(target_actor).unwrap().handle.raw();
    arena
        .family428650_actor_mut(actor)
        .unwrap()
        .helper_218
        .as_mut()
        .unwrap()
        .words_00_04[0] = Some(handle);
    let mut host = AttackHost {
        rng: 7,
        events: vec![],
        fail_sound: true,
    };
    assert_eq!(
        arena.change_family_attack(actor, true, &mut host),
        Err(FamilyAttackError::UninitializedHelper { actor, offset: 4 })
    );
    assert_eq!(host.events, ["random"]);
    arena
        .family428650_actor_mut(actor)
        .unwrap()
        .helper_218
        .as_mut()
        .unwrap()
        .words_00_04[1] = Some((-5_i32) as u32);
    arena.derived_actor_mut(actor).unwrap().sound_words_17c_182 = Some([0, 0, 1, 0]);
    let before_rng = host.rng;
    let mut expected_rng = before_rng;
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    assert_eq!(
        arena.change_family_attack(actor, true, &mut host),
        Err(FamilyAttackError::Host {
            effect: "00429C09 attack sound",
            error: "sound stopped"
        })
    );
    assert_eq!(host.events, ["random", "random", "sound"]);
    assert_eq!(host.rng, expected_rng);
    assert_eq!(
        arena
            .family428650_actor(actor)
            .unwrap()
            .helper_218
            .as_ref()
            .unwrap()
            .words_00_04,
        [Some(handle), Some((-2_i32) as u32)]
    );
}

#[test]
fn real_object_target_writer_overrides_backing_and_failure_preserves_zero_and_clone_prefix() {
    let grants = grants();
    let mut arena = selected(EXTENDED);
    let actor = construct(&mut arena, Constructor::Selector12, &mut 0, &grants);
    let source = arena.allocate(ObjectClassKind::RouteNode);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, 0, 0]);
    arena.owned_mut(source).unwrap().state.base.position_words = Some([3, 4, 0]);
    let data = arena.object_position_target(source).unwrap();
    let reference = arena.import_fixture_target(NativeTarget::ObjectPosition(data));
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[5] = Some(reference);
    let request = FamilyObjectTargetChangeRequest { entering: true };
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(0));
    arena
        .change_family_object_target(
            actor,
            request,
            WeightedTargetConstructorNull::Supplied(Some(ActorHandle::from_raw(0))),
        )
        .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(5));
    let written_target = arena.derived_actor(actor).unwrap().target_148;
    assert_ne!(written_target, Some(reference), "actual clone identity");
    let snapshot = arena.object(actor).unwrap().clone();
    construct(&mut arena, Constructor::Selector12, &mut 0, &grants);
    assert_eq!(arena.object(actor).unwrap(), &snapshot);
    // Fresh allocation backing remains in place if XYZ fails after real clone publication.
    let next = construct(&mut arena, Constructor::Selector12, &mut 0, &grants);
    arena.family428650_actor_mut(next).unwrap().targets_1d4_1fc[5] = Some(reference);
    let before_size = arena.family428650_actor(next).unwrap().words_204_208[1];
    assert_eq!(
        arena.change_family_object_target(
            next,
            request,
            WeightedTargetConstructorNull::Supplied(Some(ActorHandle::from_raw(0)))
        ),
        Err(FamilyObjectTargetChangeError::Object(
            crate::object_runtime::ObjectError::ActorPositionUnknown(next)
        ))
    );
    assert_eq!(arena.derived_actor(next).unwrap().word_14c, Some(0));
    assert!(arena.derived_actor(next).unwrap().target_148.is_some());
    assert_eq!(
        arena.family428650_actor(next).unwrap().words_204_208[1],
        before_size
    );
    assert_eq!(
        arena.family428650_actor(next).unwrap().word_228,
        0x0053_11f0
    );
}

// The genuine follow-target branch does not acquire terrain. Any terrain or
// point-publication service call in these supplied tests is a regression.
struct UnreachedGrantHost;
impl crate::object_runtime::FamilyGrantGridHost for UnreachedGrantHost {
    type Error = &'static str;
    type Terrain = ();
    fn acquire_terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<()>, Self::Error> {
        panic!("follow target does not acquire H12")
    }
    fn terrain(&self, _: ()) -> Result<&TerrainRuntime, Self::Error> {
        panic!("unreached H12")
    }
    fn spacing(&mut self) -> TerrainSpacing {
        panic!("unreached spacing")
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        panic!("unreached mask")
    }
    fn height_scale(&mut self) -> TerrainHeightScale {
        panic!("unreached height")
    }
    fn placement_kind(&mut self, _: u8) -> Result<u8, Self::Error> {
        panic!("unreached H11")
    }
    fn furniture_grant_id(&mut self, _: u8) -> Result<u16, Self::Error> {
        panic!("unreached table")
    }
    fn publish_grid_point(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::FamilyGrantGridPointRequest,
    ) -> Result<bool, Self::Error> {
        panic!("preferred update does not publish a new grid point")
    }
}

#[test]
fn preferred_update_keeps_zero_grant_lazy_and_extended_distance_does_not_fill_later_fields() {
    use crate::object_runtime::{
        LiveGrantContext, Selector12PreferredUpdateError, Selector12PreferredUpdateRequest,
    };
    let context = LiveGrantContext {
        process: None,
        executable: None,
        resident: None,
        text_data: None,
    };
    for policy in POLICIES {
        let mut arena = selected(policy);
        let actor = construct(&mut arena, Constructor::Selector12, &mut 0, &grants());
        arena.owned_mut(actor).unwrap().state.base.position_words = Some([10, 20, 30]);
        let data = arena
            .follow_target(FollowTargetRequest::resolve_actor(
                arena.actor(actor).unwrap().handle,
            ))
            .unwrap();
        let reference = arena.import_fixture_target(NativeTarget::ActorFollow(data));
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
        let request = Selector12PreferredUpdateRequest {
            microseconds: 1,
            live_grants: &context,
        };
        let mut host = UnreachedGrantHost;
        arena.actor_mut(actor).unwrap().word_7a = 0;
        arena
            .update_selector12_preferred_target(actor, request, &mut host)
            .unwrap();
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_22e & 0x0c,
            0x0c
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[4],
            None
        );
        arena.actor_mut(actor).unwrap().word_7a = 1;
        let result = arena.update_selector12_preferred_target(actor, request, &mut host);
        let expected = if policy == EXTENDED { 0x198 } else { 0x14c };
        assert!(
            matches!(result, Err(Selector12PreferredUpdateError::RequiredField { actor: stopped, offset }) if stopped == actor && offset == expected)
        );
        assert_eq!(
            arena.family428650_actor(actor).unwrap().words_188_19c[2],
            None
        );
        if policy == EXTENDED {
            arena.family428650_actor_mut(actor).unwrap().words_188_19c[4] = Some(0);
            assert!(
                matches!(arena.update_selector12_preferred_target(actor, request, &mut host),
                Err(Selector12PreferredUpdateError::RequiredField { actor: stopped, offset: 0x190 }) if stopped == actor)
            );
            arena.family428650_actor_mut(actor).unwrap().words_188_19c[2] = Some(1000);
            arena
                .update_selector12_preferred_target(actor, request, &mut host)
                .unwrap();
            assert_eq!(
                arena.family428650_actor(actor).unwrap().words_188_19c[2],
                Some(999)
            );
        }
    }
}

mod native_guards;
