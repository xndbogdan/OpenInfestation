use std::{fs, io::ErrorKind, path::Path};

use inf_formats::{
    encode_save_records, save_crc32, SaveArchive, SaveBlockSource, SaveRecord,
    SAVE_BLOCK_PAYLOAD_BYTES, SAVE_COPY_BYTES,
};
use sha2::{Digest, Sha256};

#[test]
fn installed_save_profiles_decode_and_round_trip_without_writing_retail_data() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.sav");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            eprintln!("optional retail Infest.sav is absent; save corpus test skipped");
            return;
        }
        Err(error) => panic!("cannot read optional retail save: {error}"),
    };
    let archive = SaveArchive::parse(&bytes).unwrap();
    for record in archive.records() {
        assert_eq!(
            SaveRecord::from_state(
                record.name(),
                &record.mission(),
                &record.player(),
                record.tail(),
            )
            .unwrap(),
            *record,
        );
    }
    let encoded = encode_save_records(archive.records()).unwrap();
    assert_eq!(
        SaveArchive::parse(&encoded).unwrap().records(),
        archive.records()
    );
    // A save is mutable user data. Pin the known receipt only when its exact
    // source identity matches, and otherwise validate the current records.
    if format!("{:x}", Sha256::digest(&bytes))
        == "83e86f73811d0a8f337464780e6d6cc9499d59080fe09c9956530061ed8514d9"
    {
        validate_known_receipt(&bytes, &archive);
        assert_eq!(encoded, bytes);
    }
    assert_eq!(fs::read(path).unwrap(), bytes);
}

fn validate_known_receipt(bytes: &[u8], archive: &SaveArchive) {
    assert_eq!(bytes.len(), 9216);
    let expected: &[(&[u8], u32, u32)] = &[
        (b"Jameson", 1, 14090),
        (b"Jameson", 11, 5033),
        (b"BOBSTER", 1, 2634),
        (b"Jameson", 2, 1079),
    ];
    assert_eq!(archive.records().len(), expected.len());
    for (record, (name, mission, elapsed)) in archive.records().iter().zip(expected) {
        assert_eq!(record.name(), *name);
        let state = record.mission();
        assert_eq!(state.mission_selection, *mission);
        assert_eq!(state.elapsed_seconds, *elapsed);
        assert_eq!(record.tail(), &[0x0f; 24]);
        assert_eq!(
            SaveRecord::from_state(name, &state, &record.player(), record.tail()).unwrap(),
            *record
        );
    }
    assert!(archive
        .block_sources()
        .iter()
        .flatten()
        .all(|v| *v == SaveBlockSource::Primary));
    // Inspection validates every stored copy, while the native parser uses only
    // the first valid one. This receipt must not change the parser's policy.
    for pair in bytes.chunks_exact(SAVE_COPY_BYTES * 2) {
        assert_eq!(&pair[..SAVE_COPY_BYTES], &pair[SAVE_COPY_BYTES..]);
        for copy in pair.chunks_exact(SAVE_COPY_BYTES) {
            assert_eq!(
                save_crc32(&copy[..SAVE_BLOCK_PAYLOAD_BYTES]),
                u32::from_le_bytes(copy[SAVE_BLOCK_PAYLOAD_BYTES..].try_into().unwrap())
            );
        }
    }
}
