use super::*;

/// A wheel's frame: its axis along X through the origin.
fn wheel() -> PartFrame {
    PartFrame::new([0.0; 3], [1.0, 0.0, 0.0]).unwrap()
}

/// A point on a ring `along` the axis, `radius` out, `degrees` round.
fn ring(frame: &PartFrame, along: f32, radius: f32, degrees: f32) -> [f32; 3] {
    frame.point([along, radius, degrees.to_radians()])
}

/// The buggy's tread facet: 7 sides, radius 32, 24 wide.
const SEVENTH: f32 = 360.0 / 7.0;

fn on_circle(frame: &PartFrame, face: &SplitFace, along: f32, radius: f32) -> Vec<[u32; 3]> {
    let mut points: Vec<[u32; 3]> = face
        .corners
        .iter()
        .filter(|corner| {
            let [height, distance, _] = frame.cylindrical(corner.position);
            (height - along).abs() < 1.0e-3 && (distance - radius).abs() < 1.0e-2
        })
        .map(|corner| corner.position.map(f32::to_bits))
        .collect();
    points.sort_unstable();
    points.dedup();
    points
}

#[test]
fn a_tread_facet_splits_onto_its_circle() {
    let frame = wheel();
    let positions = [
        ring(&frame, -12.0, 32.0, 0.0),
        ring(&frame, -12.0, 32.0, SEVENTH),
        ring(&frame, 12.0, 32.0, SEVENTH),
        ring(&frame, 12.0, 32.0, 0.0),
    ];
    let normals = positions.map(|p| unit([0.0, p[1], p[2]]).unwrap());
    let face = split_face(&frame, &positions, &normals, &|_, _| 5, None).unwrap();
    // Each rim edge takes four new points; ten triangles span the strip.
    assert_eq!(face.corners.len(), 4 + 2 * 4);
    assert_eq!(face.triangles.len(), 10);
    for corner in &face.corners {
        let [_, distance, angle] = frame.cylindrical(corner.position);
        assert!((distance - 32.0).abs() < 1.0e-3, "{distance}");
        assert!(dot(corner.normal, frame.outward(angle)) > 0.999);
    }
    // Wound as the facet was.
    let facing = |[a, b, c]: [[f32; 3]; 3]| {
        cross(
            std::array::from_fn(|k| b[k] - a[k]),
            std::array::from_fn(|k| c[k] - a[k]),
        )
    };
    let original = facing([positions[0], positions[1], positions[2]]);
    for triangle in &face.triangles {
        let corners = triangle.map(|i| face.corners[i as usize].position);
        assert!(dot(facing(corners), original) > 0.0);
    }
}

#[test]
fn faces_sharing_a_rim_edge_cut_it_alike() {
    let frame = wheel();
    let [a, b] = [
        ring(&frame, 12.0, 32.0, 0.0),
        ring(&frame, 12.0, 32.0, SEVENTH),
    ];
    let tread = [
        ring(&frame, -12.0, 32.0, 0.0),
        ring(&frame, -12.0, 32.0, SEVENTH),
        b,
        a,
    ];
    // The side wall runs the shared edge the other way, b to a.
    let wall = [
        b,
        a,
        ring(&frame, 19.0, 16.5, 0.0),
        ring(&frame, 19.0, 16.5, SEVENTH),
    ];
    let count = |turn: f32, distance: f32| pieces(turn, distance * 3.0, 0.5, 16);
    let tread = split_face(&frame, &tread, &[[0.0; 3]; 4], &count, None).unwrap();
    let wall = split_face(&frame, &wall, &[[0.0; 3]; 4], &count, None).unwrap();
    let shared = on_circle(&frame, &tread, 12.0, 32.0);
    assert_eq!(shared.len(), 6, "the ends and four cuts");
    assert_eq!(on_circle(&frame, &wall, 12.0, 32.0), shared);
    // The wall's hub ring, smaller on screen, takes fewer cuts.
    assert_eq!(on_circle(&frame, &wall, 19.0, 16.5).len(), 5);
}

