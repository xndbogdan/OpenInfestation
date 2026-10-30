use std::{collections::BTreeSet, fs, path::Path};

use inf_formats::{
    parse_digital_input_table, parse_frontend_input_tables, parse_key_name_table,
    parse_phase3_input_tables, parse_phase8_input_tables, parse_text_entry_input_tables,
    parse_title_input_tables, FrontendExecutable, FRONTEND_ANALOG_TABLE_POINTERS,
    FRONTEND_DIGITAL_TABLE_ADDRESSES, GAMEPLAY_DIGITAL_TABLE_ADDRESSES,
    GAMEPLAY_SUB_DIGITAL_TABLE_ADDRESSES, KEY_NAME_COUNT, PHASE12_DIGITAL_TABLE_ADDRESSES,
    PHASE3_ANALOG_TABLE_POINTERS, PHASE3_DIGITAL_TABLE_ADDRESSES, PHASE5_DIGITAL_TABLE_ADDRESSES,
    PHASE7_DIGITAL_TABLE_ADDRESSES, PHASE8_ANALOG_TABLE_POINTERS, PHASE8_DIGITAL_TABLE_ADDRESSES,
    STARTUP_DIGITAL_TABLE_ADDRESSES, TEXT_ENTRY_ANALOG_TABLE_POINTERS,
    TEXT_ENTRY_DIGITAL_TABLE_ADDRESSES, TITLE_ANALOG_TABLE_POINTERS, TITLE_DIGITAL_TABLE_ADDRESSES,
};

fn executable() -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    fs::read(root.join("Infest.exe")).expect("retail Infest.exe is required")
}

#[test]
fn retail_frontend_retains_twelve_digital_and_twelve_analog_device_slots() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let tables = parse_frontend_input_tables(source).unwrap();
    assert_eq!(
        FRONTEND_DIGITAL_TABLE_ADDRESSES,
        [0x5280b0, 0, 0x528218, 0, 0, 0, 0x528370, 0x528370, 0x528410, 0x528410, 0, 0,]
    );
    assert_eq!(
        FRONTEND_ANALOG_TABLE_POINTERS,
        [0, 0, 0x533398, 0, 0, 0, 0, 0, 0x5333e8, 0x5333e8, 0, 0,]
    );
    assert_eq!(
        tables
            .digital
            .each_ref()
            .map(|table| table.as_ref().map(|v| v.records.len())),
        [
            Some(17),
            None,
            Some(16),
            None,
            None,
            None,
            Some(7),
            Some(7),
            Some(7),
            Some(7),
            None,
            None,
        ]
    );
    assert_eq!(
        tables
            .analog
            .each_ref()
            .map(|table| table.as_ref().map(|v| v.virtual_address)),
        [
            None,
            None,
            Some(0x533350),
            None,
            None,
            None,
            None,
            None,
            Some(0x5333a0),
            Some(0x5333a0),
            None,
            None,
        ]
    );
    assert_eq!(tables.digital[6], tables.digital[7]);
    assert_eq!(tables.digital[8], tables.digital[9]);
    assert_eq!(tables.analog[8], tables.analog[9]);
    for table in tables.digital.iter().flatten() {
        assert_eq!(table.terminator.get(&bytes).unwrap(), &[0; 4]);
        for record in &table.records {
            assert_eq!(record.file_span.len(), 20);
            assert_eq!(record.descriptor.file_span.len(), 8);
            assert_eq!(
                record.file_span.get(&bytes).unwrap()[..4],
                record.descriptor.identity.to_le_bytes()
            );
        }
    }
    for table in tables.analog.iter().flatten() {
        assert_eq!(table.terminator.get(&bytes).unwrap(), &[0; 4]);
        assert_eq!(table.records.len(), 2);
        assert_eq!(table.records[0].descriptor.identity, 0x546f10);
        assert_eq!(table.records[1].descriptor.identity, 0x546f00);
        assert_eq!(table.records[0].axis, 0);
        assert_eq!(table.records[1].axis, 1);
        for record in &table.records {
            assert_eq!(record.file_span.len(), 24);
            assert_eq!(record.descriptor.file_span.unwrap().len(), 12);
            assert_eq!(record.descriptor.callback_address, 0);
            assert_eq!(record.descriptor.flags, 0);
            assert_eq!(record.descriptor.period_micros, 0);
            assert_eq!(record.flags, 0);
            assert_eq!(record.offset, 0);
        }
    }
}

