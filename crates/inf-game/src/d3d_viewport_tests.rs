use super::*;

fn vertex(x: f32, y: f32) -> D3dTlVertex {
    D3dTlVertex {
        x,
        y,
        depth: 0.125,
        rhw: 0.25,
        diffuse: [1, 3, 1, 3],
        specular: 0x0301_0301,
        uv: [0.0, 0.0],
    }
}

fn state() -> D3dViewportState {
    D3dViewportState {
        bounds: [0, 0, 8, 8],
        near_depth: 1,
        mode: 0,
    }
}

#[test]
fn identity_preserves_every_word_including_unrelated_depth_and_boundary_vertices() {
    let mut input = [
        vertex(0., 0.),
        vertex(8., 0.),
        vertex(8., 8.),
        vertex(0., 8.),
    ];
    input[0].uv = [-0.0, -0.0];
    for count in [3, 4] {
        let output = clip_polygon(&input[..count], state()).unwrap();
        assert_eq!(output, input[..count]);
        assert_eq!(output[0].uv.map(f32::to_bits), [0x8000_0000; 2]);
        assert_eq!(output[0].depth, 0.125); // not recomputed to 0.75
    }
}

#[test]
fn boundary_hits_are_not_duplicated_and_degenerate_outputs_are_preserved() {
    let input = [vertex(-2., 2.), vertex(0., 2.), vertex(0., 4.)];
    assert_eq!(clip_polygon(&input, state()).unwrap(), input[1..]);
    let input = [vertex(-2., 2.), vertex(0., 2.), vertex(-1., 4.)];
    assert_eq!(clip_polygon(&input, state()).unwrap(), [input[1]]);
    assert!(clip_polygon(
        &[vertex(-2., 2.), vertex(-2., 4.), vertex(-1., 4.)],
        state()
    )
    .unwrap()
    .is_empty());
}

#[test]
fn intersections_round_color_deltas_and_recompute_depth_from_reciprocal_depth() {
    let mut input = [vertex(-2., 2.), vertex(2., 2.), vertex(2., 4.)];
    input[1].rhw = 0.75;
    input[1].uv = [1., 1.];
    input[1].diffuse = [2; 4];
    input[1].specular = 0x0202_0202;
    let output = clip_polygon(&input, state()).unwrap();
    assert_eq!(output.len(), 4);
    assert_eq!([output[0].x, output[0].y], [0., 2.]);
    assert_eq!(output[0].rhw, 0.5);
    assert_eq!(output[0].depth, 0.5);
    assert_eq!(output[0].diffuse, [1, 3, 1, 3]);
    assert_eq!(output[0].specular, 0x0301_0301);
    assert_eq!(output[0].uv, [0.75; 2]);
    assert_eq!(output[1], input[1]);
    let affine = clip_polygon(
        &input,
        D3dViewportState {
            mode: 0x100,
            ..state()
        },
    )
    .unwrap();
    assert_eq!(affine[0].uv, [0.5; 2]);
}

#[test]
fn bypass_bit_belongs_to_the_caller_and_does_not_disable_the_clip_helper() {
    let input = [vertex(-2., 2.), vertex(2., 2.), vertex(2., 4.)];
    assert_eq!(
        clip_polygon(&input, state()).unwrap(),
        clip_polygon(
            &input,
            D3dViewportState {
                mode: 0x20,
                ..state()
            }
        )
        .unwrap()
    );
}

#[test]
fn invalid_counts_bounds_and_nonfinite_output_fail_closed() {
    let mut input = [vertex(-2., 2.), vertex(2., 2.), vertex(2., 4.)];
    assert_eq!(
        clip_polygon(&input[..2], state()),
        Err(D3dViewportError::VertexCount(2))
    );
    assert_eq!(
        clip_polygon(
            &input,
            D3dViewportState {
                bounds: [1, 0, -1, 4],
                ..state()
            }
        ),
        Err(D3dViewportError::InvertedBounds)
    );
    input[0].uv[0] = f32::NAN;
    assert_eq!(
        clip_polygon(&input, state()),
        Err(D3dViewportError::NonFinite)
    );
    input[0].uv[0] = 0.;
    input[1].rhw = -input[0].rhw;
    assert_eq!(
        clip_polygon(&input, state()),
        Err(D3dViewportError::NonFinite)
    );
    // Affine UV does not use the reciprocal of the interpolated zero rhw.
    assert!(clip_polygon(
        &input,
        D3dViewportState {
            mode: 0x100,
            ..state()
        }
    )
    .is_ok());
}

fn from_words(words: [u32; 8]) -> D3dTlVertex {
    let [b, g, r, a] = words[4].to_le_bytes();
    D3dTlVertex {
        x: f32::from_bits(words[0]),
        y: f32::from_bits(words[1]),
        depth: f32::from_bits(words[2]),
        rhw: f32::from_bits(words[3]),
        diffuse: [r, g, b, a],
        specular: words[5],
        uv: [f32::from_bits(words[6]), f32::from_bits(words[7])],
    }
}

fn words(vertex: D3dTlVertex) -> [u32; 8] {
    let [r, g, b, a] = vertex.diffuse;
    [
        vertex.x.to_bits(),
        vertex.y.to_bits(),
        vertex.depth.to_bits(),
        vertex.rhw.to_bits(),
        u32::from_le_bytes([b, g, r, a]),
        vertex.specular,
        vertex.uv[0].to_bits(),
        vertex.uv[1].to_bits(),
    ]
}

#[path = "d3d_viewport_native_tests.rs"]
mod native;
