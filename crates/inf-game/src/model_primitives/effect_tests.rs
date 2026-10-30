use super::*;
use crate::model_context::ModelProjection;

fn projection() -> ModelProjection {
    ModelProjection {
        focal: [320, 240],
        viewport: [640, 480],
        center: [320, 240],
        fade: [100, 200],
    }
}

fn tables() -> PrimitiveTables {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let executable = std::fs::read(root.join("Infest.exe")).unwrap();
    let mapped = inf_formats::FrontendExecutable::parse(&executable).unwrap();
    PrimitiveTables::from_executable(mapped).unwrap()
}

#[test]
fn original_sine_cardinals_produce_authored_rectangles_in_corner_order() {
    let tables = tables();
    let corners = expanded_sprite_corners(
        pack_xy(320, 240),
        256,
        [512, 0],
        [64, 32],
        projection().focal,
        &tables.sine_table,
    );
    assert_eq!(
        corners.map(|xy| [xy as i16, (xy >> 16) as i16]),
        [[-585, -99], [1225, -99], [1225, 579], [-585, 579],]
    );
    let rotated = expanded_sprite_corners(
        pack_xy(320, 240),
        256,
        [512, 16384],
        [64, 32],
        projection().focal,
        &tables.sine_table,
    );
    assert_eq!(
        rotated.map(|xy| [xy as i16, (xy >> 16) as i16]),
        [[772, -438], [772, 918], [-132, 918], [-132, -438],]
    );
}

#[test]
fn q31_multiply_extracts_bits_without_truncating_negative_fraction() {
    assert_eq!(multiply_q31(-1, 0x4000_0000), -1);
    assert_eq!(multiply_q31(1, 0x4000_0000), 0);
    assert_eq!(multiply_q31(i32::MIN, i32::MIN), i32::MIN);
}

#[test]
fn effect_coordinates_narrow_before_viewport_classification() {
    assert_eq!(pack_xy(65535, 65536), 65535);
    assert_eq!(screen_clip(pack_xy(65535, 65536), [640, 480]), 0x11);
    assert_eq!(screen_clip(pack_xy(640, 480), [640, 480]), 0x24);
    assert_eq!(screen_clip(pack_xy(639, 479), [640, 480]), 0x12);
}