#[test]
fn retail_frontend_preserves_digital_chords_system_actions_and_analog_gains() {
    let bytes = executable();
    let tables = parse_frontend_input_tables(FrontendExecutable::parse(&bytes).unwrap()).unwrap();
    let primary = &tables.digital[0].as_ref().unwrap().records;
    assert_eq!(primary[13].required, [[0x1c, 0, 0, 0], [0; 4]]);
    assert_eq!(primary[13].forbidden, [[0x38, 0, 0, 0], [0; 4]]);
    assert_eq!(primary[14].required[0], [0x38, 0x1c, 0, 0]);
    assert_eq!(primary[15].required[0], [0x3b, 0x38, 0, 0]);
    assert_eq!(primary[16].required[0], [0x38, 0x01, 0, 0]);
    let descriptors: BTreeSet<_> = tables
        .digital
        .iter()
        .flatten()
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x533408, 0x439260, 0),
            (0x533410, 0x437d30, 0),
            (0x533418, 0x4451d0, 0),
            (0x53bfc0, 0x445200, 2),
            (0x53bfc8, 0x445220, 2),
            (0x53bfd0, 0x445240, 2),
            (0x53bfd8, 0x445260, 2),
            (0x53bfe0, 0x445280, 2),
            (0x53bfe8, 0x4451e0, 2),
            (0x53bff0, 0x485300, 2),
            (0x53bff8, 0x42aa00, 2),
            (0x53c000, 0x42a9d0, 2),
        ])
    );
    let device_two = &tables.analog[2].as_ref().unwrap().records;
    assert_eq!(
        device_two
            .iter()
            .map(|v| (v.scale_q31, v.multiplier))
            .collect::<Vec<_>>(),
        [(0x1000_0000, 1), (0x1000_0000, -1)]
    );
    for slot in [8, 9] {
        assert_eq!(
            tables.analog[slot]
                .as_ref()
                .unwrap()
                .records
                .iter()
                .map(|v| (v.scale_q31, v.multiplier))
                .collect::<Vec<_>>(),
            [(0x4000_0000, 64), (0x4000_0000, -64)]
        );
    }
}

#[test]
fn retail_dynamic_analog_pointer_cells_are_followed_and_invalid_roots_fail() {
    let mut bytes = executable();
    let span = FrontendExecutable::parse(&bytes)
        .unwrap()
        .mapped(0x533398, 4)
        .unwrap()
        .file_span;
    bytes[span.start..span.end].copy_from_slice(&0u32.to_le_bytes());
    let tables = parse_frontend_input_tables(FrontendExecutable::parse(&bytes).unwrap()).unwrap();
    assert!(tables.analog[2].is_none());
    assert!(tables.analog[8].is_some());
    bytes[span.start..span.end].copy_from_slice(&0x0040_1000u32.to_le_bytes());
    assert!(parse_frontend_input_tables(FrontendExecutable::parse(&bytes).unwrap()).is_err());
}

#[test]
fn retail_text_entry_tables_follow_the_00459060_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let tables = parse_text_entry_input_tables(source).unwrap();
    let frontend = parse_frontend_input_tables(source).unwrap();
    assert_eq!(
        TEXT_ENTRY_DIGITAL_TABLE_ADDRESSES,
        [0x525c58, 0, 0x525d48, 0, 0, 0, 0x525e88, 0x525e88, 0x525f40, 0x525f40, 0, 0]
    );
    // Same pointer cells as the frontend, read by 004590D2/004590E0.
    assert_eq!(
        TEXT_ENTRY_ANALOG_TABLE_POINTERS,
        FRONTEND_ANALOG_TABLE_POINTERS
    );
    assert_eq!(tables.analog, frontend.analog);
    assert_eq!(
        tables
            .digital
            .each_ref()
            .map(|table| table.as_ref().map(|v| v.records.len())),
        [
            Some(11),
            None,
            Some(24),
            None,
            None,
            None,
            Some(8),
            Some(8),
            Some(8),
            Some(8),
            None,
            None,
        ]
    );
    let keyboard = tables.digital[0].as_ref().unwrap();
    let rows: Vec<(u32, u32, u8, u8)> = keyboard
        .records
        .iter()
        .map(|row| {
            (
                row.descriptor.identity,
                row.descriptor.callback_address,
                row.required[0][0],
                row.forbidden[0][0],
            )
        })
        .collect();
    assert_eq!(
        rows[..8],
        [
            (0x537c00, 0x4451e0, 0x01, 0),    // Esc: Back
            (0x537c30, 0x45ae90, 0x0f, 0),    // Tab: confirm name
            (0x537c08, 0x445200, 0x1c, 0x38), // Return unless Alt: Activate
            (0x537c10, 0x445220, 0xc8, 0),    // Up
            (0x537c18, 0x445240, 0xd0, 0),    // Down
            (0x537c20, 0x445260, 0xcb, 0),    // Left
            (0x537c28, 0x445280, 0xcd, 0),    // Right
            (0x537c38, 0x45aea0, 0x13, 0),    // R: retry after error
        ]
    );
    // The three shared Alt system records follow.
    assert_eq!(
        keyboard.records[8..]
            .iter()
            .map(|row| row.descriptor.callback_address)
            .collect::<Vec<_>>(),
        [0x437d30, 0x4451d0, 0x439260]
    );
    // Slot 2 has no terminator of its own: its sixteen Activate buttons run on
    // into the slots 6/7 table, ending at that table's terminator.
    let slot2 = tables.digital[2].as_ref().unwrap();
    let slot6 = tables.digital[6].as_ref().unwrap();
    assert!(slot2.records[..16]
        .iter()
        .all(|row| row.descriptor.callback_address == 0x445200));
    assert_eq!(slot2.records[16..], slot6.records[..]);
    assert_eq!(slot2.terminator, slot6.terminator);
    // Text-entry descriptors are distinct identities from the frontend's.
    let text_ids: BTreeSet<u32> = tables
        .digital
        .iter()
        .flatten()
        .flat_map(|table| table.records.iter().map(|row| row.descriptor.identity))
        .filter(|identity| (0x537c00..0x537c40).contains(identity))
        .collect();
    assert_eq!(text_ids.len(), 8);
    assert!(frontend.digital.iter().flatten().all(|table| table
        .records
        .iter()
        .all(|row| !text_ids.contains(&row.descriptor.identity))));
}

