//! Exact original-PE 004BCCE0 output with x87 control word 027F.
//! Synthetic inputs; no device calls or retail-frame claim.

use super::*;

#[test]
fn original_executable_case_0() {
    let input = [
        [
            0x420c0000, 0x41c80000, 0x3eb6920f, 0x394acfb8, 0xb2e0caf2, 0xfd61a039, 0x3f2c69cf,
            0xbe824622,
        ],
        [
            0x42700000, 0xc2640000, 0x3ef6db64, 0x3971165d, 0xb6e598fe, 0xcfddedae, 0x3f3f4a91,
            0xbebfb4cd,
        ],
        [
            0x42440000, 0x428a0000, 0x3ec2fcd4, 0x390fecd2, 0x1cd88380, 0x55d61aaf, 0xbf62f57d,
            0x3ffce1c0,
        ],
        [
            0xc1700000, 0x42bc0000, 0x3eec9cc2, 0x39057c4d, 0x72c845e4, 0x9d464972, 0x3fee5388,
            0x3fc2fcc8,
        ],
    ]
    .map(from_words);
    let state = D3dViewportState {
        bounds: [-7, -2, 50, 35],
        near_depth: 4294967295,
        mode: 0,
    };
    let expected: &[[u32; 8]] = &[
        [
            0x420c0000, 0x41c80000, 0x3eb6920f, 0x394acfb8, 0xb2e0caf2, 0xfd61a039, 0x3f2c69cf,
            0xbe824622,
        ],
        [
            0x422ced45, 0xc0000000, 0xc9576a0a, 0x39576a1a, 0xb3e2baf6, 0xee8ab960, 0x3f335eba,
            0xbe98e991,
        ],
        [
            0x42480000, 0xc0000000, 0xc94df256, 0x394df266, 0x8fe0a2db, 0xbeb7a28c, 0x3ee36c5a,
            0x3d861581,
        ],
        [
            0x42480000, 0x420c0000, 0xc92bfb5f, 0x392bfb6f, 0x4edb8da8, 0x80ce58a6, 0xbe29c3f8,
            0x3f78d691,
        ],
        [
            0x41de076c, 0x420c0000, 0xc940c391, 0x3940c3a1, 0xa9ddb7f0, 0xef5d9341, 0x3f4af257,
            0xbd9bb27b,
        ],
    ];
    assert_eq!(
        clip_polygon(&input, state)
            .unwrap()
            .into_iter()
            .map(words)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn original_executable_case_1() {
    let input = [
        [
            0xc1000000, 0xc2400000, 0x3f651a95, 0x39189b6f, 0x46bdfc89, 0xe3980ca0, 0xbef97edf,
            0x3faac675,
        ],
        [
            0x422c0000, 0xc2540000, 0x3f775313, 0x393deefc, 0x6a0b8c02, 0xa82d93a7, 0x3ffdac21,
            0x3e8da125,
        ],
        [
            0x427c0000, 0x42aa0000, 0x3f020561, 0x3948c710, 0x55675bf3, 0x2d8b9601, 0xbeee4bcb,
            0x3eb38d94,
        ],
        [
            0xc1800000, 0x40400000, 0x3e9e4dc8, 0x39a94b70, 0xa2272b4d, 0x9cf0b95f, 0xbe94d6bf,
            0x3f757f8a,
        ],
    ]
    .map(from_words);
    let state = D3dViewportState {
        bounds: [-8, 6, 34, 28],
        near_depth: 1,
        mode: 4294967295,
    };
    let expected: &[[u32; 8]] = &[
        [
            0x42080000, 0x41e00000, 0x3f7ff0f2, 0x3970dfc3, 0x723e5788, 0x6c94a145, 0x3e19a4ba,
            0x3f058b77,
        ],
        [
            0x41015da8, 0x41e00000, 0x3f7fed77, 0x3994496b, 0x8a3a3a80, 0x7ad1af43, 0xbeb01cc6,
            0x3f4605a8,
        ],
        [
            0xc1000000, 0x4134dc5b, 0x3f7febb6, 0x39a2511e, 0x9a2d305e, 0x91e5b656, 0xbe9de5d7,
            0x3f65ba9b,
        ],
        [
            0xc1000000, 0x40c00000, 0x3f7febd7, 0x39a1484c, 0x9a2a3355, 0x95e0b55c, 0xbe4c7a61,
            0x3f631671,
        ],
        [
            0x42080000, 0x40c00000, 0x3f7ff17b, 0x39684ad6, 0x72306363, 0x7d7e9e5f, 0x3f19d8ad,
            0x3ef52d88,
        ],
    ];
    assert_eq!(
        clip_polygon(&input, state)
            .unwrap()
            .into_iter()
            .map(words)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn original_executable_case_3() {
    let input = [
        [
            0x42240000, 0xc1400000, 0x3dbf5fea, 0x38e30294, 0x9aee7528, 0xd0924a65, 0xbf7d5fbd,
            0x3ea843db,
        ],
        [
            0xc1000000, 0x41100000, 0x3e5e1675, 0x3944fc68, 0xb3c4de41, 0x2c6bdc3f, 0x3fc23432,
            0x3f34eb3c,
        ],
        [
            0xc1d80000, 0x428c0000, 0x3e8cce6a, 0x3a42f7b9, 0x3b32f167, 0x3a6d0c28, 0xbe286106,
            0xbd716dda,
        ],
        [
            0xc2140000, 0x41c00000, 0x3eee42c2, 0x3939624d, 0x3b4c63ab, 0xb29d0131, 0x3fad0a78,
            0xbf315d8f,
        ],
    ]
    .map(from_words);
    let state = D3dViewportState {
        bounds: [7, -1, 41, 41],
        near_depth: 64,
        mode: 256,
    };
    let expected: &[[u32; 8]] = &[
        [
            0x41755555, 0xbf800000, 0x3f7d8b11, 0x391d3baf, 0xa7d8ac35, 0x7a7e9651, 0x3ea59847,
            0x3f06d484,
        ],
        [
            0x40e00000, 0x40249249, 0x3f7d5247, 0x392b6e3a, 0xabd1be39, 0x5e77af4b, 0x3f3ff1a8,
            0x3f174a58,
        ],
        [
            0x40e00000, 0x406c4ec4, 0x3f7dbca7, 0x3910d63e, 0x71a76d61, 0xc3972a4e, 0x3cfdac04,
            0xbdeed487,
        ],
        [
            0x41895555, 0xbf800000, 0x3f7de221, 0x390777d2, 0x7dbc6f50, 0xc7953455, 0xbe8c69f4,
            0x3c875f28,
        ],
    ];
    assert_eq!(
        clip_polygon(&input, state)
            .unwrap()
            .into_iter()
            .map(words)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn original_executable_case_5() {
    let input = [
        [
            0xc23c0000, 0x42640000, 0x3d851725, 0x3a0167d7, 0xa61cd955, 0x0f0a3877, 0x3f37903a,
            0xbf54a298,
        ],
        [
            0x42600000, 0x42a80000, 0x3b2285bc, 0x39adf084, 0xac7de26a, 0x451f4608, 0x3f2113df,
            0xbe9fa7ce,
        ],
        [
            0x421c0000, 0x42b20000, 0x3e730ac7, 0x3a5ae2ee, 0xeabe0e08, 0xc2f3cfa1, 0x3f59f6c9,
            0x3ec67263,
        ],
    ]
    .map(from_words);
    let state = D3dViewportState {
        bounds: [6, -6, 31, 38],
        near_depth: 0,
        mode: 4294967295,
    };
    let expected: &[[u32; 8]] = &[];
    assert_eq!(
        clip_polygon(&input, state)
            .unwrap()
            .into_iter()
            .map(words)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn original_executable_case_183() {
    let input = [
        [
            0x420c0000, 0xc0800000, 0x3f69669a, 0x39d16f47, 0x939b1d2e, 0x54708282, 0x3f3d9eb3,
            0x3ff830a6,
        ],
        [
            0xc1900000, 0xc0800000, 0x3eba71e4, 0x3dcd333d, 0xd89ac1f6, 0xa4b58b36, 0x3fc7b52c,
            0xbf33886b,
        ],
        [
            0x42be0000, 0xc1f00000, 0x3ecbf01f, 0x39400ab1, 0x62a21179, 0x01b6d4fd, 0x3e757ae0,
            0xbf42d40a,
        ],
        [
            0x40000000, 0x42940000, 0x3f63e323, 0x39c6a450, 0xa0f900bc, 0x8f2a6872, 0xbe67e966,
            0xbe554eee,
        ],
    ]
    .map(from_words);
    let state = D3dViewportState {
        bounds: [3, -9, 35, 33],
        near_depth: 1,
        mode: 16416,
    };
    let expected: &[[u32; 8]] = &[
        [
            0x420c0000, 0xc0800000, 0x3f69669a, 0x39d16f47, 0x939b1d2e, 0x54708282, 0x3f3d9eb3,
            0x3ff830a6,
        ],
        [
            0x40400000, 0xc0800000, 0x3f707901, 0x3d786fea, 0xbd9a80a7, 0x849a8754, 0x3fc76f19,
            0xbf31c4e1,
        ],
        [
            0x40400000, 0xc10d4f4b, 0x3f6b1bab, 0x3da722a4, 0xc29ca0df, 0x86b5995b, 0x3fc7a321,
            0xbf338a0e,
        ],
        [
            0x406ec4ec, 0xc1100000, 0x3f6b460f, 0x3da5cf8b, 0xc19c9fde, 0x85b5995c, 0x3fc7a25a,
            0xbf338a20,
        ],
        [
            0x420c0000, 0xc1100000, 0x3f743085, 0x3d3cf7b7, 0x9ea660b8, 0x59a9a995, 0x3fc73d61,
            0xbf33785d,
        ],
        [
            0x420c0000, 0x42040000, 0x3f7feb22, 0x39a6ef0e, 0x8bd40999, 0x5c5e8da0, 0xbca29587,
            0xbd29457f,
        ],
        [
            0x419ac4ec, 0x42040000, 0x3f7fe676, 0x39cc50a4, 0x99c80f72, 0x704f767a, 0x3e96dc9a,
            0x3f72dca8,
        ],
    ];
    assert_eq!(
        clip_polygon(&input, state)
            .unwrap()
            .into_iter()
            .map(words)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn original_executable_case_515() {
    let input = [
        [
            0xc0c00000, 0x40400000, 0x3ee71315, 0x39bd7f79, 0x0b1dce39, 0xcddb28c2, 0x3fca8172,
            0xbe9df1d4,
        ],
        [
            0x41500000, 0x41a00000, 0x3f2abc29, 0x39b6724c, 0xbbb13905, 0x32330dcc, 0x3feda620,
            0xbf59e060,
        ],
        [
            0x42c00000, 0x426c0000, 0x3eb94a7b, 0x38f9cdc1, 0xc7c56585, 0x9af1b87a, 0xbf210f5d,
            0x3f8388dd,
        ],
        [
            0x41400000, 0x42b20000, 0x3f56ce4d, 0x38f443a5, 0x265a0155, 0xb134e489, 0x3f0e912b,
            0x3fab38df,
        ],
    ]
    .map(from_words);
    let state = D3dViewportState {
        bounds: [-3, 10, 44, 57],
        near_depth: 4294967295,
        mode: 256,
    };
    let expected: &[[u32; 8]] = &[
        [
            0x3fe96969, 0x41200000, 0xc9ba9822, 0x39ba982a, 0x535a9124, 0x8d961dc6, 0x3fd8f9f6,
            0xbf082b0c,
        ],
        [
            0x41500000, 0x41a00000, 0x3f2abc29, 0x39b6724c, 0xbbb13905, 0x32330dcc, 0x3feda620,
            0xbf59e060,
        ],
        [
            0x42300000, 0x420a43db, 0xc989a0f5, 0x3989a0fd, 0xbfb84a35, 0x597a4dad, 0x3f6d9f3d,
            0xbe18fc0d,
        ],
        [
            0x42300000, 0x42640000, 0xc935abcb, 0x3935abdb, 0x687d5761, 0xaba6a690, 0x3e965f68,
            0x3f5142c0,
        ],
        [
            0x40a9aca7, 0x42640000, 0xc959b587, 0x3959b597, 0x1c434d4a, 0xbc729e9e, 0x3f703882,
            0x3f39a35c,
        ],
        [
            0xc0400000, 0x418aaaaa, 0xc9a817af, 0x39a817b7, 0x0f27ac3d, 0xc9bf47b8, 0x3fb4a2a3,
            0xbd0bc78f,
        ],
        [
            0xc0400000, 0x41200000, 0xc9b4dddc, 0x39b4dde4, 0x1e30b235, 0xbcc031bf, 0x3fc5e570,
            0xbe858405,
        ],
    ];
    assert_eq!(
        clip_polygon(&input, state)
            .unwrap()
            .into_iter()
            .map(words)
            .collect::<Vec<_>>(),
        expected
    );
}
