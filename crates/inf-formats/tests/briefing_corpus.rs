use std::{fs, path::Path};

use inf_formats::{
    parse_briefing_program, parse_overlay, parse_world_bank, BriefingPayload, Preload,
};

#[test]
fn authored_briefing_programs_are_bounded_by_native_h14_allocation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let preload =
        Preload::parse(&fs::read(root.join("PreLoad.dat")).expect("retail PreLoad.dat")).unwrap();
    let mut programs = 0;
    let expected_counts = [
        0, 37, 22, 39, 28, 31, 27, 18, 26, 32, 53, 33, 32, 46, 40, 33, 49, 50, 21, 24, 30, 0, 17,
        23, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    for (leaf, expected_count) in (11..=43).zip(expected_counts) {
        let bytes = fs::read(root.join("Overlay").join(format!("{leaf}.ovl"))).unwrap();
        let layout = parse_overlay(&bytes, &preload.leaf(leaf).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 14)
            .unwrap();
        let bank = parse_world_bank(&bytes, region).unwrap();
        for world in &bank.entries {
            let program = parse_briefing_program(&bytes, &bank, world).unwrap();
            assert_eq!(
                program.records.len(),
                world.header_words_00_to_34[10] as usize,
                "OVL {leaf}"
            );
            assert_eq!(program.records.len(), expected_count, "OVL {leaf}");
            assert_eq!(program.record_span.len(), program.records.len() * 16);
            for (index, record) in program.records.iter().enumerate() {
                assert_eq!(record.index as usize, index);
                assert_eq!(record.span.len(), 16);
                assert!(record.payload_span.end <= bank.main_payload.end);
            }
            // Mission selection 22 activates OVL 32 and has no briefing;
            // selection 18 activates OVL 28 and reads Level 1's program.
            if leaf == 28 {
                assert_eq!(program.records[0].duration_100ms, 0);
                assert_eq!(program.records[0].raw_kind, 8);
                assert!(
                    matches!(&program.records[0].payload, BriefingPayload::Camera(camera) if camera.actor_id == 39 && camera.speed_word == 50000)
                );
                assert_eq!(
                    program.records[1].payload,
                    BriefingPayload::Visibility { flags: 8 }
                );
                let texts = program
                    .records
                    .iter()
                    .filter_map(|record| match &record.payload {
                        BriefingPayload::MenuText {
                            string_id,
                            increment_bytes,
                        } => {
                            assert_eq!(*increment_bytes, [0, 0]);
                            Some(*string_id)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(texts, (875..=887).collect::<Vec<_>>());
            }
            programs += 1;
        }
    }
    assert_eq!(programs, 33);
}
