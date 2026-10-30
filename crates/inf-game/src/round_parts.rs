//! Round parts under the tracer: the faces of
//! a model the tuning names round (the ACAM's wheels) split onto the true
//! circle round its axis, as finely as the part's size on screen needs, so
//! its outline and its shadow are round where retail's facets showed. Only
//! the scene feed sees it; the raster draws retail's faces.
//!
//! An edge whose ends stand at one height along the axis and one distance
//! from it, less than 75 degrees apart round it, is a rim edge: it is cut
//! into pieces whose points lie on the circle (on a cone where the ends'
//! distances differ a little). Every other edge stays straight. A face with
//! two opposite rim edges (the tread, the side wall) becomes a strip, one
//! with a rim edge and a corner off it (the hub's cone) a fan from that
//! corner, and one whose every edge is a rim edge (an end cap) a fan from
//! its middle. An edge is cut from its ends alone, the same way whichever
//! face cuts it, so no cracks open between faces.
//!
//! On a wheel large on screen the treads' lugs become geometry ([`Lugs`]):
//! every corner on a ring the treads use sinks into its groove by the
//! tread sprite's brightness at its texture u, and a rim edge between two
//! such corners takes cuts enough to follow each lug. Every face on such a
//! ring runs u the same way along it, and the sprite repeats across a
//! facet's joint, so faces agree on every point and the treads stay whole.

/// How far two rim corners' heights along the axis may differ (the truck's
/// wheel alternates 8 and 9).
const HEIGHT_SLACK: f32 = 1.5;
/// How far, as a share of the larger, two rim corners' distances from the
/// axis may differ.
const DISTANCE_SLACK: f32 = 0.1;
/// The least distance from the axis a rim corner stands at.
const OFF_AXIS: f32 = 1.0;
/// The widest rim edge round the axis: a ring of five sides or more.
const WIDEST: f32 = 75.0 * std::f32::consts::PI / 180.0;
/// How far a normal may lean round the axis before it is taken for a
/// face's own flat normal, which the split rounds.
const SIDEWAYS: f32 = 0.05;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(v: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(v, v).sqrt();
    (length > 1.0e-6).then(|| v.map(|c| c / length))
}

/// A round part's frame in one draw (eye-relative world): a point on its
/// axis, the axis, and two directions across it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PartFrame {
    centre: [f32; 3],
    axis: [f32; 3],
    across: [[f32; 3]; 2],
    /// The distances from the axis and the heights along it (from the
    /// centre) a rim stands within, for a part inside a larger model.
    radius: Option<[f32; 2]>,
    along: Option<[f32; 2]>,
}

impl PartFrame {
    pub(crate) fn new(centre: [f32; 3], axis: [f32; 3]) -> Option<Self> {
        let axis = unit(axis)?;
        let other = if axis[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let first = unit(cross(axis, other))?;
        Some(Self {
            centre,
            axis,
            across: [first, cross(axis, first)],
            radius: None,
            along: None,
        })
    }

    /// The frame with its rims kept within `radius` and `along`.
    pub(crate) fn within(self, radius: Option<[f32; 2]>, along: Option<[f32; 2]>) -> Self {
        Self {
            radius,
            along,
            ..self
        }
    }

    /// Whether a point at `shape` (height, distance, angle) lies where the
    /// part's rims may.
    fn holds(&self, [height, distance, _]: [f32; 3]) -> bool {
        let inside = |window: Option<[f32; 2]>, value: f32| {
            window.map_or(true, |[least, most]| (least..=most).contains(&value))
        };
        inside(self.radius, distance) && inside(self.along, height)
    }

    /// A point's height along the axis, its distance from it and its angle
    /// round it.
    fn cylindrical(&self, point: [f32; 3]) -> [f32; 3] {
        let offset: [f32; 3] = std::array::from_fn(|c| point[c] - self.centre[c]);
        let height = dot(offset, self.axis);
        let [x, y] = self.across.map(|across| dot(offset, across));
        [height, x.hypot(y), y.atan2(x)]
    }

    /// The way out from the axis at `angle`.
    fn outward(&self, angle: f32) -> [f32; 3] {
        let (sin, cos) = angle.sin_cos();
        std::array::from_fn(|c| self.across[0][c] * cos + self.across[1][c] * sin)
    }

    fn point(&self, [height, distance, angle]: [f32; 3]) -> [f32; 3] {
        let out = self.outward(angle);
        std::array::from_fn(|c| self.centre[c] + self.axis[c] * height + out[c] * distance)
    }

    /// The point on the axis the part's distances run from (a sphere's
    /// centre).
    pub(crate) fn centre(&self) -> [f32; 3] {
        self.centre
    }
}

/// How a corner of a split face comes from the face's own corners: one of
/// them, `t` of the way from one to another along their edge, the middle
/// of them all, or a mix of three of them (a sphere's inner points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Blend {
    Corner(usize),
    Edge(usize, usize, f32),
    Middle,
    Mix([(usize, f32); 3]),
}

