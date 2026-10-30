//! Whole native bodies authenticate absence/order independently of backing policy.
use super::*;

#[test]
fn complete_native_allocator_constructor_and_writer_guards_remain_independent_of_policy() {
    let raw = executable();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    FrontendExecutable::parse(&raw).unwrap();
    for (start, end, digest) in [
        (
            0x004250C0_usize,
            0x00425183_usize,
            "7208653423e31230b410bdcfcc2f6b487fe6adbcd5ae4415ef203b23e9c5dedc",
        ),
        (
            0x00425E60_usize,
            0x00425F47_usize,
            "bf30316b8a96753a1746060565737be661587136bd06968fe94e80224cb9bec7",
        ),
        (
            0x004288E0_usize,
            0x00428987_usize,
            "150c44617034b93ae06c67d63c47d608452ff050466585329ea7293ec25acf76",
        ),
        (
            0x004294A0_usize,
            0x00429895_usize,
            "189764878265d1529a6045efcfb635239d380c1edb3a695f4aa9d641090b7233",
        ),
        (
            0x00429B00_usize,
            0x00429C66_usize,
            "fbc58ad2a3c4bcebf2477bb2abfcf417e43444d1e49abbf5c1249f963d36cb0f",
        ),
        (
            0x00429C70_usize,
            0x00429CAC_usize,
            "ad1e851dcd3cef807bbf4d8cfe55ae5cb9f0fd121d632d921a388fea19328280",
        ),
        (
            0x00467660_usize,
            0x00467718_usize,
            "1222c45bb1c4d8934519d7cfe800eccee35d365734d1e3c4782210b4c36b68e8",
        ),
        (
            0x00468040_usize,
            0x00468156_usize,
            "0bca2d4a7af7cb58b87055f3ce96e7ca4cb8374b08c326e36cbb6de45dc68cc9",
        ),
        (
            0x00482FF0_usize,
            0x00482FFA_usize,
            "7065e6d2a444356fc66f4960c993d0b5f25d982fe5c3f2700c71cf6c769b7aeb",
        ),
        (
            0x00511708_usize,
            0x00511716_usize,
            "9092ad69660ec58443c4366c955f320e2687fede12f753b226fd7388a20d0a4e",
        ),
        (
            0x00512398_usize,
            0x005123E0_usize,
            "cc459c028c8469a5ecb05bc4d2c47a3f8782086f8d592d9447426152c8de4787",
        ),
        (
            0x0051247C_usize,
            0x005124A8_usize,
            "72c061d39eca92bf0bc007a8c83fb759c1bb051dbd89cc41987724871f7e8110",
        ),
        (
            0x005124A8_usize,
            0x00512500_usize,
            "28ef9085b60bd264b86c76280473ce172a5070861ac8af9b39664b56566815c9",
        ),
        (
            0x005158A6_usize,
            0x005158D1_usize,
            "8b5f9032a89eaa49ea6861ae502f33131bbc738c3c2e5f41110b3e6d6f40094d",
        ),
        (
            0x005158D1_usize,
            0x00515BFC_usize,
            "c52acdafd4e077567b8d94c4081931f0a89eca8c6bef1ef30cbb0148823a2ff2",
        ),
        (
            0x00515BFC_usize,
            0x00515F05_usize,
            "6bf0afc1537fdb356bfdf781d6ce934c9f04fd8eb96e2b77773376d0b8c6334e",
        ),
        (
            0x00515F05_usize,
            0x00515FB6_usize,
            "a7ad399798778560bc00cfc263c3ef0d6ee636cbf6add10ccc8b14a7936dd103",
        ),
        (
            0x00515FB6_usize,
            0x005160B1_usize,
            "73650d6f6747e755f129c4f6fbe10ce150a4b8721ef2b42c3998cbde99323707",
        ),
        (
            0x005164F8_usize,
            0x00516513_usize,
            "d09972a4a692d2cbc1a92087d68c3ad9828a03b1adeace93f935737cc0033260",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&raw[start - 0x0040_0000..end - 0x0040_0000])
            ),
            digest
        );
    }
    //482FF0 initializes08 only: no00/04 header or row initialization.
    assert_eq!(
        &raw[0x0048_2ff0 - 0x0040_0000..0x0048_2ffa - 0x0040_0000],
        &[0x8b, 0xc1, 0xc7, 0x40, 0x08, 0, 0, 0, 0, 0xc3]
    );
    //These guards establish original source, never fresh-zero/reuse acceptance.
}
