use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    grant_text::{GrantTextData, GrantTextError, GrantTextLiteral, GrantTextPairClassification},
    input::{ActionDescriptor, AnalogActionDescriptor},
    object_runtime::{
        ActorRegistryConfig, GrantOwner, PlayerInputDescriptor, PlayerInputDescriptorKind,
        PlayerInputDescriptorRequest, PlayerMessageError, PlayerMessageOperation,
        PlayerMessageRequest,
    },
    process_particles::factories::ParticleFactoryProcessState,
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{
    grant_closure, parse_digital_action_descriptor, parse_grant_table, FrontendExecutable,
};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn executable() -> Vec<u8> {
    FrontendAssets::open(&root())
        .unwrap()
        .executable_bytes()
        .to_vec()
}

struct Descriptors<'a> {
    executable: FrontendExecutable<'a>,
    requests: Vec<PlayerInputDescriptorRequest>,
    fail_at: Option<u32>,
}

impl<'a> Descriptors<'a> {
    fn new(executable: FrontendExecutable<'a>) -> Self {
        Self {
            executable,
            requests: Vec::new(),
            fail_at: None,
        }
    }
}

impl PlayerInputDescriptorSource for Descriptors<'_> {
    type Error = &'static str;

    fn descriptor(
        &mut self,
        request: PlayerInputDescriptorRequest,
    ) -> Result<PlayerInputDescriptor, Self::Error> {
        self.requests.push(request);
        if self.fail_at == Some(request.player_offset) {
            return Err("unavailable current descriptor");
        }
        match request.kind {
            PlayerInputDescriptorKind::Analog => {
                // Explicit known startup/current fixture, backed by the supported
                // PE loader zero-tail evidence. The file-only parser must reject
                // this domain; no executable read fallback is being exercised.
                assert!(inf_formats::parse_analog_action_descriptor(
                    self.executable,
                    request.identity
                )
                .is_err());
                Ok(PlayerInputDescriptor::Analog(AnalogActionDescriptor {
                    action: ActionDescriptor {
                        identity: request.identity,
                        callback_address: None,
                        flags: 0,
                    },
                    period_micros: 0,
                }))
            }
            PlayerInputDescriptorKind::Digital => {
                let descriptor =
                    parse_digital_action_descriptor(self.executable, request.identity).unwrap();
                Ok(PlayerInputDescriptor::Digital(ActionDescriptor {
                    identity: descriptor.identity,
                    callback_address: (descriptor.callback_address != 0)
                        .then_some(descriptor.callback_address),
                    flags: descriptor.flags,
                }))
            }
        }
    }
}

fn process(network_flag: u32, null: Option<u32>) -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        network_flag,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: null.map(ActorHandle::from_raw),
    }
}

fn context<'a>(
    process: Option<&'a ParticleFactoryProcessState>,
    executable: Option<FrontendExecutable<'a>>,
) -> LiveGrantContext<'a> {
    LiveGrantContext {
        process,
        executable,
        resident: None,
        text_data: None,
    }
}

fn boot() -> ResidentRuntime {
    let mut resident = ResidentRuntime::new(Installation::open(&root()).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
}

#[test]
fn reservation_is_invisible_and_foreign_arena_fails_before_any_constructor_effect() {
    let bytes = executable();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let mut descriptors = Descriptors::new(exe);
    let mut original = ObjectArena::new();
    let mut carrier = original.reserve_world_player_construction(0x9000);
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Allocated);
    assert!(original.world_players().is_empty());
    assert!(original.world_player(carrier.player()).is_err());
    let mut foreign = ObjectArena::new();
    let mut input = InputRuntime::new(71);
    assert!(matches!(
        foreign.construct_world_player(
            &mut carrier,
            &mut input,
            &mut descriptors,
            &context(None, None)
        ),
        Err(PlayerConstructionError::ForeignArena)
    ));
    assert!(descriptors.requests.is_empty());
    assert!(carrier.dword_50.is_none());
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Allocated);
}

