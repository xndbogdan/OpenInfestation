//! Authored process byte and complete Selector12 motion reader provenance.

use inf_formats::{ByteSpan, FrontendExecutable};
use sha2::{Digest, Sha256};

#[test]
fn original_surface_snap_startup_byte_and_complete_motion_reader_are_exact() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let current = executable.mapped(0x0053_a128, 1).unwrap();
    assert_eq!(current.virtual_address, 0x0053_a128);
    assert_eq!(current.file_span, ByteSpan::new(0x0013_a128, 0x0013_a129));
    assert_eq!(current.bytes, &[1]);
    assert_eq!(current.bytes, &raw[0x0013_a128..0x0013_a129]);
    // This is authored file-backed data, distinct from the loader-zero tail
    // and unrelated to any heap-allocation carry policy.
    assert!(executable.loader_zero_data(0x0053_a128, 1).is_err());

    // Strict whole-image/profile custody above authenticates .text raw=RVA.
    // Complete467DF0 excludes the NOP padding after its final RET00468036.
    let body = &raw[0x0006_7df0..0x0006_8039];
    assert_eq!(body.len(), 585);
    assert_eq!(
        format!("{:x}", Sha256::digest(body)),
        "02306ba7c67e32c355db7ae3b2dfc8cc8bc291dd606e43b51ce63572ca77659c"
    );
    for (address, expected) in [
        (0x0046_7dfd_u32, &[0xe8, 0x3e, 0xd7, 0xf9, 0xff][..]), // H12/0 first
        (0x0046_7e1d, &[0xe8, 0x0e, 0x8c, 0xfe, 0xff][..]),     // terrain450A30
        (0x0046_7e30, &[0x83, 0xf9, 0x03][..]),                 // signedC4 compare
        (0x0046_7e33, &[0x0f, 0x8c, 0xbd, 0x00, 0x00, 0x00][..]), // dead bypass
        (0x0046_7e39, &[0x8a, 0x0d, 0x28, 0xa1, 0x53, 0x00][..]), // BYTE read
        (0x0046_7e43, &[0x66, 0x29, 0x46, 0x34][..]),           // Y write before IDIV
        (0x0046_7e4d, &[0xf7, 0xff][..]),
        (0x0046_8036, &[0xc2, 0x08, 0x00][..]),
    ] {
        let offset = (address - 0x0040_0000) as usize;
        assert_eq!(&raw[offset..offset + expected.len()], expected);
    }

    // The original text has one direct literal reference to this address.
    // This bounded static census does not exclude computed/external writers.
    let literal = 0x0053_a128_u32.to_le_bytes();
    let references: Vec<_> = raw[0x1000..0x0011_f000]
        .windows(4)
        .enumerate()
        .filter_map(|(offset, bytes)| {
            (bytes == literal.as_slice()).then_some(0x0040_1000usize + offset)
        })
        .collect();
    assert_eq!(references, vec![0x0046_7e3b]);
}
