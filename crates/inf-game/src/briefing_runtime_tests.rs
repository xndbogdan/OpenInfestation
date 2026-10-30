use super::*;
use inf_formats::ByteSpan;

fn program(records: Vec<(u32, u32, BriefingPayload)>) -> BriefingProgram {
    BriefingProgram {
        record_span: ByteSpan::new(0, records.len() * 16),
        records: records
            .into_iter()
            .enumerate()
            .map(|(index, (duration, kind, payload))| BriefingRecord {
                index: index as u32,
                span: ByteSpan::new(index * 16, (index + 1) * 16),
                duration_100ms: duration,
                raw_kind: kind,
                raw_word_08: 0,
                payload_offset: 0,
                payload_span: ByteSpan::new(0, 0),
                payload,
            })
            .collect(),
    }
}

fn text(id: u16) -> BriefingPayload {
    BriefingPayload::MenuText {
        string_id: id,
        increment_bytes: [0, 0],
    }
}

fn tick(
    runtime: &mut BriefingRuntime,
    delta: i32,
    elapsed: i32,
    rng: &mut u32,
) -> BriefingTickOutcome {
    runtime.tick(
        SchedulerTick {
            delta_micros: delta,
            elapsed_micros: elapsed,
        },
        BriefingGates::default(),
        rng,
        None,
    )
}

#[test]
fn intro_zero_records_and_native_skip_gates_finish_before_music_input_or_rng() {
    let cases = [
        (
            program(vec![]),
            BriefingGates::default(),
            BriefingFinishReason::NoRecords,
        ),
        (
            program(vec![(1, 0, text(0x10))]),
            BriefingGates {
                mission_status: 8,
                attract_mode: false,
            },
            BriefingFinishReason::MissionStatusBit8,
        ),
        (
            program(vec![(1, 0, text(0x10))]),
            BriefingGates {
                mission_status: 2,
                attract_mode: false,
            },
            BriefingFinishReason::MissionStatusBit2,
        ),
        (
            program(vec![(1, 0, text(0x10))]),
            BriefingGates {
                mission_status: 0,
                attract_mode: true,
            },
            BriefingFinishReason::AttractMode,
        ),
    ];
    for (program, gates, reason) in cases {
        let mut runtime = BriefingRuntime::new(program);
        let mut rng = 123;
        let result = runtime.tick(
            SchedulerTick {
                delta_micros: 16_000,
                elapsed_micros: 16_000,
            },
            gates,
            &mut rng,
            None,
        );
        assert_eq!(result.effects, vec![BriefingTickEffect::Complete(reason)]);
        assert_eq!(result.finish_reason, Some(reason));
        assert_eq!(rng, 123);
        assert_eq!(runtime.screen_age_millis(), 0);
        assert!(tick(&mut runtime, 16_000, 16_000, &mut rng)
            .effects
            .is_empty());
    }
}

#[test]
fn authored_briefing_waits_for_opening_then_runs_records_and_stays_for_the_start_action() {
    let mut runtime = BriefingRuntime::new(program(vec![
        (1, 0, text(0x17)),
        (0, 5, BriefingPayload::Visibility { flags: 8 }),
    ]));
    let mut rng = 0;
    let opening = tick(&mut runtime, 16_000, 1_000_000, &mut rng);
    assert!(opening
        .effects
        .contains(&BriefingTickEffect::ResumeWorldMusic));
    assert!(opening
        .effects
        .contains(&BriefingTickEffect::AdvanceSharedAnimation {
            delta_micros: 14_000,
        }));
    assert_eq!(runtime.mode(), BriefingMode::Opening);
    assert_eq!(runtime.menu_text_id(), None);
    tick(&mut runtime, 16_000, 1_000, &mut rng);
    assert_eq!(runtime.mode(), BriefingMode::Detail);
    assert_eq!(runtime.menu_text_id(), Some(0x17));
    // The authored 100 ms record uses 7/8 of the capped simulation delta.
    tick(&mut runtime, 112_000, 112_000, &mut rng);
    assert_eq!(runtime.record_age_millis(), 98);
    tick(&mut runtime, 8_000, 8_000, &mut rng);
    assert_eq!(runtime.current_record().unwrap().index, 1);
    let exhausted = tick(&mut runtime, 8_000, 8_000, &mut rng);
    assert_eq!(runtime.mode(), BriefingMode::List);
    assert!(exhausted.effects.contains(&BriefingTickEffect::ShowList));
    assert_eq!(exhausted.finish_reason, None);
    for _ in 0..80 {
        tick(&mut runtime, 125_000, 125_000, &mut rng);
    }
    assert_eq!(runtime.list_rows(), 4);
    assert!(runtime.list_closed());
    assert_eq!(runtime.finished(), None);
    assert!(runtime.arm_start());
    let mut completion = None;
    for _ in 0..19 {
        let result = tick(&mut runtime, 125_000, 125_000, &mut rng);
        completion = completion.or(result.finish_reason);
    }
    assert_eq!(completion, Some(BriefingFinishReason::StartTimer));
}

