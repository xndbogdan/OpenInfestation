//! Complete original bodies/table lanes; no instruction execution.
use sha2::{Digest, Sha256};
#[test]
fn original_e8_wrapper_change_target_virtuals_and_class_tables_remain_exact() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in BODIES {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[*start - 0x0040_0000..*end - 0x0040_0000])
            ),
            *expected
        );
    }
    for (table, slot, expected) in TABLES {
        let at = table + slot - 0x0040_0000;
        assert_eq!(
            u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()),
            *expected
        );
    }
    //The two literal descriptor wrappers share43FA80 while their final actor
    //E8 virtuals differ. LOWBYTE entering is tested only by actual4288E0.
    for table in [0x0052_0b48usize, 0x0052_26ec] {
        let at = table + 8 - 0x0040_0000;
        assert_eq!(
            u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()),
            0x0043_fa80
        );
    }
    assert_eq!(&exe[0x288e0..0x288e4], &[0x8a, 0x44, 0x24, 0x04]);
    //Captured1FC ->status ->virtual14 ->fresh1FC ->clone04 ->148 publication.
    assert_eq!(
        &exe[0x288f0..0x28900],
        &[
            0x8b, 0x8e, 0xfc, 0x01, 0x00, 0x00, 0xc7, 0x86, 0x28, 0x02, 0x00, 0x00, 0xf0, 0x11,
            0x53, 0x00
        ]
    );
    assert_eq!(&exe[0x28906..0x28909], &[0xff, 0x50, 0x14]);
    assert_eq!(
        &exe[0x2890f..0x28920],
        &[
            0x8b, 0x8e, 0xfc, 0x01, 0x00, 0x00, 0x8b, 0x11, 0xff, 0x52, 0x04, 0x89, 0x86, 0x48,
            0x01, 0x00, 0x00
        ]
    );
    //No NULL test afterclone: sourceY/X reads follow the148 store directly.
    assert_eq!(
        &exe[0x28920..0x28928],
        &[0x66, 0x8b, 0x4f, 0x34, 0x66, 0x8b, 0x47, 0x32]
    );
    //Clone allocation NULL bypasses actual fresh processNULL and position.
    assert_eq!(
        &exe[0x73ad..0x73ba],
        &[0x85, 0xc0, 0x74, 0x31, 0x8b, 0x4e, 0x14, 0x8b, 0x15, 0x94, 0xe8, 0x54, 0x00]
    );
}
const BODIES: &[(usize, usize, &str)] = &[
    (
        0x0043FA80,
        0x0043FA94,
        "e312fbd627b6f625eeb5d863cdb004fa346ae1909f2542d4d4385d53ff49c625",
    ),
    (
        0x0043FAA0,
        0x0043FAB4,
        "95d5e70eeefa3bae58dbc76712e782b79331b000112954ba8beb0314cbf35114",
    ),
    (
        0x004288E0,
        0x00428987,
        "150c44617034b93ae06c67d63c47d608452ff050466585329ea7293ec25acf76",
    ),
    (
        0x00428830,
        0x004288D6,
        "cf625df444b0e08f37fd7f3004f2610e4020d0d2bc2f55e4c4a6cf08492c965c",
    ),
    (
        0x004073A0,
        0x004073E6,
        "a5b12f08d4a1b530dacc8a212c264957e2290654c155c7d3c1198e665bf80d6c",
    ),
    (
        0x004073F0,
        0x004073F4,
        "01d3abacb68aac3537badfe563d0e21d0244fb61db1d805bcef8f455407fad96",
    ),
    (
        0x00401300,
        0x00401303,
        "4bc724f3b1d0caf4fe369c18cba3102e6c4ea057f63fe1587e3973134a7f755e",
    ),
    (
        0x0048EFC0,
        0x0048F05B,
        "cdec5838e3c2402b0deab3be8c3cd6cccff09fa60049ed34ec75610fbb6858ff",
    ),
    (
        0x004236D0,
        0x004237AA,
        "b7310f8a5f626e36d69f8c2933271fd8e248f4cbd3091b4191e4df4019aaab6b",
    ),
    (
        0x00425190,
        0x004251AC,
        "88174d8188415fefa7c539c987e706f900279fad5eadd27c6c6ddb7e92ef8c6c",
    ),
    (
        0x00481110,
        0x0048114F,
        "3ca057c85f6183d59bdc819003825d1cca7199a08b23718612fb2e4f38983a8e",
    ),
    (
        0x00467070,
        0x004671BD,
        "a1c8a13da94b34067915fe51de92f71967b79db495b6a93775f1d526cbb7b370",
    ),
    (
        0x004294A0,
        0x00429895,
        "189764878265d1529a6045efcfb635239d380c1edb3a695f4aa9d641090b7233",
    ),
];
const TABLES: &[(usize, usize, u32)] = &[
    (0x00520B84, 0xE8, 0x004288E0),
    (0x00520B84, 0xEC, 0x00428830),
    (0x00520CC8, 0xE8, 0x004288E0),
    (0x00520CC8, 0xEC, 0x00428830),
    (0x00527EE0, 0xE8, 0x004288E0),
    (0x00527EE0, 0xEC, 0x00428830),
    (0x00527058, 0xE8, 0x004288E0),
    (0x00527058, 0xEC, 0x00428830),
    (0x0052273C, 0xE8, 0x0043F700),
    (0x0052273C, 0xEC, 0x0043F590),
    (0x0051FB6C, 0x04, 0x004073A0),
    (0x0051FB6C, 0x10, 0x00401300),
    (0x0051FB6C, 0x14, 0x004073F0),
    (0x0051F154, 0x04, 0x00402730),
    (0x0051F154, 0x10, 0x00401300),
    (0x0051F154, 0x14, 0x00401300),
    (0x00528038, 0x04, 0x004828D0),
    (0x00528038, 0x10, 0x004827C0),
    (0x00528038, 0x14, 0x00401300),
];

