use std::collections::BTreeMap;

use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing},
    world_furniture_effect::FurnitureQueueRequest,
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

const PROGRAM: u32 = 0x1000;
const HOST_CALLBACK: u32 = 0x0040_1001;

fn terrain(lookup: u8) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + index * 2] = (-1_i8) as u8;
    }
    bytes[TERRAIN_PREFIX_BYTES + 1] = 4;
    let bucket_offset = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2;
    bytes[bucket_offset..bucket_offset + 2].copy_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0xa5, lookup]);
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
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

fn enqueue(queue: &mut FurnitureEffectQueue, terrain: &TerrainRuntime, xz: [u16; 2], address: u32) {
    queue
        .enqueue(
            terrain,
            FurnitureQueueRequest {
                xz,
                program: FurnitureEffectProgram::new(address).unwrap(),
            },
        )
        .unwrap();
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Word(u32),
    Placement(u8),
    Preload(u16),
    FallbackRead,
    FallbackWrite(u8),
    FallbackClear,
    Callback(
        usize,
        FurnitureCallbackRequest<u8>,
        FurnitureCallbackContext,
    ),
}

struct Host {
    words: BTreeMap<u32, u32>,
    events: Vec<Event>,
    fallback: TerrainSparsePayload,
    model_word: u16,
    null_placement: bool,
    fail_preload: bool,
    fail_fallback_read: bool,
    fail_fallback_clear_after_state: bool,
}

impl Host {
    fn new(records: &[[u32; 3]]) -> Self {
        let mut host = Self {
            words: BTreeMap::new(),
            events: Vec::new(),
            fallback: TerrainSparsePayload {
                state: 0xb0,
                resource_lookup: TerrainResourceLookup::from_raw(23),
            },
            model_word: 123,
            null_placement: false,
            fail_preload: false,
            fail_fallback_read: false,
            fail_fallback_clear_after_state: false,
        };
        host.program(PROGRAM, records);
        host
    }

    fn program(&mut self, address: u32, records: &[[u32; 3]]) {
        for (index, record) in records.iter().enumerate() {
            for (word, value) in record.iter().enumerate() {
                self.words
                    .insert(address + index as u32 * 12 + word as u32 * 4, *value);
            }
        }
    }
}

impl FurnitureEffectExecutorHost for Host {
    type Error = &'static str;
    type Placement = u8;

    fn program_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        self.events.push(Event::Word(address));
        self.words.get(&address).copied().ok_or("unmapped program")
    }

    fn placement_slot(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<FurnitureExecutionPlacement<Self::Placement>>, Self::Error> {
        self.events.push(Event::Placement(lookup.raw()));
        Ok(
            (!self.null_placement).then_some(FurnitureExecutionPlacement {
                identity: lookup.raw(),
                model_word: self.model_word as i16,
            }),
        )
    }

    fn preload_model_if_required(&mut self, model_slot: u16) -> Result<(), Self::Error> {
        self.events.push(Event::Preload(model_slot));
        if self.fail_preload {
            Err("H8 loader unavailable")
        } else {
            Ok(())
        }
    }

    fn fallback_sparse_payload(&mut self) -> Result<TerrainSparsePayload, Self::Error> {
        self.events.push(Event::FallbackRead);
        if self.fail_fallback_read {
            Err("fallback read unavailable")
        } else {
            Ok(self.fallback)
        }
    }

    fn write_fallback_sparse_state(&mut self, state: u8) -> Result<(), Self::Error> {
        self.events.push(Event::FallbackWrite(state));
        self.fallback.state = state;
        Ok(())
    }

    fn clear_fallback_sparse_payload(&mut self) -> Result<(), Self::Error> {
        self.events.push(Event::FallbackClear);
        self.fallback.state = 0;
        if self.fail_fallback_clear_after_state {
            return Err("fallback clear failed after state write");
        }
        self.fallback.resource_lookup = TerrainResourceLookup::from_raw(0);
        Ok(())
    }

    fn callback(
        &mut self,
        queue: &mut FurnitureEffectQueue,
        index: usize,
        terrain: &mut TerrainRuntime,
        context: &mut FurnitureCallbackContext,
        request: FurnitureCallbackRequest<Self::Placement>,
        rng: &mut u32,
    ) -> Result<bool, Self::Error> {
        self.events.push(Event::Callback(index, request, *context));
        if request.callback_address != HOST_CALLBACK {
            return Err("required callback unavailable");
        }
        match request.argument {
            0 => Ok(true), // Test callback explicitly records the delivered context.
            1 => {
                context.position_words[1] = 123;
                enqueue(queue, terrain, [128, 0], 0x2000);
                Ok(true)
            }
            2 => {
                queue.record_mut(index).unwrap().removal_flag = 7;
                name_entry_rng_next(rng);
                Err("required callback failed after writes")
            }
            3 => {
                queue.record_mut(index).unwrap().program =
                    FurnitureEffectProgram::new(0x3000).unwrap();
                Ok(true)
            }
            4 => {
                let mut local_request = request;
                local_request.placement = None;
                assert_eq!(local_request.placement, None);
                self.null_placement = true;
                context.position_words[0] = 128;
                Ok(true)
            }
            5 => {
                queue.record_mut(index).unwrap().removal_flag = 7;
                name_entry_rng_next(rng);
                context.position_words[0] = 128;
                Ok(true)
            }
            _ => Err("required callback unavailable"),
        }
    }
}