#[test]
fn a_hub_cone_fans_from_its_tip() {
    let frame = wheel();
    let positions = [
        [10.0, 0.0, 0.0],
        ring(&frame, 19.0, 16.5, 0.0),
        ring(&frame, 19.0, 16.5, SEVENTH),
    ];
    let face = split_face(&frame, &positions, &[[0.0; 3]; 3], &|_, _| 3, None).unwrap();
    assert_eq!(face.triangles.len(), 3);
    assert!(face.triangles.iter().all(|triangle| triangle[0] == 0));
    assert_eq!(on_circle(&frame, &face, 19.0, 16.5).len(), 4);
}

#[test]
fn an_end_cap_fans_from_its_middle() {
    let frame = wheel();
    let positions: Vec<[f32; 3]> = (0..7)
        .map(|k| ring(&frame, -19.0, 16.5, SEVENTH * k as f32))
        .collect();
    let normals = vec![[-1.0, 0.0, 0.0]; 7];
    let face = split_face(&frame, &positions, &normals, &|_, _| 2, None).unwrap();
    assert_eq!(face.triangles.len(), 14);
    let middle = face
        .corners
        .iter()
        .find(|corner| corner.blend == Blend::Middle)
        .unwrap();
    assert_eq!(middle.position, [-19.0, 0.0, 0.0]);
    assert_eq!(middle.normal, [-1.0, 0.0, 0.0]);
    assert_eq!(on_circle(&frame, &face, -19.0, 16.5).len(), 14);
}

#[test]
fn a_face_off_the_circle_stays_as_it_is() {
    let frame = wheel();
    // Rim corners 32 and 25 out: no circle to bend onto.
    let positions = [
        ring(&frame, -12.0, 32.0, 0.0),
        ring(&frame, -12.0, 25.0, SEVENTH),
        ring(&frame, 12.0, 25.0, SEVENTH),
        ring(&frame, 12.0, 32.0, 0.0),
    ];
    assert_eq!(
        split_face(&frame, &positions, &[[0.0; 3]; 4], &|_, _| 5, None),
        None
    );
    // Nor along the axis only.
    let along = [
        ring(&frame, -12.0, 32.0, 0.0),
        ring(&frame, 12.0, 32.0, 0.0),
        ring(&frame, 12.0, 20.0, 0.0),
    ];
    assert_eq!(
        split_face(&frame, &along, &[[0.0; 3]; 3], &|_, _| 5, None),
        None
    );
}

#[test]
fn the_pieces_follow_the_size_on_screen() {
    let seventh = SEVENTH.to_radians();
    let eighth = 45.0_f32.to_radians();
    // The chase view at 4K: the buggy's wheel 95 pixels out, the truck's 170.
    assert_eq!(pieces(seventh, 95.0, 0.5, 16), 5);
    assert_eq!(pieces(eighth, 170.0, 0.5, 16), 6);
    // At 1080p, four each.
    assert_eq!(pieces(seventh, 47.5, 0.5, 16), 4);
    assert_eq!(pieces(eighth, 85.0, 0.5, 16), 4);
    // Far away, the facet as it is; up close, at most `most`.
    assert_eq!(pieces(seventh, 0.4, 0.5, 16), 1);
    assert_eq!(pieces(seventh, 1.0e6, 0.5, 16), 16);
}

/// The tread sprite's columns as `tread_profile` reads them: four lugs of
/// nine columns (means 27, 13, 16, 38, 46, 74, 76, 76, 57), darkest 0.
fn tread() -> Vec<f32> {
    let lug = [27.0, 13.0, 16.0, 38.0, 46.0, 74.0, 76.0, 76.0, 57.0];
    (0..36)
        .map(|k| (lug[k % 9] - 13.0) / (76.0 - 13.0))
        .collect()
}