#[test]
fn retail_title_tables_follow_the_00404620_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let tables = parse_title_input_tables(source).unwrap();
    assert_eq!(
        TITLE_DIGITAL_TABLE_ADDRESSES,
        [0x51f300, 0, 0, 0, 0, 0, 0x51f378, 0x51f378, 0x51f4a8, 0x51f4a8, 0, 0,]
    );
    assert_eq!(TITLE_ANALOG_TABLE_POINTERS, [0; 12]);
    // No analog input: every slot stays empty.
    assert!(tables.analog.each_ref().iter().all(|slot| slot.is_none()));
    assert_eq!(
        tables
            .digital
            .each_ref()
            .map(|table| table.as_ref().map(|v| v.records.len())),
        [
            Some(5),
            None,
            None,
            None,
            None,
            None,
            Some(14),
            Some(14),
            Some(14),
            Some(14),
            None,
            None,
        ]
    );
    assert_eq!(tables.digital[6], tables.digital[7]);
    assert_eq!(tables.digital[8], tables.digital[9]);
    // Every non-system record skips the video through 00404870; the two
    // system records reuse the frontend's Alt descriptors.
    let descriptors: BTreeSet<_> = tables
        .digital
        .iter()
        .flatten()
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x52e638, 0x404870, 2),
            (0x533408, 0x439260, 0),
            (0x533410, 0x437d30, 0),
        ])
    );
    // Slot 0 is the keyboard: Esc/Space/Return skip without an Alt guard,
    // then the shared Alt+Enter / Alt+Esc system chords.
    let keyboard = &tables.digital[0].as_ref().unwrap().records;
    assert_eq!(
        keyboard
            .iter()
            .map(|row| (
                row.descriptor.callback_address,
                row.required[0][0],
                row.forbidden[0][0]
            ))
            .collect::<Vec<_>>(),
        [
            (0x404870, 0x01, 0),
            (0x404870, 0x39, 0),
            (0x404870, 0x1c, 0),
            (0x437d30, 0x38, 0),
            (0x439260, 0x38, 0),
        ]
    );
    assert_eq!(keyboard[3].required[0], [0x38, 0x1c, 0, 0]);
    assert_eq!(keyboard[4].required[0], [0x38, 0x01, 0, 0]);
}

#[test]
fn retail_phase3_tables_follow_the_0040a420_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let tables = parse_phase3_input_tables(source).unwrap();
    let frontend = parse_frontend_input_tables(source).unwrap();
    assert_eq!(
        PHASE3_DIGITAL_TABLE_ADDRESSES,
        [0x520028, 0, 0x5200e0, 0, 0, 0, 0x520238, 0x520238, 0x5202a0, 0x5202a0, 0, 0,]
    );
    // Slot 7's stack DWORD is never written; it stays empty like the
    // frontend's null digital slots. The remaining analog cells match the
    // frontend layout exactly.
    assert_eq!(
        PHASE3_ANALOG_TABLE_POINTERS,
        [0, 0, 0x533398, 0, 0, 0, 0, 0, 0x5333e8, 0x5333e8, 0, 0]
    );
    assert_eq!(
        tables
            .digital
            .each_ref()
            .map(|table| table.as_ref().map(|v| v.records.len())),
        [
            Some(8),
            None,
            Some(16),
            None,
            None,
            None,
            Some(4),
            Some(4),
            Some(4),
            Some(4),
            None,
            None,
        ]
    );
    assert_eq!(tables.digital[6], tables.digital[7]);
    assert_eq!(tables.digital[8], tables.digital[9]);
    assert_eq!(tables.analog, frontend.analog);
    let descriptors: BTreeSet<_> = tables
        .digital
        .iter()
        .flatten()
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x52eff0, 0x40d660, 2),
            (0x52eff8, 0x40d670, 2),
            (0x52f000, 0x40d680, 2),
            (0x52f008, 0x40d690, 2),
            (0x533408, 0x439260, 0),
            (0x533410, 0x437d30, 0),
            (0x533418, 0x4451d0, 0),
        ])
    );
    // Slot 0 is the keyboard: briefing callbacks on Esc/0x22, Return (unless
    // Alt), Up and Down, then the three shared Alt system records.
    let keyboard = &tables.digital[0].as_ref().unwrap().records;
    assert_eq!(
        keyboard
            .iter()
            .map(|row| (
                row.descriptor.callback_address,
                row.required[0][0],
                row.forbidden[0][0]
            ))
            .collect::<Vec<_>>(),
        [
            (0x40d660, 0x01, 0),
            (0x40d660, 0x22, 0),
            (0x40d670, 0x1c, 0x38),
            (0x40d680, 0xc8, 0),
            (0x40d690, 0xd0, 0),
            (0x437d30, 0x38, 0),
            (0x4451d0, 0x3b, 0),
            (0x439260, 0x38, 0),
        ]
    );
    // Slot 2 is sixteen joystick buttons on one callback; slots 6/7 and 8/9
    // repeat the same four-button layout from distinct tables.
    let slot2 = &tables.digital[2].as_ref().unwrap().records;
    assert!(slot2
        .iter()
        .all(|row| row.descriptor.callback_address == 0x40d670));
    assert_eq!(
        slot2
            .iter()
            .map(|row| u32::from(row.required[0][0]))
            .collect::<Vec<_>>(),
        (1..=16).collect::<Vec<_>>()
    );
    for slot in [6, 8] {
        assert_eq!(
            tables.digital[slot]
                .as_ref()
                .unwrap()
                .records
                .iter()
                .map(|row| (row.descriptor.callback_address, row.required[0][0]))
                .collect::<Vec<_>>(),
            [
                (0x40d660, 0x04),
                (0x40d680, 0x05),
                (0x40d690, 0x07),
                (0x40d670, 0x0f),
            ]
        );
    }
}

