use super::*;

const DATA: u32 = 0x0053_5E00;
const FILE: usize = 0x200;
const STRINGS: u32 = DATA + 0x400;

fn put(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// One .data section at 00535E00 holding the pointer table and strings.
fn fixture() -> Vec<u8> {
    let mut bytes = vec![0; FILE + 0x600];
    bytes[..2].copy_from_slice(b"MZ");
    put(&mut bytes, 0x3c, 0x40);
    bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
    bytes[0x44..0x46].copy_from_slice(&0x014c_u16.to_le_bytes());
    bytes[0x46..0x48].copy_from_slice(&1_u16.to_le_bytes());
    put(&mut bytes, 0x48, 0x396f_38a3);
    bytes[0x54..0x56].copy_from_slice(&0xe0_u16.to_le_bytes());
    bytes[0x58..0x5a].copy_from_slice(&0x010b_u16.to_le_bytes());
    put(&mut bytes, 0x74, 0x0040_0000);
    put(&mut bytes, 0x90, 0x0027_b000);
    bytes[0x138..0x140].copy_from_slice(b".data\0\0\0");
    put(&mut bytes, 0x138 + 12, DATA - 0x0040_0000);
    put(&mut bytes, 0x138 + 16, 0x600);
    put(&mut bytes, 0x138 + 20, FILE as u32);
    // Empty string at STRINGS, "Esc" after it; every code starts empty.
    let esc = STRINGS + 1;
    let at = |address: u32| FILE + (address - DATA) as usize;
    bytes[at(esc)..at(esc) + 4].copy_from_slice(b"Esc\0");
    for code in 0..KEY_NAME_COUNT {
        put(&mut bytes, at(KEY_NAME_TABLE_ADDRESS) + code * 4, STRINGS);
    }
    put(&mut bytes, at(KEY_NAME_TABLE_ADDRESS) + 4, esc);
    bytes
}

fn table_offset(code: usize) -> usize {
    FILE + (KEY_NAME_TABLE_ADDRESS - DATA) as usize + code * 4
}

#[test]
fn names_keep_bytes_pointer_and_terminated_span() {
    let bytes = fixture();
    let table = parse_key_name_table(FrontendExecutable::parse(&bytes).unwrap()).unwrap();
    assert_eq!(table.entries.len(), KEY_NAME_COUNT);
    assert_eq!(table.name(0), Some(&b""[..]));
    assert_eq!(table.name(1), Some(&b"Esc"[..]));
    assert_eq!(table.name(0xdd), Some(&b""[..]));
    assert_eq!(table.name(0xde), None);
    let esc = &table.entries[1];
    assert_eq!(esc.pointer, STRINGS + 1);
    let start = FILE + (STRINGS + 1 - DATA) as usize;
    assert_eq!(esc.file_span, ByteSpan::new(start, start + 4));
}

#[test]
fn null_and_unterminated_names_are_errors() {
    let mut bytes = fixture();
    put(&mut bytes, table_offset(0x39), 0);
    assert_eq!(
        parse_key_name_table(FrontendExecutable::parse(&bytes).unwrap()),
        Err(KeyNameError::Null { code: 0x39 })
    );
    let mut bytes = fixture();
    let long = STRINGS + 0x100;
    let at = FILE + (long - DATA) as usize;
    bytes[at..at + KEY_NAME_MAX_BYTES].fill(b'X');
    put(&mut bytes, table_offset(2), long);
    assert_eq!(
        parse_key_name_table(FrontendExecutable::parse(&bytes).unwrap()),
        Err(KeyNameError::Unterminated {
            code: 2,
            pointer: long
        })
    );
}
