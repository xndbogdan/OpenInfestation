use super::*;

fn fixture(role: SampleGridRole) -> (Vec<u8>, ByteSpan, Handler10Record) {
    let (components, component_bytes) = role.component_shape();
    let dimensions = 28usize;
    let descriptors = dimensions + 8;
    let samples = descriptors + components * 4;
    let mut bytes = vec![0xa5; 9 + samples + 2 * components * component_bytes as usize];
    let payload = ByteSpan::new(9, bytes.len());
    let data = &mut bytes[9..];
    data[dimensions..dimensions + 4].copy_from_slice(&2u32.to_le_bytes());
    data[dimensions + 4..dimensions + 8].copy_from_slice(&1u32.to_le_bytes());
    for index in 0..components {
        data[descriptors + index * 4..descriptors + index * 4 + 4]
            .copy_from_slice(&component_bytes.to_le_bytes());
    }
    let record = Handler10Record {
        local_index: 7,
        span: ByteSpan::new(9, 37),
        words_00_to_0c: [2, components as u32, 2, components as u32 * component_bytes],
        relative_offsets: [dimensions as u32, descriptors as u32, samples as u32],
    };
    (bytes, payload, record)
}

#[test]
fn signed_samples_preserve_source_order_offsets_and_roles() {
    let role = SampleGridRole::EightWordSamples;
    let (mut bytes, payload, record) = fixture(role);
    let at = payload.start + record.relative_offsets[2] as usize;
    let values = [i16::MIN, -3, -1, 0, 1, 3, i16::MAX, 42];
    for (i, value) in values
        .into_iter()
        .chain(values.into_iter().rev())
        .enumerate()
    {
        bytes[at + i * 2..at + i * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    let before = bytes.clone();
    let grid = decode_sample_grid(&bytes, payload, &record, role).unwrap();
    assert_eq!((grid.width, grid.height, grid.sample_count()), (2, 1, 2));
    assert_eq!(grid.samples_span, ByteSpan::new(at, at + 32));
    assert_eq!(
        grid.samples,
        SampleGridValues::SignedWords8(vec![values, std::array::from_fn(|i| values[7 - i])])
    );
    assert_eq!(bytes, before);
    assert_eq!(
        SampleGridRole::from_global_slot(1),
        Some(SampleGridRole::FourSampleMask)
    );
    assert_eq!(
        SampleGridRole::from_global_slot(record.local_index as u16),
        None
    );
}

#[test]
fn byte_consumers_keep_signed_values_without_clamping_to_current_palette() {
    for role in [
        SampleGridRole::FourSampleMask,
        SampleGridRole::MaterialIndices,
    ] {
        let (mut bytes, payload, record) = fixture(role);
        let at = payload.start + record.relative_offsets[2] as usize;
        bytes[at..at + 2].copy_from_slice(&[0x80, 0xff]);
        let grid = decode_sample_grid(&bytes, payload, &record, role).unwrap();
        assert_eq!(grid.samples, SampleGridValues::SignedBytes(vec![-128, -1]));
    }
}

#[test]
fn malformed_layouts_fail_before_reading_or_allocating_sample_data() {
    let role = SampleGridRole::EightWordSamples;
    let (bytes, payload, record) = fixture(role);
    for field in 0..3 {
        let mut bad = record;
        bad.relative_offsets[field] = payload.len() as u32;
        assert!(matches!(
            decode_sample_grid(&bytes, payload, &bad, role),
            Err(SampleGridError::OutOfBounds { .. })
        ));
    }
    let mut bad = record;
    bad.words_00_to_0c[3] = 1;
    assert!(matches!(
        decode_sample_grid(&bytes, payload, &bad, role),
        Err(SampleGridError::UnsupportedMetadata { .. })
    ));
    let mut bad_bytes = bytes.clone();
    bad_bytes[payload.start + record.relative_offsets[1] as usize] = 4;
    assert!(matches!(
        decode_sample_grid(&bad_bytes, payload, &record, role),
        Err(SampleGridError::UnsupportedDescriptor { .. })
    ));
    let mut bad_bytes = bytes.clone();
    let dims = payload.start + record.relative_offsets[0] as usize;
    bad_bytes[dims..dims + 8].fill(0);
    assert!(matches!(
        decode_sample_grid(&bad_bytes, payload, &record, role),
        Err(SampleGridError::InvalidDimensions { .. })
    ));
    bad_bytes[dims..dims + 4].copy_from_slice(&(i32::MAX as u32).to_le_bytes());
    bad_bytes[dims + 4..dims + 8].copy_from_slice(&3u32.to_le_bytes());
    assert_eq!(
        decode_sample_grid(&bad_bytes, payload, &record, role),
        Err(SampleGridError::SizeOverflow)
    );
    assert!(matches!(
        decode_sample_grid(&bytes, ByteSpan::new(9, bytes.len() + 1), &record, role),
        Err(SampleGridError::InvalidPayload(_))
    ));
}