/// A corner of a split face: how it blends the face's corners, where it
/// lies, and its normal (unit; zero where the corners had none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SplitCorner {
    pub blend: Blend,
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// A face split onto its part's shape: its corners, and its triangles over
/// them, wound as the face was.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SplitFace {
    pub corners: Vec<SplitCorner>,
    pub triangles: Vec<[u32; 3]>,
}

/// A face's lugs as geometry (the tuning's `round.treads`): which of its
/// corners stand on a ring the part's treads use, their texture u, the
/// tread's height at a u (0 a groove's bottom, 1 a lug's top) and its slope
/// along u, how deep the grooves sink, how many cuts a unit of u takes, the
/// pixels a unit of the part takes on screen, and the least radius on
/// screen, in pixels, a ring takes its lugs from.
pub(crate) struct Lugs<'a> {
    pub on_ring: &'a [bool],
    pub u: &'a [f32],
    pub height: &'a dyn Fn(f32) -> [f32; 2],
    pub depth: f32,
    pub cuts: f32,
    pub pixels: f32,
    pub from: f32,
}

struct Splitter<'a> {
    frame: &'a PartFrame,
    positions: &'a [[f32; 3]],
    normals: &'a [[f32; 3]],
    shape: Vec<[f32; 3]>,
    pieces: &'a dyn Fn(f32, f32) -> u32,
    lugs: Option<&'a Lugs<'a>>,
    face: SplitFace,
}

