//! The descriptor tables are what retail's CRT initializers build: each row's
//! initializer calls a thunk that loads the singleton into ECX and jumps to a
//! constructor storing its vtable. (Each initializer then registers the
//! singleton's exit-time destructor, which the port has no counterpart for.)

use super::*;

/// Per row, in table order: initializer, construct thunk, constructor.
const FAMILY_INITIALIZERS: [(u32, u32, u32); 10] = [
    (0x0042_3380, 0x0042_3390, 0x0042_33d0),
    (0x0042_33e0, 0x0042_33f0, 0x0042_3420),
    (0x0042_3430, 0x0042_3440, 0x0042_3470),
    (0x0042_3480, 0x0042_3490, 0x0042_34c0),
    (0x0042_34d0, 0x0042_34e0, 0x0042_3510),
    (0x0042_3520, 0x0042_3530, 0x0042_3560),
    (0x0042_3570, 0x0042_3580, 0x0042_35b0),
    (0x0042_35c0, 0x0042_35d0, 0x0042_3600),
    (0x0042_3610, 0x0042_3620, 0x0042_3650),
    (0x0042_3680, 0x0042_3690, 0x0042_36c0),
];

const SELECTOR35_INITIALIZERS: [(u32, u32, u32); 12] = [
    (0x0043_cc80, 0x0043_cc90, 0x0043_ccc0),
    (0x0043_ccd0, 0x0043_cce0, 0x0043_cd10),
    (0x0043_cd20, 0x0043_cd30, 0x0043_cd60),
    (0x0043_cd70, 0x0043_cd80, 0x0043_cdb0),
    (0x0043_cdc0, 0x0043_cdd0, 0x0043_ce00),
    (0x0043_ce10, 0x0043_ce20, 0x0043_ce60),
    (0x0043_ce70, 0x0043_ce80, 0x0043_ceb0),
    (0x0043_cec0, 0x0043_ced0, 0x0043_cf00),
    (0x0043_cf10, 0x0043_cf20, 0x0043_cf50),
    (0x0043_cf60, 0x0043_cf70, 0x0043_cfa0),
    (0x0043_cfb0, 0x0043_cfc0, 0x0043_cff0),
    (0x0043_d000, 0x0043_d010, 0x0043_d040),
];

const SELECTOR34_INITIALIZERS: [(u32, u32, u32); 8] = [
    (0x0043_3870, 0x0043_3880, 0x0043_38b0),
    (0x0043_38e0, 0x0043_38f0, 0x0043_3920),
    (0x0043_3930, 0x0043_3940, 0x0043_3970),
    (0x0043_3980, 0x0043_3990, 0x0043_39c0),
    (0x0043_39d0, 0x0043_39e0, 0x0043_3a10),
    (0x0043_3a20, 0x0043_3a30, 0x0043_3a60),
    (0x0043_3a70, 0x0043_3a80, 0x0043_3ac0),
    (0x0043_3ad0, 0x0043_3ae0, 0x0043_3b10),
];

const SELECTOR12_INITIALIZERS: [(u32, u32, u32); 9] = [
    (0x0046_6d70, 0x0046_6d80, 0x0046_6db0),
    (0x0046_6dc0, 0x0046_6dd0, 0x0046_6e00),
    (0x0046_6e10, 0x0046_6e20, 0x0046_6e50),
    (0x0046_6e60, 0x0046_6e70, 0x0046_6ea0),
    (0x0046_6eb0, 0x0046_6ec0, 0x0046_6ef0),
    (0x0046_6f00, 0x0046_6f10, 0x0046_6f40),
    (0x0046_6f50, 0x0046_6f60, 0x0046_6f90),
    (0x0046_6fc0, 0x0046_6fd0, 0x0046_7000),
    (0x0046_7010, 0x0046_7020, 0x0046_7060),
];

fn code(exe: &[u8], address: u32, length: usize) -> &[u8] {
    // .text's raw offset equals its RVA, as every native guard here reads it.
    &exe[(address - 0x0040_0000) as usize..][..length]
}

fn relative_target(exe: &[u8], instruction: u32) -> u32 {
    let displacement = i32::from_le_bytes(code(exe, instruction + 1, 4).try_into().unwrap());
    (instruction + 5).wrapping_add(displacement as u32)
}

#[test]
fn descriptor_tables_are_what_the_initializers_construct() {
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    for (table, initializers) in [
        (&FAMILY[..], &FAMILY_INITIALIZERS[..]),
        (&SELECTOR35[..], &SELECTOR35_INITIALIZERS[..]),
        (&SELECTOR34[..], &SELECTOR34_INITIALIZERS[..]),
        (&SELECTOR12[..], &SELECTOR12_INITIALIZERS[..]),
    ] {
        assert_eq!(table.len(), initializers.len());
        for (row, &(initializer, construct, constructor)) in table.iter().zip(initializers) {
            // call construct; jmp (the destructor's registration)
            assert_eq!(code(&exe, initializer, 1), [0xe8], "{initializer:#x}");
            assert_eq!(relative_target(&exe, initializer), construct);
            assert_eq!(code(&exe, initializer + 5, 1), [0xe9], "{initializer:#x}");
            // mov ecx, object; jmp constructor
            assert_eq!(code(&exe, construct, 1), [0xb9], "{construct:#x}");
            assert_eq!(
                u32::from_le_bytes(code(&exe, construct + 1, 4).try_into().unwrap()),
                row.object_address,
                "{construct:#x}"
            );
            assert_eq!(code(&exe, construct + 5, 1), [0xe9], "{construct:#x}");
            assert_eq!(relative_target(&exe, construct + 5), constructor);
            // mov eax, ecx; mov dword ptr [eax], vtable; ret
            let body = code(&exe, constructor, 9);
            assert_eq!(&body[..4], [0x8b, 0xc1, 0xc7, 0x00], "{constructor:#x}");
            assert_eq!(
                u32::from_le_bytes(body[4..8].try_into().unwrap()),
                row.vtable_address,
                "{constructor:#x}"
            );
            assert_eq!(body[8], 0xc3, "{constructor:#x}");
        }
    }
}