#[test]
fn input_failure_retains_pre_input_fields_and_never_constructs_message_or_reads_process() {
    let bytes = executable();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let mut descriptors = Descriptors::new(exe);
    descriptors.fail_at = Some(0x80);
    let mut arena = ObjectArena::new();
    let mut carrier = arena.reserve_world_player_construction(0x9100);
    let mut input = InputRuntime::new(72);
    assert!(
        matches!(arena.construct_world_player(&mut carrier, &mut input, &mut descriptors, &context(None, None)), Err(PlayerConstructionError::Input(PlayerInputError::Source { request, .. })) if request.player_offset == 0x80)
    );
    assert_eq!(carrier.dword_48, Some(0xfffffc18));
    assert_eq!(carrier.dword_4c, Some(0));
    assert_eq!(carrier.dword_50, Some(0));
    assert_eq!(carrier.byte_58, Some(1));
    assert!(carrier.unpublished_message().is_none());
    assert!(carrier.unpublished_grants().is_none());
    assert!(carrier.handle_d8.is_none());
    assert!(arena.world_players().is_empty());
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Stopped);
    carrier.release_input_references(&mut input).unwrap();
    let count = descriptors.requests.len();
    assert!(matches!(
        arena.construct_world_player(
            &mut carrier,
            &mut input,
            &mut descriptors,
            &context(None, None)
        ),
        Err(PlayerConstructionError::AlreadyAttempted(
            PlayerConstructionPhase::Stopped
        ))
    ));
    assert_eq!(descriptors.requests.len(), count);
}

#[test]
fn missing_process_keeps_first_grant_bit_after_all_inputs_without_public_lookup() {
    let bytes = executable();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let mut descriptors = Descriptors::new(exe);
    let mut arena = ObjectArena::new();
    let mut carrier = arena.reserve_world_player_construction(0x9200);
    let mut input = InputRuntime::new(73);
    assert!(
        matches!(arena.construct_world_player(&mut carrier, &mut input, &mut descriptors, &context(None, None)), Err(PlayerConstructionError::Grant(LiveGrantError::MissingProcess { owner })) if owner == GrantOwner::Player9c(carrier.player()))
    );
    assert_eq!(
        descriptors
            .requests
            .iter()
            .map(|r| r.player_offset)
            .collect::<Vec<_>>(),
        [0x5c, 0x60, 0x64, 0x68, 0x6c, 0x70, 0x74, 0x78, 0x7c, 0x80, 0x84]
    );
    assert_eq!(carrier.unpublished_grants(), Some(&[2, 0, 0, 0, 0, 0]));
    let message = carrier.unpublished_message().unwrap();
    assert_eq!(message.suppression_byte(), 1);
    let recipient = message.identity();
    assert!(
        matches!(arena.apply_player_message(PlayerMessageRequest { recipient, operation: PlayerMessageOperation::Send { text: None, flag: 0 } }), Err(PlayerMessageError::UnknownMessage(id)) if id == recipient)
    );
    assert!(arena.first_player().is_none());
    carrier.release_input_references(&mut input).unwrap();
}

#[test]
fn suppressed_constructor_still_requires_original_h0_before_null_and_publication() {
    let bytes = executable();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let current = process(0, None);
    let mut descriptors = Descriptors::new(exe);
    let mut arena = ObjectArena::new();
    let first = arena.add_world_player([0xdeadbeef; 6]);
    let before = arena.world_players().to_vec();
    let mut carrier = arena.reserve_world_player_construction(0x9300);
    let mut input = InputRuntime::new(74);
    assert!(matches!(
        arena.construct_world_player(
            &mut carrier,
            &mut input,
            &mut descriptors,
            &context(Some(&current), Some(exe))
        ),
        Err(PlayerConstructionError::Grant(
            LiveGrantError::MissingResidency { index: 0x180 }
        ))
    ));
    assert_eq!(carrier.unpublished_grants(), Some(&[2, 0, 0, 0, 0, 0]));
    assert_eq!(carrier.unpublished_message().unwrap().suppression_byte(), 1);
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(arena.world_players(), before);
    carrier.release_input_references(&mut input).unwrap();
}

#[test]
fn offline_crt_error_precedes_missing_null_even_with_suppressed_message() {
    let bytes = executable();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let resident = boot();
    let current = process(0, None);
    let mut descriptors = Descriptors::new(exe);
    let mut arena = ObjectArena::new();
    let mut carrier = arena.reserve_world_player_construction(0x9400);
    let mut input = InputRuntime::new(75);
    let context = LiveGrantContext {
        resident: Some(&resident),
        ..context(Some(&current), Some(exe))
    };
    assert!(matches!(
        arena.construct_world_player(&mut carrier, &mut input, &mut descriptors, &context),
        Err(PlayerConstructionError::Grant(LiveGrantError::Text(
            GrantTextError::MissingClassificationBinding(GrantTextLiteral::Colon)
        )))
    ));
    assert!(arena.world_players().is_empty());
    assert!(carrier.handle_d8.is_none());
    assert_eq!(carrier.unpublished_grants(), Some(&[2, 0, 0, 0, 0, 0]));
    carrier.release_input_references(&mut input).unwrap();
}

