//! Complete original bodies, final vtable rows and authored process BYTE.
//! These static guards make no claim that a loaded world reached this tail.

#[test]
fn complete_native_tail_dependencies_and_distinct_final_class_slots_are_guarded() {
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
            0x00424830usize,
            0x00424c32usize,
            "556ab56d68c4dddb67f8410cf3a89eb4ef30ce97f568355590b934f6685c50a1",
        ),
        (
            0x004679f0usize,
            0x00467d98usize,
            "c9b243d83e8d4c7a2097d398a9e58003c34626af7808c490e67ebd440f019b2d",
        ),
        (
            0x00481710usize,
            0x00481a48usize,
            "2f332cafaedb3db1828bf4ccc34063f419dcfafd3353eadec8ac852496cd9d22",
        ),
        (
            0x0043e6d0usize,
            0x0043ef5fusize,
            "e3e733f9008357f8cc653fecb662056da1bd68afea6e7036978f237e87620031",
        ),
        (
            0x00425e60usize,
            0x00425f47usize,
            "bf30316b8a96753a1746060565737be661587136bd06968fe94e80224cb9bec7",
        ),
        (
            0x0047a660usize,
            0x0047a675usize,
            "bb8af2f0dba9f6f5795022ec5f4692afd2a3c7986d8443a74725d8c63c4831ed",
        ),
        (
            0x00490a80usize,
            0x00490b11usize,
            "d794384cfa0e3e311dca206487ee2b5baa4c1eb67ae7fe6bce9333ffb918d909",
        ),
        (
            0x0048efc0usize,
            0x0048f05busize,
            "cdec5838e3c2402b0deab3be8c3cd6cccff09fa60049ed34ec75610fbb6858ff",
        ),
        (
            0x00467df0usize,
            0x00468039usize,
            "02306ba7c67e32c355db7ae3b2dfc8cc8bc291dd606e43b51ce63572ca77659c",
        ),
        (
            0x00482690usize,
            0x00482739usize,
            "6c73508648f687eb8daeb9fedcbf8417a57f2d35541083df8e1e409d56d748cc",
        ),
        (
            0x00429ee0usize,
            0x00429fe2usize,
            "be5bb7dc317f797174b4c7948b376901a184c4de4caed3b646dd40d6daf84ed9",
        ),
        (
            0x004026b0usize,
            0x004026b4usize,
            "b185eed3e098fd3dc404ff66db8be1121a2a2610cf0814ec915e002304c9bccf",
        ),
        (
            0x00481180usize,
            0x00481204usize,
            "376644e81ba6e8c86b971e7af9fe01367baec96063433d143c4c4cd140fb17a5",
        ),
    ] {
        // Strict canonical profile fixes .text/.rdata raw offsets to RVA.
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x400000..end - 0x400000])
            ),
            expected
        );
    }
    for (table, slots) in [
        (
            0x00520cc8usize,
            [
                (0x24usize, 0x00424830u32),
                (0x48usize, 0x004026b0u32),
                (0xd8usize, 0x00425af0u32),
                (0x118usize, 0x00425400u32),
            ],
        ),
        (
            0x00527058usize,
            [
                (0x24usize, 0x004679f0u32),
                (0x48usize, 0x004026b0u32),
                (0xd8usize, 0x00468d10u32),
                (0x118usize, 0x00467df0u32),
            ],
        ),
        (
            0x00527ee0usize,
            [
                (0x24usize, 0x00481710u32),
                (0x48usize, 0x004026b0u32),
                (0xd8usize, 0x00425af0u32),
                (0x118usize, 0x00481c10u32),
            ],
        ),
        (
            0x0052273cusize,
            [
                (0x24usize, 0x0043e6d0u32),
                (0x48usize, 0x004026b0u32),
                (0xd8usize, 0x0043f880u32),
                (0x118usize, 0x0043ef60u32),
            ],
        ),
    ] {
        for (slot, expected) in slots {
            let offset = table - 0x400000 + slot;
            assert_eq!(
                u32::from_le_bytes(exe[offset..offset + 4].try_into().unwrap()),
                expected
            );
        }
    }
    assert_eq!(mapped.mapped(0x0053_a128, 1).unwrap().bytes, &[1]);
}
