//! Moving models' own motion for the Vulkan raster without ray tracing
//! (plan item 23).
//!
//! The upscalers and TAA follow each pixel to where it was in the last
//! frame. The raster's prepare pass knows only depth, so every pixel moves
//! with the camera, and a moving model (the ACAM, a grenade) trails. Each
//! object's models are remembered by their view transforms instead: where a
//! model stood in the last view against where it stands in this one maps any
//! point of it from this view to the last ([`ViewMotion`]). The game's state
//! is only read.

use crate::fast_hash::FastMap as HashMap;

use crate::{
    model_context::ModelTransform,
    scene_feed::{MotionKey, ObjectDraws, SceneCamera},
};

/// A point in this frame's view space to where it was in the last frame's
/// view space: three rows of a 3x4 matrix, the last column the move.
pub type ViewMotion = [[f32; 4]; 3];

/// The most a model moves between two frames and still counts as motion,
/// in world units: past it, a respawn or a jump, which starts again (as the
/// tracer's vertex history).
const MOST_MOTION: f64 = 2048.0;

/// Where a model stood: its view transform, and its origin in the world.
#[derive(Clone, Copy, Debug)]
struct Placed {
    transform: ModelTransform,
    origin: [f64; 3],
}

/// Each drawn object's models as they stood in the last raster frame. The
/// frame keeps it between worlds.
#[derive(Debug, Default)]
pub struct RasterMotion {
    camera: Option<SceneCamera>,
    drawing: ObjectDraws,
    last: HashMap<MotionKey, Placed>,
    now: HashMap<MotionKey, Placed>,
}

impl RasterMotion {
    /// A world's draws begin, seen with `camera`.
    pub fn begin_frame(&mut self, camera: SceneCamera) {
        self.camera = Some(camera);
    }

    /// The world is drawn: its models' places become the last frame's.
    pub fn finish_frame(&mut self) {
        self.last = std::mem::take(&mut self.now);
        self.drawing.end();
    }

    /// `object`'s models start drawing.
    pub fn begin_object(&mut self, object: crate::object_runtime::ObjectId) {
        self.drawing.begin(object);
    }

    /// The object's models are drawn.
    pub fn end_object(&mut self) {
        self.drawing.end();
    }

    /// The motion of model `model`, drawn in `transform` by the object
    /// drawing now: none outside an object's draw, on its first frame, past
    /// a jump, and for a transform that cannot be undone.
    pub fn motion(&mut self, model: u16, transform: ModelTransform) -> Option<ViewMotion> {
        let camera = self.camera?;
        let key = self.drawing.key(model, transform)?;
        let placed = Placed {
            transform,
            origin: world_origin(&camera, transform.view_position),
        };
        self.now.insert(key, placed);
        let then = *self.last.get(&key)?;
        let moved: [f64; 3] = std::array::from_fn(|axis| {
            let delta = then.origin[axis] - placed.origin[axis];
            if axis == 1 {
                delta
            } else {
                (delta + 32768.0).rem_euclid(65536.0) - 32768.0
            }
        });
        if moved.iter().any(|delta| delta.abs() > MOST_MOTION) {
            return None;
        }
        view_motion(transform, then.transform)
    }
}

/// The world position of view point `view` seen with `camera`.
fn world_origin(camera: &SceneCamera, view: [i32; 3]) -> [f64; 3] {
    let [right, up, forward] = camera.basis;
    std::array::from_fn(|axis| {
        camera.eye[axis]
            + f64::from(right[axis]) * f64::from(view[0])
            + f64::from(up[axis]) * f64::from(view[1])
            + f64::from(forward[axis]) * f64::from(view[2])
    })
}

/// A transform's 3x3 part: a model's local point to view space is
/// `basis * local + view_position`, the composed vectors being columns
/// (`model_projection.rs`).
fn columns(transform: ModelTransform) -> [[f64; 3]; 3] {
    std::array::from_fn(|axis| {
        std::array::from_fn(|source| {
            f64::from(f32::from_bits(
                transform.view_basis.words[1 + source * 3 + axis],
            ))
        })
    })
}

