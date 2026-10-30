//! The ray table's sphere and box callbacks against Infest01's calls (the
//! `raycalls` and `rayhits` queries): the request's offset, length and ray
//! basis, the vertex and its radius or half extents, and the `+10` each hit
//! recorded.

use super::*;

struct Sphere {
    offset: [i32; 3],
    length: i32,
    basis: [u32; 10],
    vertex: [i32; 3],
    radius: i32,
    hit: Option<i32>,
}

struct Box {
    offset: [i32; 3],
    length: i32,
    basis: [u32; 10],
    vertex: [i32; 3],
    half: [i32; 3],
    hit: Option<i32>,
}

fn ray(length: i32, words: [u32; 10]) -> RayRequest {
    RayRequest {
        basis: NativeBasis { words },
        length,
    }
}

const SPHERES: [Sphere; 12] = [
    Sphere {
        offset: [142, -113, 3451],
        length: 2999,
        basis: [
            0x0, 0xbf7ffd78, 0x0, 0x3c101095, 0xb96fcb26, 0x3f7fe9d5, 0xbcd50bb7, 0xbc0e0937,
            0xbcd50dd2, 0xbf7fe75e,
        ],
        vertex: [0, 0, 0],
        radius: 504,
        hit: Some(18),
    },
    Sphere {
        offset: [135, -259, 3319],
        length: 2998,
        basis: [
            0x0, 0xbf7ffc38, 0x0, 0x3c300867, 0x393f9ce3, 0x3f7ff686, 0x3c8b521e, 0xbc2ed810,
            0x3c8b542d, 0xbf7ff2ca,
        ],
        vertex: [0, 0, 0],
        radius: 504,
        hit: Some(89),
    },
    Sphere {
        offset: [132, -361, 3184],
        length: 2998,
        basis: [
            0x0, 0xbf7ffb7e, 0x0, 0x3c4038b0, 0x3a17c8d6, 0x3f7fb025, 0x3d4a21e0, 0xbc3f382b,
            0x3d4a256f, 0xbf7fabac,
        ],
        vertex: [0, 0, 0],
        radius: 504,
        hit: Some(139),
    },
    Sphere {
        offset: [132, -361, 3184],
        length: 2998,
        basis: [
            0x0, 0xbf7ffb7e, 0x0, 0x3c4038b0, 0x3a17c8d6, 0x3f7fb025, 0x3d4a21e0, 0xbc3f382b,
            0x3d4a256f, 0xbf7fabac,
        ],
        vertex: [-7, 301, -96],
        radius: 168,
        hit: Some(4),
    },
    Sphere {
        offset: [133, -375, 3057],
        length: 2998,
        basis: [
            0x0, 0xbf7ffb7c, 0x0, 0x3c4050c0, 0x3a2c6085, 0x3f7f9915, 0x3d657179, 0xbc3f3744,
            0x3d657585, 0xbf7f949d,
        ],
        vertex: [0, 0, 0],
        radius: 504,
        hit: Some(250),
    },
    Sphere {
        offset: [133, -375, 3057],
        length: 2998,
        basis: [
            0x0, 0xbf7ffb7c, 0x0, 0x3c4050c0, 0x3a2c6085, 0x3f7f9915, 0x3d657179, 0xbc3f3744,
            0x3d657585, 0xbf7f949d,
        ],
        vertex: [-7, 302, -94],
        radius: 168,
        hit: Some(118),
    },
    Sphere {
        offset: [133, -308, 2939],
        length: 2999,
        basis: [
            0x0, 0xbf7ffb7e, 0x0, 0x3c4020a5, 0x39db58a9, 0x3f7fd647, 0x3d121fb0, 0xbc3f3402,
            0x3d122243, 0xbf7fd1d0,
        ],
        vertex: [0, 0, 0],
        radius: 504,
        hit: Some(436),
    },
    Sphere {
        offset: [133, -308, 2939],
        length: 2999,
        basis: [
            0x0, 0xbf7ffb7e, 0x0, 0x3c4020a5, 0x39db58a9, 0x3f7fd647, 0x3d121fb0, 0xbc3f3402,
            0x3d122243, 0xbf7fd1d0,
        ],
        vertex: [-7, 300, -100],
        radius: 168,
        hit: Some(270),
    },
    Sphere {
        offset: [388, -249, 2824],
        length: 2998,
        basis: [
            0x0, 0xbf7ffaeb, 0x0, 0x3c4c08b4, 0x390b5c90, 0x3f7ffc44, 0x3c2ed7a8, 0xbc4a2d5f,
            0x3c2edb21, 0xbf7ff747,
        ],
        vertex: [0, 0, 0],
        radius: 504,
        hit: Some(377),
    },
    Sphere {
        offset: [132, -233, 2821],
        length: 2998,
        basis: [
            0x0, 0xbf7ffaeb, 0x0, 0x3c4c08b4, 0x390b5c90, 0x3f7ffc44, 0x3c2ed7a8, 0xbc4a2d5f,
            0x3c2edb21, 0xbf7ff747,
        ],
        vertex: [0, 0, 0],
        radius: 504,
        hit: Some(603),
    },
    Sphere {
        offset: [132, -233, 2821],
        length: 2998,
        basis: [
            0x0, 0xbf7ffaeb, 0x0, 0x3c4c08b4, 0x390b5c90, 0x3f7ffc44, 0x3c2ed7a8, 0xbc4a2d5f,
            0x3c2edb21, 0xbf7ff747,
        ],
        vertex: [-7, 297, -108],
        radius: 168,
        hit: Some(377),
    },
    Sphere {
        offset: [264, -165, 2691],
        length: 2998,
        basis: [
            0x0, 0xbf7fd49e, 0x0, 0xbd1502b1, 0x3a059715, 0x3f7ff991, 0xbc655b1e, 0x3d14e82a,
            0xbc658203, 0xbf7fce3e,
        ],
        vertex: [0, 0, 0],
        radius: 504,
        hit: Some(703),
    },
];
const BOXES: [Box; 25] = [
    Box {
        offset: [-84, -3, 3459],
        length: 2999,
        basis: [
            0x0, 0x3e6f0cb0, 0x0, 0xbf78ed36, 0x3c8cc244, 0x3f7ff5c4, 0x3b872c7a, 0x3f78e3a1,
            0xbc90c233, 0x3e6efcfd,
        ],
        vertex: [0, 245, -4],
        half: [105, 280, 105],
        hit: None,
    },
    Box {
        offset: [-84, -23, 3377],
        length: 2999,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3c3f3d00, 0x3f7ffb48, 0x3b379aa7, 0x3f78e8fd,
            0xbc44ab73, 0x3e6f0223,
        ],
        vertex: [0, 245, -3],
        half: [105, 280, 105],
        hit: None,
    },
    Box {
        offset: [-85, -46, 3298],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3ba9ffd9, 0x3f7fff11, 0x3aa3368c, 0x3f78ecac,
            0xbbaed3e0, 0x3e6f05ac,
        ],
        vertex: [0, 245, -1],
        half: [105, 280, 105],
        hit: Some(2223),
    },
    Box {
        offset: [-85, -67, 3220],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xba7f00ab, 0x3f7ffff8, 0xb974d2af, 0x3f78ed8c,
            0x3a831f5e, 0x3e6f0683,
        ],
        vertex: [0, 245, 0],
        half: [105, 280, 105],
        hit: Some(2245),
    },
    Box {
        offset: [-85, -86, 3163],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbbdf1f36, 0x3f7ffe66, 0xbad63708, 0x3f78ec05,
            0x3be5757b, 0x3e6f050b,
        ],
        vertex: [0, 245, 2],
        half: [105, 280, 105],
        hit: Some(2261),
    },
    Box {
        offset: [-85, -104, 3126],
        length: 2999,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbc49dc68, 0x3f7ffabe, 0xbb41cd80, 0x3f78e877,
            0x3c4f9817, 0x3e6f01a2,
        ],
        vertex: [0, 245, 3],
        half: [105, 280, 105],
        hit: Some(2274),
    },
    Box {
        offset: [-86, -119, 3110],
        length: 2999,
        basis: [
            0x0, 0x3e6f0cb0, 0x0, 0xbf78ed36, 0xbc8cc244, 0x3f7ff5c4, 0xbb872c7a, 0x3f78e3a1,
            0x3c90c233, 0x3e6efcfd,
        ],
        vertex: [0, 245, 4],
        half: [105, 280, 105],
        hit: Some(2280),
    },
    Box {
        offset: [-86, -112, 3111],
        length: 2999,
        basis: [
            0x0, 0x3e6f0cb0, 0x0, 0xbf78ed36, 0xbc745874, 0x3f7ff84b, 0xbb6aa677, 0x3f78e616,
            0x3c7b49eb, 0x3e6eff59,
        ],
        vertex: [0, 245, 4],
        half: [105, 280, 105],
        hit: Some(2278),
    },
    Box {
        offset: [-87, -97, 3100],
        length: 2999,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbc29fdfe, 0x3f7ffc45, 0xbb2334c4, 0x3f78e9f4,
            0x3c2ed1f7, 0x3e6f030f,
        ],
        vertex: [0, 245, 3],
        half: [105, 280, 105],
        hit: Some(2277),
    },
    Box {
        offset: [-87, -85, 3077],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbbd47f5f, 0x3f7ffe8c, 0xbacc03c4, 0x3f78ec2a,
            0x3bda8865, 0x3e6f052f,
        ],
        vertex: [0, 245, 2],
        half: [105, 280, 105],
        hit: Some(2279),
    },
    Box {
        offset: [-87, -72, 3055],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbb2a0050, 0x3f7fffc5, 0xba2336fe, 0x3f78ed5b,
            0x3b2ed45a, 0x3e6f0653,
        ],
        vertex: [0, 245, 1],
        half: [105, 280, 105],
        hit: Some(2281),
    },
    Box {
        offset: [-88, -63, 3032],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x39aa0077, 0x3f800000, 0x38a33724, 0x3f78ed94,
            0xb9aed482, 0x3e6f068a,
        ],
        vertex: [0, 245, 0],
        half: [105, 280, 105],
        hit: Some(2284),
    },
    Box {
        offset: [-88, -53, 3010],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3b69c03d, 0x3f7fff90, 0x3a606b6f, 0x3f78ed27,
            0xbb7063ca, 0x3e6f0622,
        ],
        vertex: [0, 245, -1],
        half: [105, 280, 105],
        hit: Some(2286),
    },
    Box {
        offset: [-89, -44, 2977],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3bd47f5f, 0x3f7ffe8c, 0x3acc03c4, 0x3f78ec2a,
            0xbbda8865, 0x3e6f052f,
        ],
        vertex: [0, 245, -2],
        half: [105, 280, 105],
        hit: Some(2291),
    },
    Box {
        offset: [-89, -36, 2945],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3c1a0e94, 0x3f7ffcf0, 0x3b13e834, 0x3f78ea9a,
            0xbc1e6eb1, 0x3e6f03af,
        ],
        vertex: [0, 245, -2],
        half: [105, 280, 105],
        hit: Some(2296),
    },
    Box {
        offset: [-90, -39, 2918],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3c0a1f0d, 0x3f7ffd8a, 0x3b049b88, 0x3f78eb30,
            0xbc0e0b4d, 0x3e6f043f,
        ],
        vertex: [0, 245, -2],
        half: [105, 280, 105],
        hit: Some(2302),
    },
    Box {
        offset: [-84, -33, 2894],
        length: 2999,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3bbf2fc6, 0x3f7ffed3, 0x3ab78df4, 0x3f78eda0,
            0xbbc49dd9, 0x3e6ef19c,
        ],
        vertex: [0, 245, -1],
        half: [105, 280, 105],
        hit: Some(2310),
    },
    Box {
        offset: [-81, -33, 2876],
        length: 2999,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3b546ea6, 0x3f7fffa4, 0x3a4bf3b6, 0x3f78ee6b,
            0xbb5a7732, 0x3e6ef25f,
        ],
        vertex: [0, 245, -1],
        half: [105, 280, 105],
        hit: Some(2315),
    },
    Box {
        offset: [-79, -39, 2863],
        length: 2999,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0x3a7eeb83, 0x3f7ffff8, 0x3974be5f, 0x3f78eebd,
            0xba83147d, 0x3e6ef2ae,
        ],
        vertex: [0, 245, 0],
        half: [105, 280, 105],
        hit: Some(2320),
    },
    Box {
        offset: [-78, -46, 2852],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xba7f00ab, 0x3f7ffff8, 0xb974d2af, 0x3f78ed8c,
            0x3a831f5e, 0x3e6f0683,
        ],
        vertex: [0, 245, 0],
        half: [105, 280, 105],
        hit: Some(2323),
    },
    Box {
        offset: [-76, -52, 2844],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbb3f404e, 0x3f7fffb5, 0xba379dd3, 0x3f78ed4b,
            0x3b44aed9, 0x3e6f0645,
        ],
        vertex: [0, 245, 1],
        half: [105, 280, 105],
        hit: Some(346),
    },
    Box {
        offset: [-75, -56, 2839],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbb94bffe, 0x3f7fff4a, 0xba8ecfda, 0x3f78ece3,
            0x3b98f985, 0x3e6f05e1,
        ],
        vertex: [0, 245, 1],
        half: [105, 280, 105],
        hit: Some(352),
    },
    Box {
        offset: [-73, -63, 2834],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbbd47f5f, 0x3f7ffe8c, 0xbacc03c4, 0x3f78ec2a,
            0x3bda8865, 0x3e6f052f,
        ],
        vertex: [0, 245, 2],
        half: [105, 280, 105],
        hit: Some(356),
    },
    Box {
        offset: [-73, -67, 2831],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbbfefe9c, 0x3f7ffde7, 0xbaf4d0b5, 0x3f78eb8a,
            0x3c031e4f, 0x3e6f0496,
        ],
        vertex: [0, 245, 2],
        half: [105, 280, 105],
        hit: Some(359),
    },
    Box {
        offset: [-73, -71, 2828],
        length: 2998,
        basis: [
            0x0, 0x3e6efe29, 0x0, 0xbf78ee16, 0xbc1a0e94, 0x3f7ffcf0, 0xbb13e834, 0x3f78ea9a,
            0x3c1e6eb1, 0x3e6f03af,
        ],
        vertex: [0, 245, 2],
        half: [105, 280, 105],
        hit: Some(363),
    },
];

#[test]
fn sphere_hits_match_infest01() {
    for (index, case) in SPHERES.iter().enumerate() {
        assert_eq!(
            sphere(
                ray(case.length, case.basis),
                case.offset,
                case.vertex,
                case.radius
            ),
            case.hit,
            "sphere {index}"
        );
    }
}

#[test]
fn box_hits_and_misses_match_infest01() {
    for (index, case) in BOXES.iter().enumerate() {
        assert_eq!(
            box_hit(
                ray(case.length, case.basis),
                case.offset,
                case.vertex,
                case.half
            ),
            case.hit,
            "box {index}"
        );
    }
}

/// The sphere's rejections: wholly behind, wholly past the length, and
/// beside the ray.
#[test]
fn spheres_off_the_ray_miss() {
    let ray = ray(3000, [0; 10]);
    assert_eq!(sphere(ray, [0, 0, -200], [0; 3], 100), None);
    assert_eq!(sphere(ray, [0, 0, 3200], [0; 3], 100), None);
    assert_eq!(sphere(ray, [100, 1, 1000], [0; 3], 100), None);
    // Grazing at the origin clamps the entry to 0: the whole length.
    assert_eq!(sphere(ray, [0, 0, 50], [0; 3], 100), Some(3000));
}