impl Splitter<'_> {
    /// The turn round the axis from corner `from` to `to` when their edge is
    /// a rim edge.
    fn rim(&self, from: usize, to: usize) -> Option<f32> {
        let [a, b] = [self.shape[from], self.shape[to]];
        let far = a[1].max(b[1]);
        let mut turn = b[2] - a[2];
        if turn > std::f32::consts::PI {
            turn -= std::f32::consts::TAU;
        } else if turn < -std::f32::consts::PI {
            turn += std::f32::consts::TAU;
        }
        ((a[0] - b[0]).abs() <= HEIGHT_SLACK
            && a[1].min(b[1]) > OFF_AXIS
            && (a[1] - b[1]).abs() <= DISTANCE_SLACK * far
            && turn.abs() > 1.0e-4
            && turn.abs() < WIDEST
            && self.frame.holds(a)
            && self.frame.holds(b))
        .then_some(turn)
    }

    /// The normal `t` of the way from corner `from` to `to` at `angle`: the
    /// ends' lean along the axis and their outward part, blended, turned
    /// with the angle; their sideways part dropped. A lug's flank `tilt`s
    /// the outward part back along the rim, against its slope.
    fn normal(&self, from: usize, to: usize, t: f32, angle: f32, tilt: f32) -> [f32; 3] {
        let parts = |corner: usize| {
            let normal = self.normals[corner];
            let out = self.frame.outward(self.shape[corner][2]);
            [dot(normal, self.frame.axis), dot(normal, out)]
        };
        let [[a0, r0], [a1, r1]] = [parts(from), parts(to)];
        let [along, outward] = [a0 + (a1 - a0) * t, r0 + (r1 - r0) * t];
        let [out, round] =
            [angle, angle + std::f32::consts::FRAC_PI_2].map(|a| self.frame.outward(a));
        let Some(bent) = unit(std::array::from_fn(|c| out[c] - round[c] * tilt)) else {
            return [0.0; 3];
        };
        unit(std::array::from_fn(|c| {
            self.frame.axis[c] * along + bent[c] * outward
        }))
        .unwrap_or([0.0; 3])
    }

    /// Whether `corner` carries lugs: on a ring the treads use, of a wheel
    /// that large on screen.
    fn lugged(&self, corner: usize) -> bool {
        self.lugs.is_some_and(|lugs| {
            lugs.on_ring[corner] && self.shape[corner][1] * lugs.pixels > lugs.from
        })
    }

    /// Where a lugged point at `at` (height, distance, angle) with texture
    /// u `u` stands, sunk into its groove, and how far its normal tilts with
    /// the groove's slope, u running `turning` a radian round the axis.
    fn sunk(&self, at: [f32; 3], u: f32, turning: f32) -> ([f32; 3], f32) {
        let Some(lugs) = self.lugs else {
            return (self.frame.point(at), 0.0);
        };
        let [height, slope] = (lugs.height)(u);
        let distance = (at[1] - lugs.depth * (1.0 - height)).max(OFF_AXIS);
        let tilt = lugs.depth * slope * turning / distance;
        (self.frame.point([at[0], distance, at[2]]), tilt)
    }

    /// The u a radian round the axis along the rim edge from `from` to `to`
    /// turning `turn`, where both carry lugs.
    fn turning(&self, from: usize, to: usize, turn: f32) -> Option<f32> {
        let lugs = self.lugs?;
        (self.lugged(from) && self.lugged(to)).then(|| (lugs.u[to] - lugs.u[from]) / turn)
    }

    /// The corners along the edge from `from` to `to`, its ends included:
    /// cut onto the circle when it is a rim edge, else just its ends. The
    /// cuts run from the edge's lesser end (by its position's bits), so
    /// both faces on an edge make the same points.
    fn edge(&mut self, from: usize, to: usize) -> Vec<u32> {
        let key = |corner: usize| self.positions[corner].map(f32::to_bits);
        let forward = key(from) <= key(to);
        let (first, last) = if forward { (from, to) } else { (to, from) };
        let mut points = vec![first as u32];
        if let Some(turn) = self.rim(first, last) {
            let [a, b] = [self.shape[first], self.shape[last]];
            let mut pieces = (self.pieces)(turn.abs(), a[1].max(b[1])).max(1);
            let turning = self.turning(first, last, turn);
            if let (Some(turning), Some(lugs)) = (turning, self.lugs) {
                pieces = pieces.max((lugs.cuts * (turning * turn).abs()).ceil() as u32);
            }
            for piece in 1..pieces {
                let t = piece as f32 / pieces as f32;
                let at = [
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                    a[2] + turn * t,
                ];
                let (position, tilt) = match (turning, self.lugs) {
                    (Some(turning), Some(lugs)) => {
                        let u = lugs.u[first] + (lugs.u[last] - lugs.u[first]) * t;
                        self.sunk(at, u, turning)
                    }
                    _ => (self.frame.point(at), 0.0),
                };
                points.push(self.face.corners.len() as u32);
                self.face.corners.push(SplitCorner {
                    blend: Blend::Edge(first, last, t),
                    position,
                    normal: self.normal(first, last, t, at[2], tilt),
                });
            }
        }
        points.push(last as u32);
        if !forward {
            points.reverse();
        }
        points
    }

    /// Triangles between the corners `along` one side and `facing` the
    /// other (both in the same direction), wound as the face's first
    /// triangle (along[0], along[1], facing[1]) was.
    fn strip(&mut self, along: &[u32], facing: &[u32]) {
        let (mut i, mut j) = (0, 0);
        let (n, m) = (along.len() - 1, facing.len() - 1);
        while i < n || j < m {
            let advance_along =
                i < n && (j == m || (i + 1) as f32 / n as f32 <= (j + 1) as f32 / m as f32);
            if advance_along {
                self.face
                    .triangles
                    .push([along[i], along[i + 1], facing[j]]);
                i += 1;
            } else {
                self.face
                    .triangles
                    .push([along[i], facing[j + 1], facing[j]]);
                j += 1;
            }
        }
    }

    /// Triangles from `apex` to each step `along`.
    fn fan(&mut self, apex: u32, along: &[u32]) {
        for pair in along.windows(2) {
            self.face.triangles.push([apex, pair[0], pair[1]]);
        }
    }
}

