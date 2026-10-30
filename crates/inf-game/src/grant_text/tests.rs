use super::*;

fn record(bytes: &[u8]) -> GrantTextSource<'_> {
    GrantTextSource::Record(Some(bytes))
}

fn classification(colon_high: Option<u8>, space_high: Option<u8>) -> GrantTextData<'static> {
    GrantTextData {
        null_substitute: None,
        pair_classification: Some(GrantTextPairClassification {
            colon_high,
            space_high,
        }),
    }
}

#[test]
fn pickup_uses_spaces_optional_detail_and_its_larger_native_buffer() {
    let mut data = PickupTextData {
        common: classification(None, Some(0)),
        hyphen_high: None,
    };
    let prefix = [b'x'; 200];
    let prefix = [prefix.as_slice(), b"\0"].concat();
    let request = PickupTextRequest {
        prefix: record(&prefix),
        name: record(b"name\0"),
        detail: None,
    };
    let text = format_pickup_text(request, Some(&data)).unwrap();
    assert_eq!(text.length(), 205);
    assert_eq!(&text.bytes_with_nul()[200..], b" name\0");
    let request = PickupTextRequest {
        prefix: record(b"Upgrade\0"),
        name: record(b"%\xff\0"),
        detail: Some(record(b"detail\0")),
    };
    assert_eq!(
        format_pickup_text(request, Some(&data)),
        Err(GrantTextError::MissingClassificationByte(
            GrantTextLiteral::Hyphen
        ))
    );
    data.hyphen_high = Some(0);
    assert_eq!(
        format_pickup_text(request, Some(&data))
            .unwrap()
            .bytes_with_nul(),
        b"Upgrade %\xff - detail\0"
    );
    let long = [b'x'; 254];
    let long = [long.as_slice(), b"\0"].concat();
    assert_eq!(
        format_pickup_text(
            PickupTextRequest {
                prefix: record(&long),
                name: record(b"x\0"),
                detail: None
            },
            Some(&data)
        ),
        Err(GrantTextError::PickupOutputTooLong)
    );
}

#[test]
fn single_preserves_raw_bytes_and_ignores_first_nul_tail_without_data_owner() {
    let text = format_grant_text(
        GrantTextRequest::Single {
            record: record(b"%\x80\xffx\0unused%\xff"),
        },
        None,
    )
    .unwrap();
    assert_eq!(text.bytes_with_nul(), b"%\x80\xffx\0");
    assert_eq!(text.length(), 4);
}

#[test]
fn pair_preserves_argument_bytes_and_ignores_classification_bits_below_80() {
    let data = classification(Some(0x7f), Some(0x01));
    let text = format_grant_text(
        GrantTextRequest::Pair {
            category: record(b"\xfe%\0unused"),
            record: record(b"\x80value%\0unused"),
        },
        Some(&data),
    )
    .unwrap();
    assert_eq!(text.bytes_with_nul(), b"\xfe%: \x80value%\0");
    assert_eq!(text.length(), 11);
}

#[test]
fn empty_arguments_retain_only_reached_literals_and_final_nul() {
    let single = format_grant_text(
        GrantTextRequest::Single {
            record: record(b"\0"),
        },
        None,
    )
    .unwrap();
    assert_eq!(single.bytes_with_nul(), b"\0");
    assert_eq!(single.length(), 0);
    let data = classification(Some(0), Some(0));
    let pair = format_grant_text(
        GrantTextRequest::Pair {
            category: record(b"\0"),
            record: record(b"\0"),
        },
        Some(&data),
    )
    .unwrap();
    assert_eq!(pair.bytes_with_nul(), b": \0");
    assert_eq!(pair.length(), 2);
}

#[test]
fn genuine_null_uses_the_same_current_owner_borrowed_afresh_between_calls() {
    let request = GrantTextRequest::Single {
        record: GrantTextSource::Null,
    };
    let mut data = GrantTextData {
        null_substitute: Some(GrantTextNullBinding {
            record: Some(b"first\0tail"),
        }),
        pair_classification: None,
    };
    let first = format_grant_text(request, Some(&data)).unwrap();
    data.null_substitute.as_mut().unwrap().record = Some(b"second\0");
    let second = format_grant_text(request, Some(&data)).unwrap();
    assert_eq!(first.bytes_with_nul(), b"first\0");
    assert_eq!(second.bytes_with_nul(), b"second\0");
    assert_eq!(first.length(), 5);
    assert_eq!(second.length(), 6);
}

#[test]
fn genuine_null_and_unavailable_record_binding_and_backing_are_distinct() {
    let request = GrantTextRequest::Single {
        record: GrantTextSource::Null,
    };
    assert_eq!(
        format_grant_text(request, None),
        Err(GrantTextError::MissingNullBinding(
            GrantTextArgument::Record
        ))
    );
    let data = GrantTextData {
        null_substitute: Some(GrantTextNullBinding { record: None }),
        pair_classification: None,
    };
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::MissingNullBacking(
            GrantTextArgument::Record
        ))
    );
    assert_eq!(
        format_grant_text(
            GrantTextRequest::Single {
                record: GrantTextSource::Record(None),
            },
            Some(&data),
        ),
        Err(GrantTextError::MissingRecordBacking(
            GrantTextArgument::Record
        ))
    );
    let text = format_grant_text(
        GrantTextRequest::Single {
            record: record(b"known\0"),
        },
        Some(&data),
    )
    .unwrap();
    assert_eq!(text.bytes_with_nul(), b"known\0");
}