#[test]
fn complete_network_grants_reach_missing_null_late_and_preserve_all_prefixes() {
    let bytes = executable();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let current = process(0x100, None);
    let mut descriptors = Descriptors::new(exe);
    let mut arena = ObjectArena::new();
    let mut carrier = arena.reserve_world_player_construction(0x9500);
    let mut input = InputRuntime::new(76);
    assert!(matches!(
        arena.construct_world_player(
            &mut carrier,
            &mut input,
            &mut descriptors,
            &context(Some(&current), Some(exe))
        ),
        Err(PlayerConstructionError::MissingProcessNull)
    ));
    assert!(arena.world_players().is_empty());
    let mask = carrier.unpublished_grants().unwrap();
    assert_eq!(
        *mask,
        grant_closure(&parse_grant_table(exe).unwrap(), &PLAYER_INITIAL_GRANTS).unwrap()
    );
    for id in PLAYER_INITIAL_GRANTS {
        assert_ne!(mask[(id >> 5) as usize] & (1 << (id & 31)), 0);
    }
    assert_eq!(carrier.unpublished_message().unwrap().suppression_byte(), 1);
    assert_eq!(descriptors.requests.len(), 11);
    assert_eq!(input.clock_micros(), 76);
    carrier.release_input_references(&mut input).unwrap();
}

#[test]
fn complete_offline_constructor_publishes_actual_components_and_current_null_in_fifo_order() {
    let bytes = executable();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let resident = boot();
    let current = process(0, Some(0xabc00123));
    let before_process = current;
    let text = GrantTextData {
        null_substitute: None,
        pair_classification: Some(GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    };
    let context = LiveGrantContext {
        resident: Some(&resident),
        text_data: Some(&text),
        ..context(Some(&current), Some(exe))
    };
    let mut descriptors = Descriptors::new(exe);
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x08010000),
    })
    .unwrap();
    let first = arena.add_world_player([0x12345678; 6]);
    let first_before = arena.world_player(first).unwrap().clone();
    let mut carrier = arena.reserve_world_player_construction(0x9600);
    let reserved = carrier.player();
    let mut input = InputRuntime::new(77);
    let selected = input.selected_context();
    assert_eq!(
        arena
            .construct_world_player(&mut carrier, &mut input, &mut descriptors, &context)
            .unwrap(),
        reserved
    );
    assert_eq!(arena.first_player(), Some(first));
    assert_eq!(
        arena
            .world_players()
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [first, reserved]
    );
    assert_eq!(arena.world_player(first).unwrap(), &first_before);
    let live = arena.world_player(reserved).unwrap();
    assert_eq!(
        live.grants_9c,
        grant_closure(&parse_grant_table(exe).unwrap(), &PLAYER_INITIAL_GRANTS).unwrap()
    );
    assert_eq!(live.handle_d4, ActorHandle::from_raw(0xabc00123));
    assert_eq!(live.handle_d8, live.handle_d4);
    // 00469F8E zeroes the +04..+47 control block.
    assert_eq!(
        live.controls,
        crate::object_runtime::PlayerControlBlock::default()
    );
    assert_eq!((live.dword_48, live.dword_4c), (Some(0xfffffc18), Some(0)));
    assert_eq!(live.dword_50, Some(0));
    assert_eq!((live.dword_f4, live.dword_f8, live.byte_fc), (0, 0, 0));
    assert_eq!(live.message_byte_00(), 0);
    assert_eq!(live.command_mask_130, [0; 3]);
    assert_eq!(live.saved_e0, None);
    assert_eq!(live.byte_ec, None);
    assert_eq!(
        arena.player_message(reserved).unwrap().suppression_byte(),
        1
    );
    assert!(carrier.unpublished_grants().is_none());
    assert!(carrier.unpublished_message().is_none());
    assert_eq!(carrier.handle_dc, Some(live.handle_d4));
    assert_eq!(live.handle_dc, live.handle_d4);
    assert_eq!(carrier.phase(), PlayerConstructionPhase::Published);
    assert_eq!(carrier.callback_context(), 0x9600);
    assert_eq!(current, before_process);
    assert_eq!(input.clock_micros(), 77);
    assert_eq!(input.selected_context(), selected);
    assert_eq!(input.queued_len(), 0);
    // Reference cleanup is explicitly a component phase; preceding full-player
    // destruction is outside this test and is not silently simulated here.
    carrier.release_input_references(&mut input).unwrap();
}