/// `positions` and `normals` (unit, or zero) of a face's corners, in order,
/// split onto `frame`'s circle, `pieces(turn, distance)` cutting a rim edge
/// that turns `turn` round the axis at `distance` from it, with its `lugs`
/// as geometry where it has them: none when the face has no rim edge to
/// cut, or is no shape the split knows, and stays as it is.
pub(crate) fn split_face(
    frame: &PartFrame,
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    pieces: &dyn Fn(f32, f32) -> u32,
    lugs: Option<&Lugs<'_>>,
) -> Option<SplitFace> {
    // A corner repeating the one before it (a quad drawn as a triangle)
    // drops out.
    let mut order: Vec<usize> = Vec::with_capacity(positions.len());
    for (index, position) in positions.iter().enumerate() {
        if order
            .last()
            .is_some_and(|&last| positions[last] == *position)
        {
            continue;
        }
        order.push(index);
    }
    if order.len() > 1 && positions[order[0]] == positions[*order.last()?] {
        order.pop();
    }
    let count = order.len();
    if count < 3 {
        return None;
    }
    let mut splitter = Splitter {
        frame,
        positions,
        normals,
        shape: positions.iter().map(|&p| frame.cylindrical(p)).collect(),
        pieces,
        lugs,
        face: SplitFace {
            corners: (0..positions.len())
                .map(|corner| SplitCorner {
                    blend: Blend::Corner(corner),
                    position: positions[corner],
                    normal: normals[corner],
                })
                .collect(),
            triangles: Vec::new(),
        },
    };
    let rims: Vec<bool> = (0..count)
        .map(|k| splitter.rim(order[k], order[(k + 1) % count]).is_some())
        .collect();
    let corner = |k: usize| order[k % count];
    // The face's own corners on a lugged ring sink into their grooves too,
    // tilting with the slope along their rim edge here, if they have one.
    let mut sunk = false;
    for k in 0..count {
        let here = corner(k);
        if !splitter.lugged(here) {
            continue;
        }
        sunk = true;
        let along = [(corner(k + count - 1), here), (here, corner(k + 1))]
            .into_iter()
            .find_map(|(from, to)| {
                let turn = splitter.rim(from, to)?;
                splitter.turning(from, to, turn)
            })
            .unwrap_or(0.0);
        let u = lugs.map_or(0.0, |lugs| lugs.u[here]);
        let shape = splitter.shape[here];
        let (position, tilt) = splitter.sunk(shape, u, along);
        let normal = splitter.normal(here, here, 0.0, shape[2], tilt);
        splitter.face.corners[here].position = position;
        splitter.face.corners[here].normal = normal;
    }
    // A corner on the part with a normal leaning round the axis carries its
    // face's own flat normal: it rounds as its new points do.
    for k in 0..count {
        let here = corner(k);
        let shape = splitter.shape[here];
        let round = frame.outward(shape[2] + std::f32::consts::FRAC_PI_2);
        let leaning = dot(normals[here], round).abs() > SIDEWAYS;
        if leaning && !splitter.lugged(here) && rims.iter().any(|&rim| rim) && frame.holds(shape) {
            splitter.face.corners[here].normal = splitter.normal(here, here, 0.0, shape[2], 0.0);
        }
    }
    if !rims.contains(&true) {
        // No rim edge to cut; a corner that sank still takes its place.
        if !sunk {
            return None;
        }
        for k in 1..count - 1 {
            let triangle = [corner(0), corner(k), corner(k + 1)].map(|c| c as u32);
            splitter.face.triangles.push(triangle);
        }
        return Some(splitter.face);
    }
    if rims.iter().all(|&rim| rim) {
        // An end cap: a fan from its middle.
        let height = order.iter().map(|&c| splitter.shape[c][0]).sum::<f32>() / count as f32;
        let normal = unit(order.iter().fold([0.0; 3], |sum, &c| {
            std::array::from_fn(|axis| sum[axis] + normals[c][axis])
        }))
        .unwrap_or([0.0; 3]);
        let middle = splitter.face.corners.len() as u32;
        splitter.face.corners.push(SplitCorner {
            blend: Blend::Middle,
            position: frame.point([height, 0.0, 0.0]),
            normal,
        });
        for k in 0..count {
            let along = splitter.edge(corner(k), corner(k + 1));
            splitter.fan(middle, &along);
        }
    } else if count == 4 && (!rims[1] && !rims[3] || !rims[0] && !rims[2]) {
        // A strip between the face's two opposite edges, at least one a rim
        // edge and the others straight: rotated so they run 0 to 1 and 3
        // to 2.
        let start = usize::from(!(rims[0] || rims[2]));
        let along = splitter.edge(corner(start), corner(start + 1));
        let facing = splitter.edge(corner(start + 3), corner(start + 2));
        splitter.strip(&along, &facing);
    } else if count == 3 && rims.iter().filter(|&&rim| rim).count() == 1 {
        // A fan from the corner off the rim edge.
        let edge = rims.iter().position(|&rim| rim)?;
        let along = splitter.edge(corner(edge), corner(edge + 1));
        splitter.fan(corner(edge + 2) as u32, &along);
    } else {
        // A flat piece of an end cap (rim edges and chords, all its corners
        // at one height): a fan from its own middle.
        let heights: Vec<f32> = order.iter().map(|&c| splitter.shape[c][0]).collect();
        let lowest = heights.iter().copied().fold(f32::MAX, f32::min);
        let highest = heights.iter().copied().fold(f32::MIN, f32::max);
        if highest - lowest > HEIGHT_SLACK {
            return None;
        }
        let middle_at: [f32; 3] = std::array::from_fn(|axis| {
            order.iter().map(|&c| positions[c][axis]).sum::<f32>() / count as f32
        });
        let normal = unit(order.iter().fold([0.0; 3], |sum, &c| {
            std::array::from_fn(|axis| sum[axis] + normals[c][axis])
        }))
        .unwrap_or([0.0; 3]);
        let middle = splitter.face.corners.len() as u32;
        splitter.face.corners.push(SplitCorner {
            blend: Blend::Middle,
            position: middle_at,
            normal,
        });
        for k in 0..count {
            let along = splitter.edge(corner(k), corner(k + 1));
            splitter.fan(middle, &along);
        }
    }
    Some(splitter.face)
}

