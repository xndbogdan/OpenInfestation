use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Receipt {
    executable_sha256: String,
    boundaries: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    origin: [i32; 2],
    size: [i32; 2],
    scan_age: i32,
    style: u32,
    radius_high_byte: u8,
    annotations: Vec<Annotation>,
    events: Vec<(String, u32)>,
    draws: Vec<Draw>,
    final_rng: u32,
}

#[derive(Deserialize)]
struct Annotation {
    position: [u16; 3],
    label: u32,
    radius: u16,
    rgb: [u8; 3],
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Draw {
    Label {
        label: i32,
        position: [i32; 2],
        color: u32,
    },
    Radius {
        corners: [[i16; 2]; 4],
        depths: [u32; 4],
        descriptor: u32,
        color: u32,
    },
}

#[test]
fn native_annotation_pass_preserves_wrapping_geometry_scan_colors_and_style_reads() {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../tests/fixtures/map_annotation_draw_native.json"
    ))
    .unwrap();
    assert_eq!(
        receipt.executable_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert!(receipt.boundaries.contains("no instruction patches"));
    assert_eq!(receipt.cases.len(), 8);
    for case in receipt.cases {
        let inputs = BriefingAnnotationInputs {
            tile: BriefingMarkerTile {
                origin: case.origin,
                size: case.size,
            },
            scan_age: case.scan_age,
            radius_high_byte: case.radius_high_byte,
        };
        let mut events = vec![("font".to_owned(), 0), ("style".to_owned(), 0)];
        assert_eq!(case.annotations.len(), case.draws.len(), "{}", case.name);
        for (annotation, expected) in case.annotations.into_iter().zip(case.draws) {
            let actual = briefing_map_annotation(
                BriefingMapAnnotation {
                    position_words: Some(annotation.position),
                    label: annotation.label,
                    radius: annotation.radius,
                    rgb: annotation.rgb,
                },
                inputs,
                || {
                    events.push(("style".to_owned(), 0x24));
                    Ok::<_, ()>(case.style)
                },
            )
            .unwrap();
            match (actual, expected) {
                (
                    BriefingAnnotationDraw::Label {
                        label,
                        position_pixels,
                        color_word,
                    },
                    Draw::Label {
                        label: expected_label,
                        position,
                        color,
                    },
                ) => {
                    assert_eq!(label, expected_label, "{}", case.name);
                    assert_eq!(position_pixels, position, "{}", case.name);
                    assert_eq!(color_word, color, "{}", case.name);
                }
                (
                    BriefingAnnotationDraw::Radius(quad),
                    Draw::Radius {
                        corners,
                        depths,
                        descriptor,
                        color,
                    },
                ) => {
                    assert_eq!(quad.corners, corners, "{}", case.name);
                    assert_eq!(quad.vertex_depths, depths, "{}", case.name);
                    assert_eq!(quad.descriptor_word, descriptor, "{}", case.name);
                    assert_eq!(quad.mode, DirectSpriteMode::Effect { color_word: color });
                    assert_eq!(quad.sprite_slot, 7);
                }
                _ => panic!("{}: annotation kind differs from native", case.name),
            }
        }
        assert_eq!(events, case.events, "{}", case.name);
        assert_eq!(case.final_rng, 0x12345678, "annotation pass has no RNG");
    }
}

#[test]
fn missing_annotation_position_precedes_label_style_and_radius_never_reads_it() {
    let inputs = BriefingAnnotationInputs {
        tile: BriefingMarkerTile {
            origin: [0; 2],
            size: [256; 2],
        },
        scan_age: 0,
        radius_high_byte: 0,
    };
    let mut annotation = BriefingMapAnnotation {
        position_words: None,
        label: 1,
        radius: 10000,
        rgb: [23, 45, 67],
    };
    assert_eq!(
        briefing_map_annotation(annotation, inputs, || -> Result<u32, ()> {
            panic!("missing position must precede style 24")
        }),
        Err(BriefingAnnotationError::MissingPosition)
    );
    annotation.position_words = Some([32768; 3]);
    assert_eq!(
        briefing_map_annotation(annotation, inputs, || Err("style error")),
        Err(BriefingAnnotationError::Resource("style error"))
    );
    annotation.label = 0;
    assert!(matches!(
        briefing_map_annotation(annotation, inputs, || -> Result<u32, ()> {
            panic!("radius annotations have no label style lookup")
        }),
        Ok(BriefingAnnotationDraw::Radius(_))
    ));
}
