//! Complete original consumers and their real table/header units. The guard
//! supplies no native stack address or runtime/retail acceptance.
#[test]
fn complete_contact_normal_caller_and_authored_tables_are_guarded() {
    use sha2::{Digest, Sha256};
    let assets = super::tests::assets();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let exe = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    //Authenticated BF09 .text has raw=RVA. mapped() remains data/rdata.
    for (start, end, hash) in [
        (
            0x00481C10usize,
            0x00481E41usize,
            "9d102fb6143b61b1066a05f7e93eaa569a055287f82c6f98e7e40286fcb8211b",
        ),
        (
            0x00426270usize,
            0x0042672Eusize,
            "8638c9f07b39c27a7a3c77cbee0183001e9f2d72c03ee0b0fa4e137cf7ffbf50",
        ),
        (
            0x0047A2F0usize,
            0x0047A488usize,
            "e8b32026107432ba38f152e68395320f24603be9133dbefa90721771607f9684",
        ),
        (
            0x0042A7D0usize,
            0x0042A7FBusize,
            "104740bd49c557189a849560bc06db43a9f3b91884af1bf156868308d319d5fc",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[start - 0x400000..end - 0x400000])
            ),
            hash
        );
    }
    for (address, length, hash) in [
        (
            0x0051F198u32,
            4usize,
            "df3f619804a92fdb4057192dc43dd748ea778adc52bc498ce80524c014b81119",
        ),
        (
            0x00520A40u32,
            4usize,
            "ad286c64373644140ff927b256b8415467b5d27645ff997010b15519d022631b",
        ),
        (
            0x00520A70u32,
            4usize,
            "e00e5eb9444182f352323374ef4e08ebcb784725fdd4fd612d7730540b3e0c8c",
        ),
        (
            0x00525010u32,
            4usize,
            "0530f90fe451f20cf944361e7bb2fdfdc8af9cf3a616fe1d16ef8bd96bd892b6",
        ),
        (
            0x0052FE58u32,
            16usize,
            "f414188934668744cfc130aa8d0031b53754617659bb82fcf0cb64ea0cb85a48",
        ),
        (
            0x0052FE68u32,
            16usize,
            "1594ca3cbf1e30f72ec99a7f07d7459598670f6195f12d0ccfa2807b779ab049",
        ),
        (
            0x0052FE78u32,
            16usize,
            "d187fc20e962d0c43e1a6c4c30f19f394705ed5fc19f1ab08441a7fc42412598",
        ),
        (
            0x0052FE88u32,
            16usize,
            "6b395b3facda351a1ebb7eab83a68930d66a0e3b6ddd025667d32b69a58ffd0a",
        ),
        (
            0x0052FE98u32,
            16usize,
            "e4aca1796378c7ef9790afcb2c61672e027611d89180aa3209d30ff4e788e12d",
        ),
        (
            0x0052FEB8u32,
            16usize,
            "76f98377f1e8b0d6060db7cf0710f5563cbfbb8b8b63efb81860681903d4c98d",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(exe.mapped(address, length).unwrap().bytes)
            ),
            hash
        );
    }
    assert_eq!(exe.word(0x00527ee0 + 0x118).unwrap(), 0x00481c10);
    assert_eq!(exe.word(0x00527ee0 + 0xa8).unwrap(), 0x0042a760);
    assert_eq!(exe.word(0x0052273c + 0xa8).unwrap(), 0x0047bfd0);
    //Unsigned size at the native divide; signed scalar138 store; actual
    //fresh table+2 re-read after correction and signed address comparison.
    assert_eq!(
        &bytes[0x26414..0x2641a],
        &[0x33, 0xff, 0x66, 0x8b, 0x7e, 0x28]
    );
    assert_eq!(&bytes[0x2644a..0x26450], &[0x89, 0x96, 0x38, 0x01, 0, 0]);
    assert_eq!(&bytes[0x265a6..0x265aa], &[0x3b, 0xc1, 0x7d, 0x37]);
    assert_eq!(&bytes[0x265dd..0x265e1], &[0x0f, 0xbf, 0x5d, 0x02]);
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    let characteristics = u16::from_le_bytes(bytes[pe + 22..pe + 24].try_into().unwrap());
    assert_eq!(characteristics & 0x20, 0); //Original PE is not LargeAddressAware.
}