/// A face of a sphere part split onto its sphere round `centre`: each
/// triangle of the face (a fan from its first corner) halves an edge that
/// turns more than a piece may round the centre (the widest that stays
/// within `tolerance` pixels of the sphere, a unit `pixels` wide on
/// screen), at most `most` pieces an edge. Every new point is pushed out to
/// the sphere, at its edge's distance from the centre, and every corner
/// faces out from it. None when the face's corners stand at different
/// distances from the centre (no part of the sphere) or no edge needs a
/// cut. An edge is halved from its ends alone, so the faces on it cut it
/// alike.
pub(crate) fn split_sphere(
    centre: [f32; 3],
    positions: &[[f32; 3]],
    pixels: f32,
    tolerance: f32,
    most: u32,
) -> Option<SplitFace> {
    let mut order: Vec<usize> = Vec::with_capacity(positions.len());
    for (index, position) in positions.iter().enumerate() {
        if !order.iter().any(|&known| positions[known] == *position) {
            order.push(index);
        }
    }
    if order.len() < 3 {
        return None;
    }
    let offset = |point: [f32; 3]| -> [f32; 3] { std::array::from_fn(|c| point[c] - centre[c]) };
    let distances: Vec<f32> = order
        .iter()
        .map(|&corner| dot(offset(positions[corner]), offset(positions[corner])).sqrt())
        .collect();
    let far = distances.iter().copied().fold(0.0, f32::max);
    let near = distances.iter().copied().fold(f32::MAX, f32::min);
    if near <= OFF_AXIS || far - near > DISTANCE_SLACK * far || far * pixels <= tolerance {
        return None;
    }
    let mut sphere = Sphere {
        centre,
        widest: 2.0 * (1.0 - tolerance / (far * pixels)).acos(),
        // Each halving doubles an edge's pieces.
        deepest: most.max(1).ilog2(),
        face: SplitFace {
            corners: positions
                .iter()
                .enumerate()
                .map(|(corner, &position)| SplitCorner {
                    blend: Blend::Corner(corner),
                    position,
                    normal: unit(offset(position)).unwrap_or([0.0; 3]),
                })
                .collect(),
            triangles: Vec::new(),
        },
        weights: (0..positions.len())
            .map(|corner| {
                (0..positions.len())
                    .map(|k| f32::from(u8::from(k == corner)))
                    .collect()
            })
            .collect(),
        halves: Default::default(),
    };
    let count = order.len();
    if (0..count).all(|k| !sphere.cut(order[k] as u32, order[(k + 1) % count] as u32, 0)) {
        return None;
    }
    for k in 1..count - 1 {
        sphere.triangle(
            [order[0], order[k], order[k + 1]].map(|corner| corner as u32),
            0,
        );
    }
    Some(sphere.face)
}

