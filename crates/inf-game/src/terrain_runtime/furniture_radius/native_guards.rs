//! Full native load/radius consumers and the strict process loader-zero proof.
use sha2::{Digest, Sha256};

fn original() -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let exe = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(&exe).unwrap();
    exe
}

#[test]
fn complete_native_radius_load_and_required_model_radius_consumers_are_exact() {
    let exe = original();
    let mapped = inf_formats::FrontendExecutable::parse(&exe).unwrap();
    for (start, end, expected) in BODIES {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[*start - 0x400000..*end - 0x400000])
            ),
            *expected
        );
    }
    for (address, bytes, expected) in DATA {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(mapped.mapped(*address, *bytes).unwrap().bytes)
            ),
            *expected
        );
    }
    //4508F5 resets the WORD before any lookup or table acquisition.
    assert_eq!(
        &exe[0x508f5..0x508fc],
        &[0x66, 0x89, 0x2d, 0xc0, 0x83, 0x54, 0]
    );
    //The lookup-zero branch makes NULL before the unconditional first WORD read.
    assert_eq!(&exe[0x5094d..0x50952], &[0x33, 0xff, 0x66, 0x8b, 0x07]);
    //Current WORD is signed; model WORD0E is zero-extended before comparison.
    assert_eq!(
        &exe[0x509b2..0x509c4],
        &[
            0x0f, 0xbf, 0x0d, 0xc0, 0x83, 0x54, 0, 0x8b, 0xc6, 0x25, 0xff, 0xff, 0, 0, 0x3b, 0xc1,
            0x7e, 0x07
        ]
    );
    //Normal-only signed cap after all512 buckets.
    assert_eq!(
        &exe[0x50a0d..0x50a1f],
        &[
            0x66, 0x39, 0x05, 0xc0, 0x83, 0x54, 0, 0x5e, 0x5d, 0x5b, 0x7e, 0x06, 0x66, 0xa3, 0xc0,
            0x83, 0x54, 0
        ]
    );
    //44FDD0 calls4508E0 before normalizing the actual captured H12 light.
    assert_eq!(&exe[0x4fe12..0x4fe17], &[0xe8, 0xc9, 0x0a, 0, 0]);
    //48DC30 reaches44FDD0 before later map construction and placements.
    assert_eq!(&exe[0x8de0c..0x8de11], &[0xe8, 0xbf, 0x1f, 0xfc, 0xff]);
    //Future47C1D0 requires actor H8 radius before either zero-radius exit.
    //Full body is guarded; the publication does not supply collision success.
}

