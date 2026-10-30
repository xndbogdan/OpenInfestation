use super::*;

fn packet(far: bool) -> PrimitiveCommand<'static> {
    let mut command = PrimitiveCommand::new(
        if far {
            PrimitiveCallback::ColoredQuadFar
        } else {
            PrimitiveCallback::ColoredQuadNear
        },
        0,
        4,
    );
    for (i, [x, y]) in [[0i16, 0], [8, 0], [8, 8], [0, 8]].into_iter().enumerate() {
        command.write_u32(i * 8, u32::from(x as u16) | (u32::from(y as u16) << 16));
        command.write_u32(i * 8 + 4, 128);
        command.write_rgb(0x28 + i * 4, [128, 64, 32]);
    }
    command.write_u32(0x20, 0x4000);
    command.write_u32(0x24, 0x7fff);
    if far {
        command.write_u32(0x38, 0x001f);
        for (i, fog) in [0, 64, 128, 255].into_iter().enumerate() {
            command.write_byte(0x3c + i, fog);
        }
    }
    command
}

fn renderer() -> D3dFlatState {
    D3dFlatState {
        material_shifts: [6, 1, 4],
        near_depth: 64,
        manages_depth: true,
        depth_enable: 1,
        depth_write: 1,
    }
}

#[test]
fn consumes_only_rgb_and_rejects_unknown_reached_far_bytes() {
    let mut command = packet(true);
    let quad = read_colored_terrain(&command).unwrap();
    assert_eq!(quad.shades, [[128, 64, 32]; 4]);
    assert_eq!(quad.far, Some((0x1f, [0, 64, 128, 255])));
    command.payload[0x3d] = None;
    assert!(matches!(
        read_colored_terrain(&command),
        Err(SoftwarePixelError::Unknown {
            field: "fog",
            offset: 0x3d
        })
    ));
    command.callback = PrimitiveCallback::ColoredQuadNear;
    assert!(read_colored_terrain(&command).unwrap().far.is_none());
}

#[test]
fn far_blends_unrounded_shade_into_unreplicated_material() {
    let near = prepare_colored_terrain(&read_colored_terrain(&packet(false)).unwrap(), renderer());
    assert_eq!(near.vertices[0].diffuse, [248, 124, 62, 255]);
    assert_eq!(near.triangles, [[0, 1, 2], [0, 2, 3]]);
    assert_eq!(near.vertices[0].rhw, 1. / 128.);
    assert_eq!(near.vertices[0].depth, 0.5);
    let far = prepare_colored_terrain(&read_colored_terrain(&packet(true)).unwrap(), renderer());
    assert_eq!(
        far.vertices.map(|v| v.diffuse),
        [
            [248, 124, 62, 255],
            [186, 93, 108, 255],
            [124, 62, 155, 255],
            [1, 0, 247, 255]
        ]
    );
    assert_eq!(
        far.vertices.map(|v| v.specular),
        [0xff000000, 0xbf000000, 0x7f000000, 0]
    );
    assert!(!far.policy.fog_enabled);
}

#[test]
fn additive_retains_enable_and_disables_write_without_changing_other_mode_bits() {
    let mut command = packet(false);
    command.write_u32(0x20, 0x22);
    let submission = prepare_colored_terrain(&read_colored_terrain(&command).unwrap(), renderer());
    assert_eq!(submission.policy.depth_enable, None);
    assert_eq!(submission.policy.depth_write, Some(0));
    assert_eq!(submission.policy.blend, D3dFlatBlend::Additive);
    assert_eq!(submission.clip, D3dFlatClip::Bypass);
    assert_eq!(submission.vertices[0].diffuse[3], 0);
    assert_eq!(submission.triangles, [[1, 2, 3], [1, 3, 0]]);
}
