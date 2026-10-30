//!Original complete entry/scan/miss and literalRET registration chain.
//!No execution or current projector/history acceptance is supplied by this guard.
#[test]
fn complete_collision_prefix_and_class_virtual_slots_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let exe = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    for (start, end, hash) in [
        (
            0x00429EE0usize,
            0x00429FE2usize,
            "be5bb7dc317f797174b4c7948b376901a184c4de4caed3b646dd40d6daf84ed9",
        ),
        (
            0x0047C1D0usize,
            0x0047C6CCusize,
            "70c744e0ca1fc009d760d2516289a54013cf3a078dc4a023f3825de0451071ad",
        ),
        (
            0x0040D6A0usize,
            0x0040D7AFusize,
            "89cfb37d18878148bae7fcc7dbf0456cd5abd80517c28a42bf2dbde929426ea0",
        ),
        (
            0x0047F810usize,
            0x0047FA31usize,
            "a58073ab9b3df74648bf648bc3ba5d4c80e805876a92512e301b7eb7f4d06785",
        ),
        (
            0x00437A70usize,
            0x00437A84usize,
            "f3ce733b9ac7a121a1e5611687ae5e443ec8ae2866975bba993de29bfe2dad3a",
        ),
        (
            0x004026B0usize,
            0x004026B4usize,
            "b185eed3e098fd3dc404ff66db8be1121a2a2610cf0814ec915e002304c9bccf",
        ),
        (
            0x004F2BE0usize,
            0x004F2C3Eusize,
            "d2bedc011bd9bcaf69b6b376c070a7b375ff81ef07dd53a4af82d0e7784a6ca4",
        ),
        (
            0x0047C6D0usize,
            0x0047C6D1usize,
            "ae3f4619b0413d70d3004b9131c3752153074e45725be13b9a148978895e359e",
        ),
        (
            0x00511D89usize,
            0x00511D9Busize,
            "f08b5f4550f61873da85812c9ed808ffb1b8d3a95f4c91123034d42c9cadc5e1",
        ),
        (
            0x00511D0Busize,
            0x00511D89usize,
            "fac3b20f685075b86fa0ca4c6beaef1fe4b6dbfc6d6d735418c122c81af3f51b",
        ),
        (
            0x00511D9Busize,
            0x00511DCAusize,
            "8c7dd1dddf08cdd5fa012ec68f35a18ca60f6b901a0abfe2a29a6dc6c4db2bf4",
        ),
        (
            0x005122C7usize,
            0x0051236Cusize,
            "eece897de5b81f6888e5fd5f416c142ae066f3d4271859f9b5e8e38b5921bfe6",
        ),
    ] {
        //Supported originalPE .text raw=RVA, authenticated by fullEXE hash
        //and strict parser. mapped() intentionally remains .data/.rdata only.
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[start - 0x400000..end - 0x400000])
            ),
            hash
        );
    }
    for (table, response) in [
        (0x00520cc8, 0x0042a760),
        (0x00527058, 0x0042a760),
        (0x00527ee0, 0x0042a760),
        (0x0052273c, 0x0047bfd0),
    ] {
        assert_eq!(exe.word(table + 0x48).unwrap(), 0x004026b0);
        assert_eq!(exe.word(table + 0xa8).unwrap(), response);
    }
    assert_eq!(exe.word(0x005276d8).unwrap(), 0x004efbf0); //H8 null loader
    assert_eq!(exe.word(0x00527778).unwrap(), 0x004efbf0); //actualH12 null loader
    for (address, length) in [
        (0x005483c4, 4),
        (0x0055f0dc, 1),
        (0x0055ef74, 4),
        (0x0055ef44, 40),
        (0x0055efd0, 40),
        (0x0055f004, 40),
        (0x0055f038, 40),
        (0x0055f06c, 40),
    ] {
        exe.loader_zero_data(address, length).unwrap();
    }
    let process =
        crate::world_furniture_effect::ProcessFurnitureQueryState::native_startup(exe).unwrap();
    assert_eq!(
        process.current_record(),
        &crate::world_furniture_effect::FurnitureQueryRecord::NativeAddress(
            exe.word(0x00546b84).unwrap()
        )
    );
    //Nativecaller ignores CRT EAX: only ADDESP follows this call.
    assert_eq!(
        &bytes[0x7c25f..0x7c26c],
        &[0x68, 0xd0, 0xc6, 0x47, 0, 0xe8, 0x20, 0x5b, 9, 0, 0x83, 0xc4, 4]
    );
}
