//! Whole BF09 executable, complete native consumers and exact descriptor/final tables.
//! Static guards do not establish runtime descriptor selection or scene acceptance.
#[test]
fn complete_point_change_update_clone_consumers_and_final_tables_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let _profile = inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in [
        (
            0x00428650usize,
            0x00428729usize,
            "42f4992c11ffd8b87ab75bee34edd89b920c3182a2c8ba1e936abe574351b1cf",
        ),
        (
            0x004294A0usize,
            0x00429895usize,
            "189764878265d1529a6045efcfb635239d380c1edb3a695f4aa9d641090b7233",
        ),
        (
            0x00467070usize,
            0x004671BDusize,
            "a1c8a13da94b34067915fe51de92f71967b79db495b6a93775f1d526cbb7b370",
        ),
        (
            0x00467390usize,
            0x004675D6usize,
            "0907503171675ad556bfc8a14501451549d98c46f96ee090b90c2a27e9089fc1",
        ),
        (
            0x004675E0usize,
            0x00467653usize,
            "9e82c362b48f511f69493a0925aba4f854c7c6a0efedaf788ece5af0578ab609",
        ),
        (
            0x004691F0usize,
            0x00469204usize,
            "ca137d9423f4a5a254a10d9be88375e4ce064ece0c6493388258b077fbf9f9a9",
        ),
        (
            0x00469210usize,
            0x00469224usize,
            "6de908b7e06a5c50bea54563521e076e8e48e70f9fbe60b5fb2e4e6ca7e7d10e",
        ),
        (
            0x0048EFC0usize,
            0x0048F05Busize,
            "cdec5838e3c2402b0deab3be8c3cd6cccff09fa60049ed34ec75610fbb6858ff",
        ),
        (
            0x0048F060usize,
            0x0048F079usize,
            "e7848b6caf06850740711a5eb206007001af8db642280353cd14fd509d714f86",
        ),
        (
            0x00511708usize,
            0x00511716usize,
            "9092ad69660ec58443c4366c955f320e2687fede12f753b226fd7388a20d0a4e",
        ),
        (
            0x0051247Cusize,
            0x005124A8usize,
            "72c061d39eca92bf0bc007a8c83fb759c1bb051dbd89cc41987724871f7e8110",
        ),
        (
            0x005124A8usize,
            0x005124F6usize,
            "4195a1032251b00e213328f6816a7b1c5970c2cfe7625165828556a0bc1b38b8",
        ),
        (
            0x00482690usize,
            0x00482739usize,
            "6c73508648f687eb8daeb9fedcbf8417a57f2d35541083df8e1e409d56d748cc",
        ),
        (
            0x00402700usize,
            0x00402706usize,
            "e36623525b7c35b277173e56616b2d865cf6515aa2be1d12d6814ef725cecffb",
        ),
        (
            0x00469170usize,
            0x00469184usize,
            "747f14e9eec97c3cae6c950efd123953e1c4a20257b800a1d09b84abd9571e01",
        ),
        (
            0x004672F0usize,
            0x00467336usize,
            "2c4edb48118f7b22cddde906230c2d0a72af00fcab245c72b12c06fb7eb4a2d6",
        ),
        (
            0x00402730usize,
            0x00402770usize,
            "558b1f333eed18cf5997503a46d31a896d5e83c284f85292b547ed9965809808",
        ),
        (
            0x004073A0usize,
            0x004073E6usize,
            "a5b12f08d4a1b530dacc8a212c264957e2290654c155c7d3c1198e665bf80d6c",
        ),
        (
            0x004828D0usize,
            0x00482934usize,
            "94dae4a849808a9965c020d6bc7981130df5bcce1908ce9409bcf078649818b4",
        ),
    ] {
        //Strict BF09 profile: these file-backed .text/.rdata/.data raw offsets equal RVA.
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x400000..end - 0x400000])
            ),
            expected
        );
    }
    for (address, bytes, expected) in [
        (
            0x0053A148usize,
            12usize,
            "defccc339c9c0ec73fdedee19973793482db076c83804e6990bc229ad248fac9",
        ),
        (
            0x00526FF8usize,
            16usize,
            "dedb74abb9b120efddf8cb5479b50c46e40e5566451bc0fbdae7f8dd5462b8d2",
        ),
        (
            0x0053A098usize,
            32usize,
            "20a8af10733dfd18d69f8396781d76d4f797dd2b7211b2eef16fc940e77b2345",
        ),
        (
            0x0053A0B8usize,
            36usize,
            "ed704b8c8ca4794bcc9a64f606d75753eb729e57a8461505b9b745d488c21abf",
        ),
        (
            0x0051F154usize,
            32usize,
            "6a6943f7a0b3ba68e5256f533a8134c6f7d6d48bc8b004240ba4da16fc664a75",
        ),
        (
            0x0051FB6Cusize,
            32usize,
            "aec46068cfc045bcc2e4cac48961fa15d6b13ed56d0579e60bec3dce843c7392",
        ),
        (
            0x00528038usize,
            32usize,
            "eba06da677b76294bc8c5984dcaceffa8ddc097d66468a40554d4a16aa94b352",
        ),
        (
            0x0053A12Cusize,
            12usize,
            "d5c90157c77f06e958aac33e32bd3da94ec8d37340234c2a54ef8a58234c9f79",
        ),
        (
            0x00526FE8usize,
            16usize,
            "0ce1156bd0e56ef3d2d829db1029ead55f910e1abf46b26601cf684efe64554a",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[address - 0x400000..address - 0x400000 + bytes])
            ),
            expected
        );
    }
    for (offset, expected) in [
        (0x12cusize, 0x4672f0u32),
        (0x14c, 0x4675e0),
        (0x148, 0x467390),
    ] {
        let at = 0x527058 - 0x400000 + offset;
        assert_eq!(
            u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()),
            expected
        );
    }
}
