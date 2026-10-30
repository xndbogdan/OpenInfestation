use std::path::Path;

use super::*;

fn known_classification() -> ProfileNameClassification {
    ProfileNameClassification {
        high_bytes: Some([Some(0); 256]),
    }
}

#[test]
fn empty_and_first_nul_bypass_unavailable_classification() {
    let owner = ProfileNameClassification { high_bytes: None };
    let text = format_profile_name(Some(b"\0%s\xff"), &owner).unwrap();
    assert_eq!(text.bytes_with_nul(), b"\0");
    assert_eq!(text.length(), 0);
    assert_eq!(owner.high_bytes, None);
    assert_eq!(
        format_profile_name(None, &owner),
        Err(ProfileNameFormatError::CurrentNameUnavailable)
    );
    assert_eq!(
        format_profile_name(Some(b""), &owner),
        Err(ProfileNameFormatError::Unterminated { offset: 0 })
    );
}

#[test]
fn raw_unsigned_bytes_and_escaped_percent_preserve_format_custody() {
    let format = b"\x80\xffA%%B\0unused%s";
    let mut owner = ProfileNameClassification {
        high_bytes: Some([None; 256]),
    };
    for byte in [0x80, 0xff, b'A', b'%', b'B'] {
        owner.high_bytes.as_mut().unwrap()[usize::from(byte)] = Some(0x7f);
    }
    let before = owner.clone();
    let output = format_profile_name(Some(format), &owner).unwrap();
    assert_eq!(output.bytes_with_nul(), b"\x80\xffA%B\0");
    assert_eq!(output.length(), 5);
    assert_eq!(owner, before);
    assert_eq!(format, b"\x80\xffA%%B\0unused%s");
}

#[test]
fn current_classification_is_borrowed_afresh_and_prior_result_is_owned() {
    let mut owner = known_classification();
    let first = format_profile_name(Some(b"A%%B\0"), &owner).unwrap();
    assert_eq!(first.length() + 1, 4);
    assert_eq!(b"A%%B\0".len(), 5, "raw codec/source cursor is distinct");
    owner.high_bytes.as_mut().unwrap()[usize::from(b'A')] = Some(0x80);
    assert_eq!(
        format_profile_name(Some(b"A\0"), &owner),
        Err(ProfileNameFormatError::ClassificationBoundary {
            offset: 0,
            byte: b'A',
            high_byte: 0x80,
        })
    );
    owner.high_bytes = None;
    assert_eq!(
        format_profile_name(Some(b"A\0"), &owner),
        Err(ProfileNameFormatError::MissingClassificationBinding {
            offset: 0,
            byte: b'A',
        })
    );
    assert_eq!(first.bytes_with_nul(), b"A%B\0");
}

#[test]
fn unavailable_binding_byte_and_bit80_have_distinct_reached_errors() {
    let mut owner = ProfileNameClassification { high_bytes: None };
    assert_eq!(
        format_profile_name(Some(b"%%\0"), &owner),
        Err(ProfileNameFormatError::MissingClassificationBinding {
            offset: 1,
            byte: b'%',
        })
    );
    owner.high_bytes = Some([None; 256]);
    owner.high_bytes.as_mut().unwrap()[usize::from(b'A')] = Some(0);
    assert_eq!(
        format_profile_name(Some(b"AB\0"), &owner),
        Err(ProfileNameFormatError::MissingClassificationByte {
            offset: 1,
            byte: b'B',
        })
    );
    owner.high_bytes.as_mut().unwrap()[usize::from(b'B')] = Some(0xff);
    assert_eq!(
        format_profile_name(Some(b"AB"), &owner),
        Err(ProfileNameFormatError::ClassificationBoundary {
            offset: 1,
            byte: b'B',
            high_byte: 0xff,
        }),
        "reached classification precedes a later missing terminator"
    );
    owner.high_bytes.as_mut().unwrap()[usize::from(b'B')] = Some(0x7f);
    assert_eq!(
        format_profile_name(Some(b"AB"), &owner),
        Err(ProfileNameFormatError::Unterminated { offset: 2 })
    );
}

#[test]
fn directive_and_trailing_percent_are_explicit_narrower_admission() {
    let owner = ProfileNameClassification { high_bytes: None };
    for (format, byte) in [(b"%s\0".as_slice(), b's'), (b"%*s\0", b'*')] {
        assert_eq!(
            format_profile_name(Some(format), &owner),
            Err(ProfileNameFormatError::FormatBoundary { offset: 1, byte })
        );
    }
    assert_eq!(
        format_profile_name(Some(b"%\0"), &owner),
        Err(ProfileNameFormatError::TrailingPercentBoundary { offset: 0 }),
        "native NUL-before-state would complete; this is only our domain boundary"
    );
    assert_eq!(
        format_profile_name(Some(b"%"), &owner),
        Err(ProfileNameFormatError::Unterminated { offset: 1 })
    );
}

#[test]
fn bound_counts_emitted_bytes_and_keeps_later_data_errors_lazy() {
    let mut owner = known_classification();
    let mut escaped = b"%%".repeat(31);
    escaped.push(0);
    let text = format_profile_name(Some(&escaped), &owner).unwrap();
    assert_eq!(text.length(), 31);
    assert_eq!(text.bytes_with_nul(), [b"%".repeat(31), vec![0]].concat());
    let mut longer = vec![b'A'; 31];
    longer.extend_from_slice(b"B\0");
    owner.high_bytes.as_mut().unwrap()[usize::from(b'B')] = None;
    assert_eq!(
        format_profile_name(Some(&longer), &owner),
        Err(ProfileNameFormatError::MissingClassificationByte {
            offset: 31,
            byte: b'B',
        })
    );
    owner.high_bytes.as_mut().unwrap()[usize::from(b'B')] = Some(0);
    assert_eq!(
        format_profile_name(Some(&longer), &owner),
        Err(ProfileNameFormatError::OutputTooLong)
    );
}

#[test]
fn startup_reads_structural_original_backing_once_without_defaulting() {
    let mut raw =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
            .expect("supported retail EXE required by workspace doctor");
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let seeded = ProfileNameClassification::native_startup(executable).unwrap();
    assert_eq!(seeded.high_bytes, Some([Some(0); 256]));
    let pointer_span = executable
        .mapped(CLASSIFICATION_POINTER, 4)
        .unwrap()
        .file_span;
    let table_span = executable
        .mapped(STARTUP_CLASSIFICATION_TABLE, 512)
        .unwrap()
        .file_span;
    // Explicit structurally supported supplied data proves source-derived
    // bytes and retained ownership. The runtime parser has no hash contract.
    raw[table_span.start + 2 * usize::from(b'A') + 1] = 0x7f;
    let changed =
        ProfileNameClassification::native_startup(FrontendExecutable::parse(&raw).unwrap())
            .unwrap();
    assert_eq!(changed.high_bytes.unwrap()[usize::from(b'A')], Some(0x7f));
    assert_eq!(seeded.high_bytes, Some([Some(0); 256]));
    raw[pointer_span.start..pointer_span.end].copy_from_slice(&0x0054_5c78u32.to_le_bytes());
    assert_eq!(
        ProfileNameClassification::native_startup(FrontendExecutable::parse(&raw).unwrap()),
        Err(ProfileNameStartupError::UnsupportedStartupBinding {
            pointer: 0x0054_5c78,
        })
    );
}
