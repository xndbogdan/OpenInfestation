//! Native Direct3D TL-vertex clipping at 004BCCE0, 004BDA00, 004BE4E0,
//! and 004BEFD0. These passes run before triangle-fan device submission.
//!
//! Arithmetic follows startup's x87 53-bit, nearest-even mode, including
//! float spills. Other FPU modes and nonfinite input are not admitted here.
//! GPU raster precision is separate from these CPU vertex operations.

use thiserror::Error;

/// Native D3DTLVERTEX: screen XY, transformed depth, reciprocal view depth,
/// packed diffuse/specular ARGB, and allocation-relative texture UV.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct D3dTlVertex {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
    pub rhw: f32,
    /// RGBA byte order in Rust; native memory contains packed ARGB.
    pub diffuse: [u8; 4],
    pub specular: u32,
    pub uv: [f32; 2],
}

impl D3dTlVertex {
    pub(crate) fn finite(self) -> bool {
        [self.x, self.y, self.depth, self.rhw, self.uv[0], self.uv[1]]
            .into_iter()
            .all(f32::is_finite)
    }

    /// A packet depth of zero: the conversions load it with FILD and divide
    /// one by it (`004D5603`), so `rhw` is infinite and the depth
    /// `1 - near * rhw` infinite or NaN, while the screen position, colours
    /// and texture coordinates stay as converted.
    pub(crate) fn at_depth_pole(self) -> bool {
        self.rhw.is_infinite()
            && [self.x, self.y, self.uv[0], self.uv[1]]
                .into_iter()
                .all(f32::is_finite)
    }
}

/// Whether every triangle of a fan covers no screen area. D3D rasterizes no
/// pixel of such a triangle, so its depth, colours and texture coordinates
/// are never consumed. A non-finite position is never zero area.
pub(crate) fn zero_screen_area(vertices: &[D3dTlVertex], triangles: &[[usize; 3]]) -> bool {
    triangles.iter().all(|&[a, b, c]| {
        let [a, b, c] = [vertices[a], vertices[b], vertices[c]];
        (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y) == 0.0
    })
}

/// ReciprocalDepthPoleApproximation: a textured primitive whose every
/// corner sits at packet depth 0 and that covers screen area. Retail hands
/// D3D infinite `rhw` and depth for it, whose result the driver decides;
/// the port draws nothing. Explosion debris, drawn 250 nearer than it lies,
/// reaches it 250 from the eye.
pub(crate) fn log_depth_pole_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("ReciprocalDepthPoleApproximation: a textured primitive at packet depth 0 covers screen area; retail's 1/z is infinite (004D5603) and D3D's result is the driver's, so the port draws nothing (PIXEL_RUNTIME.md)");
    }
}

