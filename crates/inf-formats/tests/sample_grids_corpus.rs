use inf_formats::{
    decode_sample_grid, parse_handler10_bank, parse_overlay, Preload, SampleGridRole,
    SampleGridValues,
};
use std::{fs, path::Path};

#[test]
fn every_retail_sample_grid_is_bounded_signed_and_losslessly_reconstructable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let mut sample_bytes = 0;
    let mut typed_words = 0;
    let mut byte_samples = [0; 2];
    for (difficulty, width) in [24, 38, 48, 60, 76].into_iter().enumerate() {
        let bytes = fs::read(root.join("Overlay").join(format!("1d{difficulty}.ovl"))).unwrap();
        let requests = &preload.leaf(1).unwrap().difficulty;
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 10)
            .unwrap();
        let bank = parse_handler10_bank(&bytes, region).unwrap();
        let mut ranges = Vec::new();
        for record in &bank.records {
            let global = requests.0[10]
                .global_index(record.local_index as u16)
                .unwrap();
            let role = SampleGridRole::from_global_slot(global).unwrap();
            let grid = decode_sample_grid(&bytes, bank.payload, record, role).unwrap();
            let size = if role == SampleGridRole::MaterialIndices {
                width * 2
            } else {
                width
            };
            assert_eq!((grid.width, grid.height), (size, size));
            let reconstructed: Vec<u8> = match &grid.samples {
                SampleGridValues::SignedWords8(values) => {
                    typed_words += values.len() * 8;
                    values
                        .iter()
                        .flatten()
                        .flat_map(|word| word.to_le_bytes())
                        .collect()
                }
                SampleGridValues::SignedBytes(values) => {
                    byte_samples[usize::from(role == SampleGridRole::MaterialIndices)] +=
                        values.len();
                    assert!(values.iter().all(|&value| (0..=15).contains(&value)));
                    values.iter().map(|&byte| byte as u8).collect()
                }
            };
            assert_eq!(reconstructed, grid.samples_span.get(&bytes).unwrap());
            sample_bytes += reconstructed.len();
            ranges.extend([
                grid.dimensions_span,
                grid.descriptor_span,
                grid.samples_span,
            ]);
        }
        // Installed arrays are contiguous. This does not assume a generic
        // allocation order, and every byte is compared through its real span.
        ranges.sort_by_key(|span| span.start);
        let mut cursor = bank.table.end;
        for range in ranges {
            assert_eq!(range.start, cursor);
            cursor = range.end;
        }
        assert_eq!(cursor, bank.payload.end);
    }
    assert_eq!(sample_bytes, 287700);
    assert_eq!(typed_words, 109600);
    assert_eq!(byte_samples, [13700, 54800]);
}
