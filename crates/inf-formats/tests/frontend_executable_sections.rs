use inf_formats::{ByteSpan, FrontendExecutable, FrontendExecutableError};

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
    for (header, name, rva, file) in [
        (0x138, b".rdata\0\0", 0x128000, 0x200),
        (0x160, b".data\0\0\0", 0x133000, 0x300),
    ] {
        bytes[header..header + 8].copy_from_slice(name);
        put(&mut bytes, header + 8, 0x180); // virtual tail has no file bytes
        put(&mut bytes, header + 12, rva);
        put(&mut bytes, header + 16, 0x100);
        put(&mut bytes, header + 20, file);
    }
    put(&mut bytes, 0x214, 0x1122_3344);
    put(&mut bytes, 0x314, 0x5566_7788);
    bytes
}

#[test]
fn read_only_mapping_preserves_physical_origin_without_changing_data_view() {
    let bytes = fixture();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(source.word(0x528014).unwrap(), 0x1122_3344);
    assert_eq!(source.word(0x533014).unwrap(), 0x5566_7788);
    let range = source.mapped(0x528014, 4).unwrap();
    assert_eq!(range.virtual_address, 0x528014);
    assert_eq!(range.file_span, ByteSpan::new(0x214, 0x218));
    assert_eq!(range.bytes, &bytes[0x214..0x218]);
    assert_eq!(source.data().virtual_address, 0x533000);
    assert_eq!(source.data().bytes, &bytes[0x300..0x400]);
    assert_eq!(source.file_span(), ByteSpan::new(0x300, 0x400));
}

#[test]
fn mapping_rejects_cross_section_ranges_gaps_virtual_zero_fill_and_overflow() {
    let bytes = fixture();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    for (address, length) in [
        (0x527fff, 1),
        (0x5280ff, 2),
        (0x528100, 1),
        (0x528170, 4),
        (0x52a000, 4),
        (0x533100, 1),
        (0x528014, usize::MAX),
        (u32::MAX, 4),
    ] {
        assert!(
            source.mapped(address, length).is_err(),
            "{address:x}+{length}"
        );
    }
    assert!(source.word(0x5280fd).is_err());
    assert!(source.mapped(0x5280fc, 4).is_ok());
}

#[test]
fn duplicate_and_virtually_overlapping_read_only_sections_are_rejected() {
    let mut bytes = fixture();
    bytes[0x46..0x48].copy_from_slice(&3_u16.to_le_bytes());
    bytes.copy_within(0x138..0x160, 0x188);
    assert!(matches!(
        FrontendExecutable::parse(&bytes),
        Err(FrontendExecutableError::ReadOnlyDataSection)
    ));
    for rva in [0x132f80, 0x133000, 0x133080] {
        let mut bytes = fixture();
        put(&mut bytes, 0x144, rva);
        assert!(
            matches!(
                FrontendExecutable::parse(&bytes),
                Err(FrontendExecutableError::ReadOnlyDataSection)
            ),
            "rva={rva:x}"
        );
    }
    let mut bytes = fixture();
    put(&mut bytes, 0x144, 0x133100); // exact adjacent file-backed virtual ranges
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(source.word(0x533114).unwrap(), 0x1122_3344);
    assert!(
        source.mapped(0x5330fc, 8).is_err(),
        "mapping cannot join two sections"
    );
}

#[test]
fn read_only_section_is_optional_but_never_maps_code_or_truncated_storage() {
    let mut bytes = fixture();
    bytes[0x138..0x140].copy_from_slice(b".text\0\0\0");
    let source = FrontendExecutable::parse(&bytes).unwrap();
    assert!(source.mapped(0x528014, 4).is_err());
    assert_eq!(source.word(0x533014).unwrap(), 0x5566_7788);
    let mut bytes = fixture();
    put(&mut bytes, 0x14c, 0x380);
    assert!(matches!(
        FrontendExecutable::parse(&bytes),
        Err(FrontendExecutableError::Truncated { .. })
    ));
    let mut bytes = fixture();
    put(&mut bytes, 0x144, 0x27afff);
    assert!(FrontendExecutable::parse(&bytes).is_err());
}

#[test]
fn loader_zero_proof_is_separate_from_original_data_mapping() {
    let bytes = fixture();
    let source = FrontendExecutable::parse(&bytes).unwrap();
    let proof = source.loader_zero_data(0x533100, 0x80).unwrap();
    assert_eq!((proof.start(), proof.end()), (0x533100, 0x533180));
    assert!(source.mapped(proof.start(), 1).is_err());
    for (address, length) in [
        (0x5330ff, 1),
        (0x53317f, 2),
        (0x533180, 1),
        (0x528100, 1),
        (u32::MAX, 4),
        (0x533100, usize::MAX),
    ] {
        assert!(matches!(
            source.loader_zero_data(address, length),
            Err(FrontendExecutableError::LoaderZeroRange { .. })
        ));
    }
}

#[test]
fn loader_zero_proof_rejects_bad_virtual_extent_without_changing_file_mapping() {
    for extent in [0_u32, 0x80, u32::MAX] {
        let mut bytes = fixture();
        put(&mut bytes, 0x168, extent);
        let source = FrontendExecutable::parse(&bytes).unwrap();
        assert_eq!(source.word(0x533014).unwrap(), 0x5566_7788);
        assert!(source.loader_zero_data(0x533100, 4).is_err());
    }
}

#[test]
fn loader_zero_proof_rejects_other_section_file_and_virtual_overlap() {
    for (name, raw, virtual_size) in [
        (b".rdata\0\0", 0x100, 0x180),
        (b".text\0\0\0", 0x100, 0x180),
        (b".bss\0\0\0\0", 0, 0x180),
    ] {
        let mut bytes = fixture();
        bytes[0x138..0x140].copy_from_slice(name);
        put(&mut bytes, 0x144, 0x133100);
        put(&mut bytes, 0x148, raw);
        put(&mut bytes, 0x140, virtual_size);
        let source = FrontendExecutable::parse(&bytes).unwrap();
        assert!(source.loader_zero_data(0x533100, 0x80).is_err());
    }
}
