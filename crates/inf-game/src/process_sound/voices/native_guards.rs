use super::*;
use sha2::{Digest, Sha256};
#[test]
fn complete_original_voice_bodies_and_once_loader_domain_remain_source_bound() {
    let raw = super::tests::raw();
    let exe = FrontendExecutable::parse(&raw).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let bodies: [(u32, usize, &str); 12] = [
        (
            0x00510450,
            249,
            "34ff0f6800fa022292ac4c1dd668b6a35828e10fa78f83ff49d0552c878c6a0c",
        ),
        (
            0x00480110,
            568,
            "13a967c879d83ec6d82ab3c8e8ee19f0976f40a19744ec5976fc33d9f4f438e5",
        ),
        (
            0x004F4100,
            263,
            "d9e44e6a6c4b4b19b6d2f3ef66d04ef3afc90201f31caf8fc1a2a901cd9a14cf",
        ),
        (
            0x004F4230,
            808,
            "f24d55bea3860528df8b0b0e0856325e2d98d0d4a5b00576118831b546edaf00",
        ),
        (
            0x004F4560,
            77,
            "49f7beec3131a1160f632877b68367b872a9e7ef8d81066919c48ebf1ec66837",
        ),
        (
            0x004F45B0,
            77,
            "c182c05a0bcd2624a83f21fafa94c9254fb6068802316125496cb8c75cf30851",
        ),
        (
            0x004F4600,
            223,
            "d142fd7a8fe48a1fece5288e630445561cbcad0184ea0e173791622eefbca1e8",
        ),
        (
            0x004F46E0,
            155,
            "312b628342d9dd6cff63be98ec3ef4b2b32b60ef136392e8bdd77c6f828ba276",
        ),
        (
            0x004F40B0,
            59,
            "8cf66e7efad80a82cf8ae3e4c7462f0afd17b27e01d55c4bc3fa04fd1211975b",
        ),
        (
            0x004F4780,
            152,
            "561f8a550d883257dcc687d00719affeee06287ec53aa9e6865e10a15f96f263",
        ),
        (
            0x00510250,
            55,
            "a86504b9e5befdaf4728990d02b4e0ddfea18161f9a9bea7f5d63c952417475d",
        ),
        (
            0x00510330,
            31,
            "99a1317c73e84b97c2d67077972c331873cac5cb994680f25c620331fc94093f",
        ),
    ];
    //Whole BF09 hash fixes .text raw offset=RVA; mapped() is data-only.
    for (start, length, hash) in bodies {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(
                    &raw[(start - 0x0040_0000) as usize..(start - 0x0040_0000) as usize + length]
                )
            ),
            hash,
            "complete body {start:08x}"
        );
    }
    exe.loader_zero_data(0x0065f248, 1).unwrap(); //enabled BYTE
    exe.loader_zero_data(0x0065f24c, 4).unwrap(); //device DWORD
    exe.loader_zero_data(0x0065f250, 1).unwrap(); //configuration BYTE
    exe.loader_zero_data(0x0065f248, 12).unwrap();
    let zero_range = exe.loader_zero_data(0x0065f248, 12).unwrap();
    assert_eq!(
        (zero_range.start(), zero_range.end()),
        (0x0065f248, 0x0065f254)
    );
}