#[test]
fn repeated_start_does_not_extend_the_native_two_second_countdown() {
    let mut runtime = BriefingRuntime::new(program(vec![(1000, 0, text(9))]));
    let mut rng = 0;
    assert!(runtime.arm_start());
    for _ in 0..18 {
        assert_eq!(
            tick(&mut runtime, 125_000, 500_000, &mut rng).finish_reason,
            None
        );
        assert!(!runtime.arm_start());
    }
    // Each item tick contributes trunc(125000*7/8)/1000 = 109 ms.
    assert_eq!(runtime.start_remaining_millis(), 38);
    let result = tick(&mut runtime, 125_000, 500_000, &mut rng);
    assert_eq!(result.finish_reason, Some(BriefingFinishReason::StartTimer));
    assert_eq!(runtime.start_remaining_millis(), 1);
}

#[test]
fn zero_duration_records_advance_one_per_item_tick_and_preserve_id_increment_wrapping() {
    let mut runtime = BriefingRuntime::new(program(vec![
        (0, 0, text(u16::MAX)),
        (
            0,
            0,
            BriefingPayload::MenuText {
                string_id: 77,
                increment_bytes: [0, 1],
            },
        ),
        (0, 0, text(99)),
    ]));
    let mut rng = 0;
    runtime.show_detail();
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    assert_eq!(runtime.menu_text_id(), Some(u16::MAX));
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    assert_eq!(runtime.menu_text_id(), Some(0));
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    assert_eq!(runtime.menu_text_id(), Some(99));
    assert_eq!(runtime.mode(), BriefingMode::Detail);
}

