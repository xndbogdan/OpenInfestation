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
            0x0048D6E0usize,
            0x0048D723usize,
            "249af73f6e6f51893e680de419599b6c608663e108580edb88c3fef0a1ee2214",
        ), //weapon_position
        (
            0x0048CC00usize,
            0x0048CC68usize,
            "f98539eec51897daf2fa7a7754425b32cb6ff9314339eeed6fd10c8471f9c209",
        ), //group_world
        (
            0x0047A6C0usize,
            0x0047A6D5usize,
            "44f50d9640ae14efd34d72337166aadddf440acc1ee588aa4642db72d7a95c94",
        ), //word_transform_wrapper
        (
            0x004909E0usize,
            0x00490A71usize,
            "2beac16736bd73ed6deb2937569825f302f942b67bb7149130fc031ef43a20f6",
        ), //word_transform
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x400000..end - 0x400000])
            ),
            expected
        );
    }
    assert_eq!(mapped.word(0x527f28).unwrap(), 0x4026b0);
}
