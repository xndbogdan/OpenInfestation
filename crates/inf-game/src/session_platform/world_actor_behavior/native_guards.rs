//! Whole original profile, complete adapter wrappers and final component slots.
use sha2::{Digest, Sha256};

#[test]
fn original_complete_host_wrappers_and_final_component_tables_are_exact() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(&raw).unwrap();
    // The strict original PE profile authenticates text/rdata raw=VA-400000.
    for (start, end, expected) in [
        (
            0x00401300_u32,
            0x00401303_u32,
            "4bc724f3b1d0caf4fe369c18cba3102e6c4ea057f63fe1587e3973134a7f755e",
        ),
        (
            0x00404610_u32,
            0x00404613_u32,
            "58367ffa2a0179375018fa0f5c26da24391e42ebe0ed8fdda35a21fc7bdc396f",
        ),
        (
            0x004026D0_u32,
            0x004026D3_u32,
            "1d10894625976bca9d8906f3ecf1e766b94e9219ec3bf1c30e02feea5e18e45a",
        ),
        (
            0x00425CF0_u32,
            0x00425CF8_u32,
            "d6cf25cb6af13f738222b990ac2b0d41ea0fb44ffdb538efa5ee02b7f200eecf",
        ),
        (
            0x00425D00_u32,
            0x00425D08_u32,
            "e5d927035c3b1ee4823323cdf2911e3b39cff55464aa1850f52cb3b1a3b8fec9",
        ),
        (
            0x00425E00_u32,
            0x00425E11_u32,
            "195e55d514ddd8406e9f7f93ffea6fd388db77f9bcecdea15709c4bd280d8552",
        ),
        (
            0x00425E20_u32,
            0x00425E31_u32,
            "e2e080ec7b3ad4071ab8d54fae967618cefd63547632572882682d5605033f7f",
        ),
        (
            0x0043FA80_u32,
            0x0043FA94_u32,
            "e312fbd627b6f625eeb5d863cdb004fa346ae1909f2542d4d4385d53ff49c625",
        ),
        (
            0x0043FAA0_u32,
            0x0043FAB4_u32,
            "95d5e70eeefa3bae58dbc76712e782b79331b000112954ba8beb0314cbf35114",
        ),
        (
            0x0043FB00_u32,
            0x0043FB14_u32,
            "742a4e714fd1523d9322ddd6151e4ac471d8ff3f4a532cf84bc546ef346a4df5",
        ),
        (
            0x0043FBA0_u32,
            0x0043FBB1_u32,
            "22b91efa41c9de67dec00c2e7522314d02b46e35077ec1fffa7ea43b68c64902",
        ),
        (
            0x0043FBC0_u32,
            0x0043FBD1_u32,
            "1f279a7ac903a71e62db27aada807804689dbb65268d1606f4d072655744249e",
        ),
        (
            0x0043FC00_u32,
            0x0043FC11_u32,
            "a81dd43d5fc84e43bd9071b1cf925904319eb7c89a91ed142e98d220de4ee412",
        ),
        (
            0x0043FC20_u32,
            0x0043FC31_u32,
            "27e2a201eb3ea21211b5050777264bca9d82068a894a96ec374dbb8f724961f6",
        ),
        (
            0x0043FE40_u32,
            0x0043FE54_u32,
            "014a03d45a429130536719278e403d557bcdd7f758ae3777bb36ae5686f5cbb4",
        ),
        (
            0x00435130_u32,
            0x00435144_u32,
            "34c677fab532f4f383f5b44155aa60946bfab84b6de849c4df14adf25e149494",
        ),
        (
            0x00435150_u32,
            0x00435164_u32,
            "a04bec305569a8cde2ebb576859b61ba0345a97228a32d5d7e5841522450a5df",
        ),
        (
            0x00435170_u32,
            0x00435184_u32,
            "1c05234ce03c6876ef90a0418cd8975031e7f9c8afd566c65139cbf842b5cc2b",
        ),
        (
            0x00435190_u32,
            0x004351A1_u32,
            "a94a2e79fdc4a494c8bcd66816a23a33dd5c4f1cc8087a0054460fede4bc7b15",
        ),
        (
            0x00435220_u32,
            0x00435234_u32,
            "c61b1e7f5ad2298c08f51845d5dde1b43887b3d31f60f66cbdd53d381f762e2a",
        ),
        (
            0x00435240_u32,
            0x00435254_u32,
            "53860d6c90b2fb1e3b3992fe8b667a5ef2d47824e60d1db99334ffe0ad1152ff",
        ),
        (
            0x00425D70_u32,
            0x00425D84_u32,
            "29b2f55e907273c0b505d3ed29600e10903c923a5f20e84c900cdd9d28b94049",
        ),
        (
            0x00425D90_u32,
            0x00425DA4_u32,
            "6caf60efeb95d63591f06ff0612cc74f0aa4ffe886ef7a9c6a866e0f0c7e6a1b",
        ),
        (
            0x00469170_u32,
            0x00469184_u32,
            "747f14e9eec97c3cae6c950efd123953e1c4a20257b800a1d09b84abd9571e01",
        ),
        (
            0x004691D0_u32,
            0x004691E4_u32,
            "03858fe98f92cb291f85f7a1da87f7ea5eb097b17e4db0900ac8a334da2074d1",
        ),
        (
            0x004691F0_u32,
            0x00469204_u32,
            "ca137d9423f4a5a254a10d9be88375e4ce064ece0c6493388258b077fbf9f9a9",
        ),
        (
            0x00469210_u32,
            0x00469224_u32,
            "6de908b7e06a5c50bea54563521e076e8e48e70f9fbe60b5fb2e4e6ca7e7d10e",
        ),
        (
            0x004692F0_u32,
            0x00469304_u32,
            "40d9f9975ba1c5b2555bf3d235dfc24c27c5c8a6752ee65468af8ec8b4f86493",
        ),
        (
            0x00469310_u32,
            0x00469324_u32,
            "3a5530ae01d1e8606aa0264b784e5233c392ac4a0ed19f7c4fed67002ac242b9",
        ),
        (
            0x00487EC0_u32,
            0x00487F0F_u32,
            "b93cc46a1713ea0f69e85ba1919adf882560c5af83801dc631af343b50b6d7f8",
        ),
        (
            0x00487F10_u32,
            0x00487F5C_u32,
            "6e33ac5afdcb69e927ed8ba251d684bce48bb153ff6d55ae589a912c83e9d560",
        ),
    ] {
        let bytes = &raw[(start - 0x0040_0000) as usize..(end - 0x0040_0000) as usize];
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            expected,
            "complete {start:08X}"
        );
    }
    for (selector, table, actor) in [
        (1_u8, 0x00527B24_u32, false),
        (2_u8, 0x00520860_u32, true),
        (3_u8, 0x00527C00_u32, true),
        (4_u8, 0x00527C00_u32, true),
        (5_u8, 0x00527C00_u32, true),
        (6_u8, 0x00527C00_u32, true),
        (7_u8, 0x00520E58_u32, true),
        (8_u8, 0x00520B84_u32, true),
        (9_u8, 0x00520B84_u32, true),
        (10_u8, 0x00520CC8_u32, true),
        (11_u8, 0x0051F080_u32, true),
        (12_u8, 0x00527058_u32, true),
        (13_u8, 0x005269C4_u32, true),
        (14_u8, 0x00527208_u32, false),
        (15_u8, 0x00521940_u32, true),
        (16_u8, 0x005204D0_u32, true),
        (17_u8, 0x005272A0_u32, true),
        (18_u8, 0x005279E0_u32, true),
        (19_u8, 0x00526978_u32, false),
        (20_u8, 0x00525030_u32, true),
        (21_u8, 0x0051F7A8_u32, true),
        (22_u8, 0x00521830_u32, false),
        (23_u8, 0x005203C0_u32, true),
        (24_u8, 0x0052577C_u32, true),
        (25_u8, 0x00525B88_u32, true),
        (26_u8, 0x005268AC_u32, true),
        (27_u8, 0x00525848_u32, true),
        (28_u8, 0x005259E8_u32, true),
        (29_u8, 0x00525AB4_u32, true),
        (30_u8, 0x005289C0_u32, true),
        (31_u8, 0x00528524_u32, true),
        (32_u8, 0x005285F0_u32, true),
        (33_u8, 0x00520E58_u32, true),
        (34_u8, 0x00521EAC_u32, true),
        (35_u8, 0x0052273C_u32, true),
        (36_u8, 0x00527EE0_u32, true),
        (37_u8, 0x0051FF54_u32, true),
        (38_u8, 0x00525914_u32, true),
        (39_u8, 0x00522398_u32, true),
        (40_u8, 0x005260A0_u32, true),
        (41_u8, 0x005267E0_u32, true),
        (42_u8, 0x0051F018_u32, false),
        (43_u8, 0x00525730_u32, false),
        (44_u8, 0x00525648_u32, true),
        (45_u8, 0x0052616C_u32, false),
        (46_u8, 0x005278A0_u32, false),
        (47_u8, 0x00527CD4_u32, true),
        (48_u8, 0x005225D0_u32, false),
        (49_u8, 0x00527908_u32, true),
        (50_u8, 0x00522888_u32, false),
        (51_u8, 0x0051F658_u32, false),
        (52_u8, 0x00526714_u32, true),
        (53_u8, 0x00526648_u32, true),
        (54_u8, 0x005265FC_u32, false),
        (55_u8, 0x005274C4_u32, true),
        (56_u8, 0x00526530_u32, true),
        (57_u8, 0x00526464_u32, true),
        (58_u8, 0x00527838_u32, false),
        (59_u8, 0x0051F900_u32, true),
        (60_u8, 0x00527DE8_u32, true),
        (61_u8, 0x00520700_u32, true),
        (62_u8, 0x00526DC8_u32, true),
        (63_u8, 0x005229D8_u32, false),
        (64_u8, 0x00522A28_u32, false),
        (65_u8, 0x0052187C_u32, false),
        (66_u8, 0x0051F6C0_u32, true),
        (67_u8, 0x005286D8_u32, false),
        (68_u8, 0x00526398_u32, true),
        (69_u8, 0x005205D8_u32, true),
        (70_u8, 0x00527390_u32, false),
        (71_u8, 0x005273DC_u32, false),
        (72_u8, 0x00527428_u32, false),
        (73_u8, 0x005262CC_u32, true),
        (74_u8, 0x00521764_u32, true),
        (75_u8, 0x00522008_u32, false),
        (76_u8, 0x00521A40_u32, false),
        (77_u8, 0x0051FBB0_u32, true),
        (78_u8, 0x0051FA9C_u32, true),
        (79_u8, 0x0051FC8C_u32, true),
        (80_u8, 0x0051FD68_u32, true),
        (81_u8, 0x0051FE44_u32, false),
        (82_u8, 0x0051FE88_u32, true),
        (83_u8, 0x00522570_u32, false),
        (84_u8, 0x00522488_u32, true),
        (85_u8, 0x0051F9CC_u32, true),
        (86_u8, 0x005218CC_u32, false),
        (87_u8, 0x00528AB0_u32, false),
        (88_u8, 0x00524AC0_u32, true),
        (89_u8, 0x00527254_u32, false),
        (90_u8, 0x00526B50_u32, true),
        (91_u8, 0x0051F1E0_u32, true),
        (92_u8, 0x0051F2B0_u32, false),
        (93_u8, 0x005228F0_u32, true),
        (94_u8, 0x00524E20_u32, false),
    ] {
        let at = (table - 0x0040_0000 + 0x18) as usize;
        let actual = u32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
        assert_eq!(
            actual,
            if actor { 0x0040_4610 } else { 0x0040_1300 },
            "class {selector} +18"
        );
    }
    // Pin every canonical actor slot, including source-owned interfaces not
    // currently admitted by a typed construction class in this host.
    for (selector, table, expected68, expected6c) in [
        (2_u8, 0x00520860_u32, 0x00401300_u32, 0x00401300_u32),
        (3_u8, 0x00527C00_u32, 0x00401300_u32, 0x00401300_u32),
        (4_u8, 0x00527C00_u32, 0x00401300_u32, 0x00401300_u32),
        (5_u8, 0x00527C00_u32, 0x00401300_u32, 0x00401300_u32),
        (6_u8, 0x00527C00_u32, 0x00401300_u32, 0x00401300_u32),
        (7_u8, 0x00520E58_u32, 0x00401300_u32, 0x00401300_u32),
        (8_u8, 0x00520B84_u32, 0x00401300_u32, 0x00401300_u32),
        (9_u8, 0x00520B84_u32, 0x00401300_u32, 0x00401300_u32),
        (10_u8, 0x00520CC8_u32, 0x00401300_u32, 0x00401300_u32),
        (11_u8, 0x0051F080_u32, 0x00401300_u32, 0x00401300_u32),
        (12_u8, 0x00527058_u32, 0x00401300_u32, 0x00401300_u32),
        (13_u8, 0x005269C4_u32, 0x00401300_u32, 0x00401300_u32),
        (15_u8, 0x00521940_u32, 0x00401300_u32, 0x00401300_u32),
        (16_u8, 0x005204D0_u32, 0x00401300_u32, 0x00401300_u32),
        (17_u8, 0x005272A0_u32, 0x00401300_u32, 0x00401300_u32),
        (18_u8, 0x005279E0_u32, 0x00401300_u32, 0x00401300_u32),
        (20_u8, 0x00525030_u32, 0x00401300_u32, 0x00401300_u32),
        (21_u8, 0x0051F7A8_u32, 0x00401300_u32, 0x00401300_u32),
        (23_u8, 0x005203C0_u32, 0x00401300_u32, 0x00401300_u32),
        (24_u8, 0x0052577C_u32, 0x00404610_u32, 0x00401300_u32),
        (25_u8, 0x00525B88_u32, 0x00404610_u32, 0x00401300_u32),
        (26_u8, 0x005268AC_u32, 0x00404610_u32, 0x00401300_u32),
        (27_u8, 0x00525848_u32, 0x00404610_u32, 0x00401300_u32),
        (28_u8, 0x005259E8_u32, 0x00404610_u32, 0x00401300_u32),
        (29_u8, 0x00525AB4_u32, 0x00404610_u32, 0x00401300_u32),
        (30_u8, 0x005289C0_u32, 0x00401300_u32, 0x00401300_u32),
        (31_u8, 0x00528524_u32, 0x00401300_u32, 0x00404610_u32),
        (32_u8, 0x005285F0_u32, 0x00401300_u32, 0x00404610_u32),
        (33_u8, 0x00520E58_u32, 0x00401300_u32, 0x00401300_u32),
        (34_u8, 0x00521EAC_u32, 0x00401300_u32, 0x00401300_u32),
        (35_u8, 0x0052273C_u32, 0x00401300_u32, 0x00401300_u32),
        (36_u8, 0x00527EE0_u32, 0x00401300_u32, 0x00401300_u32),
        (37_u8, 0x0051FF54_u32, 0x00401300_u32, 0x00401300_u32),
        (38_u8, 0x00525914_u32, 0x00404610_u32, 0x00401300_u32),
        (39_u8, 0x00522398_u32, 0x00401300_u32, 0x00401300_u32),
        (40_u8, 0x005260A0_u32, 0x00401300_u32, 0x00401300_u32),
        (41_u8, 0x005267E0_u32, 0x00401300_u32, 0x00401300_u32),
        (44_u8, 0x00525648_u32, 0x00401300_u32, 0x00401300_u32),
        (47_u8, 0x00527CD4_u32, 0x00401300_u32, 0x00401300_u32),
        (49_u8, 0x00527908_u32, 0x00401300_u32, 0x00401300_u32),
        (52_u8, 0x00526714_u32, 0x00401300_u32, 0x00401300_u32),
        (53_u8, 0x00526648_u32, 0x00401300_u32, 0x00401300_u32),
        (55_u8, 0x005274C4_u32, 0x00401300_u32, 0x00401300_u32),
        (56_u8, 0x00526530_u32, 0x00401300_u32, 0x00401300_u32),
        (57_u8, 0x00526464_u32, 0x00401300_u32, 0x00401300_u32),
        (59_u8, 0x0051F900_u32, 0x00401300_u32, 0x00401300_u32),
        (60_u8, 0x00527DE8_u32, 0x00401300_u32, 0x00401300_u32),
        (61_u8, 0x00520700_u32, 0x00401300_u32, 0x00401300_u32),
        (62_u8, 0x00526DC8_u32, 0x00401300_u32, 0x00401300_u32),
        (66_u8, 0x0051F6C0_u32, 0x00401300_u32, 0x00401300_u32),
        (68_u8, 0x00526398_u32, 0x00404610_u32, 0x00401300_u32),
        (69_u8, 0x005205D8_u32, 0x00401300_u32, 0x00401300_u32),
        (73_u8, 0x005262CC_u32, 0x00401300_u32, 0x00401300_u32),
        (74_u8, 0x00521764_u32, 0x00401300_u32, 0x00401300_u32),
        (77_u8, 0x0051FBB0_u32, 0x00401300_u32, 0x00401300_u32),
        (78_u8, 0x0051FA9C_u32, 0x00401300_u32, 0x00401300_u32),
        (79_u8, 0x0051FC8C_u32, 0x00401300_u32, 0x00401300_u32),
        (80_u8, 0x0051FD68_u32, 0x00401300_u32, 0x00401300_u32),
        (82_u8, 0x0051FE88_u32, 0x00401300_u32, 0x00401300_u32),
        (84_u8, 0x00522488_u32, 0x00401300_u32, 0x00401300_u32),
        (85_u8, 0x0051F9CC_u32, 0x00401300_u32, 0x00401300_u32),
        (88_u8, 0x00524AC0_u32, 0x00401300_u32, 0x00401300_u32),
        (90_u8, 0x00526B50_u32, 0x00401300_u32, 0x00401300_u32),
        (91_u8, 0x0051F1E0_u32, 0x00401300_u32, 0x00401300_u32),
        (93_u8, 0x005228F0_u32, 0x00401300_u32, 0x00401300_u32),
    ] {
        for (offset, expected) in [(0x68, expected68), (0x6c, expected6c)] {
            let at = (table - 0x0040_0000 + offset) as usize;
            assert_eq!(
                u32::from_le_bytes(raw[at..at + 4].try_into().unwrap()),
                expected,
                "class {selector} +{offset:X}"
            );
        }
    }
}

