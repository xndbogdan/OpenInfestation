//! Whole canonical EXE and complete native change/wrapper/target consumers.
//! Table/source guards prove static identities, not runtime descriptor selection.
#[test]
fn complete_changes_wrappers_target_consumers_and_final_profile_tables_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let _mapped = inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in [
        (
            0x004691D0usize,
            0x004691E4usize,
            "03858fe98f92cb291f85f7a1da87f7ea5eb097b17e4db0900ac8a334da2074d1",
        ),
        (
            0x004692F0usize,
            0x00469304usize,
            "40d9f9975ba1c5b2555bf3d235dfc24c27c5c8a6752ee65468af8ec8b4f86493",
        ),
        (
            0x00467340usize,
            0x0046738Eusize,
            "22c1ad26deac4323e8d841104087671190ab1c96396c78404faf3f7b8a3d6729",
        ),
        (
            0x00467760usize,
            0x00467832usize,
            "9b90a0d024c1a29728c8ff2982d7e36bcc732c3a00f27563d297d81a6fa3e996",
        ),
        (
            0x004827C0usize,
            0x004827CAusize,
            "36839f28b80a982fe32446ba9f5207c3d55402127d122f0bc9558de8d6646dbf",
        ),
        (
            0x00402730usize,
            0x00402770usize,
            "558b1f333eed18cf5997503a46d31a896d5e83c284f85292b547ed9965809808",
        ),
        (
            0x004828D0usize,
            0x00482934usize,
            "94dae4a849808a9965c020d6bc7981130df5bcce1908ce9409bcf078649818b4",
        ),
        (
            0x004073A0usize,
            0x004073E6usize,
            "a5b12f08d4a1b530dacc8a212c264957e2290654c155c7d3c1198e665bf80d6c",
        ),
        (
            0x00401300usize,
            0x00401303usize,
            "4bc724f3b1d0caf4fe369c18cba3102e6c4ea057f63fe1587e3973134a7f755e",
        ),
    ] {
        //Canonical .text/.rdata raw offsets equal RVA after strict profile validation.
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x400000..end - 0x400000])
            ),
            expected
        );
    }
    for (offset, expected) in [
        (0x134usize, 0x461c20u32),
        (0x138, 0x467340),
        (0x13c, 0x467660),
        (0x140, 0x467840),
        (0x144, 0x467760),
        (0x148, 0x467390),
    ] {
        let at = 0x527058 - 0x400000 + offset;
        assert_eq!(
            u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()),
            expected
        );
    }
    for (address, expected) in [
        (0x527008usize, [0x466fa0u32, 0x469190, 0x4691d0, 0x425d90]),
        (0x527038, [0x466fa0, 0x4692b0, 0x4692f0, 0x469310]),
    ] {
        for (index, word) in expected.into_iter().enumerate() {
            let at = address - 0x400000 + index * 4;
            assert_eq!(
                u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()),
                word
            );
        }
    }
    let absent = [
        0x54ecd0u32,
        0x54ecd8,
        0x54ecc0,
        0x54ecd4,
        0x54ece0,
        0x54eccc,
        0x54ecdc,
        0x54ecc8,
    ];
    let present = [
        0x54ecd0u32,
        0x54ecd8,
        0x54ecc0,
        0x54ecd4,
        0x54ecc4,
        0x54ece0,
        0x54eccc,
        0x54ecdc,
        0x54ecc8,
    ];
    for (address, words) in [
        (0x53a098usize, absent.as_slice()),
        (0x53a0b8, present.as_slice()),
    ] {
        for (index, word) in words.iter().enumerate() {
            let at = address - 0x400000 + index * 4;
            assert_eq!(
                u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()),
                *word
            );
        }
    }
}