#[test]
fn microsecond_due_gate_precedes_resources_and_uses_aligned_raw_height() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [7, 9], PROGRAM);
    let original = queue.records()[0].position_words;
    let mut host = Host::new(&[[500_000, HOST_CALLBACK, 0], [500_000, 0, 0]]);
    assert_eq!(
        queue
            .tick(&mut terrain, 499_999, &mut host, &mut 0)
            .unwrap()
            .callbacks,
        0
    );
    assert_eq!(host.events, [Event::Word(PROGRAM)]);
    host.events.clear();
    let result = queue.tick(&mut terrain, 1, &mut host, &mut 0).unwrap();
    assert_eq!(result.callbacks, 1);
    assert_eq!(
        &host.events[..3],
        &[
            Event::Word(PROGRAM),
            Event::Placement(5),
            Event::Preload(123)
        ]
    );
    assert!(host.events.contains(&Event::Callback(
        0,
        FurnitureCallbackRequest {
            callback_address: HOST_CALLBACK,
            argument: 0,
            placement: Some(5),
        },
        FurnitureCallbackContext {
            position_words: [0, (-32_i16) as u16, 0],
        }
    )));
    assert_eq!(queue.records()[0].position_words, original);
    assert_eq!(queue.records()[0].program.address(), PROGRAM + 12);
    assert_eq!(queue.records()[0].age_microseconds, 0);
}

#[test]
fn timer_arithmetic_wraps_before_signed_due_and_remaining_comparisons() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    queue.record_mut(0).unwrap().age_microseconds = 0x7fff_fffe;
    queue.record_mut(0).unwrap().remaining_microseconds = 0x8000_0000;
    let mut host = Host::new(&[[0, HOST_CALLBACK, 0], [100, 0, 0]]);
    queue.tick(&mut terrain, 10, &mut host, &mut 0).unwrap();
    assert_eq!(queue.records()[0].age_microseconds, 0x8000_0008);
    assert_eq!(queue.records()[0].remaining_microseconds, 0);
    assert_eq!(host.events, [Event::Word(PROGRAM)]);
    queue.record_mut(0).unwrap().age_microseconds = 0;
    queue.record_mut(0).unwrap().remaining_microseconds = 1;
    host.events.clear();
    queue
        .tick(&mut terrain, 0x8000_0000, &mut host, &mut 0)
        .unwrap();
    assert_eq!(queue.records()[0].remaining_microseconds, 0x8000_0001);
    assert_eq!(host.events, [Event::Word(PROGRAM)]);
}

