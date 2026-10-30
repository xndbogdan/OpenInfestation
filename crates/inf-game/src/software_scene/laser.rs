//! The laser's 0048FE60 and beam 00485520 on this scene: its quads enter the
//! shared command arena like the barrier's, and its jitter draws the
//! process RNG.
use super::*;
use crate::model_context::{ModelContext, ModelDispatch};
use crate::model_vertices::ModelVertexProjector;
use crate::object_runtime::{
    laser31_draw, ribbon_sort_key, Laser31DrawError, ObjectArena, ObjectDrawCallbackBoundary,
    ObjectDrawContext, ObjectDrawServiceError, ObjectId, Selector69Projected, LASER_MODE,
    LASER_NEGATIVE_TEXTURE_SLOT, LASER_TEXTURE_SLOT,
};

fn stopped<E>(reason: impl ToString) -> ObjectDrawServiceError<E> {
    ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::Selector31LaserDraw {
        reason: reason.to_string(),
    })
}

impl SoftwareScene<'_, '_> {
    /// The sprite lookups come first, as at 00485529..004855AB; the context
    /// then projects at the laser's position, and each quad sorts against
    /// the renderer's current material policy.
    pub(super) fn draw_laser(
        &mut self,
        arena: &ObjectArena,
        laser: ObjectId,
        context: &ObjectDrawContext,
        rng: &mut u32,
    ) -> Result<(), ObjectDrawServiceError<SoftwareSceneError>> {
        let beam = arena.laser31_beam::<String>(laser).map_err(stopped)?;
        let slot = if beam.negative {
            LASER_NEGATIVE_TEXTURE_SLOT
        } else {
            LASER_TEXTURE_SLOT
        };
        let texture = if slot as usize > self.resources.pool_capacity(RenderResourceKind::Sprite) {
            None
        } else {
            Some(
                self.resources
                    .resolve(RenderResourceRequest {
                        kind: RenderResourceKind::Sprite,
                        argument: slot,
                    })
                    .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?,
            )
        };
        // 0048FE60 receives the laser position's sign-extended WORDs.
        let origin = arena
            .object(laser)
            .map_err(stopped)?
            .base
            .position_words
            .ok_or_else(|| stopped("the laser's position +32"))?
            .map(|word| i32::from(word as i16));
        let environment = self.environment;
        let mut projector = crate::model_projection::ModelProjectionBackend {
            transform: ModelContext::new(origin, crate::menu::MenuBasis::INITIAL)
                .publish(context.view, environment.coordinate_shift),
            view: context.view,
            projection: environment.projection,
            dispatch: ModelDispatch::Far,
            coordinate_shift: environment.coordinate_shift,
            fog_factor: environment.fog_factor,
        };
        let mut points = Vec::with_capacity(34);
        let project = |point: [i32; 3]| -> Result<Selector69Projected, ObjectDrawServiceError<_>> {
            points.push(point);
            let mut output = crate::model_vertices::ModelVertexProjection::default();
            let depth = match projector.project(
                crate::model_vertices::ModelVertexInput::Local(point),
                &mut output,
            ) {
                Ok(depth) => depth as i32,
                Err(error) => match error {},
            };
            Ok(Selector69Projected {
                xy: output
                    .read_u32(0)
                    .ok_or_else(|| stopped("the projection left its XY unwritten"))?,
                depth,
                clip: output.bytes[8]
                    .ok_or_else(|| stopped("the projection left byte 8 unwritten"))?,
                fog: output.bytes[9],
            })
        };
        let sorting = self.material_sorting();
        let queue = &mut self.queue;
        let identities = &mut self.texture_identities;
        let effect_color = self.primitive_state.effect_color;
        laser31_draw(
            beam,
            &self.tables.sine_table,
            &self.tables.clip_acceptance,
            rng,
            project,
            |quad| {
                let sort = ribbon_sort_key(quad.depth, LASER_MODE, texture, sorting, |texture| {
                    identities.resolve(texture)
                })
                .map_err(stopped)?;
                let mut command =
                    quad.command(sort, LASER_MODE, texture, effect_color, context.far_color);
                command.glow_fed = true;
                queue
                    .submit(DrawSubmission {
                        depth: command.depth,
                        payload_blocks: command.payload_blocks,
                        receiver_offset: 0,
                        command,
                    })
                    .map(|_| ())
                    .map_err(|error| ObjectDrawServiceError::Backend(error.into()))
            },
        )
        .map_err(|error| match error {
            Laser31DrawError::Host(error) => error,
            other => stopped(other),
        })?;
        // Under the tracer the beam lights and glints as a continuous line
        // along its aim, and its two 16-segment 3D strips enter the mirrored
        // emitters whole (even when the muzzle starts behind the near plane
        // in first person or a segment lies off screen).
        let (strength, reach) = crate::modern_tuning::current().materials().shots;
        if let (Some(resource), Some(builder)) = (texture, self.scene_feed_mut()) {
            let sprite =
                builder.sprite(slot as i16, resource, inf_formats::SpriteRenderMode::Mode0);
            let eye = builder.feed.camera.eye;
            let focal = builder.feed.camera.focal[0].max(1.0);
            let fog_end = builder.feed.fog.end;
            let wrapped = |value: f64| (value + 32768.0).rem_euclid(65536.0) - 32768.0;
            let to_eye = |point: [i32; 3]| -> [f32; 3] {
                let world: [f64; 3] =
                    std::array::from_fn(|axis| f64::from(origin[axis] + point[axis]));
                [
                    wrapped(world[0] - eye[0]) as f32,
                    (world[1] - eye[1]) as f32,
                    wrapped(world[2] - eye[2]) as f32,
                ]
            };
            if strength > 0.0 && reach > 0.0 {
                let along = |share: f64| -> [f32; 3] {
                    let world: [f64; 3] = std::array::from_fn(|axis| {
                        f64::from(origin[axis]) + f64::from(beam.aim[axis]) * share
                    });
                    [
                        wrapped(world[0] - eye[0]) as f32,
                        (world[1] - eye[1]) as f32,
                        wrapped(world[2] - eye[2]) as f32,
                    ]
                };
                builder.lone_glow(
                    along(0.15),
                    crate::scene_feed::SceneGlow {
                        end: Some(along(0.98)),
                        texture: Some(sprite),
                        color: [255; 4],
                        strength: strength * 1.5,
                        reach: reach * 1.25,
                        model: crate::world_scene::SHOT_GLOWS,
                        ..Default::default()
                    },
                );
            }
            for strip in points.chunks_exact(17) {
                for pair in strip.windows(2) {
                    let a = to_eye(pair[0]);
                    let b = to_eye(pair[1]);
                    let mid: [f32; 3] = std::array::from_fn(|axis| 0.5 * (a[axis] + b[axis]));
                    let d2: f32 = mid.iter().map(|axis| axis * axis).sum();
                    if d2 > fog_end * fog_end {
                        continue;
                    }
                    let radius = (3.5 * d2.sqrt() / focal).clamp(6.0, 28.0);
                    builder.feed.emitters.push(crate::scene_feed::SceneEmitter {
                        position: a,
                        span: std::array::from_fn(|axis| b[axis] - a[axis]),
                        radius,
                        texture: Some(sprite),
                        color: [255; 3],
                    });
                }
            }
        }
        Ok(())
    }
}
