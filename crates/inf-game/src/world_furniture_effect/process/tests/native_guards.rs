use super::*;
use sha2::{Digest, Sha256};

#[test]
fn complete_original_process_entry_writers_cleanup_and_loader_profile_are_guarded() {
    let assets = assets();
    let original = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(original)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = FrontendExecutable::parse(original).unwrap();
    //The authenticated supported PE .text has raw offset==RVA. Do not widen
    //mapped(), which deliberately exposes only file-backed data sections.
    for (name, start, end, expected) in [
        (
            "clear_furniture_callback",
            0x00436e00,
            0x00436f03,
            "8b627e52ac4be64726e2ee96537963fd65c82ac48b093342754ca488523b1587",
        ),
        (
            "set_furniture_state_callback",
            0x00437650,
            0x00437745,
            "12b1c853f6c6e5757b3efbbe1dcc6cc2d7e9fa631beb8e439e31d9fa76e01336",
        ),
        (
            "start_furniture_timer_callback",
            0x00437780,
            0x0043787c,
            "7cdb676018413e09ac27c4c8285958298ef94af8a938e1a990b4bf32c9fb91d9",
        ),
        (
            "furniture_collision",
            0x0047c1d0,
            0x0047c6cc,
            "70c744e0ca1fc009d760d2516289a54013cf3a078dc4a023f3825de0451071ad",
        ),
        (
            "basis_constructor",
            0x0040d6a0,
            0x0040d7af,
            "89cfb37d18878148bae7fcc7dbf0456cd5abd80517c28a42bf2dbde929426ea0",
        ),
        (
            "clear_furniture_effect_queue",
            0x00436b80,
            0x00436bb6,
            "76596c9b15863b20787ccb983c9f70760b4f57c3aaec684f039e89b6b90d0453",
        ),
        (
            "clear_world_effects",
            0x00450210,
            0x00450222,
            "e67caa6e82e249fac7d8ac143aebffb489de4ec8d289768e19e34f5595175dff",
        ),
        (
            "literal_atexit_target",
            0x0047c6d0,
            0x0047c6d1,
            "ae3f4619b0413d70d3004b9131c3752153074e45725be13b9a148978895e359e",
        ),
        (
            "scalar_copy",
            0x00402670,
            0x0040267d,
            "9c375493fe6e8516cd7ebdc429fa91c81045e81d62b4225dc8b83f1e3b160f39",
        ),
        (
            "scalar_ratio",
            0x00402680,
            0x0040268f,
            "cde7106428f710e61e93d73d535a8c9e5bb067aa11d6032a506ef2e940ce9ae6",
        ),
        (
            "vector_store",
            0x00402690,
            0x004026a9,
            "66f50239e7eca922033451ce5bfdf4e3e44cdc54514dbac3e5ad6a7f5d93a90d",
        ),
    ] {
        let bytes = &original[(start - 0x0040_0000) as usize..(end - 0x0040_0000) as usize];
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), expected, "{name}");
    }
    for (address, length) in [(0x0054_83c4, 4), (0x0055_f0dc, 1)] {
        let proof = executable.loader_zero_data(address, length).unwrap();
        assert_eq!(proof.start(), address);
        assert_eq!(proof.end(), address + length as u32);
    }
    for address in QUERY_BASIS_ADDRESSES {
        let proof = executable.loader_zero_data(address, 40).unwrap();
        assert_eq!(proof.start(), address);
        assert_eq!(proof.end(), address + 40);
    }
    assert!(matches!(
        executable.loader_zero_data(0x0054_6fff, 4),
        Err(FrontendExecutableError::LoaderZeroRange { .. })
    ));
    //Exact native entry publishes mask before five constructor calls.
    assert_eq!(
        &original[0x7c22c..0x7c232],
        &[0x88, 0x15, 0xdc, 0xf0, 0x55, 0x00]
    );
    assert_eq!(
        MenuBasis::INITIAL.words,
        [0, 0x3f80_0000, 0, 0, 0, 0x3f80_0000, 0, 0, 0, 0x3f80_0000,]
    );
}