#[test]
fn detail_replay_keeps_display_blank_until_text_and_retains_the_increment_predecessor() {
    use crate::{
        briefing_controls::{BriefingControls, BriefingMenuTemplates},
        frontend_assets::FrontendAssets,
        menu::MenuCommand,
        menu_basis::MenuBasisMath,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    let basis = MenuBasisMath::new(assets.sine_table().values);
    let mut controls = BriefingControls::new(BriefingMenuTemplates::parse(&assets).unwrap(), 18);
    let mut runtime = BriefingRuntime::new(program(vec![
        (
            0,
            8,
            BriefingPayload::Camera(BriefingCamera {
                actor_id: 9,
                heading_degrees: 0,
                rotation_word: 0,
                speed_word: 0,
                track_byte: 0,
                terrain_byte: 0,
            }),
        ),
        (
            1000,
            0,
            BriefingPayload::MenuText {
                string_id: 77,
                increment_bytes: [1, 0],
            },
        ),
    ]));
    // Fixture for screen +2A0 after an earlier explicit text writer. D1E0
    // clears the separate display scratch; it never clears this WORD.
    runtime.menu_text_id = Some(17);
    let mut rng = 0;
    let item_tick =
        |runtime: &mut BriefingRuntime, controls: &mut BriefingControls, rng: &mut u32| {
            let outcome = tick(runtime, 16_000, 16_000, rng);
            for effect in &outcome.effects {
                controls.apply_effect(effect, runtime, &basis).unwrap();
            }
        };
    let text_argument = |controls: &BriefingControls| {
        controls.text.current().unwrap().data().records().items[0].draw_arguments[0]
    };
    controls
        .command(MenuCommand::Activate, &mut runtime, &basis)
        .unwrap();
    item_tick(&mut runtime, &mut controls, &mut rng);
    assert_eq!(runtime.current_record().unwrap().raw_kind, 8);
    assert_eq!(runtime.menu_text_id(), Some(17));
    assert_eq!(text_argument(&controls), 0);
    item_tick(&mut runtime, &mut controls, &mut rng);
    assert_eq!(runtime.menu_text_id(), Some(18));
    assert_eq!(text_argument(&controls), 18);

    controls
        .command(MenuCommand::Activate, &mut runtime, &basis)
        .unwrap();
    assert_eq!(text_argument(&controls), 0);
    item_tick(&mut runtime, &mut controls, &mut rng);
    assert_eq!(runtime.current_record().unwrap().raw_kind, 8);
    assert_eq!(runtime.menu_text_id(), Some(18));
    assert_eq!(
        text_argument(&controls),
        0,
        "camera must not restore the prior display text"
    );
    item_tick(&mut runtime, &mut controls, &mut rng);
    assert_eq!(runtime.menu_text_id(), Some(19));
    assert_eq!(text_argument(&controls), 19);
}

#[test]
fn pulse_rng_uses_the_shared_word_and_skipped_briefing_consumes_none() {
    let mut runtime = BriefingRuntime::new(program(vec![(1000, 0, text(9))]));
    let mut rng = 0;
    let mut expected = 0;
    let next = name_entry_rng_next(&mut expected);
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    assert_eq!(rng, expected);
    assert_eq!(runtime.pulse_remaining_micros(), 150_000 + next * 2);
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    assert_eq!(rng, expected);
    let pause_random = name_entry_rng_next(&mut expected);
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    assert_eq!(rng, expected);
    assert_eq!(runtime.pulse_pause_micros(), (62_500 + pause_random) * 32);
}

#[test]
fn camera_disabled_draw_refreshes_the_tick_timer_without_consuming_rng() {
    let mut runtime = BriefingRuntime::new(program(vec![(1000, 0, text(9))]));
    let mut rng = 0;
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    runtime.camera_disabled_drawn();
    assert_eq!(runtime.pulse_remaining_micros(), 250_000);
    let before = rng;
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    assert_eq!(
        runtime.pulse_remaining_micros(),
        140_625,
        "native item delta is 125000*7/8"
    );
    assert_eq!(rng, before);
    runtime.camera_disabled_drawn();
    assert_eq!(runtime.pulse_remaining_micros(), 250_000);
}

#[test]
fn panel_actions_preserve_native_bit_priority_and_half_second_stage_steps() {
    let mut runtime = BriefingRuntime::new(program(vec![(
        1000,
        5,
        BriefingPayload::Visibility { flags: 8 },
    )]));
    let mut rng = 0;
    tick(&mut runtime, 125_000, 1_001_000, &mut rng);
    assert_eq!(runtime.visibility(BriefingPanel::Text).requested_stage, 2);
    for _ in 0..4 {
        tick(&mut runtime, 125_000, 125_000, &mut rng);
    }
    assert_eq!(runtime.visibility(BriefingPanel::Text).current_stage, 0);
    tick(&mut runtime, 125_000, 125_000, &mut rng);
    assert_eq!(runtime.visibility(BriefingPanel::Text).current_stage, 1);
    for _ in 0..5 {
        tick(&mut runtime, 125_000, 125_000, &mut rng);
    }
    assert_eq!(runtime.visibility(BriefingPanel::Text).current_stage, 2);
    assert_eq!(visibility_stage(2 | 4 | 8), 0);
    assert_eq!(visibility_stage(4 | 8), 1);
    assert_eq!(visibility_stage(8), 2);
}

#[test]
fn increment_without_a_preceding_native_writer_skips_that_feature_with_an_explicit_effect() {
    let mut runtime = BriefingRuntime::new(program(vec![(
        0,
        0,
        BriefingPayload::MenuText {
            string_id: 1,
            increment_bytes: [1, 0],
        },
    )]));
    let mut rng = 0;
    runtime.show_detail();
    let result = tick(&mut runtime, 16_000, 16_000, &mut rng);
    assert_eq!(runtime.menu_text_id(), None);
    assert!(result
        .effects
        .contains(&BriefingTickEffect::UnownedTextIncrement { index: 0 }));
    assert_eq!(result.finish_reason, None);
}
