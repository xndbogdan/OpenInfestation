use std::path::{Path, PathBuf};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    grant_text::{
        GrantTextArgument, GrantTextLiteral, GrantTextNullBinding, GrantTextPairClassification,
        GrantTextSource,
    },
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest, ObjectClassKind, Selector12GrantContext, WorldPlayer,
    },
    HandlerResources, Installation, ResidencyRequest,
};
use inf_formats::{GrantTable, GRANT_TABLE_ADDRESS};

// This actual constructor arm does not consume terrain/sine data.
const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn executable_bytes() -> Vec<u8> {
    FrontendAssets::open(&data_root())
        .unwrap()
        .executable_bytes()
        .to_vec()
}

fn arena() -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x0801_0000),
    })
    .unwrap();
    let base = arena.allocate(ObjectClassKind::Base);
    arena.set_flag0(base, true).unwrap();
    arena.allocate(ObjectClassKind::VisualBase);
    arena
}

fn selector12(arena: &mut ObjectArena) -> ObjectId {
    let grants = GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let mut rng = 0;
    arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: None,
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &UNUSED_SINE,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut rng,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap()
}

fn mask(ids: &[u16]) -> [u32; GRANT_MASK_WORDS] {
    let mut mask = [0; GRANT_MASK_WORDS];
    for &id in ids {
        mask[usize::from(id >> 5)] |= 1 << (id & 31);
    }
    mask
}

fn high_owned() -> [u32; GRANT_MASK_WORDS] {
    mask(&(0x95..0xaf).collect::<Vec<_>>())
}

fn absent_context() -> LiveGrantContext<'static> {
    LiveGrantContext {
        process: None,
        executable: None,
        resident: None,
        text_data: None,
    }
}

fn process(network_flag: u32) -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        network_flag,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    }
}

fn set(
    arena: &mut ObjectArena,
    target: GrantOwner,
    id: u32,
    context: &LiveGrantContext<'_>,
) -> Result<(), LiveGrantError> {
    arena.mutate_live_grants(
        LiveGrantRequest {
            target,
            operation: LiveGrantOperation::SetRaw { id },
        },
        context,
    )
}

// A genuine PlayerId with no corresponding local owner. This does not alter
// the existing bare PlayerId's arena-local numeric identity policy.
fn missing_player() -> PlayerId {
    let mut foreign = ObjectArena::new();
    foreign.add_world_player([0; 6]);
    foreign.add_world_player([0; 6])
}

fn expected_mask(players: &[WorldPlayer], player: PlayerId, grants: [u32; 6]) -> Vec<WorldPlayer> {
    let mut expected = players.to_vec();
    expected
        .iter_mut()
        .find(|p| p.id == player)
        .unwrap()
        .grants_9c = grants;
    expected
}

// All actual arena owners except player records, compared separately with
// exactly the declared mask changes.
fn other_owners(arena: &ObjectArena) -> String {
    format!(
        "{:?}{:?}{:?}",
        (
            &arena.slots,
            &arena.free,
            arena.live_count,
            &arena.lists,
            &arena.list_owners,
            &arena.routes,
            &arena.actors,
            &arena.map_annotations,
        ),
        (
            &arena.actor_handles,
            &arena.actor_payloads,
            &arena.constructor_allocations,
            &arena.selector60_rows,
            arena.world_load,
        ),
        (arena.identity, arena.players.camera_74),
    )
}

fn patch_record(bytes: &mut [u8], id: u16, offset: u32, value: &[u8]) {
    let span = FrontendExecutable::parse(bytes)
        .unwrap()
        .mapped(resources::record_address(id) + offset, value.len())
        .unwrap()
        .file_span;
    bytes[span.start..span.end].copy_from_slice(value);
}

