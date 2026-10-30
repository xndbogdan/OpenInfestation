#[test]
fn complete_original_grant_grid_dependencies_and_literal_periodic_record_hold() {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    // Canonical PE profile guard plus authenticated .text raw=RVA. mapped()
    // remains the required data/rdata-only API, not a widened text adapter.
    for (start, end, hash) in [
        (
            0x00468e30usize,
            0x0046902ausize,
            "3c3a75dad64998e675db9edad4ba71503935339e5fe495ce0b1ca9c578513094",
        ),
        (
            0x00436bc0usize,
            0x00436cf4usize,
            "fc73e2e49166c36b8167def90febf3789592c522daef1a09710cfac4b9efe3e3",
        ),
        (
            0x004752e0usize,
            0x0047531ausize,
            "3c3c7175221a4064d194baeb7a48a0f34fd6ddb37c41db332b2cdef9cdf321c6",
        ),
        (
            0x00427620usize,
            0x00427638usize,
            "cff2c20271ae285b589656b865260a5e187e5de61401fb935e717d841b6c4283",
        ),
        (
            0x00427390usize,
            0x0042748busize,
            "e2d14d5e36b6f3d018f83349da10a511400926c1607eea728e3419c4ee5e1b29",
        ),
        (
            0x004502c0usize,
            0x00450342usize,
            "026dbc8f51201b8cca92d03d1027eb0255797fe922078448c12fba861021d002",
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
    let record = executable.mapped(0x53a110, 12).unwrap().bytes;
    assert_eq!(record, [0x30, 0x8e, 0x46, 0, 0x84, 2, 0, 0, 0, 0, 0, 0]);
}