#[test]
fn retail_gameplay_digital_tables_follow_the_00440990_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(
        GAMEPLAY_DIGITAL_TABLE_ADDRESSES,
        [0x534290, 0, 0, 0, 0, 0, 0x522a78, 0x522a78, 0x522ae0, 0x522ae0, 0, 0,]
    );
    assert_eq!(
        GAMEPLAY_SUB_DIGITAL_TABLE_ADDRESSES,
        [0x534570, 0, 0x534cc0, 0, 0, 0, 0x522d98, 0x522d98, 0x522fb8, 0x522fb8, 0, 0,]
    );
    let screen: Vec<_> = GAMEPLAY_DIGITAL_TABLE_ADDRESSES
        .into_iter()
        .filter(|address| *address != 0)
        .map(|address| parse_digital_input_table(source, address).unwrap())
        .collect();
    let sub: Vec<_> = GAMEPLAY_SUB_DIGITAL_TABLE_ADDRESSES
        .into_iter()
        .filter(|address| *address != 0)
        .map(|address| parse_digital_input_table(source, address).unwrap())
        .collect();
    // Slots 6/7 and 8/9 share one table each per struct.
    assert_eq!(
        screen
            .iter()
            .map(|table| table.records.len())
            .collect::<Vec<_>>(),
        [12, 4, 4, 4, 4]
    );
    assert_eq!(
        sub.iter()
            .map(|table| table.records.len())
            .collect::<Vec<_>>(),
        [45, 16, 26, 26, 26, 26]
    );
    assert_eq!(screen[1], screen[2]);
    assert_eq!(screen[3], screen[4]);
    assert_eq!(sub[2], sub[3]);
    assert_eq!(sub[4], sub[5]);
    fn row_key(
        rows: &[inf_formats::DigitalInputRecord],
    ) -> Vec<(
        inf_formats::DigitalActionDescriptor,
        [u8; 4],
        [u8; 4],
        [u8; 4],
        [u8; 4],
    )> {
        rows.iter()
            .map(|row| {
                (
                    row.descriptor,
                    row.required[0],
                    row.required[1],
                    row.forbidden[0],
                    row.forbidden[1],
                )
            })
            .collect()
    }
    // The shared pairs repeat identical rows from distinct tables (virtual
    // addresses and spans differ, so only the data-bearing fields compare).
    assert_eq!(
        row_key(&screen[1].records),
        row_key(&screen[3].records),
        "slots 6/7 repeat slots 8/9 row for row"
    );
    assert_eq!(
        row_key(&sub[2].records),
        row_key(&sub[4].records),
        "slots 6/7 repeat slots 8/9 row for row"
    );
    let descriptors: BTreeSet<_> = screen
        .iter()
        .chain(sub.iter())
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x533410, 0x437d30, 0),
            (0x534210, 0x443470, 2),
            (0x534218, 0x4435c0, 2),
            (0x534220, 0x4435f0, 2),
            (0x534228, 0x443620, 2),
            (0x534230, 0x4436d0, 2),
            (0x534238, 0x443700, 2),
            (0x534260, 0x443650, 2),
            (0x534268, 0x443680, 2),
            (0x534280, 0x443720, 2),
            (0x534288, 0x4436b0, 2),
            (0x53a358, 0x0, 6),
            (0x53a360, 0x0, 6),
            (0x53a368, 0x0, 6),
            (0x53a370, 0x0, 6),
            (0x53a378, 0x0, 6),
            (0x53a388, 0x0, 6),
            (0x53a390, 0x4694d0, 6),
            (0x53a398, 0x469580, 0),
            (0x53a3a0, 0x4695a0, 0),
            (0x53a3a8, 0x4695c0, 0),
            (0x53a3b0, 0x4695e0, 0),
            (0x53a3b8, 0x469600, 0),
            (0x53a3c0, 0x469620, 0),
            (0x53a3d0, 0x469660, 0),
            (0x53a3d8, 0x469670, 0),
            (0x53a3e0, 0x469680, 0),
            (0x53a3e8, 0x4696a0, 0),
            (0x53a3f0, 0x4696c0, 0),
            (0x53a3f8, 0x4696e0, 0),
            (0x53a400, 0x469700, 0),
            (0x53a408, 0x469720, 0),
            (0x53a410, 0x4694b0, 0),
            (0x53a418, 0x469740, 0),
            (0x53a420, 0x469750, 0),
            (0x53a428, 0x469760, 0),
            (0x53a430, 0x469770, 0),
            (0x53a438, 0x469450, 0),
            (0x53a440, 0x469480, 0),
            (0x53a448, 0x469780, 0),
            (0x53a450, 0x4697c0, 0),
            (0x53a458, 0x4697d0, 0),
            (0x53a460, 0x4697e0, 0),
            (0x53a468, 0x4697f0, 0),
            (0x53a470, 0x469800, 0),
            (0x53a478, 0x469820, 0),
            (0x53a480, 0x469840, 0),
            (0x53a488, 0x469860, 0),
            (0x53a490, 0x469880, 0),
            (0x53a498, 0x4698a0, 0),
            (0x53a4a0, 0x4698c0, 0),
            (0x53a4a8, 0x4698e0, 0),
        ])
    );
    // The screen keyboard drives gameplay actions; only Alt+Enter survives
    // from the shared system trio: gameplay has no Alt+Esc exit binding.
    // Rows 7, 8 and 10 are two-byte chords pinned below the table.
    assert_eq!(
        screen[0]
            .records
            .iter()
            .map(|row| (
                row.descriptor.callback_address,
                row.required[0][0],
                row.forbidden[0][0]
            ))
            .collect::<Vec<_>>(),
        [
            (0x4435f0, 0x14, 0),
            (0x443620, 0x15, 0),
            (0x4435c0, 0x0e, 0),
            (0x4436d0, 0x32, 0),
            (0x443700, 0x19, 0),
            (0x443650, 0x58, 0),
            (0x443680, 0x16, 0),
            (0x443470, 0x01, 0),
            (0x4436b0, 0x3d, 0),
            (0x443700, 0x01, 0x2a),
            (0x443720, 0x31, 0),
            (0x437d30, 0x38, 0),
        ]
    );
    assert_eq!(
        screen[0]
            .records
            .iter()
            .map(|row| row.required[0])
            .collect::<Vec<_>>()[7..11],
        [
            [0x01, 0x2a, 0, 0],
            [0x3d, 0x38, 0, 0],
            [0x01, 0, 0, 0],
            [0x31, 0x38, 0, 0],
        ]
    );
}