fn data_header(exe: &[u8]) -> usize {
    let pe = u32::from_le_bytes(exe[0x3c..0x40].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(exe[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let optional = u16::from_le_bytes(exe[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let sections = pe + 24 + optional;
    (0..count)
        .map(|n| sections + n * 40)
        .find(|at| &exe[*at..*at + 8] == b".data\0\0\0")
        .unwrap()
}

#[test]
fn radius_startup_requires_the_complete_loader_zero_word_and_is_not_file_data() {
    let exe = original();
    let mapped = inf_formats::FrontendExecutable::parse(&exe).unwrap();
    let header = data_header(&exe);
    let word = |offset| {
        u32::from_le_bytes(
            exe[header + offset..header + offset + 4]
                .try_into()
                .unwrap(),
        )
    };
    assert_eq!(word(12), 0x0012e000);
    assert_eq!(word(20), 0x0012e000);
    assert_eq!(word(16), 0x00019000);
    assert_eq!(word(8), 0x00132c38);
    assert_eq!(0x400000 + word(12) + word(16), 0x00547000);
    assert_eq!(0x400000 + word(12) + word(8), 0x00660c38);
    let proof = mapped.loader_zero_data(0x005483c0, 2).unwrap();
    assert_eq!((proof.start(), proof.end()), (0x005483c0, 0x005483c2));
    assert!(mapped.mapped(0x005483c0, 2).is_err());
    let mut radius = super::ProcessFurnitureRadius::native_startup(mapped).unwrap();
    assert_eq!(radius.word(), 0);
    *radius.word_mut() = 0xbeef;
    assert_eq!(
        radius.word(),
        0xbeef,
        "startup proof does not imply a later reset"
    );
    //Only an in-memory negative profile fixture is changed. The original file
    //and all original full-body guards above remain untouched.
    let mut short_tail = exe.clone();
    let virtual_size = 0x005483c1_u32 - 0x0052e000;
    short_tail[header + 8..header + 12].copy_from_slice(&virtual_size.to_le_bytes());
    let short_profile = inf_formats::FrontendExecutable::parse(&short_tail).unwrap();
    assert!(matches!(
        super::ProcessFurnitureRadius::native_startup(short_profile),
        Err(inf_formats::FrontendExecutableError::LoaderZeroRange {
            address: 0x005483c0,
            length: 2,
        })
    ));
    assert_eq!(radius.word(), 0xbeef);
}

const BODIES: &[(usize, usize, &str)] = &[
    (
        0x0044FDD0,
        0x0044FF50,
        "7efe8b4b20d185216f56dc6f67753dd6c560ff7af13a772adaebab0c295e5db8",
    ), //world_terrain_initializer_44FDD0
    (
        0x004508E0,
        0x00450A23,
        "17b623411737901d3afd595a5bb1c1e7cfc7d8963f3b40317c53982eecc1c04f",
    ), //furniture_radius_producer_4508E0
    (
        0x0048DC30,
        0x0048E11E,
        "926440546d9e980807aab5a2b50298a2e4c9506368bae4855a0eeabc07145cbf",
    ), //world_loader_48DC30
    (
        0x0047A050,
        0x0047A105,
        "e3e7e0f401cc6c9643c2ddb8eb2a4cfb5885c99175f9be199e4a4248bc8a70a3",
    ), //actor_visual_initializer_47A050
    (
        0x0047A110,
        0x0047A195,
        "4215c27d3a029d68d07381c08899dedd6e101fedcb120040c8c8c876605bf3f3",
    ), //actor_visual_refresh_47A110
    (
        0x0047C1D0,
        0x0047C6CC,
        "70c744e0ca1fc009d760d2516289a54013cf3a078dc4a023f3825de0451071ad",
    ), //furniture_collision_47C1D0
    (
        0x004EFBF0,
        0x004EFBF5,
        "5fed5afb29946811bf02359627a94bc01d08d31b779528feaadde3866af9c855",
    ), //final_resource_null_virtual_4EFBF0
    (
        0x0048C850,
        0x0048C851,
        "ae3f4619b0413d70d3004b9131c3752153074e45725be13b9a148978895e359e",
    ), //warning_literal_return_48C850
    (
        0x00474840,
        0x00474898,
        "3c3fca2cca2fcdc3411d6bb439a590e2bd4bf379ea8fe690b1e5b914b28d8986",
    ), //handler8_constructor_474840
    (
        0x00474B30,
        0x00474B88,
        "99b8c014c67d62a7f8d6cb0c4de21e4e05bf94ca38f3b2874895e70a86c5398f",
    ), //handler11_constructor_474B30
    (
        0x00474EF0,
        0x00474F48,
        "f3b519b44eaf234442c274fe56119c1705064051e57fa1a0562323aa9ab253c2",
    ), //handler15_constructor_474EF0
    (
        0x004747C0,
        0x004747CA,
        "7faa21270d321c9e8908415235e265be9926e1c43db64e0cbbdef2dea0a4d2e5",
    ), //handler8_global_binding_4747C0
    (
        0x00474AB0,
        0x00474ABA,
        "e8aa140d3e457a602f08e6f8f62691f5958147b078f99d28740c98ccdf514ed8",
    ), //handler11_global_binding_474AB0
    (
        0x00474E70,
        0x00474E7A,
        "c90d4c605eb840ff7826bf776b08edc0ae51a76a68a2d4c0bd18593e4d4ea5f8",
    ), //handler15_global_binding_474E70
];
const DATA: &[(u32, usize, &str)] = &[
    (
        0x0053AA14,
        64,
        "022dfffa2dec986dbbb4f7f75c1c7e99d84d7acc0297ec7d1f045fcdd2887c52",
    ), //canonical_handler_array_53AA14
    (
        0x005276D8,
        16,
        "da0976b2dc288bbe8e5f2db6d00495a2a758536d58414b0d637841b87bc40a1a",
    ), //h8_final_virtual_prefix_5276D8
    (
        0x00527750,
        16,
        "a772a56bebb6d19d6a4ff17fdd5eb44c291d8d99bd3620278733f6ff2e0514c0",
    ), //h11_final_virtual_prefix_527750
    (
        0x00527778,
        16,
        "fefe551212ee40246f1741076a14ed56cb19ea4969e01b119ca6324a9a019194",
    ), //h12_final_virtual_prefix_527778
    (
        0x0052778C,
        16,
        "1186b009201db1c32d89571d063a7e4c125f658a7525cc21778fec12c0866bf0",
    ), //h12_secondary_virtual_prefix_52778C
    (
        0x005277F0,
        16,
        "e555766a374d4ba074c628dec1c6d820a62761521784ec24c597a04833242557",
    ), //h15_final_virtual_prefix_5277F0
];