/// The map from `now`'s view point of a model to the view point `then` gave
/// it: `then * now^-1`, none when `now`'s basis is degenerate.
fn view_motion(now: ModelTransform, then: ModelTransform) -> Option<ViewMotion> {
    let a = columns(now);
    let cofactor = |row: usize, column: usize| {
        let [r0, r1] = [(row + 1) % 3, (row + 2) % 3];
        let [c0, c1] = [(column + 1) % 3, (column + 2) % 3];
        a[r0][c0] * a[r1][c1] - a[r0][c1] * a[r1][c0]
    };
    let determinant = (0..3)
        .map(|column| a[0][column] * cofactor(0, column))
        .sum::<f64>();
    if determinant.abs() < 1.0e-6 {
        return None;
    }
    let inverse: [[f64; 3]; 3] = std::array::from_fn(|row| {
        std::array::from_fn(|column| cofactor(column, row) / determinant)
    });
    let b = columns(then);
    let turn: [[f64; 3]; 3] = std::array::from_fn(|row| {
        std::array::from_fn(|column| (0..3).map(|k| b[row][k] * inverse[k][column]).sum())
    });
    let [from, to] = [now, then].map(|transform| transform.view_position.map(f64::from));
    Some(std::array::from_fn(|row| {
        let moved = to[row] - (0..3).map(|k| turn[row][k] * from[k]).sum::<f64>();
        [
            turn[row][0] as f32,
            turn[row][1] as f32,
            turn[row][2] as f32,
            moved as f32,
        ]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::MenuBasis;

    fn transform(position: [i32; 3], columns: [[f32; 3]; 3]) -> ModelTransform {
        let mut words = MenuBasis::INITIAL.words;
        for (source, column) in columns.iter().enumerate() {
            for (axis, value) in column.iter().enumerate() {
                words[1 + source * 3 + axis] = value.to_bits();
            }
        }
        ModelTransform {
            view_position: position,
            view_basis: MenuBasis { words },
        }
    }

    fn apply(motion: &ViewMotion, point: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|row| {
            (0..3)
                .map(|k| f64::from(motion[row][k]) * point[k])
                .sum::<f64>()
                + f64::from(motion[row][3])
        })
    }

    fn view_of(transform: ModelTransform, local: [f64; 3]) -> [f64; 3] {
        let a = columns(transform);
        std::array::from_fn(|axis| {
            (0..3).map(|k| a[axis][k] * local[k]).sum::<f64>()
                + f64::from(transform.view_position[axis])
        })
    }

    /// A turned and moved model's point maps from this view to where the
    /// last view saw the same local point.
    #[test]
    fn a_point_maps_to_where_its_model_stood() {
        let (sin, cos) = 0.3_f32.sin_cos();
        let now = transform(
            [40, -10, 900],
            [[cos, 0.0, -sin], [0.0, 1.0, 0.0], [sin, 0.0, cos]],
        );
        let then = transform(
            [10, -12, 950],
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        );
        let motion = view_motion(now, then).expect("an invertible basis");
        for local in [[0.0, 0.0, 0.0], [32.0, 5.0, -70.0], [-50.0, 20.0, 10.0]] {
            let mapped = apply(&motion, view_of(now, local));
            let expected = view_of(then, local);
            for axis in 0..3 {
                assert!(
                    (mapped[axis] - expected[axis]).abs() < 1.0e-3,
                    "{mapped:?} {expected:?}"
                );
            }
        }
    }

    /// A model is followed from its second frame, outside an object's draw
    /// nothing moves, and a jump past the most motion starts again.
    #[test]
    fn a_model_is_followed_from_its_second_frame() {
        let object = crate::object_runtime::ObjectArena::new()
            .allocate(crate::object_runtime::ObjectClassKind::VisualBase);
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut motion = RasterMotion::default();
        let camera = SceneCamera {
            basis: identity,
            ..SceneCamera::default()
        };
        for (frame, x) in [(0, 0), (1, 30), (2, 4000)] {
            motion.begin_frame(camera);
            assert!(motion.motion(7, transform([x, 0, 500], identity)).is_none());
            motion.begin_object(object);
            let found = motion.motion(7, transform([x, 0, 500], identity));
            motion.end_object();
            motion.finish_frame();
            match frame {
                1 => assert_eq!(found.map(|motion| motion[0][3]), Some(-30.0)),
                _ => assert!(found.is_none(), "frame {frame}"),
            }
        }
    }
}
