use super::*;

const READ_ONLY: u32 = 0x0052_8000;
const DATA: u32 = 0x0053_3000;

fn put(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture() -> Vec<u8> {
    let mut bytes = vec![0; 0x400];
    bytes[..2].copy_from_slice(b"MZ");
    put(&mut bytes, 0x3c, 0x40);
    bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
    bytes[0x44..0x46].copy_from_slice(&0x014c_u16.to_le_bytes());
    bytes[0x46..0x48].copy_from_slice(&2_u16.to_le_bytes());
    put(&mut bytes, 0x48, 0x396f_38a3);
    bytes[0x54..0x56].copy_from_slice(&0xe0_u16.to_le_bytes());
    bytes[0x58..0x5a].copy_from_slice(&0x010b_u16.to_le_bytes());
    put(&mut bytes, 0x74, 0x0040_0000);
    put(&mut bytes, 0x90, 0x0027_b000);
    for (header, name, address, file) in [
        (0x138, b".rdata\0\0", READ_ONLY, 0x200),
        (0x160, b".data\0\0\0", DATA, 0x300),
    ] {
        bytes[header..header + 8].copy_from_slice(name);
        put(&mut bytes, header + 12, address - 0x0040_0000);
        put(&mut bytes, header + 16, 0x100);
        put(&mut bytes, header + 20, file);
    }
    bytes
}

#[test]
fn digital_records_keep_cross_section_identity_code_groups_and_file_spans() {
    let mut bytes = fixture();
    put(&mut bytes, 0x220, DATA + 8);
    bytes[0x224..0x234].copy_from_slice(&[1, 0, 3, 4, 5, 6, 0, 8, 9, 10, 11, 12, 13, 0, 15, 16]);
    put(&mut bytes, 0x308, 0x0044_5200);
    put(&mut bytes, 0x30c, 0x8000_0007);
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let table = parse_digital_input_table(source, READ_ONLY + 0x20).unwrap();
    assert_eq!(table.records.len(), 1);
    let row = table.records[0];
    assert_eq!(row.virtual_address, READ_ONLY + 0x20);
    assert_eq!(row.file_span, ByteSpan::new(0x220, 0x234));
    assert_eq!(row.required, [[1, 0, 3, 4], [5, 6, 0, 8]]);
    assert_eq!(row.forbidden, [[9, 10, 11, 12], [13, 0, 15, 16]]);
    assert_eq!(row.descriptor.identity, DATA + 8);
    assert_eq!(row.descriptor.file_span, ByteSpan::new(0x308, 0x310));
    assert_eq!(row.descriptor.callback_address, 0x0044_5200);
    assert_eq!(row.descriptor.flags, 0x8000_0007);
    assert_eq!(table.terminator, ByteSpan::new(0x234, 0x238));
}

#[test]
fn analog_words_keep_signed_arithmetic_and_per_record_raw_policy() {
    let mut bytes = fixture();
    for (index, values) in [
        [
            DATA + 0x10,
            u32::MAX,
            1,
            (-10_i32) as u32,
            0x8000_0000,
            (-64_i32) as u32,
        ],
        [DATA + 0x10, 1, 0x80, 11, 0x4000_0000, 64],
    ]
    .into_iter()
    .enumerate()
    {
        for (field, value) in values.into_iter().enumerate() {
            put(&mut bytes, 0x240 + index * 24 + field * 4, value);
        }
    }
    put(&mut bytes, 0x310, 0);
    put(&mut bytes, 0x314, 2);
    put(&mut bytes, 0x318, 123_456);
    let table =
        parse_analog_input_table(FrontendExecutable::parse(&bytes).unwrap(), READ_ONLY + 0x40)
            .unwrap();
    assert_eq!(table.records.len(), 2);
    assert_eq!(table.records[0].axis, u32::MAX);
    assert_eq!(table.records[0].flags, 1);
    assert_eq!(
        table.records[1].flags, 0x80,
        "preserve raw word even though first row controls native policy"
    );
    assert_eq!(table.records[0].offset, -10);
    assert_eq!(table.records[0].scale_q31, i32::MIN);
    assert_eq!(table.records[0].multiplier, -64);
    assert_eq!(table.records[1].multiplier, 64);
    assert_eq!(table.records[0].descriptor.callback_address, 0);
    assert_eq!(table.records[0].descriptor.flags, 2);
    assert_eq!(table.records[0].descriptor.period_micros, 123_456);
    assert_eq!(
        table.records[0].descriptor.file_span,
        Some(ByteSpan::new(0x310, 0x31c))
    );
    assert_eq!(table.terminator, ByteSpan::new(0x270, 0x274));
}

#[test]
fn sentinel_requires_only_its_zero_word_even_at_section_end() {
    let bytes = fixture();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let digital = parse_digital_input_table(source, READ_ONLY + 0xfc).unwrap();
    let analog = parse_analog_input_table(source, READ_ONLY + 0xfc).unwrap();
    assert!(digital.records.is_empty());
    assert!(analog.records.is_empty());
    assert_eq!(digital.terminator, ByteSpan::new(0x2fc, 0x300));
    assert_eq!(analog.terminator, digital.terminator);
}

#[test]
fn absent_sentinel_short_record_and_unmapped_descriptor_fail() {
    let mut bytes = fixture();
    put(&mut bytes, 0x2ec, DATA);
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert!(matches!(
        parse_digital_input_table(source, READ_ONLY + 0xec),
        Err(InputTableError::MissingTerminator { .. })
    ));
    assert!(parse_analog_input_table(source, READ_ONLY + 0xec).is_err());
    put(&mut bytes, 0x220, 0x0040_1000);
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert!(matches!(
        parse_digital_input_table(source, READ_ONLY + 0x20),
        Err(InputTableError::Executable(
            FrontendExecutableError::DataAddress(0x0040_1000)
        ))
    ));
    assert!(parse_digital_input_table(source, u32::MAX - 3).is_err());
    assert!(parse_frontend_input_tables(source).is_err());
}

#[test]
fn descriptor_widths_are_eight_and_twelve_with_no_read_of_adjacent_globals() {
    let mut bytes = fixture();
    put(&mut bytes, 0x3f4, 0x1234_5678);
    put(&mut bytes, 0x3f8, 7);
    put(&mut bytes, 0x3fc, 0x8765_4321);
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let analog = parse_analog_action_descriptor(source, DATA + 0xf4).unwrap();
    assert_eq!(analog.period_micros, 0x8765_4321);
    assert_eq!(analog.file_span, Some(ByteSpan::new(0x3f4, 0x400)));
    let digital = parse_digital_action_descriptor(source, DATA + 0xf8).unwrap();
    assert_eq!(digital.callback_address, 7);
    assert_eq!(digital.flags, 0x8765_4321);
    assert!(parse_analog_action_descriptor(source, DATA + 0xf8).is_err());
    assert!(parse_digital_action_descriptor(source, DATA + 0xfc).is_err());
}

#[test]
fn missing_terminator_cannot_continue_into_an_adjacent_section() {
    for (stride, analog) in [(20, false), (24, true)] {
        let mut bytes = fixture();
        let adjacent = READ_ONLY + 0x100;
        put(&mut bytes, 0x16c, adjacent - 0x0040_0000);
        let file = 0x300 - stride;
        put(&mut bytes, file, adjacent + 8);
        let address = READ_ONLY + (file - 0x200) as u32;
        let source = FrontendExecutable::parse(&bytes).unwrap();
        assert_eq!(source.word(adjacent).unwrap(), 0);
        let error = if analog {
            parse_analog_input_table(source, address).unwrap_err()
        } else {
            parse_digital_input_table(source, address).unwrap_err()
        };
        assert_eq!(error, InputTableError::MissingTerminator { address });
    }
}