#[test]
fn original_selector12_virtual124_ret4_and_active_other_class_chain_are_exact() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(&raw).unwrap();
    //Strict original PE proves .text and these immutable table raw offsets.
    //All body spans exclude padding; do not use the data-only mapped API.
    for (start, end, expected) in [
        (
            0x00435170_u32,
            0x00435184_u32,
            "1c05234ce03c6876ef90a0418cd8975031e7f9c8afd566c65139cbf842b5cc2b",
        ),
        (
            0x00404840_u32,
            0x00404843_u32,
            "e598d0c3ba86d917b177d7adde0556aa99bc355543c57aee0a3e50b684dd7e99",
        ),
        (
            0x00468D10_u32,
            0x00468DDB_u32,
            "43032d16d4c2568b679880e2070e5e7a01fda314857d0b15a45cb276f9da0058",
        ),
        (
            0x004294A0_u32,
            0x00429895_u32,
            "189764878265d1529a6045efcfb635239d380c1edb3a695f4aa9d641090b7233",
        ),
        (
            0x0043F9B0_u32,
            0x0043F9CF_u32,
            "4fa0d3091842bc44ccf48814225a0cb35d0d138cf2aff7efaf0326a825b6b2cd",
        ),
        (
            0x00467070_u32,
            0x004671BD_u32,
            "a1c8a13da94b34067915fe51de92f71967b79db495b6a93775f1d526cbb7b370",
        ),
        (
            0x00469170_u32,
            0x00469184_u32,
            "747f14e9eec97c3cae6c950efd123953e1c4a20257b800a1d09b84abd9571e01",
        ),
        (
            0x004672F0_u32,
            0x00467336_u32,
            "2c4edb48118f7b22cddde906230c2d0a72af00fcab245c72b12c06fb7eb4a2d6",
        ),
        (
            0x004236D0_u32,
            0x004237AA_u32,
            "b7310f8a5f626e36d69f8c2933271fd8e248f4cbd3091b4191e4df4019aaab6b",
        ),
        (
            0x00425190_u32,
            0x004251AC_u32,
            "88174d8188415fefa7c539c987e706f900279fad5eadd27c6c6ddb7e92ef8c6c",
        ),
        (
            0x00481110_u32,
            0x0048114F_u32,
            "3ca057c85f6183d59bdc819003825d1cca7199a08b23718612fb2e4f38983a8e",
        ),
        (
            0x0043D010_u32,
            0x0043D0CA_u32,
            "a8d6b7f17598ce578f6f41948b865ea61bc086ecd8e535863c2b026c63c24c62",
        ),
        (
            0x00423BD0_u32,
            0x00423E4B_u32,
            "fb00b07b1306e4addf30f1361bf0cc6868e80abce89c27aef8c1967fc70bc795",
        ),
        (
            0x0043DB90_u32,
            0x0043DDFF_u32,
            "b42274044c32e1d87b1c921073ab90229496525c104574d7c0663cd0afaf8185",
        ),
        (
            0x00423490_u32,
            0x0042349A_u32,
            "29d0699ad58e1e008487699eb8946b069cf4cd71a25245d4191563b5ba96d653",
        ),
        (
            0x004234C0_u32,
            0x004234C9_u32,
            "1c7ca3f57f3f68faf95c94436e8172809de44f0b370f630f68ba25cba557d1fa",
        ),
        (
            0x0043CD80_u32,
            0x0043CD8A_u32,
            "b430ac51083d34f61a28d5dd4deb19b1edced61c92444e9174a0281ade1e48fb",
        ),
        (
            0x0043CDB0_u32,
            0x0043CDB9_u32,
            "c3005e32e23c6cc2dd8e62343466ebb20053079c473f3571585bde2052157fae",
        ),
        (
            0x00466E20_u32,
            0x00466E2A_u32,
            "6b27a0f233ce14e10b3514aff221b77949be26ba6601644097f92a9a2b97907f",
        ),
        (
            0x00466E50_u32,
            0x00466E59_u32,
            "aa157cd5d21cbc6dddcc966ec2a719711cd433360c62f9a05aadef11d9eac618",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&raw[(start - 0x0040_0000) as usize..(end - 0x0040_0000) as usize])
            ),
            expected,
            "complete {start:08X}"
        );
    }
    for (address, expected) in [
        (0x00526FE8_u32, 0x00466FA0_u32),
        (0x00526FEC_u32, 0x00469130_u32),
        (0x00526FF0_u32, 0x00469170_u32),
        (0x00526FF4_u32, 0x00435170_u32),
        (0x00520AF8_u32, 0x00423660_u32),
        (0x00520AFC_u32, 0x0043FB20_u32),
        (0x00520B00_u32, 0x00435150_u32),
        (0x00520B04_u32, 0x00435170_u32),
        (0x00520B08_u32, 0x0043FA00_u32),
        (0x00522688_u32, 0x0043D050_u32),
        (0x0052268C_u32, 0x0043FB20_u32),
        (0x00522690_u32, 0x00435150_u32),
        (0x00522694_u32, 0x00435170_u32),
        (0x00522698_u32, 0x0043FA00_u32),
        (0x005271AC_u32, 0x00425C30_u32),
        (0x005271B0_u32, 0x0043F990_u32),
        (0x005271B4_u32, 0x0043F9B0_u32),
        (0x005271B8_u32, 0x004690F0_u32),
        (0x0053A098_u32, 0x0054ECD0_u32),
        (0x0053A09C_u32, 0x0054ECD8_u32),
        (0x0053A0A0_u32, 0x0054ECC0_u32),
        (0x0053A0A4_u32, 0x0054ECD4_u32),
        (0x0053A0A8_u32, 0x0054ECE0_u32),
        (0x0053A0AC_u32, 0x0054ECCC_u32),
        (0x0053A0B0_u32, 0x0054ECDC_u32),
        (0x0053A0B4_u32, 0x0054ECC8_u32),
        (0x0053A0B8_u32, 0x0054ECD0_u32),
        (0x0053A0BC_u32, 0x0054ECD8_u32),
        (0x0053A0C0_u32, 0x0054ECC0_u32),
        (0x0053A0C4_u32, 0x0054ECD4_u32),
        (0x0053A0C8_u32, 0x0054ECC4_u32),
        (0x0053A0CC_u32, 0x0054ECE0_u32),
        (0x0053A0D0_u32, 0x0054ECCC_u32),
        (0x0053A0D4_u32, 0x0054ECDC_u32),
        (0x0053A0D8_u32, 0x0054ECC8_u32),
        (0x00520CA8_u32, 0x00423BD0_u32),
        (0x00520DEC_u32, 0x00423BD0_u32),
        (0x0052717C_u32, 0x00404840_u32),
        (0x00522860_u32, 0x0043DB90_u32),
        (0x00528004_u32, 0x00423BD0_u32),
        (0x00527134_u32, 0x00404840_u32),
    ] {
        let offset = (address - 0x0040_0000) as usize;
        assert_eq!(
            u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap()),
            expected,
            "table {address:08X}"
        );
    }
    for (address, expected) in [
        (0x0043517B_u32, &[0xff, 0x90, 0x24, 0x01, 0x00, 0x00][..]),
        (0x0043F9C8_u32, &[0xff, 0x52, 0x0c][..]),
        (0x00429876_u32, &[0xff, 0x52, 0x08][..]),
        (0x00404840_u32, &[0xc2, 0x04, 0x00][..]),
        (0x00467101_u32, &[0xc7, 0x06, 0x58, 0x70, 0x52, 0x00][..]),
    ] {
        let offset = (address - 0x0040_0000) as usize;
        assert_eq!(
            &raw[offset..offset + expected.len()],
            expected,
            "opcode {address:08X}"
        );
    }
}
