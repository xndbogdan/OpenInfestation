//! Whole-source and original fixed-layout guards; no debugger dependency.
use sha2::{Digest, Sha256};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[test]
fn complete_auxiliary_projection_publication_and_allocation_bodies_are_original() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        sha(&bytes),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let exe = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    // Raw .text offsets are authenticated RVA=raw for this exact whole PE.
    for (start, end, expected) in [
        (
            0x429ee0usize,
            0x429fe2usize,
            "be5bb7dc317f797174b4c7948b376901a184c4de4caed3b646dd40d6daf84ed9",
        ),
        (
            0x47c1d0usize,
            0x47c6ccusize,
            "70c744e0ca1fc009d760d2516289a54013cf3a078dc4a023f3825de0451071ad",
        ),
        (
            0x40d6a0usize,
            0x40d7afusize,
            "89cfb37d18878148bae7fcc7dbf0456cd5abd80517c28a42bf2dbde929426ea0",
        ),
        (
            0x47f810usize,
            0x47fa31usize,
            "a58073ab9b3df74648bf648bc3ba5d4c80e805876a92512e301b7eb7f4d06785",
        ),
        (
            0x4f2be0usize,
            0x4f2c3eusize,
            "d2bedc011bd9bcaf69b6b376c070a7b375ff81ef07dd53a4af82d0e7784a6ca4",
        ),
        (
            0x4f2c50usize,
            0x4f3549usize,
            "3fd86d1ee3f192734fd1d35176eedd2f674c16ddac81b7981ce259ba544f0e15",
        ),
        (
            0x4f18b0usize,
            0x4f191busize,
            "c0f43834d7bf4ac7cbebba85cca2b94da6d54888fdead277b4fa3b106f01d249",
        ),
        (
            0x4ebe60usize,
            0x4ebedfusize,
            "b1421130aff125ac2808500e7bc368e72819da3e201290edcda3729f91048731",
        ),
        (
            0x4ebee0usize,
            0x4ebef7usize,
            "4af09e63c3a81a6f810c7664fb4ee24d0fa04bf55be8c11805f2864e9a7f15c5",
        ),
        (
            0x4a00e0usize,
            0x4a0239usize,
            "18ce68e4b32e26da6882e6866c0c2bb7412e12ab2faf49940a5ff55bfd025a02",
        ),
        (
            0x4eb5a0usize,
            0x4eb724usize,
            "db229d3f9fb5d873c351cd6d253063f15323547b005ab6cf0ac0d83605c0f89a",
        ),
        (
            0x4eb8e0usize,
            0x4eba9cusize,
            "4769c67c096e60e2e5e0fc79fa08814c9ee9fbb58c4355662d5babc06885c77e",
        ),
        (
            0x4ebf00usize,
            0x4ebfd3usize,
            "20ce626e297772f96cebd001f7901f10c758cc192f347bae193ab4392e793a42",
        ),
        (
            0x490900usize,
            0x490970usize,
            "2ced0a2761db73c93e39081d97d1d497e0fa201b23e70c329ea6c7341a10318d",
        ),
        (
            0x437a70usize,
            0x437a84usize,
            "f3ce733b9ac7a121a1e5611687ae5e443ec8ae2866975bba993de29bfe2dad3a",
        ),
        (
            0x48fb40usize,
            0x48fe57usize,
            "3121f9bb739d9705d43dc243e73cae40b8047c287389a740854472f2d90e2e0c",
        ),
        (
            0x4f1420usize,
            0x4f1504usize,
            "04fb10fe1057c0a49bf95a9c4a2e66b8166aff5fb2403a0c5f6c30fb379445bb",
        ),
        (
            0x4f1940usize,
            0x4f1a14usize,
            "348b4b8112d287291f2daf9c2263ee4503b92a81ceddff5cdceff7d3053b0411",
        ),
        (
            0x4a0ed0usize,
            0x4a114eusize,
            "8d3574347d72aa21e20656af46ae4b095a0ad849b9a832b106903cbbe3ce291c",
        ),
        (
            0x439880usize,
            0x439981usize,
            "1a2acdc748ebaa314b0c4006d3aa2f13cac0096ce5b4df12d3c0e6f08258b1c0",
        ),
        (
            0x43b4a0usize,
            0x43b5causize,
            "02860a116fb37b406d24262635a16bc4d86e5d147aa983c149520a7c1ceff6bc",
        ),
        (
            0x438640usize,
            0x4387aeusize,
            "2d7c32c4d2a55482dc8001bc6ba99f14c3572ea51236e81cbbe8c035cfeb8b3c",
        ),
        (
            0x49fef0usize,
            0x4a00e0usize,
            "3999652a8b38bc97f0041cf803ec2e533d94831eabe8121c9caa13ecb84e7141",
        ),
    ] {
        assert_eq!(sha(&bytes[start - 0x400000..end - 0x400000]), expected);
    }
    assert_eq!(
        sha(exe.mapped(0x544740, 36).unwrap().bytes),
        "2f0899d89997431f6b21e97549aa4dac194facff69766424f25a922cbda8260f"
    );
    assert_eq!(
        sha(exe.mapped(0x544c08, 20).unwrap().bytes),
        "db3e6dc9ac0d033785e9114e50dd9c19622a0033ae5d955e8ea98ecf72feab3b"
    );
    assert_eq!(
        sha(exe.mapped(0x544c68, 20).unwrap().bytes),
        "64a2e21742e9d01273f98d75ba870a7e95b4a67d8815bdcd406f2b83a2a60660"
    );
    assert_eq!(
        sha(exe.mapped(0x51f198, 4).unwrap().bytes),
        "df3f619804a92fdb4057192dc43dd748ea778adc52bc498ce80524c014b81119"
    );
    exe.loader_zero_data(0x55ef38, 12).unwrap();
    exe.loader_zero_data(0x55ef6c, 4).unwrap();
    exe.loader_zero_data(0x55ef94, 4).unwrap();
    exe.loader_zero_data(0x564728, 0xfa004).unwrap();
}