// An explicitly supplied structural-EXE variant: retain all original bytes,
// but narrow the file-backed section mapping at an individual reached field.
// No retail file, original offset or original section bytes are overwritten.
fn limit_mapping(bytes: &mut [u8], end_address: u32) {
    let word = |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let pe = word(bytes, 0x3c) as usize;
    let count = u16::from_le_bytes(bytes[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let optional = u16::from_le_bytes(bytes[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let sections = pe + 24 + optional;
    for at in (sections..sections + count * 40).step_by(40) {
        if &bytes[at..at + 8] != b".data\0\0\0" && &bytes[at..at + 8] != b".rdata\0\0" {
            continue;
        }
        let start = 0x0040_0000 + word(bytes, at + 12);
        let size = word(bytes, at + 16);
        if start < end_address && end_address <= start + size {
            bytes[at + 16..at + 20].copy_from_slice(&(end_address - start).to_le_bytes());
            FrontendExecutable::parse(bytes).unwrap();
            return;
        }
    }
    panic!("declared field boundary must be inside an actual mapped data section");
}

fn boot() -> ResidentRuntime {
    let mut resident = ResidentRuntime::new(Installation::open(&data_root()).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
}

#[test]
fn lowword_zero_and_already_owned_bypass_all_later_owners() {
    let mut arena = arena();
    let missing = GrantOwner::Player9c(missing_player());
    let before = format!("{arena:?}");
    set(&mut arena, missing, 0xdead_0000, &absent_context()).unwrap();
    assert_eq!(format!("{arena:?}"), before);

    let player = arena.add_world_player(mask(&[1, 191]));
    let before = format!("{arena:?}");
    for raw in [0x10001, 0xffff_00bf] {
        set(
            &mut arena,
            GrantOwner::Player9c(player),
            raw,
            &absent_context(),
        )
        .unwrap();
    }
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn newly_committed_bit_survives_missing_process_without_any_other_write() {
    let mut arena = arena();
    let first = arena.add_world_player(mask(&[2]));
    let second = arena.add_world_player(mask(&[3, 191]));
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    let target = GrantOwner::Player9c(second);
    assert!(
        matches!(set(&mut arena, target, 0xface_0040, &absent_context()),
        Err(LiveGrantError::MissingProcess { owner }) if owner == target)
    );
    assert_eq!(
        arena.world_players(),
        expected_mask(&players, second, mask(&[3, 64, 191]))
    );
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn owner_authentication_precedes_adjacent_mask_and_full_network_bypasses_records() {
    let mut arena = arena();
    let player = arena.add_world_player(high_owned());
    let target = GrantOwner::Player9c(player);
    let before = format!("{arena:?}");
    assert!(matches!(set(&mut arena, target, 192, &absent_context()),
        Err(LiveGrantError::MaskBoundary { owner, word_index: 6 }) if owner == target));
    let missing = missing_player();
    assert!(
        matches!(set(&mut arena, GrantOwner::Player9c(missing), 192, &absent_context()),
        Err(LiveGrantError::Player(PlayerAttachError::UnknownPlayer(id))) if id == missing)
    );
    assert_eq!(format!("{arena:?}"), before);

    let current_process = process(0x100);
    let retained_process = current_process;
    let context = LiveGrantContext {
        process: Some(&current_process),
        ..absent_context()
    };
    set(&mut arena, target, 0xffff_00bf, &context).unwrap();
    assert!(arena.live_grant_has(target, 191).unwrap());
    assert_eq!(current_process, retained_process);
    assert_eq!(current_process.process_null_handle, None);
    let bytes = executable_bytes();
    let context = LiveGrantContext {
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..context
    };
    assert!(matches!(
        resources::read_text(&context, 191),
        Err(LiveGrantError::AdjacentRecord { id: 191, .. })
    ));
}

#[test]
fn selector12_null_and_unrepresented_binding_remain_late_offline_policies() {
    let mut arena = arena();
    let actor = selector12(&mut arena);
    arena
        .selector12_actor_mut(actor)
        .unwrap()
        .grants_24c
        .mask_04 = high_owned();
    arena
        .selector12_actor_mut(actor)
        .unwrap()
        .grants_24c
        .vtable_00 = 0xdead_beef;
    arena.actor_mut(actor).unwrap().word_b4 = u32::MAX;
    let target = GrantOwner::Selector12Actor24c(actor);
    assert!(matches!(
        set(&mut arena, target, 1, &absent_context()),
        Err(LiveGrantError::MissingProcess { .. })
    ));
    let offline = process(0);
    let context = LiveGrantContext {
        process: Some(&offline),
        ..absent_context()
    };
    set(&mut arena, target, 2, &context).unwrap();
    arena
        .selector12_actor_mut(actor)
        .unwrap()
        .grants_24c
        .notification_owner_1c = 0x1234_5678;
    assert!(matches!(set(&mut arena, target, 3, &context),
        Err(LiveGrantError::NotificationBinding { actor: id, raw: 0x1234_5678 }) if id == actor));
    let online = process(0x100);
    let context = LiveGrantContext {
        process: Some(&online),
        ..absent_context()
    };
    set(&mut arena, target, 4, &context).unwrap();
    let selector = arena.selector12_actor(actor).unwrap();
    let mut expected = high_owned();
    expected[0] |= 0x1e;
    assert_eq!(selector.grants_24c.mask_04, expected);
    assert_eq!(selector.grants_24c.vtable_00, 0xdead_beef);
    assert_eq!(selector.byte_29d, 0);
    assert_eq!(arena.actor(actor).unwrap().word_b4, u32::MAX);
}

#[test]
fn zero_text_bypasses_unavailable_flags_residency_formatter_and_enabled_message() {
    let mut bytes = executable_bytes();
    limit_mapping(&mut bytes, resources::record_address(0x32) + 2);
    let current_process = process(0);
    let context = LiveGrantContext {
        process: Some(&current_process),
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..absent_context()
    };
    assert_eq!(resources::read_text(&context, 0x32).unwrap(), 0);
    assert!(matches!(
        resources::read_flags(&context, 0x32),
        Err(LiveGrantError::Executable { .. })
    ));
    let mut arena = arena();
    let player = arena.add_world_player(high_owned());
    let recipient = arena.player_message(player).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    let before = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    set(&mut arena, GrantOwner::Player9c(player), 0x32, &context).unwrap();
    let mut expected = high_owned();
    expected[1] |= 1 << 18;
    assert_eq!(
        arena.world_players(),
        expected_mask(&before, player, expected)
    );
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn flag_bit_zero_wins_bit_three_and_single_nonnull_needs_no_crt_binding() {
    let resident = boot();
    let current_process = process(0);
    for flags in [1_u32, 9] {
        let mut bytes = executable_bytes();
        patch_record(&mut bytes, 1, 4, &flags.to_le_bytes());
        let context = LiveGrantContext {
            process: Some(&current_process),
            executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
            resident: Some(&resident),
            text_data: None,
        };
        assert!(matches!(
            capture_h0_text(H0TextCaptureRequest {
                index_word: 0x180,
                resident: context.resident
            })
            .unwrap(),
            GrantTextSource::Record(Some(_))
        ));
        let mut arena = arena();
        let player = arena.add_world_player(high_owned());
        set(&mut arena, GrantOwner::Player9c(player), 1, &context).unwrap();
        assert!(arena
            .live_grant_has(GrantOwner::Player9c(player), 1)
            .unwrap());
    }
}

#[test]
fn pair_failure_precedes_enabled_message_and_complete_format_reaches_it_before_closure() {
    let resident = boot();
    let mut bytes = executable_bytes();
    limit_mapping(&mut bytes, resources::record_address(1) + 8);
    let current_process = process(0);
    let classification = GrantTextData {
        null_substitute: None,
        pair_classification: Some(GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    };
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    let target = GrantOwner::Player9c(player);
    let recipient = arena.player_message(player).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    let context = LiveGrantContext {
        process: Some(&current_process),
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        resident: Some(&resident),
        text_data: None,
    };
    assert!(matches!(
        set(&mut arena, target, 1, &context),
        Err(LiveGrantError::Text(
            GrantTextError::MissingClassificationBinding(GrantTextLiteral::Colon)
        ))
    ));
    assert_eq!(arena.world_player(player).unwrap().grants_9c, mask(&[1]));
    assert_eq!(arena.player_message(player).unwrap().suppression_byte(), 0);
    // A fresh real owner reaches the complete lower phases; the already-owned
    // entry on the first player itself never retries.
    let second = arena.add_world_player([0; 6]);
    let second_recipient = arena.player_message(second).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient: second_recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    let context = LiveGrantContext {
        text_data: Some(&classification),
        ..context
    };
    assert!(
        matches!(set(&mut arena, GrantOwner::Player9c(second), 1, &context),
        Err(LiveGrantError::Message(PlayerMessageError::EnabledMessageBoundary(id))) if id == second_recipient)
    );
    assert_eq!(arena.world_player(second).unwrap().grants_9c, mask(&[1]));
    assert!(matches!(
        resources::read_dependency(&context, 0x95, 0),
        Err(LiveGrantError::Executable { .. })
    ));
}

#[test]
fn pair_acquires_record_before_category_even_though_category_formats_first() {
    let bytes = executable_bytes();
    let current_process = process(0);
    let context = LiveGrantContext {
        process: Some(&current_process),
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..absent_context()
    };
    let mut arena = arena();
    let player = arena.add_world_player(high_owned());
    // Original ID1 is Pair13C with record0180. The first unavailable pool
    // read must name the record, before category013C or the formatter.
    assert_eq!(resources::read_text(&context, 1).unwrap(), 0x180);
    assert_eq!(resources::read_flags(&context, 1).unwrap(), 0);
    assert!(matches!(
        set(&mut arena, GrantOwner::Player9c(player), 1, &context),
        Err(LiveGrantError::MissingResidency { index: 0x180 })
    ));
    assert!(arena
        .live_grant_has(GrantOwner::Player9c(player), 1)
        .unwrap());
}

#[test]
fn signed_text_word_bypasses_missing_h0_but_still_requires_real_null_formatting() {
    let mut bytes = executable_bytes();
    patch_record(&mut bytes, 1, 0, &0xffff_u16.to_le_bytes());
    patch_record(&mut bytes, 1, 4, &1_u32.to_le_bytes());
    let current_process = process(0);
    let substitute = b"current-null\0";
    let text_data = GrantTextData {
        null_substitute: Some(GrantTextNullBinding {
            record: Some(substitute),
        }),
        pair_classification: None,
    };
    let context = LiveGrantContext {
        process: Some(&current_process),
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        resident: None,
        text_data: None,
    };
    assert_eq!(
        capture_h0_text(H0TextCaptureRequest {
            index_word: 0xffff,
            resident: context.resident
        })
        .unwrap(),
        GrantTextSource::Null
    );
    let mut arena = arena();
    let first = arena.add_world_player(high_owned());
    assert!(matches!(
        set(&mut arena, GrantOwner::Player9c(first), 1, &context),
        Err(LiveGrantError::Text(GrantTextError::MissingNullBinding(
            GrantTextArgument::Record
        )))
    ));
    let second = arena.add_world_player(high_owned());
    let context = LiveGrantContext {
        text_data: Some(&text_data),
        ..context
    };
    set(&mut arena, GrantOwner::Player9c(second), 1, &context).unwrap();
    assert_eq!(substitute, b"current-null\0");
}

#[test]
fn h0_signed_greater_equal_and_canonical_hole_have_distinct_lazy_policies() {
    let inactive = ResidentRuntime::new(Installation::open(&data_root()).unwrap()).unwrap();
    assert_eq!(inactive.resource_capacity(0), Some(950));
    let missing = absent_context();
    assert_eq!(
        capture_h0_text(H0TextCaptureRequest {
            index_word: 0xffff,
            resident: missing.resident
        })
        .unwrap(),
        GrantTextSource::Null
    );
    assert!(matches!(
        capture_h0_text(H0TextCaptureRequest {
            index_word: 951,
            resident: missing.resident
        }),
        Err(H0TextCaptureError::MissingResidency { index: 951 })
    ));
    let context = LiveGrantContext {
        resident: Some(&inactive),
        ..absent_context()
    };
    assert_eq!(
        capture_h0_text(H0TextCaptureRequest {
            index_word: 951,
            resident: context.resident
        })
        .unwrap(),
        GrantTextSource::Null
    );
    assert_eq!(
        capture_h0_text(H0TextCaptureRequest {
            index_word: 1,
            resident: context.resident
        })
        .unwrap(),
        GrantTextSource::Null
    );
    assert!(matches!(
        capture_h0_text(H0TextCaptureRequest {
            index_word: 950,
            resident: context.resident
        }),
        Err(H0TextCaptureError::OnePast {
            index: 950,
            count: 950
        })
    ));
}

#[test]
fn actual_boot_registration_selects_local_nul_record_without_changing_current_owners() {
    let resident = boot();
    let before = resident.summary();
    let revision = resident.revision();
    let context = LiveGrantContext {
        resident: Some(&resident),
        ..absent_context()
    };
    let (index, resource) = (0..resident.resource_capacity(0).unwrap())
        .filter_map(|index| {
            resident
                .resource(0, index as u16)
                .map(|r| (index as u16, r))
        })
        .find(|(_, r)| r.registration.local_index != r.registration.global_index)
        .expect("actual Boot contains an offset string registration");
    let HandlerResources::Strings(bank) =
        &resource.overlay.handlers()[resource.registration.region_index]
    else {
        panic!("actual H0 string bank")
    };
    let expected = bank.entries[usize::from(resource.registration.local_index)]
        .record
        .get(resource.overlay.bytes())
        .unwrap();
    assert_eq!(expected.last(), Some(&0));
    assert_eq!(
        capture_h0_text(H0TextCaptureRequest {
            index_word: index,
            resident: context.resident
        })
        .unwrap(),
        GrantTextSource::Record(Some(expected))
    );
    assert_eq!(resident.summary(), before);
    assert_eq!(resident.revision(), revision);
}

#[test]
fn captured_second_player_notification_never_retargets_first_player() {
    let bytes = executable_bytes();
    let resident = boot();
    let current_process = process(0);
    let context = LiveGrantContext {
        process: Some(&current_process),
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        resident: Some(&resident),
        text_data: None,
    };
    let mut arena = arena();
    let first = arena.add_world_player([0; 6]);
    let first_recipient = arena.player_message(first).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient: first_recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    let second = arena.add_world_player(high_owned());
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    // Original record23 has Single flags3 and a real Boot resident string.
    assert_eq!(resources::read_flags(&context, 23).unwrap(), 3);
    set(&mut arena, GrantOwner::Player9c(second), 23, &context).unwrap();
    let mut expected = high_owned();
    expected[0] |= 1 << 23;
    assert_eq!(
        arena.world_players(),
        expected_mask(&players, second, expected)
    );
    assert_eq!(other_owners(&arena), owners);
    assert_eq!(arena.first_player(), Some(first));
}

#[test]
fn first_satisfied_dependency_recurses_before_later_words_and_retains_parent_child_bits() {
    let mut bytes = executable_bytes();
    limit_mapping(&mut bytes, resources::record_address(0x95) + 10);
    let current_process = process(0x100);
    let context = LiveGrantContext {
        process: Some(&current_process),
        executable: Some(FrontendExecutable::parse(&bytes).unwrap()),
        ..absent_context()
    };
    assert_eq!(resources::read_dependency(&context, 0x95, 0).unwrap(), 0x60);
    assert!(matches!(
        resources::read_dependency(&context, 0x95, 1),
        Err(LiveGrantError::Executable { id: 0x95, .. })
    ));
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    assert!(
        matches!(set(&mut arena, GrantOwner::Player9c(player), 0x60, &context),
        Err(LiveGrantError::Executable { id: 0x96, address, bytes: 2, .. }) if address == resources::record_address(0x96) + 8)
    );
    assert_eq!(
        arena.world_player(player).unwrap().grants_9c,
        mask(&[0x60, 0x95])
    );
}

#[test]
fn merge_source_first_empty_bypass_and_child_failure_stop_later_source_ids() {
    let mut arena = arena();
    let source = arena.add_world_player([0; 6]);
    let missing = missing_player();
    arena
        .mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Player9c(missing),
                operation: LiveGrantOperation::Merge {
                    source: GrantOwner::Player9c(source),
                },
            },
            &absent_context(),
        )
        .unwrap();
    let actor = selector12(&mut arena);
    let base = arena.allocate(ObjectClassKind::Base);
    assert!(matches!(arena.mutate_live_grants(LiveGrantRequest {
        target: GrantOwner::Selector12Actor24c(base), operation: LiveGrantOperation::Merge { source: GrantOwner::Player9c(missing) },
    }, &absent_context()), Err(LiveGrantError::Player(PlayerAttachError::UnknownPlayer(id))) if id == missing));
    arena.world_player_mut(source).unwrap().grants_9c = mask(&[1, 64]);
    assert!(matches!(
        arena.mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Selector12Actor24c(actor),
                operation: LiveGrantOperation::Merge {
                    source: GrantOwner::Player9c(source)
                },
            },
            &absent_context()
        ),
        Err(LiveGrantError::MissingProcess { .. })
    ));
    assert_eq!(
        arena.selector12_actor(actor).unwrap().grants_24c.mask_04,
        mask(&[1])
    );
    assert_eq!(
        arena.world_player(source).unwrap().grants_9c,
        mask(&[1, 64])
    );
}

#[test]
fn merge_ignores_source_high_ids_preserves_target_high_bits_and_same_owner_bypasses_context() {
    let mut arena = arena();
    let source = arena.add_world_player(mask(&[0, 1, 175, 191]));
    let mut initial = high_owned();
    initial[5] |= 1 << 30;
    let target = arena.add_world_player(initial);
    let current_process = process(0x100);
    let context = LiveGrantContext {
        process: Some(&current_process),
        ..absent_context()
    };
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    arena
        .mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Player9c(target),
                operation: LiveGrantOperation::Merge {
                    source: GrantOwner::Player9c(source),
                },
            },
            &context,
        )
        .unwrap();
    let mut expected = initial;
    expected[0] |= 2;
    assert_eq!(
        arena.world_players(),
        expected_mask(&players, target, expected)
    );
    assert_eq!(other_owners(&arena), owners);
    let before = format!("{arena:?}");
    arena
        .mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Player9c(target),
                operation: LiveGrantOperation::Merge {
                    source: GrantOwner::Player9c(target),
                },
            },
            &absent_context(),
        )
        .unwrap();
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn copy_transfers_all_six_words_without_context_headers_messages_or_closure() {
    let mut arena = arena();
    let actor = selector12(&mut arena);
    let values = [
        0xa5a5_0001,
        0x1234_5678,
        0x8765_4321,
        0x8000_0000,
        0,
        0xffff_ffff,
    ];
    let selector = arena.selector12_actor_mut(actor).unwrap();
    selector.grants_24c.mask_04 = values;
    selector.grants_24c.notification_owner_1c = u32::MAX;
    selector.grants_24c.vtable_00 = 0xdead_beef;
    let player = arena.add_world_player([0; 6]);
    let recipient = arena.player_message(player).unwrap().identity();
    arena
        .apply_player_message(PlayerMessageRequest {
            recipient,
            operation: PlayerMessageOperation::Enable,
        })
        .unwrap();
    let players = arena.world_players().to_vec();
    let owners = other_owners(&arena);
    arena
        .mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Player9c(player),
                operation: LiveGrantOperation::Copy {
                    source: GrantOwner::Selector12Actor24c(actor),
                },
            },
            &absent_context(),
        )
        .unwrap();
    assert_eq!(
        arena.world_players(),
        expected_mask(&players, player, values)
    );
    assert_eq!(other_owners(&arena), owners);
}

#[test]
fn copy_equal_identity_still_authenticates_source_before_target() {
    let mut arena = arena();
    let player = arena.add_world_player([u32::MAX, 1, 2, 3, 4, 5]);
    let owner = GrantOwner::Player9c(player);
    let before = format!("{arena:?}");
    arena
        .mutate_live_grants(
            LiveGrantRequest {
                target: owner,
                operation: LiveGrantOperation::Copy { source: owner },
            },
            &absent_context(),
        )
        .unwrap();
    assert_eq!(format!("{arena:?}"), before);
    let missing = missing_player();
    let invalid = GrantOwner::Player9c(missing);
    for target in [
        invalid,
        GrantOwner::Selector12Actor24c(arena.allocate(ObjectClassKind::Base)),
    ] {
        assert!(
            matches!(arena.mutate_live_grants(LiveGrantRequest { target, operation: LiveGrantOperation::Copy { source: invalid } }, &absent_context()),
            Err(LiveGrantError::Player(PlayerAttachError::UnknownPlayer(id))) if id == missing)
        );
    }
}