fn lugs<'a>(on_ring: &'a [bool], u: &'a [f32], height: &'a dyn Fn(f32) -> [f32; 2]) -> Lugs<'a> {
    Lugs {
        on_ring,
        u,
        height,
        depth: 2.0,
        cuts: 24.0,
        pixels: 3.0,
        from: 40.0,
    }
}

#[test]
fn the_tread_repeats_across_a_facet() {
    let profile = tread();
    assert_eq!(tread_height(&profile, 0.0), tread_height(&profile, 1.0));
    let [middle, _] = tread_height(&profile, 1.5 / 36.0);
    assert!((middle - profile[1]).abs() < 1.0e-6);
}

#[test]
fn a_lugged_facet_sinks_into_its_grooves() {
    let frame = wheel();
    let positions = [
        ring(&frame, -12.0, 32.0, 0.0),
        ring(&frame, -12.0, 32.0, SEVENTH),
        ring(&frame, 12.0, 32.0, SEVENTH),
        ring(&frame, 12.0, 32.0, 0.0),
    ];
    let normals = positions.map(|p| unit([0.0, p[1], p[2]]).unwrap());
    let profile = tread();
    let height = |u: f32| tread_height(&profile, u);
    let lugs = lugs(&[true; 4], &[0.0, 1.0, 1.0, 0.0], &height);
    let face = split_face(&frame, &positions, &normals, &|_, _| 5, Some(&lugs)).unwrap();
    // Each rim takes 24 cuts: 23 new points.
    assert_eq!(face.corners.len(), 4 + 2 * 23);
    for corner in &face.corners {
        let [_, distance, angle] = frame.cylindrical(corner.position);
        let [h, _] = tread_height(&profile, angle / SEVENTH.to_radians());
        assert!(
            (distance - (32.0 - 2.0 * (1.0 - h))).abs() < 1.0e-3,
            "{distance}"
        );
    }
    // The lug tops stay at retail's radius.
    let outermost = face
        .corners
        .iter()
        .map(|corner| frame.cylindrical(corner.position)[1])
        .fold(0.0, f32::max);
    assert!((outermost - 32.0).abs() < 1.0e-3);
    // A wheel small on screen keeps its round rim.
    let far = Lugs {
        pixels: 1.0,
        ..lugs
    };
    let face = split_face(&frame, &positions, &normals, &|_, _| 5, Some(&far)).unwrap();
    assert_eq!(face.corners.len(), 4 + 2 * 4);
}

#[test]
fn a_facet_joint_sinks_alike_from_both_facets() {
    let frame = wheel();
    let facet = |from: f32| {
        [
            ring(&frame, -12.0, 32.0, from),
            ring(&frame, -12.0, 32.0, from + SEVENTH),
            ring(&frame, 12.0, 32.0, from + SEVENTH),
            ring(&frame, 12.0, 32.0, from),
        ]
    };
    let profile = tread();
    let height = |u: f32| tread_height(&profile, u);
    let lugs = lugs(&[true; 4], &[0.0, 1.0, 1.0, 0.0], &height);
    let split = |positions: [[f32; 3]; 4]| {
        split_face(&frame, &positions, &[[0.0; 3]; 4], &|_, _| 5, Some(&lugs)).unwrap()
    };
    let [first, second] = [split(facet(0.0)), split(facet(SEVENTH))];
    // The joint: the first facet's corner 1 (u 1), the second's corner 0 (u 0).
    assert_eq!(first.corners[1].position, second.corners[0].position);
    assert_eq!(first.corners[2].position, second.corners[3].position);
}

