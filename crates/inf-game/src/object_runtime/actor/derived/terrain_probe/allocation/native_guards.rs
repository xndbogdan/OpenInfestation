//! Complete source bodies for captured native cache/locals/readback order.
#[test]
fn complete_original_cache_bodies_and_actor_handoff_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    // Authenticate the canonical PE profile before RVA==raw .text access.
    inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in [
        (
            0x00454B30usize,
            0x00454B90usize,
            "1aa819ec3c1b3a06dbe916e3a70111baab07bfd95be65c73c6992a31e5ad4af4",
        ),
        (
            0x00454B90usize,
            0x00454B9Ausize,
            "5945a51581124f4f30bf43c491e2869e627520292fb73096af83fc23a7911707",
        ),
        (
            0x00454BA0usize,
            0x00454BE0usize,
            "e7240fd4f378f787c659654e94632f8cad95615a9d0f6222dfb40fde9d26194f",
        ),
        (
            0x00454BE0usize,
            0x00454C1Fusize,
            "67eb9075e2a15e9609689b2b0c8ba98df2e146b32049032403cbc5e83351188d",
        ),
        (
            0x00454C20usize,
            0x00454C5Busize,
            "9b0c1014f16b0acce67d1092d2f087977fa9040abe5fdcbdf198e4538b0437b2",
        ),
        (
            0x00454C60usize,
            0x00454CEEusize,
            "e5fb661c7c6f5d457057a4e5e66ac2a15ae13346ba64dbe65b5ef388056f29d2",
        ),
        (
            0x00454CF0usize,
            0x00454D6Busize,
            "e6b418c9b7622dd3b47a1d2111e67066f78fcd651d9260ed8d94dfcb4f82c528",
        ),
        (
            0x00454D70usize,
            0x00454E2Cusize,
            "b225417b313918c0d3b9864da8c3ee5f630e11eead51c45e28f8c85b91f3afc5",
        ),
        (
            0x00454E30usize,
            0x00454ECCusize,
            "f967d133722a9f15b53e5a624f20fb028ef46d6163b3555ee6d80d0b41204d7c",
        ),
        (
            0x00454ED0usize,
            0x00455075usize,
            "e69e77d812059ed84ae51bf4da77e181af97a9d93b3b542192323f48599177a0",
        ),
        (
            0x00429EE0usize,
            0x00429FE2usize,
            "be5bb7dc317f797174b4c7948b376901a184c4de4caed3b646dd40d6daf84ed9",
        ),
        (
            0x00429FF0usize,
            0x0042A2B9usize,
            "0ec037cd71f33fa4f9db24b18f26624456dd8b5932eef11c74bfa39a778c87d1",
        ),
        (
            0x00429E70usize,
            0x00429ED6usize,
            "7cfe12bd638b4bd99be2bb0333232e8cef828b787c51da196461c93d6933f031",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x400000..end - 0x400000])
            ),
            expected
        );
    }
    // Normal miss uses captured ESI header/EDI+EBX relative cells; the setter
    // then reloads current stride and buffer. Host failure has no suffix.
    assert_eq!(&exe[0x55046..0x5504b], &[0xe8, 0x95, 0x4e, 0xfd, 0xff]);
    assert_eq!(
        &exe[0x5504b..0x55058],
        &[0x25, 0xff, 0, 0, 0, 0x8b, 0xce, 0x40, 0x8b, 0xe8, 0x55, 0x53, 0x57]
    );
    assert_eq!(&exe[0x54bef..0x54bf2], &[0x8a, 0x51, 0x0a]);
    assert_eq!(&exe[0x54bfc..0x54bfe], &[0x8b, 0x01]);
}
