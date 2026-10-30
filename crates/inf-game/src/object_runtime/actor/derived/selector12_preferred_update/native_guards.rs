//! Complete native consumers and final tables; static bytes do not prove selection.
#[test]
fn complete_preferred_update_notification_and_grant_owners_remain_exact() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let mapped = inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in BODIES {
        //The strict canonical .text mapping is raw offset==RVA.
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[*start - 0x400000..*end - 0x400000])
            ),
            *expected
        );
    }
    for (address, expected) in [
        (0x527058 + 0x13cusize, 0x467660u32),
        (0x527038 + 12, 0x469310),
    ] {
        let at = address - 0x400000;
        assert_eq!(
            u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()),
            expected
        );
    }
    for (address, count) in [(0x53a098u32, 8usize), (0x53a0b8, 9)] {
        let pointer = mapped.mapped(address + (count as u32 - 1) * 4, 4).unwrap();
        assert_eq!(
            u32::from_le_bytes(pointer.bytes.try_into().unwrap()),
            0x54ecc8
        );
    }
    //54ECC8 is virtual-tail storage. Constructor publishes its real vtable;
    //do not pretend a raw file slice at that VA is the object DWORD.
    assert_eq!(
        &exe[0x66fd0..0x66fda],
        &[0xb9, 0xc8, 0xec, 0x54, 0, 0xe9, 0x26, 0, 0, 0]
    );
    assert_eq!(&exe[0x67002..0x67008], &[0xc7, 0, 0x38, 0x70, 0x52, 0]);
    //14C before198; flags before190/194 before setter before fresh notification.
    assert_eq!(
        &exe[0x676a5..0x676b1],
        &[0x8b, 0x86, 0x4c, 1, 0, 0, 0x8b, 0x8e, 0x98, 1, 0, 0]
    );
    assert_eq!(&exe[0x676e3..0x676ea], &[0x80, 0x8e, 0x2e, 2, 0, 0, 0x0c]);
    assert_eq!(
        &exe[0x67705..0x67712],
        &[0xe8, 0xb6, 0xd9, 0, 0, 0x53, 0x8b, 0xce, 0xe8, 0x0e, 0, 0, 0]
    );
}
const BODIES: &[(usize, usize, &str)] = &[
    (
        0x00469310,
        0x00469324,
        "3a5530ae01d1e8606aa0264b784e5233c392ac4a0ed19f7c4fed67002ac242b9",
    ), //preferred_update_wrapper
    (
        0x00467660,
        0x00467718,
        "1222c45bb1c4d8934519d7cfe800eccee35d365734d1e3c4782210b4c36b68e8",
    ), //preferred_update13c
    (
        0x00467720,
        0x00467756,
        "9f5ff43b1f94c7196ab8cbcbe11fa5265dcabdb206e333df2b8ed927e358a4b7",
    ), //single_grant_notification
    (
        0x00488AA0,
        0x00488AB8,
        "9b594118f5712139882504b45ab023d976170685230e7d3358ba064a2981f1a5",
    ), //first_player
    (
        0x004750C0,
        0x004752D2,
        "615f417bc2afb437601e544b1c485aa667480f1e9314abfade491ad56500e7ff",
    ), //grant_set
    (
        0x004752E0,
        0x0047531A,
        "3c3c7175221a4064d194baeb7a48a0f34fd6ddb37c41db332b2cdef9cdf321c6",
    ), //grant_query
    (
        0x00475320,
        0x00475348,
        "936513084d8287ee722d024b702bbd2b57a2e9814c637e3a918b080a3a9a6198",
    ), //grant_record_address
    (
        0x00436BC0,
        0x00436CF4,
        "fc73e2e49166c36b8167def90febf3789592c522daef1a09710cfac4b9efe3e3",
    ), //terrain_furniture_grant
    (
        0x00475020,
        0x0047503B,
        "cfae372ea5e3ef4b6036b35902061f9776b997549efe9759f2e9ccbe537db1b7",
    ), //grant_constructor
    (
        0x004750B0,
        0x004750BF,
        "9867261731dde0fd03b297d053b18c508d81c6d63f5422cd9a8ae150c5254701",
    ), //grant_zero_mask
    (
        0x00467760,
        0x00467832,
        "9b90a0d024c1a29728c8ff2982d7e36bcc732c3a00f27563d297d81a6fa3e996",
    ), //preferred_change144
    (
        0x004827C0,
        0x004827CA,
        "36839f28b80a982fe32446ba9f5207c3d55402127d122f0bc9558de8d6646dbf",
    ), //target_actor_query
    (
        0x00401300,
        0x00401303,
        "4bc724f3b1d0caf4fe369c18cba3102e6c4ea057f63fe1587e3973134a7f755e",
    ), //target_null_query
    (
        0x00467070,
        0x004671BD,
        "a1c8a13da94b34067915fe51de92f71967b79db495b6a93775f1d526cbb7b370",
    ), //selector12_constructor
    (
        0x00425E60,
        0x00425F47,
        "bf30316b8a96753a1746060565737be661587136bd06968fe94e80224cb9bec7",
    ), //derived_constructor
    (
        0x00428650,
        0x00428729,
        "42f4992c11ffd8b87ab75bee34edd89b920c3182a2c8ba1e936abe574351b1cf",
    ), //family428650_constructor
    (
        0x00429E20,
        0x00429E41,
        "7029b147d82d514c0f32d97171127f1c7aca1f78e9a12ee238034eb5ec77b72f",
    ), //family429e20_constructor
    (
        0x004679F0,
        0x00467D98,
        "c9b243d83e8d4c7a2097d398a9e58003c34626af7808c490e67ebd440f019b2d",
    ), //selector12_full_update24
    (
        0x004294A0,
        0x00429895,
        "189764878265d1529a6045efcfb635239d380c1edb3a695f4aa9d641090b7233",
    ), //behavior_dispatch
    (
        0x00468D10,
        0x00468DDB,
        "43032d16d4c2568b679880e2070e5e7a01fda314857d0b15a45cb276f9da0058",
    ), //selector12_dispatch_d8
    (
        0x004288E0,
        0x00428987,
        "150c44617034b93ae06c67d63c47d608452ff050466585329ea7293ec25acf76",
    ), //common_target_change_e8
    (
        0x00466FC0,
        0x00466FCA,
        "4154085efc09ee8d36ae72be05d3630d9e5b39d96e774aeb7667552fe06aa4f2",
    ), //descriptor_crt_initializer
    (
        0x00466FD0,
        0x00466FDA,
        "5300f2d52a4041a8ef3910d4859d25b29d369c50d218dc2da6feb1dc1efa8d69",
    ), //descriptor_receiver_initializer
    (
        0x00467000,
        0x00467009,
        "04ce13d52630c6f76dfb7de7886c2447f837a3eb0adf0ee5dac41f327afa5369",
    ), //descriptor_vtable_constructor
];