#[test]
fn faces_sharing_a_lugged_ring_cut_it_alike() {
    let frame = wheel();
    let [a, b] = [
        ring(&frame, 12.0, 32.0, 0.0),
        ring(&frame, 12.0, 32.0, SEVENTH),
    ];
    let tread_face = [
        ring(&frame, -12.0, 32.0, 0.0),
        ring(&frame, -12.0, 32.0, SEVENTH),
        b,
        a,
    ];
    let wall = [
        a,
        b,
        ring(&frame, 19.0, 16.5, SEVENTH),
        ring(&frame, 19.0, 16.5, 0.0),
    ];
    let profile = tread();
    let height = |u: f32| tread_height(&profile, u);
    let count = |turn: f32, distance: f32| pieces(turn, distance * 3.0, 0.5, 16);
    let on_tread = lugs(&[true; 4], &[0.0, 1.0, 1.0, 0.0], &height);
    let on_wall = lugs(&[true, true, false, false], &[0.0, 1.0, 1.0, 0.0], &height);
    let tread_face =
        split_face(&frame, &tread_face, &[[0.0; 3]; 4], &count, Some(&on_tread)).unwrap();
    let wall = split_face(&frame, &wall, &[[0.0; 3]; 4], &count, Some(&on_wall)).unwrap();
    let on_ring = |face: &SplitFace| -> Vec<[u32; 3]> {
        let mut points: Vec<[u32; 3]> = face
            .corners
            .iter()
            .filter(|corner| (frame.cylindrical(corner.position)[0] - 12.0).abs() < 1.0e-3)
            .map(|corner| corner.position.map(f32::to_bits))
            .collect();
        points.sort_unstable();
        points.dedup();
        points
    };
    assert_eq!(on_ring(&tread_face).len(), 25);
    assert_eq!(on_ring(&wall), on_ring(&tread_face));
}

type Faces = Vec<(Vec<u16>, Vec<f32>, bool)>;

fn faces_of(program: &inf_formats::ModelProgram, faces: &mut Faces) {
    use inf_formats::ModelInstructionKind as Kind;
    for instruction in &program.instructions {
        match &instruction.kind {
            Kind::Quad(face) => faces.push((
                face.vertices.iter().map(|slot| slot.raw()).collect(),
                vec![0.0, 1.0, 1.0, 0.0],
                face.flags.emits_mirror_instance(),
            )),
            Kind::Triangle(face) => faces.push((
                face.vertices.iter().map(|slot| slot.raw()).collect(),
                vec![0.0, 1.0, 1.0],
                face.flags.emits_mirror_instance(),
            )),
            Kind::NestedProgram(nested) => faces_of(&nested.program, faces),
            _ => {}
        }
    }
}