#[test]
fn retail_phase5_digital_tables_follow_the_00443190_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(
        PHASE5_DIGITAL_TABLE_ADDRESSES,
        [0x522b48, 0, 0x534418, 0, 0, 0, 0x522c28, 0x522c28, 0x522ce0, 0x522ce0, 0, 0,]
    );
    let tables: Vec<_> = PHASE5_DIGITAL_TABLE_ADDRESSES
        .into_iter()
        .filter(|address| *address != 0)
        .map(|address| parse_digital_input_table(source, address).unwrap())
        .collect();
    assert_eq!(
        tables
            .iter()
            .map(|table| table.records.len())
            .collect::<Vec<_>>(),
        [10, 16, 8, 8, 8, 8]
    );
    assert_eq!(tables[2], tables[3]);
    assert_eq!(tables[4], tables[5]);
    let descriptors: BTreeSet<_> = tables
        .iter()
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x533410, 0x437d30, 0),
            (0x534210, 0x443470, 2),
            (0x534238, 0x443700, 2),
            (0x534240, 0x4434a0, 2),
            (0x534248, 0x4434d0, 2),
            (0x534250, 0x443500, 2),
            (0x534258, 0x443530, 2),
            (0x534270, 0x443560, 2),
            (0x534278, 0x443590, 2),
        ])
    );
    // The keyboard mixes gameplay actions, two-byte chords and one Alt
    // chord; Alt+Esc exit is absent here as on the gameplay screen.
    assert_eq!(
        tables[0]
            .records
            .iter()
            .map(|row| (
                row.descriptor.callback_address,
                row.required[0],
                row.forbidden[0]
            ))
            .collect::<Vec<_>>(),
        [
            (0x443590, [0x01, 0, 0, 0], [0x2a, 0, 0, 0]),
            (0x443590, [0x0f, 0, 0, 0], [0, 0, 0, 0]),
            (0x443700, [0x01, 0x2a, 0, 0], [0, 0, 0, 0]),
            (0x443700, [0x19, 0, 0, 0], [0, 0, 0, 0]),
            (0x4434a0, [0xc8, 0, 0, 0], [0, 0, 0, 0]),
            (0x4434d0, [0xd0, 0, 0, 0], [0, 0, 0, 0]),
            (0x443500, [0xcb, 0, 0, 0], [0, 0, 0, 0]),
            (0x443530, [0xcd, 0, 0, 0], [0, 0, 0, 0]),
            (0x443560, [0x1c, 0, 0, 0], [0x38, 0, 0, 0]),
            (0x437d30, [0x38, 0x1c, 0, 0], [0, 0, 0, 0]),
        ]
    );
}