#[test]
fn negative_delay_is_signed_and_negative_model_word_skips_preload() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    let mut host = Host::new(&[[u32::MAX, HOST_CALLBACK, 0], [100, 0, 0]]);
    host.model_word = 0x8000;
    assert_eq!(
        queue
            .tick(&mut terrain, 0, &mut host, &mut 0)
            .unwrap()
            .callbacks,
        1
    );
    assert_eq!(queue.records()[0].age_microseconds, 1);
    assert!(!host
        .events
        .iter()
        .any(|event| matches!(event, Event::Preload(_))));
}

#[test]
fn resource_failure_retains_timers_without_consuming_program_delay() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    let mut host = Host::new(&[[100, HOST_CALLBACK, 0]]);
    host.fail_preload = true;
    assert_eq!(
        queue.tick(&mut terrain, 105, &mut host, &mut 0),
        Err(FurnitureEffectExecutorError::Host("H8 loader unavailable"))
    );
    assert_eq!(queue.records()[0].age_microseconds, 105);
    assert_eq!(queue.records()[0].program.address(), PROGRAM);
    assert_eq!(
        host.events,
        [
            Event::Word(PROGRAM),
            Event::Placement(5),
            Event::Preload(123)
        ]
    );
}

#[test]
fn state_dispatch_skips_intervening_callbacks_without_consuming_their_delays() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    let mut host = Host::new(&[
        [50, SET_FURNITURE_STATE, 1],
        [200, HOST_CALLBACK, 99],
        [500_000, SET_FURNITURE_STATE, 2],
        [100, 0, 0],
    ]);
    let result = queue.tick(&mut terrain, 60, &mut host, &mut 0).unwrap();
    assert_eq!(result.skipped_program_records, 2);
    assert_eq!(result.callbacks, 1);
    assert_eq!(queue.records()[0].age_microseconds, 10);
    assert_eq!(queue.records()[0].program.address(), PROGRAM + 36);
    assert_eq!(terrain.sparse_record_at([0, 0]).unwrap().1.state, 0xa6);
    assert!(!host
        .events
        .iter()
        .any(|event| matches!(event, Event::Callback(..))));
}

#[test]
fn owned_policy_timer_clear_and_termination_publish_real_effects() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    let mut host = Host::new(&[
        [0, SET_REMOVAL_POLICY, 1],
        [0, START_FURNITURE_TIMER, 0],
        [20_000_000, 0, 0],
    ]);
    let result = queue.tick(&mut terrain, 0, &mut host, &mut 0).unwrap();
    assert_eq!(result.callbacks, 2);
    assert_eq!(queue.records()[0].removal_flag, 1);
    assert_eq!(queue.records()[0].remaining_microseconds, 7_000_000);
    assert_eq!(terrain.sparse_record_at([0, 0]).unwrap().1.state, 0xad);
    queue
        .tick(&mut terrain, 1_000_000, &mut host, &mut 0)
        .unwrap();
    assert_eq!(queue.records()[0].remaining_microseconds, 6_000_000);
    queue.record_mut(0).unwrap().program = FurnitureEffectProgram::new(0x4000).unwrap();
    host.program(0x4000, &[[0, CLEAR_FURNITURE, 0], [0, TERMINATE, 0]]);
    let result = queue.tick(&mut terrain, 0, &mut host, &mut 0).unwrap();
    assert_eq!(result.callbacks, 2);
    assert_eq!(result.removed_records, 1);
    let record = terrain.sparse_record_at([0, 0]).unwrap().1;
    assert_eq!((record.state, record.resource_lookup.raw()), (0, 0));
    assert!(queue.records().is_empty());
}

