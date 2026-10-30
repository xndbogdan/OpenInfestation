// Complete canonical consumers: player world/spawn/attach camera, camera
// reselect, selector93 and selector90 route/follow publication. These guards
// authenticate static ordering; portable heap failure policy is tested separately.
#[test]
fn original_target_consumers_preserve_allocation_read_and_publication_order() {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let _profile = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    // Canonical .text raw offset equals RVA, guarded by the whole PE/profile.
    // mapped() remains the data/rdata service.
    for (start, end, hash) in [
        (
            0x00478e00usize,
            0x00478f16usize,
            "de9ed1f79af449422759f0d82568a4a7db55b08b9f36d8adf85069d6df76dc6e",
        ),
        (
            0x0046ae90usize,
            0x0046af69usize,
            "118a14559b85e6a09a1ab92e1ef9159a9614c072a5849bcf9455a594814dc5ad",
        ),
        (
            0x0046a890usize,
            0x0046ae56usize,
            "4b6c4483a4b2c416ec0e4341e2a7f328c25cafe2e6f3a525b7739433d91f7618",
        ),
        (
            0x0040fb40usize,
            0x0040fd09usize,
            "760123c1540a4ed29e1dd6c94acdd525aa3624c00766b8a0e971d8a1b1d6b1af",
        ),
        (
            0x00440170usize,
            0x004404ccusize,
            "0cac1a7237f19651dbef82b214dfa38e4b6d6a84989b1e43f5b80cc9a694daf7",
        ),
        (
            0x00462cc0usize,
            0x00462d3cusize,
            "a47548ef24c310971309fc104a5620df0df4356f60ca7a1fc8629b3e4f633f52",
        ),
        (
            0x00462d40usize,
            0x00462d6fusize,
            "0c59b4cc4e4efb9804963af0584e2bfd4af019eaa4321c03c29f781c5ad2c2d4",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[start - 0x400000..end - 0x400000])
            ),
            hash
        );
    }
    let raw =
        |address: usize, length: usize| &bytes[address - 0x400000..address - 0x400000 + length];
    // 478EAB malloc(20) precedes the success-only fresh actor+84 read.
    assert_eq!(
        raw(0x00478ea9, 7),
        &[0x6a, 0x20, 0xe8, 0x58, 0x88, 0x09, 0x00]
    );
    // JE skips that read and 4825D0; NULL then flows to camera publication.
    assert_eq!(
        raw(0x00478eb7, 12),
        &[0x85, 0xc0, 0xc7, 0x44, 0x24, 0x18, 0x00, 0x00, 0x00, 0x00, 0x74, 0x12]
    );
    assert_eq!(
        raw(0x00478ec3, 16),
        &[
            0x8b, 0xbf, 0x84, 0x00, 0x00, 0x00, 0x6a, 0x00, 0x57, 0x8b, 0xc8, 0xe8, 0xfd, 0x96,
            0x00, 0x00
        ]
    );
    // Spawn 46AD88 likewise allocates before its second, fresh vehicle+84 read.
    assert_eq!(
        raw(0x0046ad86, 7),
        &[0x6a, 0x20, 0xe8, 0x7b, 0x69, 0x0a, 0x00]
    );
    assert_eq!(
        raw(0x0046ad94, 12),
        &[0x85, 0xc0, 0xc7, 0x44, 0x24, 0x40, 0x00, 0x00, 0x00, 0x00, 0x74, 0x12]
    );
    assert_eq!(
        raw(0x0046ada0, 16),
        &[
            0x8b, 0xb6, 0x84, 0x00, 0x00, 0x00, 0x6a, 0x00, 0x56, 0x8b, 0xc8, 0xe8, 0x20, 0x78,
            0x01, 0x00
        ]
    );
    // Attach already captured the incoming handle in ESI before allocation.
    assert_eq!(raw(0x0046aea6, 4), &[0x8b, 0x74, 0x24, 0x14]);
    assert_eq!(raw(0x0046af01, 5), &[0xe8, 0x02, 0x68, 0x0a, 0x00]);
    // The success constructor takes that captured ESI, not another actor+84 read.
    assert_eq!(
        raw(0x0046af17, 12),
        &[0x74, 0x0c, 0x6a, 0x00, 0x56, 0x8b, 0xc8, 0xe8, 0xad, 0x76, 0x01, 0x00]
    );
    // Route malloc precedes its lazy process54E894 read and final +150 store.
    assert_eq!(
        raw(0x00462cde, 12),
        &[0xe8, 0x25, 0xea, 0x0a, 0x00, 0x83, 0xc4, 0x04, 0x85, 0xc0, 0x74, 0x36]
    );
    assert_eq!(raw(0x00462cea, 6), &[0x8b, 0x0d, 0x94, 0xe8, 0x54, 0x00]);
    assert_eq!(raw(0x00462d15, 6), &[0x89, 0x87, 0x50, 0x01, 0x00, 0x00]);
    // Selector93 allocation NULL still publishes148=NULL and reaches angular tail.
    assert_eq!(
        raw(0x004401f6, 22),
        &[
            0x6a, 0x20, 0xe8, 0x0b, 0x15, 0x0d, 0x00, 0x83, 0xc4, 0x04, 0x89, 0x44, 0x24, 0x14,
            0x3b, 0xc3, 0x89, 0x5c, 0x24, 0x30, 0x74, 0x14
        ]
    );
    assert_eq!(
        raw(0x00440220, 15),
        &[
            0x33, 0xc0, 0xc7, 0x44, 0x24, 0x30, 0xff, 0xff, 0xff, 0xff, 0x89, 0x86, 0x48, 0x01,
            0x00
        ]
    );
    assert_eq!(
        raw(0x00440429, 11),
        &[0xe8, 0x12, 0xd0, 0x03, 0x00, 0x81, 0xfd, 0xb8, 0x0b, 0x00, 0x00]
    );
}
