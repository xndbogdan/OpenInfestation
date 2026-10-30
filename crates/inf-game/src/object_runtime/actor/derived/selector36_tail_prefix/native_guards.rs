//! Complete original consumers, not execution or whole-scene acceptance.
#[test]
fn complete_original_consumers_and_source_identity_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let mapped = inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in [
        (
            0x00481710usize,
            0x00481A48usize,
            "2f332cafaedb3db1828bf4ccc34063f419dcfafd3353eadec8ac852496cd9d22",
        ), //caller36
        (
            0x00481260usize,
            0x0048170Busize,
            "5e8a74b3a93d1c2f22901db243cbbcb6fc975c0b97fa5840aa1b798fbd1b417b",
        ), //aim
        (
            0x00481180usize,
            0x00481204usize,
            "376644e81ba6e8c86b971e7af9fe01367baec96063433d143c4c4cd140fb17a5",
        ), //counters
        (
            0x00481210usize,
            0x00481256usize,
            "6c89f68b830592b21d228b73356a7268fede3b404c02a79fa9b89018661b5532",
        ), //angle
        (
            0x00402670usize,
            0x0040267Dusize,
            "9c375493fe6e8516cd7ebdc429fa91c81045e81d62b4225dc8b83f1e3b160f39",
        ), //copy_float
        (
            0x00402680usize,
            0x0040268Fusize,
            "cde7106428f710e61e93d73d535a8c9e5bb067aa11d6032a506ef2e940ce9ae6",
        ), //ratio_float
        (
            0x00402690usize,
            0x004026A9usize,
            "66f50239e7eca922033451ce5bfdf4e3e44cdc54514dbac3e5ad6a7f5d93a90d",
        ), //vector_constructor
        (
            0x0048EF30usize,
            0x0048EF80usize,
            "3f5c4bec14834012bdfa6394094ab681dd372b5fdc8939c0f40126e1530b27db",
        ), //q31_ratio
        (
            0x0048F520usize,
            0x0048F540usize,
            "462b87c2774f0f8a15278f58c314abb5cfaf6acde555ec3f63abac19bc2048fa",
        ), //q31_inverse
        (
            0x00490A80usize,
            0x00490B11usize,
            "d794384cfa0e3e311dca206487ee2b5baa4c1eb67ae7fe6bce9333ffb918d909",
        ), //inverse_words
        (
            0x004824A0usize,
            0x004825C1usize,
            "f6cfbe957916c73d4d67d7ce939910800f2a24b57b8d25d011ecaa3a66d3e658",
        ), //effect_boundary
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x400000..end - 0x400000])
            ),
            expected
        );
    }
    for (address, expected) in [
        (0x527f04, 0x481710),
        (0x527f28, 0x4026b0),
        (0x5270a0, 0x4026b0),
        (0x527fb8, 0x425af0),
        (0x527ff8, 0x481c10),
        (0x53bdb4, 0x4000),
    ] {
        assert_eq!(mapped.word(address).unwrap(), expected);
    }
    assert_eq!(
        &exe[0x4026b0 - 0x400000..0x4026b4 - 0x400000],
        &[0x8d, 0x41, 0x44, 0xc3]
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(&exe[0x481110 - 0x400000..0x48114f - 0x400000])
        ),
        "3ca057c85f6183d59bdc819003825d1cca7199a08b23718612fb2e4f38983a8e"
    );
    let needle = 0x0053_bdb4_u32.to_le_bytes();
    let references = exe[0x1000..0x11e475]
        .windows(4)
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == needle).then_some(0x401000 + offset))
        .collect::<Vec<_>>();
    assert_eq!(references, vec![0x4811c6]);
    // Caller phase order: real WeaponTick before fresh148 Avoidance, then
    // distinct virtual118 and audio. No callback is replaced by a stub.
    assert_eq!(
        &exe[0x481a08 - 0x400000..0x481a0d - 0x400000],
        &[0xe8, 0x23, 0xb6, 0x00, 0x00]
    );
}