struct Sphere {
    centre: [f32; 3],
    widest: f32,
    deepest: u32,
    face: SplitFace,
    /// Each corner's share of each of the face's own corners.
    weights: Vec<Vec<f32>>,
    /// The point halving each edge cut so far, by its ends.
    halves: crate::fast_hash::FastMap<(u32, u32), u32>,
}

impl Sphere {
    /// Whether the edge from `a` to `b`, met `depth` halvings in, is cut.
    fn cut(&self, a: u32, b: u32, depth: u32) -> bool {
        let [p, q] = [a, b].map(|corner| {
            let position = self.face.corners[corner as usize].position;
            std::array::from_fn(|c| position[c] - self.centre[c])
        });
        let cosine = dot(p, q) / (dot(p, p) * dot(q, q)).sqrt();
        depth < self.deepest && cosine.clamp(-1.0, 1.0).acos() > self.widest
    }

    /// The point halving the edge from `a` to `b`, on the sphere.
    fn half(&mut self, a: u32, b: u32) -> u32 {
        if let Some(&half) = self.halves.get(&(a.min(b), a.max(b))) {
            return half;
        }
        let [p, q] = [a, b].map(|corner| self.face.corners[corner as usize].position);
        let length = |point: [f32; 3]| {
            let offset: [f32; 3] = std::array::from_fn(|c| point[c] - self.centre[c]);
            dot(offset, offset).sqrt()
        };
        let distance = (length(p) + length(q)) * 0.5;
        let middle: [f32; 3] = std::array::from_fn(|c| (p[c] + q[c]) * 0.5 - self.centre[c]);
        let out = unit(middle).unwrap_or([0.0; 3]);
        let weights: Vec<f32> = self.weights[a as usize]
            .iter()
            .zip(&self.weights[b as usize])
            .map(|(a, b)| (a + b) * 0.5)
            .collect();
        let mut mix = [(0, 0.0); 3];
        for (slot, (corner, &weight)) in weights
            .iter()
            .enumerate()
            .filter(|(_, &weight)| weight > 0.0)
            .take(3)
            .enumerate()
        {
            mix[slot] = (corner, weight);
        }
        let half = self.face.corners.len() as u32;
        self.face.corners.push(SplitCorner {
            blend: Blend::Mix(mix),
            position: std::array::from_fn(|c| self.centre[c] + out[c] * distance),
            normal: out,
        });
        self.weights.push(weights);
        self.halves.insert((a.min(b), a.max(b)), half);
        half
    }

    /// Triangle `a`, `b`, `c`, met `depth` halvings in, cut where its
    /// edges need it, each piece wound as it was.
    fn triangle(&mut self, [a, b, c]: [u32; 3], depth: u32) {
        let next = depth + 1;
        match [
            self.cut(a, b, depth),
            self.cut(b, c, depth),
            self.cut(c, a, depth),
        ] {
            [false, false, false] => self.face.triangles.push([a, b, c]),
            [true, true, true] => {
                let [ab, bc, ca] = [self.half(a, b), self.half(b, c), self.half(c, a)];
                for piece in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
                    self.triangle(piece, next);
                }
            }
            [true, false, false] => {
                let ab = self.half(a, b);
                self.triangle([a, ab, c], next);
                self.triangle([ab, b, c], next);
            }
            [false, true, false] => {
                let bc = self.half(b, c);
                self.triangle([a, b, bc], next);
                self.triangle([a, bc, c], next);
            }
            [false, false, true] => {
                let ca = self.half(c, a);
                self.triangle([a, b, ca], next);
                self.triangle([ca, b, c], next);
            }
            [true, true, false] => {
                let [ab, bc] = [self.half(a, b), self.half(b, c)];
                for piece in [[ab, b, bc], [a, ab, bc], [a, bc, c]] {
                    self.triangle(piece, next);
                }
            }
            [false, true, true] => {
                let [bc, ca] = [self.half(b, c), self.half(c, a)];
                for piece in [[bc, c, ca], [a, b, bc], [a, bc, ca]] {
                    self.triangle(piece, next);
                }
            }
            [true, false, true] => {
                let [ab, ca] = [self.half(a, b), self.half(c, a)];
                for piece in [[a, ab, ca], [ab, b, c], [ab, c, ca]] {
                    self.triangle(piece, next);
                }
            }
        }
    }
}