#[test]
fn retail_phase12_tables_follow_the_00444bd0_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(
        PHASE12_DIGITAL_TABLE_ADDRESSES,
        [0x524b90, 0, 0x524c70, 0, 0, 0, 0, 0, 0, 0, 0, 0,]
    );
    let tables: Vec<_> = PHASE12_DIGITAL_TABLE_ADDRESSES
        .into_iter()
        .filter(|address| *address != 0)
        .map(|address| parse_digital_input_table(source, address).unwrap())
        .collect();
    assert_eq!(
        tables
            .iter()
            .map(|table| table.records.len())
            .collect::<Vec<_>>(),
        [10, 16]
    );
    let descriptors: BTreeSet<_> = tables
        .iter()
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x533408, 0x439260, 0),
            (0x533410, 0x437d30, 0),
            (0x533418, 0x4451d0, 0),
            (0x535620, 0x4451e0, 2),
            (0x535628, 0x445200, 2),
            (0x535630, 0x445220, 2),
            (0x535638, 0x445240, 2),
            (0x535640, 0x445260, 2),
            (0x535648, 0x445280, 2),
        ])
    );
    // The keyboard presses the screen's own held bits (00445200..00445280,
    // back 004451E0 on Esc and G) plus the shared Alt chords.
    assert_eq!(
        tables[0]
            .records
            .iter()
            .map(|row| (
                row.descriptor.callback_address,
                row.required[0],
                row.forbidden[0]
            ))
            .collect::<Vec<_>>(),
        [
            (0x4451e0, [0x01, 0, 0, 0], [0, 0, 0, 0]),
            (0x4451e0, [0x22, 0, 0, 0], [0, 0, 0, 0]),
            (0x445200, [0x1c, 0, 0, 0], [0x38, 0, 0, 0]),
            (0x445220, [0xc8, 0, 0, 0], [0, 0, 0, 0]),
            (0x445240, [0xd0, 0, 0, 0], [0, 0, 0, 0]),
            (0x445260, [0xcb, 0, 0, 0], [0, 0, 0, 0]),
            (0x445280, [0xcd, 0, 0, 0], [0, 0, 0, 0]),
            (0x437d30, [0x38, 0x1c, 0, 0], [0, 0, 0, 0]),
            (0x4451d0, [0x3b, 0x38, 0, 0], [0, 0, 0, 0]),
            (0x439260, [0x38, 0x01, 0, 0], [0, 0, 0, 0]),
        ]
    );
    // Slot 2 is sixteen joystick buttons on the Activate callback.
    assert!(tables[1]
        .records
        .iter()
        .all(|row| row.descriptor.callback_address == 0x445200));
}

