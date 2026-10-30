//! Complete native consumers/helpers, actual finaltables and constructor260.
use sha2::{Digest, Sha256};

#[test]
fn complete_fc_target_route_rng_geometry_and_dispatcher_sources_are_guarded() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, hash) in [
        (
            0x0043FA40usize,
            0x0043FA80usize,
            "571b68f95fb9d70a793ed6f018a3557c404327e6288e024ea6c297a746cb0d57",
        ),
        (
            0x00424F50usize,
            0x004250BEusize,
            "58b25928c067de5e93e4f10375589ac1f37debefda13affa05fdbdc0e702c4d4",
        ),
        (
            0x00427320usize,
            0x00427383usize,
            "0fff10fa843a8d43098f478007bf825cf08429055d8b4f17c61b5222646b7246",
        ),
        (
            0x00427620usize,
            0x00427638usize,
            "cff2c20271ae285b589656b865260a5e187e5de61401fb935e717d841b6c4283",
        ),
        (
            0x00404C70usize,
            0x00404C92usize,
            "127da2b3718435b4106facaffc09fd2cb9a0899bc9f79e19d4c613bdf9debb4d",
        ),
        (
            0x00405100usize,
            0x00405115usize,
            "623b2d898bca8e6a03359ad83ae53a62bcf54fce5ba9b707cb94e748fe88b8b4",
        ),
        (
            0x00417DA0usize,
            0x00417E32usize,
            "8628949746c03273f9661ba840983d96a848c441e196b3d1da38c84b9cb69039",
        ),
        (
            0x00511BA8usize,
            0x00511BCFusize,
            "d0a2aca68648a0ae9d5cbfae1b276d4a64d4f797fc0c60f28b64096cdfa3148c",
        ),
        (
            0x0048F060usize,
            0x0048F079usize,
            "e7848b6caf06850740711a5eb206007001af8db642280353cd14fd509d714f86",
        ),
        (
            0x004026B0usize,
            0x004026B4usize,
            "b185eed3e098fd3dc404ff66db8be1121a2a2610cf0814ec915e002304c9bccf",
        ),
        (
            0x004073F0usize,
            0x004073F4usize,
            "01d3abacb68aac3537badfe563d0e21d0244fb61db1d805bcef8f455407fad96",
        ),
        (
            0x00401300usize,
            0x00401303usize,
            "4bc724f3b1d0caf4fe369c18cba3102e6c4ea057f63fe1587e3973134a7f755e",
        ),
        (
            0x004236D0usize,
            0x004237AAusize,
            "b7310f8a5f626e36d69f8c2933271fd8e248f4cbd3091b4191e4df4019aaab6b",
        ),
        (
            0x00481110usize,
            0x0048114Fusize,
            "3ca057c85f6183d59bdc819003825d1cca7199a08b23718612fb2e4f38983a8e",
        ),
        (
            0x004294A0usize,
            0x00429895usize,
            "189764878265d1529a6045efcfb635239d380c1edb3a695f4aa9d641090b7233",
        ),
    ] {
        //Canonical BF09 .text raw offset=RVA, guarded by whole image/profile.
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x0040_0000..end - 0x0040_0000])
            ),
            hash
        );
    }
    for (address, method) in [
        (0x00520C80usize, 0x00424F50u32),
        (0x00520BCCusize, 0x004026B0u32),
        (0x00520DC4usize, 0x00424F50u32),
        (0x00520D10usize, 0x004026B0u32),
        (0x00527FDCusize, 0x00424F50u32),
        (0x00527F28usize, 0x004026B0u32),
        (0x00527154usize, 0x00427320u32),
        (0x005270A0usize, 0x004026B0u32),
        (0x00522838usize, 0x00427320u32),
        (0x00522784usize, 0x004026B0u32),
    ] {
        let offset = address - 0x0040_0000;
        assert_eq!(
            u32::from_le_bytes(exe[offset..offset + 4].try_into().unwrap()),
            method
        );
    }
    let raw = |address: usize, count| &exe[address - 0x0040_0000..address - 0x0040_0000 + count];
    assert_eq!(raw(0x00520cc4, 4), &0x3f66_6666u32.to_le_bytes());
    assert_eq!(raw(0x0051f198, 4), &[0, 0, 0, 0]);
    assert_eq!(
        raw(0x0042371f, 7),
        &[0xc6, 0x86, 0x60, 0x02, 0x00, 0x00, 0x01]
    );
    //Both selected and candidate calls pass fullDWORD1/0; no upper-byte guess.
    assert_eq!(raw(0x0042976e, 2), &[0x6a, 0x01]);
    assert_eq!(raw(0x0042978a, 2), &[0x6a, 0x00]);
}