/// ReciprocalDepthSignApproximation: a textured primitive whose corners'
/// packet depths have both signs and that covers screen area. Retail hands
/// D3D their mixed `rhw` (004D5603) and the driver decides; the port draws
/// nothing for it and goes on with the frame. A far sprite across the
/// world's 16-bit wrap, half a world away, reaches it: two corners wrap
/// behind the eye.
pub(crate) fn log_depth_sign_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("ReciprocalDepthSignApproximation: a textured primitive's packet depths have both signs (a far primitive across the world's 16-bit wrap, half a world away); retail's 1/z is mixed (004D5603) and D3D's result is the driver's, so the port draws nothing for it (PIXEL_RUNTIME.md)");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D3dViewportState {
    /// Signed renderer WORDs at +48/+4A/+4C/+4E, inclusive.
    pub bounds: [i16; 4],
    /// Unsigned renderer +38 used to recompute depth at each intersection.
    pub near_depth: u32,
    /// The complete native mode; bit 100 selects affine UV interpolation.
    /// Bit 20 is the caller's bypass, not interpreted by 004BCCE0 itself.
    pub mode: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum D3dViewportError {
    #[error("native viewport adapter requires a triangle or quad, received {0} vertices")]
    VertexCount(usize),
    #[error("native viewport bounds are inverted")]
    InvertedBounds,
    #[error("native viewport adapter requires finite input and output attributes")]
    NonFinite,
    #[error("clipped polygon exceeds the native ten-vertex temporary buffer")]
    Capacity,
}

/// Clip a submitted triangle/quad in native order: top, left, bottom, right.
/// Wholly admitted vertices are copied exactly, including their original
/// depth. Intersections recompute depth and round color *deltas* before adding
/// the starting byte. Rounding a final interpolated color is not equivalent.
///
/// Native callers consume polygons with at least three vertices as triangle
/// fans. Empty, point, and line outputs are preserved for that decision.
pub fn clip_polygon(
    vertices: &[D3dTlVertex],
    state: D3dViewportState,
) -> Result<Vec<D3dTlVertex>, D3dViewportError> {
    let mut polygon = ClippedPolygon::new();
    clip_polygon_into(vertices, state, &mut polygon)?;
    Ok(polygon.to_vec())
}

/// A clipped polygon on the stack: at most ten vertices, and two more
/// before [`D3dViewportError::Capacity`] is checked.
pub(crate) type ClippedPolygon = tinyvec::ArrayVec<[D3dTlVertex; 12]>;

/// [`clip_polygon`] into `out` (emptied first), without a heap allocation.
pub(crate) fn clip_polygon_into(
    vertices: &[D3dTlVertex],
    state: D3dViewportState,
    out: &mut ClippedPolygon,
) -> Result<(), D3dViewportError> {
    check_shape(vertices, state)?;
    if vertices.iter().any(|v| !v.finite()) {
        return Err(D3dViewportError::NonFinite);
    }
    clip_finite_into(vertices, state, out)
}

fn check_shape(vertices: &[D3dTlVertex], state: D3dViewportState) -> Result<(), D3dViewportError> {
    if !(3..=4).contains(&vertices.len()) {
        return Err(D3dViewportError::VertexCount(vertices.len()));
    }
    let [left, top, right, bottom] = state.bounds;
    if left > right || top > bottom {
        return Err(D3dViewportError::InvertedBounds);
    }
    Ok(())
}

/// [`clip_polygon_into`] for vertices the caller has found
/// [`D3dTlVertex::finite`].
pub(crate) fn clip_finite_into(
    vertices: &[D3dTlVertex],
    state: D3dViewportState,
    out: &mut ClippedPolygon,
) -> Result<(), D3dViewportError> {
    check_shape(vertices, state)?;
    let [left, top, right, bottom] = state.bounds;
    out.clear();
    out.extend_from_slice(vertices);
    let planes = [
        Plane::minimum(Axis::Y, top),
        Plane::minimum(Axis::X, left),
        Plane::maximum(Axis::Y, bottom),
        Plane::maximum(Axis::X, right),
    ];
    // Every pass copies a polygon inside its plane unchanged.
    if vertices
        .iter()
        .all(|&v| planes.iter().all(|plane| plane.contains(v)))
    {
        return Ok(());
    }
    // Two lists, each pass filling one from the other; four passes end in
    // `out`.
    let mut scratch = ClippedPolygon::new();
    let (mut polygon, mut output) = (out, &mut scratch);
    // Each pass starts at the original first vertex. Putting the closing
    // edge first rotates the polygon and changes native fan triangulation.
    for plane in planes {
        output.clear();
        for (index, &start) in polygon.iter().enumerate() {
            let end = polygon[(index + 1) % polygon.len()];
            let start_inside = plane.contains(start);
            if start_inside {
                output.push(start);
            }
            // A boundary endpoint is already copied on its own iteration;
            // retail suppresses the duplicate intersection at t=0 or t=1.
            if start_inside != plane.contains(end)
                && plane.coordinate(start) != plane.boundary
                && plane.coordinate(end) != plane.boundary
            {
                output.push(intersection(start, end, plane, state)?);
            }
            if output.len() > 10 {
                return Err(D3dViewportError::Capacity);
            }
        }
        std::mem::swap(&mut polygon, &mut output);
        if polygon.is_empty() {
            // `out` may be the other list: empty both.
            output.clear();
            break;
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

#[derive(Clone, Copy)]
struct Plane {
    axis: Axis,
    boundary: f32,
    minimum: bool,
}

impl Plane {
    fn minimum(axis: Axis, boundary: i16) -> Self {
        Self {
            axis,
            boundary: f32::from(boundary),
            minimum: true,
        }
    }
    fn maximum(axis: Axis, boundary: i16) -> Self {
        Self {
            axis,
            boundary: f32::from(boundary),
            minimum: false,
        }
    }
    fn coordinate(self, vertex: D3dTlVertex) -> f32 {
        match self.axis {
            Axis::X => vertex.x,
            Axis::Y => vertex.y,
        }
    }
    fn contains(self, vertex: D3dTlVertex) -> bool {
        if self.minimum {
            self.coordinate(vertex) >= self.boundary
        } else {
            self.coordinate(vertex) <= self.boundary
        }
    }
}

fn intersection(
    start: D3dTlVertex,
    end: D3dTlVertex,
    plane: Plane,
    state: D3dViewportState,
) -> Result<D3dTlVertex, D3dViewportError> {
    let coordinate = f64::from(plane.coordinate(start));
    let fraction =
        (f64::from(plane.boundary) - coordinate) / (f64::from(plane.coordinate(end)) - coordinate);
    let lerp = |a: f64, b: f64| (b - a) * fraction + a;
    // FST leaves the unrounded reciprocal depth on the x87 stack. Both
    // depth recomputation and the reciprocal below consume that value.
    let rhw = lerp(f64::from(start.rhw), f64::from(end.rhw));
    let uv = if state.mode & 0x100 != 0 {
        std::array::from_fn(|i| lerp(start.uv[i].into(), end.uv[i].into()) as f32)
    } else {
        // 004BCF9B..004BCFA3 spills 1/rhw to float before use. At
        // 004BD401..004BD40D U*rhw stays in a register; V*rhw spills.
        let reciprocal = f64::from((1.0 / rhw) as f32);
        let start_u = f64::from(start.uv[0]) * f64::from(start.rhw);
        let start_v = f64::from((f64::from(start.uv[1]) * f64::from(start.rhw)) as f32);
        [
            (lerp(start_u, f64::from(end.uv[0]) * f64::from(end.rhw)) * reciprocal) as f32,
            (lerp(start_v, f64::from(end.uv[1]) * f64::from(end.rhw)) * reciprocal) as f32,
        ]
    };
    let channel = |a: u8, b: u8| {
        (i32::from(a)
            + (f64::from(i32::from(b) - i32::from(a)) * fraction).round_ties_even() as i32)
            as u8
    };
    let mut vertex = D3dTlVertex {
        x: lerp(start.x.into(), end.x.into()) as f32,
        y: lerp(start.y.into(), end.y.into()) as f32,
        depth: (1.0 - f64::from(state.near_depth) * rhw) as f32,
        rhw: rhw as f32,
        diffuse: std::array::from_fn(|i| channel(start.diffuse[i], end.diffuse[i])),
        specular: u32::from_le_bytes(std::array::from_fn(|i| {
            channel(
                start.specular.to_le_bytes()[i],
                end.specular.to_le_bytes()[i],
            )
        })),
        uv,
    };
    match plane.axis {
        Axis::X => vertex.x = plane.boundary,
        Axis::Y => vertex.y = plane.boundary,
    }
    if !vertex.finite() {
        return Err(D3dViewportError::NonFinite);
    }
    Ok(vertex)
}

#[cfg(test)]
#[path = "d3d_viewport_tests.rs"]
mod tests;