#[test]
fn retail_phase8_tables_follow_the_0042ab00_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(
        PHASE8_DIGITAL_TABLE_ADDRESSES,
        [0x5212a8, 0, 0x521410, 0, 0, 0, 0x521568, 0x521568, 0x521648, 0x521648, 0, 0,]
    );
    assert_eq!(PHASE8_ANALOG_TABLE_POINTERS, [0; 12]);
    let tables = parse_phase8_input_tables(source).unwrap();
    assert!(tables.analog.each_ref().iter().all(|slot| slot.is_none()));
    assert_eq!(
        tables
            .digital
            .each_ref()
            .map(|table| table.as_ref().map(|v| v.records.len())),
        [
            Some(17),
            None,
            Some(16),
            None,
            None,
            None,
            Some(10),
            Some(10),
            Some(10),
            Some(10),
            None,
            None,
        ]
    );
    assert_eq!(tables.digital[6], tables.digital[7]);
    assert_eq!(tables.digital[8], tables.digital[9]);
    let descriptors: BTreeSet<_> = tables
        .digital
        .iter()
        .flatten()
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x531398, 0x42aa60, 2),
            (0x5313a0, 0x42aa30, 2),
            (0x5313a8, 0x42aaa0, 2),
            (0x5313b0, 0x445220, 2),
            (0x5313b8, 0x445240, 2),
            (0x5313c0, 0x445260, 2),
            (0x5313c8, 0x445280, 2),
            (0x5313d0, 0x485300, 2),
            (0x5313d8, 0x42aa00, 2),
            (0x5313e0, 0x42a9d0, 2),
            (0x5313e8, 0x445200, 2),
            (0x533408, 0x439260, 0),
            (0x533410, 0x437d30, 0),
        ])
    );
    // The keyboard mixes results callbacks with frontend navigation rows;
    // Alt+Esc exits and Alt+Enter toggles fullscreen, but the Alt+Enter-noop
    // record is absent: every Alt chord here acts.
    let keyboard = &tables.digital[0].as_ref().unwrap().records;
    assert_eq!(
        keyboard
            .iter()
            .map(|row| (row.descriptor.callback_address, row.required[0][0]))
            .collect::<Vec<_>>(),
        [
            (0x42aa30, 0x01),
            (0x42aa60, 0x39),
            (0x42aa60, 0x22),
            (0x42aaa0, 0x1f),
            (0x437d30, 0x38),
            (0x439260, 0x38),
            (0x42a9d0, 0x0e),
            (0x445220, 0xc8),
            (0x445240, 0xd0),
            (0x445260, 0xcb),
            (0x445280, 0xcd),
            (0x445200, 0x1c),
            (0x485300, 0x15),
            (0x485300, 0x18),
            (0x485300, 0x24),
            (0x485300, 0x1f),
            (0x42aa00, 0x31),
        ]
    );
    // Slot 2 is sixteen joystick buttons on the dominant results callback.
    assert!(tables.digital[2]
        .as_ref()
        .unwrap()
        .records
        .iter()
        .all(|row| row.descriptor.callback_address == 0x42aa60));
}

#[test]
fn retail_phase7_tables_follow_the_00451020_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(
        PHASE7_DIGITAL_TABLE_ADDRESSES,
        [0x525120, 0, 0x525238, 0, 0, 0, 0x525390, 0x525390, 0x525498, 0x525498, 0, 0,]
    );
    let tables: Vec<_> = PHASE7_DIGITAL_TABLE_ADDRESSES
        .into_iter()
        .filter(|address| *address != 0)
        .map(|address| parse_digital_input_table(source, address).unwrap())
        .collect();
    assert_eq!(
        tables
            .iter()
            .map(|table| table.records.len())
            .collect::<Vec<_>>(),
        [13, 16, 12, 12, 12, 12]
    );
    assert_eq!(tables[2], tables[3]);
    assert_eq!(tables[4], tables[5]);
    // Slots 6/7 and 8/9 repeat identical rows from distinct tables.
    fn sorted_rows(
        rows: &[inf_formats::DigitalInputRecord],
    ) -> Vec<(u32, u32, u32, [u8; 4], [u8; 4], [u8; 4], [u8; 4])> {
        let mut keys: Vec<_> = rows
            .iter()
            .map(|row| {
                (
                    row.descriptor.identity,
                    row.descriptor.callback_address,
                    row.descriptor.flags,
                    row.required[0],
                    row.required[1],
                    row.forbidden[0],
                    row.forbidden[1],
                )
            })
            .collect();
        keys.sort();
        keys
    }
    assert_eq!(
        sorted_rows(&tables[2].records),
        sorted_rows(&tables[4].records)
    );
    let descriptors: BTreeSet<_> = tables
        .iter()
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x533408, 0x439260, 0),
            (0x533410, 0x437d30, 0),
            (0x537430, 0x4520a0, 2),
            (0x537438, 0x4520c0, 2),
            (0x537440, 0x4520f0, 2),
            (0x537448, 0x452120, 2),
            (0x537450, 0x452150, 2),
            (0x537458, 0x452180, 2),
            (0x537460, 0x4521b0, 2),
        ])
    );
    // The keyboard mixes map callbacks with the shared Alt pair (exit and
    // fullscreen; the noop record is absent). Slot 2 is sixteen joystick
    // buttons on the dominant callback.
    assert_eq!(
        tables[0]
            .records
            .iter()
            .map(|row| (row.descriptor.callback_address, row.required[0][0]))
            .collect::<Vec<_>>(),
        [
            (0x4520a0, 0x01),
            (0x4520a0, 0x39),
            (0x4520a0, 0x0f),
            (0x4520a0, 0x1c),
            (0x4520a0, 0x32),
            (0x4520c0, 0xc8),
            (0x4520f0, 0xd0),
            (0x452120, 0xcb),
            (0x452150, 0xcd),
            (0x452180, 0xd2),
            (0x4521b0, 0xd3),
            (0x437d30, 0x38),
            (0x439260, 0x38),
        ]
    );
    assert!(tables[1]
        .records
        .iter()
        .all(|row| row.descriptor.callback_address == 0x4520a0));
}