/// A model the tuning names round, as a frame's draws take it: its part,
/// the vertex slots its tread faces use (sorted; a mirrored face's twins
/// included) and the first tread sprite they show.
#[derive(Clone, Debug)]
pub(crate) struct RoundModel {
    pub part: crate::modern_tuning::RoundPart,
    pub tread_slots: Vec<u16>,
    pub tread_sprite: Option<i16>,
}

impl RoundModel {
    /// `part`, with the slots of `program`'s faces in one of `sprites`.
    pub(crate) fn new(
        part: crate::modern_tuning::RoundPart,
        program: &inf_formats::ModelProgram,
        sprites: &[i16],
    ) -> Self {
        fn walk(
            program: &inf_formats::ModelProgram,
            sprites: &[i16],
            slots: &mut Vec<u16>,
            sprite: &mut Option<i16>,
        ) {
            use inf_formats::ModelInstructionKind as Kind;
            for instruction in &program.instructions {
                let (material, corners, mirror): (_, Vec<u16>, _) = match &instruction.kind {
                    Kind::Triangle(face) => (
                        face.material,
                        face.vertices.iter().map(|slot| slot.raw()).collect(),
                        face.flags.emits_mirror_instance(),
                    ),
                    Kind::Quad(face) => (
                        face.material,
                        face.vertices.iter().map(|slot| slot.raw()).collect(),
                        face.flags.emits_mirror_instance(),
                    ),
                    Kind::TriangleFan(face) => (
                        face.material,
                        face.vertices
                            .iter()
                            .map(|vertex| vertex.vertex.raw())
                            .collect(),
                        face.flags.emits_mirror_instance(),
                    ),
                    Kind::NestedProgram(nested) => {
                        walk(&nested.program, sprites, slots, sprite);
                        continue;
                    }
                    _ => continue,
                };
                let tread = matches!(
                    material.domain,
                    inf_formats::ModelMaterialDomain::Handler3Texture
                ) && sprites.contains(&material.global_id);
                if !tread {
                    continue;
                }
                sprite.get_or_insert(material.global_id);
                for corner in corners {
                    slots.push(corner);
                    if mirror {
                        slots.push(corner ^ 1);
                    }
                }
            }
        }
        let (mut tread_slots, mut tread_sprite) = (Vec::new(), None);
        walk(program, sprites, &mut tread_slots, &mut tread_sprite);
        tread_slots.sort_unstable();
        tread_slots.dedup();
        Self {
            part,
            tread_slots,
            tread_sprite,
        }
    }
}

/// The tread's height at texture `u` (0 to 1 across its sprite, repeating),
/// linear between the middles of `profile`'s columns, and its slope along
/// u: the same at u 0 and 1, so a facet's joint sinks alike from both.
pub(crate) fn tread_height(profile: &[f32], u: f32) -> [f32; 2] {
    if profile.is_empty() {
        return [1.0, 0.0];
    }
    let width = profile.len() as f32;
    let at = u * width - 0.5;
    let low = at.floor();
    let fraction = at - low;
    let column = |index: f32| profile[(index as i64).rem_euclid(profile.len() as i64) as usize];
    let [a, b] = [column(low), column(low + 1.0)];
    [a + (b - a) * fraction, (b - a) * width]
}

/// How many pieces a rim edge turning `turn` round the axis at `pixels`
/// from it on screen takes, so each lies within `tolerance` pixels of the
/// circle: at most `most`.
pub(crate) fn pieces(turn: f32, pixels: f32, tolerance: f32, most: u32) -> u32 {
    if pixels <= tolerance || tolerance <= 0.0 {
        return 1;
    }
    let widest = 2.0 * (1.0 - tolerance / pixels).acos();
    ((turn / widest).ceil() as u32).clamp(1, most.max(1))
}

#[cfg(test)]
#[path = "round_parts_tests.rs"]
mod tests;