#[test]
fn actual_223_51_and_492_392_auxiliary_bytes_and_preload_joins_are_original() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    assert_eq!(
        sha(&std::fs::read(root.join("PreLoad.dat")).unwrap()),
        "d41b9eb6aceae2df8c53e802ee27c6e5fcf5fbc9608c3df4758171ab6e8931d3"
    );
    let ovl = std::fs::read(root.join("Overlay/1.ovl")).unwrap();
    assert_eq!(
        sha(&ovl),
        "aeefe3f312118008f9a3379e41927b8dd71a749345706503b0abe17860cc7ce7"
    );
    assert_eq!(
        sha(&ovl[189808..190552]),
        "9bf65fe005192656254ba1732453633d11e19b1d3af7bcd4b9901f3a37f59ea4"
    );
    assert_eq!(
        sha(&ovl[189840..189872]),
        "c4f23681acbdcdab102bdd90cad2648cd834422ac248d46fd70958e1197ae03e"
    );
    assert_eq!(
        sha(&ovl[18832..19196]),
        "8604f95170f9a60ff6de989e3a2a4a0a6e55ced55a9d7aebd60e80d71c8c027c"
    );
    assert_eq!(
        sha(&ovl[18864..18896]),
        "a16c924c69800a8bdb0f3afcca7c4ae75b95d1079dd32eeb4d3f151d56ab2d3b"
    );
    let ovl = std::fs::read(root.join("Overlay/32.ovl")).unwrap();
    assert_eq!(
        sha(&ovl),
        "d50d19f644f33e82e2e0332d413a2a901cb0eecd8f3156ba35edd5b81f8245fa"
    );
    assert_eq!(
        sha(&ovl[438744..439404]),
        "778bd03fee03857ae5420717e6fbeba59d18d66a890c00e6d2f61ae0e8551daa"
    );
    assert_eq!(
        sha(&ovl[438776..438880]),
        "1f956f3c364702f32076596dd540e27dcb6a431863d0d46c9acce17eaa1aa983"
    );
    assert_eq!(
        sha(&ovl[374300..375012]),
        "507f163d9cc606f305c96e67571577cb5f0379d24f269c29a5ba2b2fdd80a6b7"
    );
    assert_eq!(
        sha(&ovl[374332..374376]),
        "b41838fad4cd5c1e0359f452a2e87264f5532a3d5e44d75318e4ea4f38381a3d"
    );
}