#[test]
fn pair_null_arguments_are_substituted_in_their_reached_phases() {
    let data = GrantTextData {
        null_substitute: Some(GrantTextNullBinding {
            record: Some(b"(null)\0"),
        }),
        pair_classification: Some(GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    };
    let text = format_grant_text(
        GrantTextRequest::Pair {
            category: GrantTextSource::Null,
            record: GrantTextSource::Null,
        },
        Some(&data),
    )
    .unwrap();
    assert_eq!(text.bytes_with_nul(), b"(null): (null)\0");
    assert_eq!(text.length(), 14);
}

#[test]
fn pair_errors_follow_category_then_colon_then_space_then_record() {
    let unavailable_record = GrantTextSource::Record(None);
    assert_eq!(
        format_grant_text(
            GrantTextRequest::Pair {
                category: unavailable_record,
                record: unavailable_record,
            },
            None,
        ),
        Err(GrantTextError::MissingRecordBacking(
            GrantTextArgument::Category
        ))
    );
    let request = GrantTextRequest::Pair {
        category: record(b"category\0"),
        record: unavailable_record,
    };
    assert_eq!(
        format_grant_text(request, None),
        Err(GrantTextError::MissingClassificationBinding(
            GrantTextLiteral::Colon
        ))
    );
    let mut data = classification(None, None);
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::MissingClassificationByte(
            GrantTextLiteral::Colon
        ))
    );
    data.pair_classification.as_mut().unwrap().colon_high = Some(0);
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::MissingClassificationByte(
            GrantTextLiteral::Space
        ))
    );
    data.pair_classification.as_mut().unwrap().space_high = Some(0);
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::MissingRecordBacking(
            GrantTextArgument::Record
        ))
    );
}

#[test]
fn pair_record_null_binding_is_not_required_before_separator_errors() {
    let request = GrantTextRequest::Pair {
        category: record(b"category\0"),
        record: GrantTextSource::Null,
    };
    let mut data = classification(Some(0), None);
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::MissingClassificationByte(
            GrantTextLiteral::Space
        ))
    );
    data.pair_classification.as_mut().unwrap().space_high = Some(0);
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::MissingNullBinding(
            GrantTextArgument::Record
        ))
    );
    assert_eq!(
        format_grant_text(
            GrantTextRequest::Pair {
                category: GrantTextSource::Null,
                record: record(b"record\0"),
            },
            None,
        ),
        Err(GrantTextError::MissingNullBinding(
            GrantTextArgument::Category
        ))
    );
}

#[test]
fn each_reached_bit80_classification_has_a_named_boundary() {
    let request = GrantTextRequest::Pair {
        category: record(b"category\0"),
        record: GrantTextSource::Record(None),
    };
    let mut data = classification(Some(0x80), Some(0xff));
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::ClassificationBoundary {
            literal: GrantTextLiteral::Colon,
            high_byte: 0x80,
        })
    );
    data.pair_classification.as_mut().unwrap().colon_high = Some(0x7f);
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::ClassificationBoundary {
            literal: GrantTextLiteral::Space,
            high_byte: 0xff,
        })
    );
}

#[test]
fn exact_191_payload_bytes_complete_for_both_modes() {
    let mut single_source = vec![0xff; 191];
    single_source.push(0);
    let single = format_grant_text(
        GrantTextRequest::Single {
            record: record(&single_source),
        },
        None,
    )
    .unwrap();
    assert_eq!(single.bytes_with_nul(), single_source);
    assert_eq!(single.length(), 191);

    let mut category = vec![b'c'; 94];
    category.push(0);
    let mut record_source = vec![b'r'; 95];
    record_source.push(0);
    let data = classification(Some(0), Some(0));
    let pair = format_grant_text(
        GrantTextRequest::Pair {
            category: record(&category),
            record: record(&record_source),
        },
        Some(&data),
    )
    .unwrap();
    let mut expected = vec![b'c'; 94];
    expected.extend_from_slice(b": ");
    expected.extend_from_slice(&record_source);
    assert_eq!(pair.bytes_with_nul(), expected);
    assert_eq!(pair.length(), 191);
}

#[test]
fn oversized_sources_do_not_truncate_and_still_scan_before_copy_admission() {
    let mut source = vec![b'x'; 192];
    assert_eq!(
        format_grant_text(
            GrantTextRequest::Single {
                record: record(&source),
            },
            None,
        ),
        Err(GrantTextError::Unterminated {
            argument: GrantTextArgument::Record,
            backing: GrantTextBacking::Record,
        })
    );
    source.push(0);
    assert_eq!(
        format_grant_text(
            GrantTextRequest::Single {
                record: record(&source),
            },
            None,
        ),
        Err(GrantTextError::OutputTooLong)
    );
    let mut category = vec![b'c'; 191];
    category.push(0);
    let request = GrantTextRequest::Pair {
        category: record(&category),
        record: GrantTextSource::Record(None),
    };
    assert_eq!(
        format_grant_text(request, None),
        Err(GrantTextError::MissingClassificationBinding(
            GrantTextLiteral::Colon
        ))
    );
    let data = classification(Some(0), None);
    assert_eq!(
        format_grant_text(request, Some(&data)),
        Err(GrantTextError::OutputTooLong)
    );
}

#[test]
fn unterminated_substitute_is_distinct_from_a_source_record_and_unused_data() {
    let data = GrantTextData {
        null_substitute: Some(GrantTextNullBinding {
            record: Some(b"unterminated"),
        }),
        pair_classification: None,
    };
    assert_eq!(
        format_grant_text(
            GrantTextRequest::Single {
                record: GrantTextSource::Null,
            },
            Some(&data),
        ),
        Err(GrantTextError::Unterminated {
            argument: GrantTextArgument::Record,
            backing: GrantTextBacking::NullSubstitute,
        })
    );
    assert_eq!(
        format_grant_text(
            GrantTextRequest::Single {
                record: record(b"known\0"),
            },
            Some(&data),
        )
        .unwrap()
        .bytes_with_nul(),
        b"known\0"
    );
}