#[test]
fn fallback_record_has_shared_mutable_payload_and_missing_furniture_removal_precedes_callbacks() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [128, 0], PROGRAM);
    let mut host = Host::new(&[[0, START_FURNITURE_TIMER, 0], [20_000_000, 0, 0]]);
    queue.tick(&mut terrain, 0, &mut host, &mut 0).unwrap();
    assert_eq!(host.fallback.state, 0xb8);
    assert_eq!(host.fallback.resource_lookup.raw(), 23);
    assert_eq!(queue.records()[0].remaining_microseconds, 7_000_000);
    queue.record_mut(0).unwrap().removal_flag = 1;
    queue.record_mut(0).unwrap().program = FurnitureEffectProgram::new(PROGRAM).unwrap();
    host.events.clear();
    assert_eq!(
        queue
            .tick(&mut terrain, 0, &mut host, &mut 0)
            .unwrap()
            .removed_records,
        1
    );
    assert_eq!(host.events, [Event::Word(PROGRAM)]);
}

#[test]
fn fallback_clear_writes_native_bytes_without_reading_previous_payload() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [128, 0], PROGRAM);
    let mut host = Host::new(&[[50, CLEAR_FURNITURE, 0], [1_000, 0, 0]]);
    host.fail_fallback_read = true;

    let result = queue.tick(&mut terrain, 60, &mut host, &mut 0).unwrap();

    assert_eq!(result.callbacks, 1);
    assert_eq!(queue.records()[0].program.address(), PROGRAM + 12);
    assert_eq!(queue.records()[0].age_microseconds, 10);
    assert_eq!(host.fallback.state, 0);
    assert_eq!(host.fallback.resource_lookup.raw(), 0);
    assert!(host.events.contains(&Event::FallbackClear));
    assert!(!host.events.contains(&Event::FallbackRead));
}

#[test]
fn fallback_clear_failure_retains_consumed_delay_prior_callback_and_first_byte_write() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    queue.record_mut(0).unwrap().remaining_microseconds = 200;
    let mut host = Host::new(&[
        [10, HOST_CALLBACK, 5],
        [20, CLEAR_FURNITURE, 0],
        [0, TERMINATE, 0],
    ]);
    host.fail_fallback_read = true;
    host.fail_fallback_clear_after_state = true;
    let mut rng = 77;
    let mut expected = rng;
    name_entry_rng_next(&mut expected);

    assert_eq!(
        queue.tick(&mut terrain, 100, &mut host, &mut rng),
        Err(FurnitureEffectExecutorError::Host(
            "fallback clear failed after state write"
        ))
    );

    assert_eq!(queue.records().len(), 1);
    assert_eq!(queue.records()[0].age_microseconds, 70);
    assert_eq!(queue.records()[0].remaining_microseconds, 100);
    assert_eq!(queue.records()[0].program.address(), PROGRAM + 12);
    assert_eq!(queue.records()[0].removal_flag, 7);
    assert_eq!(rng, expected);
    assert_eq!(host.fallback.state, 0);
    assert_eq!(host.fallback.resource_lookup.raw(), 23);
    assert!(host.events.contains(&Event::FallbackClear));
    assert!(!host.events.contains(&Event::FallbackRead));
}

#[test]
fn required_callback_failure_keeps_consumed_delay_and_prior_mutations() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    let mut host = Host::new(&[[50, HOST_CALLBACK, 2]]);
    let mut rng = 77;
    let mut expected = rng;
    name_entry_rng_next(&mut expected);
    assert_eq!(
        queue.tick(&mut terrain, 53, &mut host, &mut rng),
        Err(FurnitureEffectExecutorError::Host(
            "required callback failed after writes"
        ))
    );
    assert_eq!(queue.records()[0].age_microseconds, 3);
    assert_eq!(queue.records()[0].removal_flag, 7);
    assert_eq!(queue.records()[0].program.address(), PROGRAM);
    assert_eq!(rng, expected);
}

