//! Exact output from all eight native alpha-capable32-bit upload kernels.
//! Synthetic descriptors; every pixel of the uploaded rectangle is retained.

use super::*;

#[test]
fn original_executable_kernel_0() {
    let (descriptor, source) = descriptor(
        2,
        3,
        0,
        &[3, 0, 13, 9, 12, 9],
        &[
            0, 32768, 65535, 32767, 1494, 30349, 48424, 54692, 33069, 16791, 12636, 62131, 49518,
            27545, 48090, 30564,
        ],
    );
    let state = NativeTextureUploadState {
        source_rgb_shifts: [10, 5, 0],
        global_policy: 8,
    };
    let texture = upload(&descriptor, &source, state, TextureAllocationId(0)).unwrap();
    let expected = [
        0xfffffff8, 0xfffffff8, 0xff000000, 0xff000000, 0xfffffff8, 0xfffffff8, 0xff000000,
        0xff000000, 0xffd7e6c8, 0xffd7e6c8, 0xff8365b8, 0xff8365b8, 0xff825b70, 0xff825b70,
        0xff8365b8, 0xff8365b8, 0xff825b70, 0xff825b70, 0xff8365b8, 0xff8365b8,
    ];
    let mut actual = Vec::new();
    for y in 0..u32::from(descriptor.height) + 2 {
        for x in 0..u32::from(descriptor.width) + 2 {
            let [r, g, b, a] = texture.texel(x, y).unwrap();
            actual.push(u32::from_le_bytes([b, g, r, a]));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn original_executable_kernel_1() {
    let (descriptor, source) = descriptor(
        2,
        2,
        1,
        &[10, 9, 0, 11],
        &[
            0, 32768, 65535, 32767, 37310, 13729, 2084, 14325, 38176, 41277, 45642, 58671, 54110,
            43733, 23245, 32182,
        ],
    );
    let state = NativeTextureUploadState {
        source_rgb_shifts: [7, 0, 11],
        global_policy: 8,
    };
    let texture = upload(&descriptor, &source, state, TextureAllocationId(0)).unwrap();
    let expected = [
        0xff2450b2, 0xff2450b2, 0xff13e8a1, 0xff13e8a1, 0xff2450b2, 0xff2450b2, 0xff13e8a1,
        0xff13e8a1, 0x00000000, 0x00000000, 0xff5278e5, 0xff5278e5, 0x00000000, 0x00000000,
        0xff5278e5, 0xff5278e5,
    ];
    let mut actual = Vec::new();
    for y in 0..u32::from(descriptor.height) + 2 {
        for x in 0..u32::from(descriptor.width) + 2 {
            let [r, g, b, a] = texture.texel(x, y).unwrap();
            actual.push(u32::from_le_bytes([b, g, r, a]));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn original_executable_kernel_2() {
    let (descriptor, source) = descriptor(
        1,
        3,
        8,
        &[3, 0, 2],
        &[
            0, 32768, 65535, 32767, 27023, 63822, 53915, 16250, 59889, 53268, 12688, 10024, 55997,
            45874, 37271, 31050,
        ],
    );
    let state = NativeTextureUploadState {
        source_rgb_shifts: [1, 9, 18],
        global_policy: 8,
    };
    let texture = upload(&descriptor, &source, state, TextureAllocationId(0)).unwrap();
    let expected = [
        0xfff8ff01, 0xfff8ff01, 0xfff8ff01, 0xfff8ff01, 0xfff8ff01, 0xfff8ff01, 0xff000000,
        0xff000000, 0xff000000, 0xfff8ff03, 0xfff8ff03, 0xfff8ff03, 0xfff8ff03, 0xfff8ff03,
        0xfff8ff03,
    ];
    let mut actual = Vec::new();
    for y in 0..u32::from(descriptor.height) + 2 {
        for x in 0..u32::from(descriptor.width) + 2 {
            let [r, g, b, a] = texture.texel(x, y).unwrap();
            actual.push(u32::from_le_bytes([b, g, r, a]));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn original_executable_kernel_3() {
    let (descriptor, source) = descriptor(
        2,
        3,
        9,
        &[3, 3, 9, 8, 2, 10],
        &[
            0, 32768, 65535, 32767, 46296, 5338, 31894, 51314, 54045, 54890, 44404, 46136, 1384,
            19199, 15864, 8238,
        ],
    );
    let state = NativeTextureUploadState {
        source_rgb_shifts: [27, 14, 20],
        global_policy: 8,
    };
    let texture = upload(&descriptor, &source, state, TextureAllocationId(0)).unwrap();
    let expected = [
        0xff001f00, 0xff001f00, 0xff001f00, 0xff001f00, 0xff001f00, 0xff001f00, 0xff001f00,
        0xff001f00, 0xff003500, 0xff003500, 0xff003400, 0xff003400, 0xff003f00, 0xff003f00,
        0xff002b00, 0xff002b00, 0xff003f00, 0xff003f00, 0xff002b00, 0xff002b00,
    ];
    let mut actual = Vec::new();
    for y in 0..u32::from(descriptor.height) + 2 {
        for x in 0..u32::from(descriptor.width) + 2 {
            let [r, g, b, a] = texture.texel(x, y).unwrap();
            actual.push(u32::from_le_bytes([b, g, r, a]));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn original_executable_kernel_4() {
    let (descriptor, source) = descriptor(
        3,
        2,
        2,
        &[62751, 59171, 32768, 32767, 32768, 62568],
        &[
            0, 32768, 65535, 32767, 57261, 51852, 37349, 62751, 20933, 59171, 62568, 17173, 28971,
            9376, 40069, 35054,
        ],
    );
    let state = NativeTextureUploadState {
        source_rgb_shifts: [10, 5, 0],
        global_policy: 8,
    };
    let texture = upload(&descriptor, &source, state, TextureAllocationId(0)).unwrap();
    let expected = [
        0xffea47f8, 0xffea47f8, 0xffcec818, 0xff000000, 0xff000000, 0xffea47f8, 0xffea47f8,
        0xffcec818, 0xff000000, 0xff000000, 0xfffffff8, 0xfffffff8, 0xff000000, 0xffe81a40,
        0xffe81a40, 0xfffffff8, 0xfffffff8, 0xff000000, 0xffe81a40, 0xffe81a40,
    ];
    let mut actual = Vec::new();
    for y in 0..u32::from(descriptor.height) + 2 {
        for x in 0..u32::from(descriptor.width) + 2 {
            let [r, g, b, a] = texture.texel(x, y).unwrap();
            actual.push(u32::from_le_bytes([b, g, r, a]));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn original_executable_kernel_5() {
    let (descriptor, source) = descriptor(
        1,
        2,
        3,
        &[28902, 54967],
        &[
            0, 32768, 65535, 32767, 905, 41939, 41778, 20661, 2082, 28902, 59933, 20615, 54967,
            6044, 60656, 43804,
        ],
    );
    let state = NativeTextureUploadState {
        source_rgb_shifts: [10, 5, 0],
        global_policy: 8,
    };
    let texture = upload(&descriptor, &source, state, TextureAllocationId(0)).unwrap();
    let expected = [
        0xffe13930, 0xffe13930, 0xffe13930, 0xffe13930, 0xffe13930, 0xffe13930, 0xffadadb8,
        0xffadadb8, 0xffadadb8, 0xffadadb8, 0xffadadb8, 0xffadadb8,
    ];
    let mut actual = Vec::new();
    for y in 0..u32::from(descriptor.height) + 2 {
        for x in 0..u32::from(descriptor.width) + 2 {
            let [r, g, b, a] = texture.texel(x, y).unwrap();
            actual.push(u32::from_le_bytes([b, g, r, a]));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn original_executable_kernel_6() {
    let (descriptor, source) = descriptor(
        3,
        2,
        10,
        &[8369, 64178, 65535, 64178, 65416, 30385],
        &[
            0, 32768, 65535, 32767, 30558, 37135, 65416, 30385, 19603, 13342, 19612, 33655, 8369,
            64178, 44549, 19225,
        ],
    );
    let state = NativeTextureUploadState {
        source_rgb_shifts: [10, 5, 0],
        global_policy: 8,
    };
    let texture = upload(&descriptor, &source, state, TextureAllocationId(0)).unwrap();
    let expected = [
        0xff825810, 0xff825810, 0xffea5920, 0xfffffff0, 0xfffffff0, 0xff825810, 0xff825810,
        0xffea5920, 0xfffffff0, 0xfffffff0, 0xffea5920, 0xffea5920, 0xfffec480, 0xffda5810,
        0xffda5810, 0xffea5920, 0xffea5920, 0xfffec480, 0xffda5810, 0xffda5810,
    ];
    let mut actual = Vec::new();
    for y in 0..u32::from(descriptor.height) + 2 {
        for x in 0..u32::from(descriptor.width) + 2 {
            let [r, g, b, a] = texture.texel(x, y).unwrap();
            actual.push(u32::from_le_bytes([b, g, r, a]));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn original_executable_kernel_7() {
    let (descriptor, source) = descriptor(
        1,
        1,
        11,
        &[58114],
        &[
            0, 32768, 65535, 32767, 61483, 7648, 61120, 58577, 37128, 32847, 1368, 58114, 24435,
            38080, 52589, 1505,
        ],
    );
    let state = NativeTextureUploadState {
        source_rgb_shifts: [10, 5, 0],
        global_policy: 8,
    };
    let texture = upload(&descriptor, &source, state, TextureAllocationId(0)).unwrap();
    let expected = [
        0xff8c8120, 0xff8c8120, 0xff8c8120, 0xff8c8120, 0xff8c8120, 0xff8c8120, 0xff8c8120,
        0xff8c8120, 0xff8c8120,
    ];
    let mut actual = Vec::new();
    for y in 0..u32::from(descriptor.height) + 2 {
        for x in 0..u32::from(descriptor.width) + 2 {
            let [r, g, b, a] = texture.texel(x, y).unwrap();
            actual.push(u32::from_le_bytes([b, g, r, a]));
        }
    }
    assert_eq!(actual, expected);
}