#[test]
fn retail_startup_tables_follow_the_00432500_constructor() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(
        STARTUP_DIGITAL_TABLE_ADDRESSES,
        [0x521ae0, 0, 0x521b58, 0, 0, 0, 0x521cb0, 0x521cb0, 0x521d28, 0x521d28, 0, 0,]
    );
    let tables: Vec<_> = STARTUP_DIGITAL_TABLE_ADDRESSES
        .into_iter()
        .filter(|address| *address != 0)
        .map(|address| parse_digital_input_table(source, address).unwrap())
        .collect();
    assert_eq!(
        tables
            .iter()
            .map(|table| table.records.len())
            .collect::<Vec<_>>(),
        [5, 16, 5, 5, 5, 5]
    );
    assert_eq!(tables[2], tables[3]);
    assert_eq!(tables[4], tables[5]);
    fn sorted_rows(
        rows: &[inf_formats::DigitalInputRecord],
    ) -> Vec<(u32, u32, u32, [u8; 4], [u8; 4], [u8; 4], [u8; 4])> {
        let mut keys: Vec<_> = rows
            .iter()
            .map(|row| {
                (
                    row.descriptor.identity,
                    row.descriptor.callback_address,
                    row.descriptor.flags,
                    row.required[0],
                    row.required[1],
                    row.forbidden[0],
                    row.forbidden[1],
                )
            })
            .collect();
        keys.sort();
        keys
    }
    // Slots 6/7 and 8/9 repeat identical rows from distinct tables. All
    // action descriptors use flags 0 here, unlike every other screen's 2.
    assert_eq!(
        sorted_rows(&tables[2].records),
        sorted_rows(&tables[4].records)
    );
    let descriptors: BTreeSet<_> = tables
        .iter()
        .flat_map(|table| table.records.iter())
        .map(|record| {
            (
                record.descriptor.identity,
                record.descriptor.callback_address,
                record.descriptor.flags,
            )
        })
        .collect();
    assert_eq!(
        descriptors,
        BTreeSet::from([
            (0x5328a8, 0x4327b0, 0),
            (0x5328b0, 0x4327b0, 0),
            (0x5328b8, 0x4327c0, 0),
            (0x5328c0, 0x4327e0, 0),
            (0x533410, 0x437d30, 0),
        ])
    );
    // Slot 0 is the keyboard: Return (not with Alt) and Esc accept, Left and
    // Right turn the carousel, and Alt+Enter keeps the shared display toggle.
    // There is no Alt+Esc exit row on this screen.
    assert_eq!(
        tables[0]
            .records
            .iter()
            .map(|row| (
                row.descriptor.callback_address,
                row.required[0],
                row.forbidden[0]
            ))
            .collect::<Vec<_>>(),
        [
            (0x4327b0, [0x1c, 0, 0, 0], [0x38, 0, 0, 0]),
            (0x4327b0, [0x01, 0, 0, 0], [0, 0, 0, 0]),
            (0x4327c0, [0xcb, 0, 0, 0], [0, 0, 0, 0]),
            (0x4327e0, [0xcd, 0, 0, 0], [0, 0, 0, 0]),
            (0x437d30, [0x38, 0x1c, 0, 0], [0, 0, 0, 0]),
        ]
    );
    // Slot 2 is sixteen joystick buttons on the accept callback.
    assert!(tables[1]
        .records
        .iter()
        .all(|row| row.descriptor.callback_address == 0x4327b0 && row.descriptor.flags == 0));
    assert_eq!(
        tables[1]
            .records
            .iter()
            .map(|row| row.required[0][0])
            .collect::<Vec<_>>(),
        [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10
        ]
    );
}

#[test]
fn retail_key_name_table_covers_0xde_scan_codes() {
    let bytes = executable();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let names = parse_key_name_table(source).unwrap();
    assert_eq!(names.entries.len(), KEY_NAME_COUNT);
    for (code, name) in [
        (0x00, &b""[..]),
        (0x01, b"Esc"),
        (0x0e, b"Backspace"),
        (0x0f, b"Tab"),
        (0x10, b"Q"),
        (0x1c, b"Enter"),
        (0x39, b"Space"),
        (0x47, b"Keypad 7"),
        (0x54, b""),
        (0x9c, b"Keypad Enter"),
        (0xd3, b"Delete"),
        (0xdd, b"AppMenu"),
    ] {
        assert_eq!(names.name(code), Some(name), "code {code:#04x}");
    }
    assert_eq!(names.name(0xde), None);
    assert_eq!(
        names.entries.iter().map(|entry| entry.name.len()).max(),
        Some(12)
    );
    // Letters report single uppercase bytes; digits and punctuation single bytes.
    let singles: Vec<u8> = names
        .entries
        .iter()
        .filter(|entry| entry.name.len() == 1)
        .map(|entry| entry.name[0])
        .collect();
    assert!(singles.iter().all(|byte| !byte.is_ascii_lowercase()));
    assert_eq!(
        singles
            .iter()
            .filter(|byte| byte.is_ascii_uppercase())
            .count(),
        26
    );
}
