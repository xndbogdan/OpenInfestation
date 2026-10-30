//! The direct planet background (004737A0), as the briefing and the results
//! page draw it, and the briefing's milestone sprites (0040B0F0). Original H3
//! resources remain the pixel source; this module owns only the native
//! screen arithmetic, descriptor words, and source H6 color bytes.

use inf_formats::native_basis::sine_q31;

use crate::{
    briefing_scene::BriefingSceneFrame,
    menu::MenuBasis,
    menu_basis::{MenuBasisMath, RotationPlane},
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
};

fn narrow(points: [[i32; 2]; 4]) -> [[i16; 2]; 4] {
    points.map(|point| point.map(|value| value as i16))
}

/// 00490A80 multiplies input signed WORDs by each stored basis ROW, then
/// FISTP-nearest QWORD and copies the low WORD. This differs from 00490900's
/// local-to-world COLUMN transform; the native transpose is deliberate.
fn rotated_radius(radius: u16, age_millis: i32, math: &MenuBasisMath) -> [i16; 2] {
    let mut basis = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut basis, age_millis, RotationPlane::Xz);
    let radius = f64::from(radius as i16);
    [1, 7].map(|row| {
        let x = f64::from(f32::from_bits(basis.words[row]));
        let z = f64::from(f32::from_bits(basis.words[row + 2]));
        ((radius * x + 0.0) + radius * z).round_ties_even() as i64 as i16
    })
}

/// The briefing's packed center is (-width/8,height/2), and its radius WORD
/// is width*7/10, turned by the screen's age.
pub fn briefing_background_quads(
    frame: BriefingSceneFrame,
    style_word: u32,
    math: &MenuBasisMath,
) -> [DirectSpriteQuad; 5] {
    let [width, height] = frame.projection.viewport;
    background_quads(
        [(-(width >> 3)) as i16, (height >> 1) as i16],
        (width.wrapping_mul(7) / 10) as u16,
        frame.age_millis,
        frame.projection.focal,
        style_word,
        math,
    )
}

/// 004737A0 reads only its packed center, radius WORD and age (arguments 1,
/// 2 and 4). The first quad receives quarter H6/59 RGB and +A4; the four
/// wedge quads share H3/670 and the ordinary +9C consumer with no color read.
pub fn background_quads(
    center: [i16; 2],
    radius: u16,
    age_millis: i32,
    focal: [i32; 2],
    style_word: u32,
    math: &MenuBasisMath,
) -> [DirectSpriteQuad; 5] {
    let [cx, cy] = center.map(i32::from);
    let [a, b] = rotated_radius(radius, age_millis, math).map(i32::from);
    let [fx, fy] = focal;
    let projected = |value: i32| value.wrapping_mul(fx) / fy;
    let a23 = a.wrapping_mul(23) / 32;
    let b23 = b.wrapping_mul(23) / 32;
    let mut bytes = style_word.to_le_bytes();
    bytes[..3].iter_mut().for_each(|byte| *byte >>= 2);
    let effect = DirectSpriteQuad {
        sprite_slot: 0x5b7,
        corners: narrow([
            [cx.wrapping_sub(projected(a23)), cy.wrapping_sub(b23)],
            [cx.wrapping_add(projected(b23)), cy.wrapping_sub(a23)],
            [cx.wrapping_add(projected(a23)), cy.wrapping_add(b23)],
            [cx.wrapping_sub(projected(b23)), cy.wrapping_add(a23)],
        ]),
        vertex_depths: [100_000; 4],
        descriptor_word: 0,
        mode: DirectSpriteMode::Effect {
            color_word: u32::from_le_bytes(bytes),
        },
    };
    let pa = projected(a);
    let pb = projected(b);
    let sum_projection = pa.wrapping_add(pb) / 2;
    let difference_projection = pb.wrapping_sub(pa) / 2;
    let sum = a.wrapping_add(b) / 2;
    let difference = b.wrapping_sub(a) / 2;
    let outer = [
        [-sum_projection, -difference],
        [-pa, -b],
        [difference_projection, -sum],
        [pb, -a],
        [sum_projection, difference],
        [pa, b],
        [-difference_projection, sum],
        [-pb, a],
    ];
    let position = |point: [i32; 2]| [cx.wrapping_add(point[0]), cy.wrapping_add(point[1])];
    let wedge = |first: usize| DirectSpriteQuad {
        sprite_slot: 0x29e,
        corners: narrow([
            position(outer[first]),
            position(outer[first + 1]),
            position(outer[(first + 2) % 8]),
            [cx, cy],
        ]),
        vertex_depths: [100_000; 4],
        descriptor_word: 0,
        mode: DirectSpriteMode::Textured,
    };
    [effect, wedge(0), wedge(2), wedge(4), wedge(6)]
}

fn multiply_q31(value: i32, fraction: i32) -> i32 {
    let product = i64::from(value) * i64::from(fraction);
    ((product >> 32) as i32).wrapping_shl(1) | i32::from((product as i32) < 0)
}

/// H14+24 is the milestone count; H14+28 is a different briefing-record count.
/// A Start timer suppresses all progress sprites. Native signed count and age
/// comparisons, shift/division units and high color byte remain explicit.
pub fn briefing_progress_quads(
    frame: BriefingSceneFrame,
    milestone_count: u32,
    style_word: u32,
    sine_quarter: &[u16; 4096],
) -> Vec<DirectSpriteQuad> {
    if frame.start_remaining_millis != 0 || (milestone_count as i32) <= 0 {
        return Vec::new();
    }
    let [width, height] = frame.projection.viewport;
    let half_width = (width.wrapping_mul(10) / 256) / 2;
    let half_height = (height.wrapping_mul(14) / 256) / 2;
    let y = height.wrapping_mul(140) / 256;
    let mut output = Vec::new();
    for index in 0..milestone_count {
        if frame.age_millis < 2000i32.wrapping_add((index as i32).wrapping_mul(500)) {
            break;
        }
        let x = width.wrapping_mul(35i32.wrapping_add((index as i32).wrapping_mul(10))) / 256;
        let angle = frame
            .age_millis
            .wrapping_add((index as i32).wrapping_mul(100))
            .wrapping_mul(50);
        let fraction = sine_q31(angle, sine_quarter);
        let mut bytes = style_word.to_le_bytes();
        bytes[..3].iter_mut().for_each(|byte| {
            *byte = (*byte >> 1).wrapping_add(multiply_q31(i32::from(*byte >> 2), fraction) as u8);
        });
        output.push(DirectSpriteQuad {
            sprite_slot: 0x2ad,
            corners: narrow([
                [x.wrapping_sub(half_width), y.wrapping_sub(half_height)],
                [x.wrapping_add(half_width), y.wrapping_sub(half_height)],
                [x.wrapping_add(half_width), y.wrapping_add(half_height)],
                [x.wrapping_sub(half_width), y.wrapping_add(half_height)],
            ]),
            vertex_depths: [0x4000; 4],
            descriptor_word: 0,
            mode: DirectSpriteMode::Effect {
                color_word: u32::from_le_bytes(bytes),
            },
        });
    }
    output
}

#[cfg(test)]
#[path = "briefing_sprites_tests.rs"]
mod tests;