#[test]
fn original_ec_update_route_and_target_replacement_keep_exact_lazy_read_and_store_order() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in UPDATE_BODIES {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[*start - 0x0040_0000..*end - 0x0040_0000])
            ),
            *expected
        );
    }
    //Actual descriptor EC wrapper43FAA0;43FAC0 is a distinct F0 score wrapper.
    assert_eq!(&exe[0x3faab..0x3fab1], &[0xff, 0x90, 0xec, 0, 0, 0]);
    for table in [0x0052_0b48usize, 0x0052_26ec] {
        let at = table + 12 - 0x0040_0000;
        assert_eq!(
            u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()),
            0x0043_faa0
        );
    }
    //There is no NULL148 test before virtual14. The later signed branch is JGE;
    //the far squared comparison is JAE, without any root/bit31 correction call.
    assert_eq!(
        &exe[0x28834..0x2883f],
        &[0x8b, 0x8e, 0x48, 0x01, 0, 0, 0x8b, 0x01, 0xff, 0x50, 0x14]
    );
    assert_eq!(&exe[0x2885b..0x2885d], &[0x7d, 0x17]);
    assert_eq!(&exe[0x288c3..0x288c5], &[0x73, 0x0c]);
    //Fresh current148 read then target virtual24; replacement identity stores
    //before native DWORD XY copy and WORD Z copy.
    assert_eq!(
        &exe[0x288c5..0x288d1],
        &[0x8b, 0x8e, 0x48, 0x01, 0, 0, 0x50, 0x8b, 0x11, 0xff, 0x52, 0x24]
    );
    assert_eq!(
        &exe[0x8286b..0x82880],
        &[
            0x89, 0x41, 0x14, 0x83, 0xc0, 0x32, 0x83, 0xc1, 0x08, 0x8b, 0x10, 0x89, 0x11, 0x66,
            0x8b, 0x40, 0x04, 0x66, 0x89, 0x41, 0x04
        ]
    );
    assert_eq!(
        u32::from_le_bytes(exe[0x11fb90..0x11fb94].try_into().unwrap()),
        0x0048_2860
    );
}
const UPDATE_BODIES: &[(usize, usize, &str)] = &[
    (
        0x0043FAA0,
        0x0043FAB4,
        "95d5e70eeefa3bae58dbc76712e782b79331b000112954ba8beb0314cbf35114",
    ),
    (
        0x0043FAC0,
        0x0043FAFE,
        "29b7f96a1e098b9df0fac21cec12d219befda4e6e555904398232dd3c2a2927f",
    ),
    (
        0x00428830,
        0x004288D6,
        "cf625df444b0e08f37fd7f3004f2610e4020d0d2bc2f55e4c4a6cf08492c965c",
    ),
    (
        0x00404C50,
        0x00404C70,
        "99a80e2c1e4175a34669cce350901d044d7d3fc7dfa4e946d1c40578d3b87dfd",
    ),
    (
        0x004050E0,
        0x004050F3,
        "56c5a334fdd0025a84973452ca411d7a90e4ad3a792c22071f15c88f9cfbf295",
    ),
    (
        0x00482860,
        0x00482885,
        "d63a648752d49f983946bcd579d5836914f6a7baae1fcb9da6d4978df3e3696e",
    ),
    (
        0x0043F590,
        0x0043F6F6,
        "990e205234c2e771ac37e5054383afbc928fc2cc3470ec81472da408ee99633e",
    ),
];