/// The real wheels (H8 133 to 136) hold what the lugs need to stay whole:
/// every face on an edge between two tread slots runs u the same way along
/// it, and every face on a tread slot takes it at u 0 or 1, where the tread
/// repeats.
#[test]
fn the_wheels_run_their_treads_alike() {
    let (residency, _, _) = crate::test_worlds::offline_world_retained(
        18,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let part = crate::modern_tuning::RoundPart {
        names: vec![],
        axis: crate::modern_tuning::RoundAxis::X,
        centre: [0.0; 3],
        radius: None,
        along: None,
    };
    for model_slot in 133..=136 {
        let binding = crate::object_runtime::resident_model_resource(&residency, model_slot)
            .unwrap()
            .unwrap();
        let model = binding.model;
        let round = RoundModel::new(part.clone(), &model.program, &[195]);
        assert_eq!(round.tread_sprite, Some(195), "model {model_slot}");
        let tread = |slot: u16| round.tread_slots.binary_search(&slot).is_ok();
        let mut faces = Faces::new();
        faces_of(&model.program, &mut faces);
        let mut edges = std::collections::HashMap::new();
        for (slots, u, mirror) in &faces {
            let passes: &[u16] = if *mirror { &[0, 1] } else { &[0] };
            for pass in passes {
                let slots: Vec<u16> = slots.iter().map(|&slot| slot ^ pass).collect();
                for a in 0..slots.len() {
                    let b = (a + 1) % slots.len();
                    if !tread(slots[a]) {
                        continue;
                    }
                    assert!(
                        u[a] == 0.0 || u[a] == 1.0,
                        "model {model_slot}: slot {} at u {}",
                        slots[a],
                        u[a]
                    );
                    if slots[a] == slots[b] || !tread(slots[b]) || u[a] == u[b] {
                        continue;
                    }
                    let (low, high) = if slots[a] < slots[b] { (a, b) } else { (b, a) };
                    let along = (u[low], u[high]);
                    let known = *edges.entry((slots[low], slots[high])).or_insert(along);
                    assert_eq!(
                        known,
                        along,
                        "model {model_slot}: edge {:?}",
                        (slots[low], slots[high])
                    );
                }
            }
        }
        assert!(!edges.is_empty(), "model {model_slot}");
    }
}

/// The hovercraft's duct (`scatfan`): its front cap is a flat quad on its
/// ring with three rim edges and a chord; it fans from its middle, and
/// its arcs are cut as the barrel's lip cuts them.
#[test]
fn a_flat_cap_piece_fans_from_its_middle() {
    let frame = wheel();
    let cap = [
        ring(&frame, 12.0, 18.0, 0.0),
        ring(&frame, 12.0, 18.0, SEVENTH),
        ring(&frame, 12.0, 18.0, 2.0 * SEVENTH),
        ring(&frame, 12.0, 18.0, 3.0 * SEVENTH),
    ];
    let lip = [
        ring(&frame, 0.0, 21.0, SEVENTH),
        ring(&frame, 12.0, 18.0, SEVENTH),
        ring(&frame, 12.0, 18.0, 2.0 * SEVENTH),
        ring(&frame, 0.0, 21.0, 2.0 * SEVENTH),
    ];
    let normals = [[1.0, 0.0, 0.0]; 4];
    let count = |turn: f32, distance: f32| pieces(turn, distance * 3.0, 0.5, 16);
    let cap = split_face(&frame, &cap, &normals, &count, None).unwrap();
    let lip = split_face(&frame, &lip, &[[0.0; 3]; 4], &count, None).unwrap();
    let middle = cap
        .corners
        .iter()
        .find(|corner| corner.blend == Blend::Middle)
        .unwrap();
    assert!((frame.cylindrical(middle.position)[0] - 12.0).abs() < 1.0e-3);
    // The arc both share, from the cap's second corner to its third.
    let shared = |face: &SplitFace| -> Vec<[u32; 3]> {
        let mut points: Vec<[u32; 3]> = face
            .corners
            .iter()
            .filter(|corner| {
                let [height, distance, angle] = frame.cylindrical(corner.position);
                (height - 12.0).abs() < 1.0e-3
                    && (distance - 18.0).abs() < 1.0e-2
                    && angle >= SEVENTH.to_radians() - 1.0e-4
                    && angle <= 2.0 * SEVENTH.to_radians() + 1.0e-4
            })
            .map(|corner| corner.position.map(f32::to_bits))
            .collect();
        points.sort_unstable();
        points.dedup();
        points
    };
    assert!(shared(&cap).len() > 2);
    assert_eq!(shared(&cap), shared(&lip));
}

/// A face retail lit with one normal (the duct's, the jet's) rounds: its
/// corners lose the lean round the axis, as its new points do.
#[test]
fn a_flat_face_rounds_its_normals() {
    let frame = wheel();
    let positions = [
        ring(&frame, -12.0, 32.0, 0.0),
        ring(&frame, -12.0, 32.0, SEVENTH),
        ring(&frame, 12.0, 32.0, SEVENTH),
        ring(&frame, 12.0, 32.0, 0.0),
    ];
    // The facet's own normal, out through its middle.
    let flat = frame.outward((SEVENTH / 2.0).to_radians());
    let face = split_face(&frame, &positions, &[flat; 4], &|_, _| 5, None).unwrap();
    for corner in &face.corners {
        let angle = frame.cylindrical(corner.position)[2];
        assert!(dot(corner.normal, frame.outward(angle)) > 0.999);
    }
}

/// A part inside a larger model (the settled skimmer's jet): only edges
/// within its windows curve.
#[test]
fn rims_outside_the_windows_stay_straight() {
    let frame = wheel().within(Some([10.0, 20.0]), Some([-30.0, 30.0]));
    let facet = |along: f32, radius: f32| {
        [
            ring(&frame, along - 2.0, radius, 0.0),
            ring(&frame, along - 2.0, radius, SEVENTH),
            ring(&frame, along + 2.0, radius, SEVENTH),
            ring(&frame, along + 2.0, radius, 0.0),
        ]
    };
    let split =
        |positions: [[f32; 3]; 4]| split_face(&frame, &positions, &[[0.0; 3]; 4], &|_, _| 5, None);
    assert!(split(facet(0.0, 15.0)).is_some());
    assert_eq!(split(facet(0.0, 32.0)), None);
    assert_eq!(split(facet(80.0, 15.0)), None);
}

/// A sphere's eighth (radius 100, 200 pixels on screen) splits onto the
/// sphere: every point on it, facing out, every piece wound as the face.
#[test]
fn a_sphere_face_splits_onto_its_sphere() {
    let positions = [[100.0, 0.0, 0.0], [0.0, 100.0, 0.0], [0.0, 0.0, 100.0]];
    let face = split_sphere([0.0; 3], &positions, 2.0, 0.5, 16).unwrap();
    // Its 90-degree edges take 16 pieces each.
    assert_eq!(face.triangles.len(), 16 * 16);
    for corner in &face.corners {
        let distance = dot(corner.position, corner.position).sqrt();
        assert!((distance - 100.0).abs() < 1.0e-3, "{distance}");
        assert!(dot(corner.normal, corner.position.map(|c| c / distance)) > 0.9999);
    }
    for triangle in &face.triangles {
        let [a, b, c] = triangle.map(|i| face.corners[i as usize].position);
        let facing = cross(
            std::array::from_fn(|k| b[k] - a[k]),
            std::array::from_fn(|k| c[k] - a[k]),
        );
        assert!(dot(facing, a) > 0.0);
    }
    // A new point's blend mixes the face's own corners.
    for corner in &face.corners[3..] {
        let Blend::Mix(mix) = corner.blend else {
            panic!("{:?}", corner.blend);
        };
        let total: f32 = mix.iter().map(|&(_, weight)| weight).sum();
        assert!((total - 1.0).abs() < 1.0e-6);
    }
}

/// Two faces on one edge of a sphere, wound against each other, put the
/// same points on it.
#[test]
fn faces_sharing_a_sphere_edge_cut_it_alike() {
    let [a, b] = [[100.0, 0.0, 0.0], [0.0, 100.0, 0.0]];
    let upper = split_sphere([0.0; 3], &[a, b, [0.0, 0.0, 100.0]], 2.0, 0.5, 16).unwrap();
    let lower = split_sphere([0.0; 3], &[b, a, [0.0, 0.0, -100.0]], 2.0, 0.5, 16).unwrap();
    let on_edge = |face: &SplitFace| -> Vec<[u32; 3]> {
        let mut points: Vec<[u32; 3]> = face
            .corners
            .iter()
            .filter(|corner| corner.position[2] == 0.0)
            .map(|corner| corner.position.map(f32::to_bits))
            .collect();
        points.sort_unstable();
        points.dedup();
        points
    };
    assert_eq!(on_edge(&upper).len(), 17);
    assert_eq!(on_edge(&upper), on_edge(&lower));
}

/// A face whose corners stand at different distances from the centre is
/// no part of the sphere; a sphere too small on screen keeps its faces.
#[test]
fn a_face_off_its_sphere_stays_as_it_is() {
    let off = [[100.0, 0.0, 0.0], [0.0, 100.0, 0.0], [0.0, 0.0, 50.0]];
    assert_eq!(split_sphere([0.0; 3], &off, 2.0, 0.5, 16), None);
    let on = [[100.0, 0.0, 0.0], [0.0, 100.0, 0.0], [0.0, 0.0, 100.0]];
    assert_eq!(split_sphere([0.0; 3], &on, 0.004, 0.5, 16), None);
}