#[test]
fn callback_cursor_and_stack_mutations_are_read_after_return() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    let mut host = Host::new(&[[0, HOST_CALLBACK, 3]]);
    host.program(0x3000, &[[0, HOST_CALLBACK, 99], [100, HOST_CALLBACK, 0]]);
    queue.tick(&mut terrain, 0, &mut host, &mut 0).unwrap();
    assert_eq!(queue.records()[0].program.address(), 0x300c);

    queue.record_mut(0).unwrap().program = FurnitureEffectProgram::new(PROGRAM).unwrap();
    host.program(
        PROGRAM,
        &[[0, HOST_CALLBACK, 1], [0, HOST_CALLBACK, 0], [100, 0, 0]],
    );
    host.program(0x2000, &[[100, 0, 0]]);
    host.events.clear();
    queue.tick(&mut terrain, 0, &mut host, &mut 0).unwrap();
    assert!(host.events.iter().any(|event| matches!(
        event,
        Event::Callback(
            _,
            FurnitureCallbackRequest { argument: 0, .. },
            FurnitureCallbackContext {
                position_words: [0, 123, 0],
                ..
            }
        )
    )));
}

#[test]
fn callback_userdata_keeps_cached_h11_identity_when_position_and_registrations_change() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    let mut host = Host::new(&[
        [0, HOST_CALLBACK, 4],
        [0, SET_REMOVAL_POLICY, 1],
        [0, HOST_CALLBACK, 0],
        [1_000, 0, 0],
    ]);

    let result = queue.tick(&mut terrain, 0, &mut host, &mut 0).unwrap();

    assert_eq!(result.callbacks, 3);
    assert_eq!(result.removed_records, 0);
    assert_eq!(queue.records()[0].removal_flag, 1);
    assert_eq!(queue.records()[0].program.address(), PROGRAM + 36);
    assert!(host.null_placement);
    assert_eq!(
        host.events
            .iter()
            .filter(|event| matches!(event, Event::Placement(_)))
            .count(),
        1
    );
    assert!(host.events.contains(&Event::Callback(
        0,
        FurnitureCallbackRequest {
            callback_address: HOST_CALLBACK,
            argument: 0,
            placement: Some(5),
        },
        FurnitureCallbackContext {
            position_words: [128, (-32_i16) as u16, 0],
        }
    )));
}

#[test]
fn saved_next_delays_tail_append_but_reaches_append_behind_an_existing_successor() {
    let mut terrain = terrain(5);
    let mut queue = FurnitureEffectQueue::default();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    let mut host = Host::new(&[[0, HOST_CALLBACK, 1], [0, 0, 0]]);
    host.program(0x2000, &[[10, 0, 0]]);
    assert_eq!(
        queue
            .tick(&mut terrain, 5, &mut host, &mut 0)
            .unwrap()
            .visited_records,
        1
    );
    assert_eq!(queue.records().len(), 1);
    assert_eq!(queue.records()[0].age_microseconds, 0);

    queue.clear();
    enqueue(&mut queue, &terrain, [0, 0], PROGRAM);
    enqueue(&mut queue, &terrain, [256, 0], 0x4000);
    host.program(0x4000, &[[10, 0, 0]]);
    let result = queue.tick(&mut terrain, 5, &mut host, &mut 0).unwrap();
    assert_eq!(result.visited_records, 3);
    assert_eq!(result.removed_records, 1);
    assert_eq!(queue.records().len(), 2);
    assert_eq!(queue.records()[0].age_microseconds, 5);
    assert_eq!(queue.records()[1].age_microseconds, 5);
}

#[test]
fn original_program_words_pin_callbacks_delays_and_unmapped_memory_boundary() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let programs = ExecutableFurnitureEffectPrograms {
        executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    };
    let expected = [
        [0, 0x0043_6d00, 1],
        [0, 0x0043_6de0, 11],
        [500_000, 0x0043_6d20, 2],
        [0, 0x0043_6de0, 12],
        [500_000, 0x0043_7060, 0],
        [0, 0, 0],
    ];
    for (index, record) in expected.into_iter().enumerate() {
        for (word, value) in record.into_iter().enumerate() {
            assert_eq!(
                programs
                    .word(0x0053_2c88 + index as u32 * 12 + word as u32 * 4)
                    .unwrap(),
                value
            );
        }
    }
    assert_eq!(
        programs.word(0x0054_83c4),
        Err(FrontendExecutableError::DataAddress(0x0054_83c4))
    );
}
