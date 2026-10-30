//! Captured object packets on explicit common49FEA0 or manager-late4ED610 roots.

use super::*;
use crate::frontend_camera::AnimationClockSource;
use crate::frontend_scene::{FrontendSceneHost, SceneCommand};
use crate::object_runtime::{
    Flag0ObjectDrawHost, ObjectArena, ObjectDrawCallbackBoundary, ObjectDrawContext,
    ObjectDrawServiceError, ObjectModelCallbacks, ObjectModelDrawSubmission, ObjectModelPacket,
};

/// The packet source, actual current caller context and root entry retain
/// separate custody. Packed values read the live arena and process clock.
pub struct SoftwareObjectModelDrawRequest<'a> {
    pub terrain: ModelTerrainRead<'a>,
    pub packet: ObjectModelPacket,
    pub root_mode: crate::model_execution::ModelRootMode,
    pub context: &'a ObjectDrawContext,
    pub arena: &'a mut ObjectArena,
    pub animation: &'a dyn AnimationClockSource,
    pub executable: inf_formats::FrontendExecutable<'a>,
}

/// One exclusive arena loan through a common/late root and all its children.
/// Getters create a fresh read view at each reach; custom callbacks retain real
/// actor writes. The caller context stays separate from model/child position.
pub(super) struct ObjectModelServiceLoan<'a> {
    pub arena: &'a mut ObjectArena,
    pub source: crate::object_runtime::ObjectId,
    pub context: &'a ObjectDrawContext,
    pub animation: &'a dyn AnimationClockSource,
    pub executable: inf_formats::FrontendExecutable<'a>,
    pub effects: Option<ObjectCustomEffectServices<'a>>,
    /// Port fix `guided_mounts` ([`crate::object_runtime::guided_mount_deploy`]).
    pub guided_mounts: bool,
}

/// The particle owners and world inputs a source+10 arm reaches, borrowed
/// from the manager's effect services. The RNG stays the model draw's loan.
pub struct ObjectCustomEffectServices<'a> {
    pub particles: &'a mut crate::process_particles::ProcessParticleRuntime,
    pub factory_process: &'a crate::process_particles::factories::ParticleFactoryProcessState,
    pub phase: Option<u32>,
    pub world: SoftwareCustomWorld<'a>,
    pub sine: Arc<[u16; 4096]>,
}

/// World inputs of trail-birthing and leg custom vertices.
#[derive(Clone, Copy)]
pub struct SoftwareCustomWorld<'a> {
    /// Raw signed DWORD 0055F154.
    pub load_factor: i32,
    pub terrain: &'a crate::terrain_runtime::TerrainRuntime,
    /// H14/0 header DWORD +4.
    pub flags: u32,
    /// Process byte 0053B890 as the manager pass began; only that pass's
    /// normal completion changes it.
    pub update_byte: u8,
}

struct ObjectCustomEffects<'e, 'a> {
    services: &'e mut ObjectCustomEffectServices<'a>,
    executable: inf_formats::FrontendExecutable<'a>,
    animation: &'a dyn AnimationClockSource,
    rng: Option<&'e mut u32>,
}

/// A type-0 wheel particle with flags 0 (00415EE1..00415F72).
fn wheel_particle(
    process: &crate::process_particles::factories::ParticleFactoryProcessState,
    phase: Option<u32>,
    position: [u16; 3],
    velocity: [u16; 3],
) -> crate::process_particles::births::ParticleBirthRequest<'_> {
    crate::process_particles::births::ParticleBirthRequest {
        particle_type: 0,
        position,
        velocity: Some(velocity),
        flags: 0,
        inputs: crate::process_particles::births::ParticleBirthInputs {
            network_active: process.network_flag != 0,
            phase,
            process,
        },
    }
}

impl crate::object_runtime::ObjectCustomVertexEffects for ObjectCustomEffects<'_, '_> {
    fn load_factor(&mut self) -> Result<i32, String> {
        Ok(self.services.world.load_factor)
    }

    #[track_caller]
    fn random(&mut self) -> Result<u32, String> {
        let rng = self
            .rng
            .as_deref_mut()
            .ok_or("custom trail needs the process RNG")?;
        Ok(crate::name_entry::name_entry_rng_next(rng))
    }

    fn birth_trail(
        &mut self,
        trail: crate::model_vertices::FurnitureCustomTrail,
    ) -> Result<(), String> {
        let rng = self
            .rng
            .as_deref_mut()
            .ok_or("custom trail needs the process RNG")?;
        self.services
            .particles
            .birth_trail(
                crate::process_particles::births::ParticleBirthRequest {
                    particle_type: trail.particle_type,
                    position: trail.position,
                    velocity: Some(trail.velocity),
                    flags: trail.flags,
                    inputs: crate::process_particles::births::ParticleBirthInputs {
                        network_active: self.services.factory_process.network_flag != 0,
                        phase: self.services.phase,
                        process: self.services.factory_process,
                    },
                },
                self.executable,
                rng,
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn birth_material_sprite(
        &mut self,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), String> {
        let request = wheel_particle(
            self.services.factory_process,
            self.services.phase,
            position,
            velocity,
        );
        let rng = self
            .rng
            .as_deref_mut()
            .ok_or("wheel dust needs the process RNG")?;
        self.services
            .particles
            .birth_material_sprite(request, false, self.executable, rng)
            .map(|_| ())
            .map_err(|error| format!("004894A0 wheel dust: {error}"))
    }

    fn birth_impact(&mut self, position: [u16; 3]) -> Result<(), String> {
        let request = wheel_particle(
            self.services.factory_process,
            self.services.phase,
            position,
            [0; 3],
        );
        let rng = self
            .rng
            .as_deref_mut()
            .ok_or("wheel splash needs the process RNG")?;
        self.services
            .particles
            .birth_impact(request, self.executable, rng)
            .map(|_| ())
            .map_err(|error| format!("00489600 wheel splash: {error}"))
    }

    fn legs(&mut self) -> Result<crate::object_runtime::ObjectLegServices<'_>, String> {
        if self.rng.is_none() {
            return Err("selector10 legs need the process RNG".into());
        }
        let sine = Arc::clone(&self.services.sine);
        let executable = self.executable;
        Ok(crate::object_runtime::ObjectLegServices {
            services: self,
            executable,
            sine,
        })
    }
}

impl crate::object_runtime::Selector10LegGround for ObjectCustomEffects<'_, '_> {
    fn height(&mut self, x: u16, z: u16, _: u32) -> i16 {
        self.services.world.terrain.height_at([x, z])
    }

    fn wheel_height(&mut self, x: u16, z: u16, _: u32) -> i16 {
        self.services.world.terrain.wheel_height_at([x, z])
    }

    fn water(&mut self, x: u16, z: u16, clock: u32, flags: u32, terrain: i16) -> i16 {
        self.services.world.terrain.water_surface_height(
            crate::terrain_runtime::TerrainWaterSurfaceRequest {
                xz: [x, z],
                wave_clock: clock,
                world_flags: flags,
                terrain_height: terrain,
            },
            &self.services.sine,
        )
    }
}

impl crate::object_runtime::Selector10CustomServices for ObjectCustomEffects<'_, '_> {
    fn clock(&self) -> u32 {
        self.animation.animation_value()
    }

    fn world_flags(&self) -> u32 {
        self.services.world.flags
    }

    fn update_byte(&self) -> u8 {
        self.services.world.update_byte
    }

    #[track_caller]
    fn random(&mut self) -> u32 {
        let rng = self
            .rng
            .as_deref_mut()
            .expect("legs() admits only a lent RNG");
        crate::name_entry::name_entry_rng_next(rng)
    }

    fn birth_trail(
        &mut self,
        trail: crate::model_vertices::FurnitureCustomTrail,
    ) -> Result<(), String> {
        crate::object_runtime::ObjectCustomVertexEffects::birth_trail(self, trail)
    }
}

impl ObjectModelServiceLoan<'_> {
    pub(super) fn packed_value(
        &self,
        selector: u32,
    ) -> Result<u16, crate::object_runtime::ObjectModelCallbackError> {
        // The vehicle's value 7 is its weapon's deployment +210.
        if self.guided_mounts && selector == 7 {
            if let Ok(state) = self.arena.selector2_actor(self.source) {
                return Ok(crate::object_runtime::guided_mount_deploy(state));
            }
        }
        ObjectModelCallbacks {
            arena: &*self.arena,
            source: self.source,
            animation: self.animation,
            executable: self.executable,
        }
        .packed_value(selector)
    }

    pub(super) fn custom_vertex(
        &mut self,
        request: crate::model_vertices::ModelCustomVertexRequest<'_>,
        vertices: &crate::model_vertices::ModelVertices,
        output: &mut Option<[i32; 3]>,
        rng: Option<&mut u32>,
    ) -> Result<(), crate::object_runtime::ObjectCustomVertexError> {
        let mut effects = self.effects.as_mut().map(|services| ObjectCustomEffects {
            services,
            executable: self.executable,
            animation: self.animation,
            rng,
        });
        self.arena.compute_model_custom_vertex(
            crate::object_runtime::ObjectCustomVertexRequest {
                source: self.source,
                compute: request,
                vertices,
                output,
                effects: effects.as_mut().map(|effects| {
                    effects as &mut dyn crate::object_runtime::ObjectCustomVertexEffects
                }),
            },
        )?;
        //00478950 reads invocation+58 only after virtual+10 returns. Each
        //WORD delta is sign-extended before adding the full caller DWORD eye.
        if let Some(world) = output.as_mut() {
            for (axis, origin) in world.iter_mut().zip(self.context.view.eye) {
                *axis = i32::from((*axis as u16).wrapping_sub(origin as u16) as i16)
                    .wrapping_add(origin);
            }
        }
        Ok(())
    }
}

impl SoftwareScene<'_, '_> {
    /// Reached command/RNG prefixes survive every return. Neither a failed
    /// model nor a required custom callback pretends to complete the manager.
    pub fn draw_object_model(
        &mut self,
        request: SoftwareObjectModelDrawRequest<'_>,
    ) -> Result<(), SoftwareSceneError> {
        self.draw_object_model_with_effects(request, None)
    }

    /// As `draw_object_model`, lending the particle owners that a source+10
    /// trail reaches.
    pub(super) fn draw_object_model_with_effects<'a>(
        &mut self,
        request: SoftwareObjectModelDrawRequest<'a>,
        effects: Option<ObjectCustomEffectServices<'a>>,
    ) -> Result<(), SoftwareSceneError> {
        if self.stopped {
            return Err(SoftwareSceneError::Stopped);
        }
        let packet = request.packet;
        //0049FEF0/004ED610 call 0048FB40 against the published global view.
        //004A00E0 and the Far projectors consume the reached global fade/factor.
        //The caller context still owns admission, custom rebasing and far color;
        //copying its view/fade here would fabricate a native publication.
        let environment = self.environment;
        let mut rng = self.process_rng;
        if self.scene_feed.is_some() {
            // The root's H8 slot, which the feed's models go by, and its
            // name.
            let root = self
                .resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Model,
                    argument: i32::from(packet.model_slot),
                })
                .ok()
                .and_then(|resource| match resource.data {
                    RenderResourceData::Model(model) => Some((
                        model.identity.global_index,
                        model.name.as_ref().and_then(|name| {
                            resource.source.bytes.get(name.bytes.start..name.bytes.end)
                        }),
                    )),
                    _ => None,
                });
            if let Some(feed) = self.scene_feed.as_mut() {
                feed.begin_object(
                    packet.source,
                    packet.position,
                    root.map(|(slot, _)| slot),
                    root.and_then(|(_, name)| name),
                );
            }
        }
        if let Some(motion) = self.raster_motion.as_mut() {
            motion.begin_object(packet.source);
        }
        let result = self.execute_model_program(SoftwareModelDrawRequest {
            root_mode: request.root_mode,
            environment,
            argument: i32::from(packet.model_slot),
            context: ModelContext::new(packet.position, packet.basis),
            callbacks: ModelObjectCallbacks::Object(ObjectModelServiceLoan {
                arena: request.arena,
                context: request.context,
                source: packet.source,
                animation: request.animation,
                executable: request.executable,
                effects,
                guided_mounts: self.guided_mounts,
            }),
            terrain: request.terrain,
            surface_table: ModelSurfaceTableOrigin::InheritedRenderer,
            primitive_context: ModelPrimitiveContext::Caller {
                far_color: request.context.far_color,
                mode: self.primitive_state.mode,
            },
            failure_policy: ModelFailurePolicy::StopScene,
            stack_registers:
                crate::model_execution::ModelStackRegisterPolicy::ObjectStackHistoryApproximations,
            rng: rng.as_mut(),
        });
        if let Some(feed) = self.scene_feed.as_mut() {
            feed.end_object();
        }
        if let Some(motion) = self.raster_motion.as_mut() {
            motion.end_object();
        }
        self.process_rng = rng;
        if let Err(error) = &result {
            self.latch_stop(error);
        }
        result
    }
}

/// Scoped manager backend. Attract byte and clock are actual process loans;
/// context, arena and model publication are supplied at each reach.
pub struct SoftwareObjectDrawHost<'a, 'f, 'r> {
    pub terrain: ModelTerrainRead<'a>,
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub animation: &'a dyn AnimationClockSource,
    pub executable: inf_formats::FrontendExecutable<'a>,
    pub attract_demo: &'a bool,
    pub failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy,
    pub effects: Option<SoftwareObjectEffectServices<'a>>,
}

/// Shared lower owners for synchronous object callbacks. Weapon tables and
/// pickup mission/terrain loans are independent feature inputs; RNG remains
/// the actual renderer loan. Missing weapon tables or sound retains the old
/// firing admission order, before reading RNG.
pub struct SoftwareObjectEffectServices<'a> {
    pub tables: Option<&'a mut crate::object_runtime::ProcessActorBehaviorTables>,
    pub factory_process: &'a crate::process_particles::factories::ParticleFactoryProcessState,
    pub particles: &'a mut crate::process_particles::ProcessParticleRuntime,
    pub statistics: Option<&'a mut [u8; crate::world_mission::RECORD_0C_BYTES]>,
    pub sounds: Option<&'a mut crate::process_sound::ProcessSoundRuntime>,
    pub residency: &'a crate::ResidentRuntime,
    pub phase: Option<u32>,
    pub pickup: Option<super::pickup::SoftwarePickupServices<'a>>,
    /// World inputs of source+10 trails and legs; None fails at reach.
    pub custom: Option<SoftwareCustomWorld<'a>>,
    /// The world's child factory for the shots a weapon fires (the owner's
    /// +4C); None keeps the actor-resource arm at its boundary.
    pub children: Option<
        &'a mut dyn crate::object_runtime::ActorChildHost<
            Error = crate::object_runtime::Selector36ChildServiceError,
        >,
    >,
    /// Sonic Shock's map jitter and flash; None keeps the pulse at its
    /// boundary.
    pub pulse: Option<&'a mut dyn crate::object_runtime::DetonatorPulseHost>,
    /// Live settings +7C, read by a shot's draw through its target refresh.
    pub settings_7c: Option<u32>,
    /// Live settings +70, which a Force Field's draw rings by.
    pub settings_70: Option<u32>,
    /// Each network slot's initials (`0054E71C`), which a vehicle's name tag
    /// shows; None keeps the tag at its boundary.
    pub network_initials: Option<&'a [[u8; 6]; crate::network::game::SLOTS]>,
    /// The host's team flag `0054E81C`, which Powerball's vehicles' draw
    /// reads; None keeps that draw at its boundary.
    pub network_team_flag: Option<u8>,
}

/// The name tag's glyphs, each a `+A4` effect quad sorted at the tag's key
/// as the overlay item `00423370` replays them.
struct QueuedGlyphs<'s, 'f, 'r> {
    scene: &'s mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    key: u32,
}

impl crate::menu_text::MenuTextHost for QueuedGlyphs<'_, '_, '_> {
    type Error = SoftwareSceneError;
    fn submit_glyph(&mut self, glyph: crate::menu_text::MenuGlyphQuad) -> Result<(), Self::Error> {
        let mut command = crate::software_menu::glyph_command(self.resources, glyph)?;
        command.depth = self.key;
        self.scene.queue.submit(DrawSubmission {
            depth: self.key,
            payload_blocks: command.payload_blocks,
            receiver_offset: 0,
            command,
        })?;
        Ok(())
    }
}

impl Flag0ObjectDrawHost for SoftwareObjectDrawHost<'_, '_, '_> {
    fn ui_state(&mut self) -> Option<u32> {
        self.effects.as_ref().and_then(|effects| effects.phase)
    }

    fn settings_7c(&mut self) -> Option<u32> {
        self.effects
            .as_ref()
            .and_then(|effects| effects.settings_7c)
    }

    fn process_null_handle(&mut self) -> Option<crate::object_runtime::ActorHandle> {
        self.effects
            .as_ref()
            .and_then(|effects| effects.factory_process.process_null_handle)
    }

    fn basis_math(&mut self) -> Option<inf_formats::native_basis::NativeBasisMath> {
        Some(inf_formats::native_basis::NativeBasisMath::new(
            *self.scene.tables.sine_table,
        ))
    }

    fn sine_quarter(&mut self) -> Option<[u16; 4096]> {
        Some(*self.scene.tables.sine_table)
    }

    fn name_tag(
        &mut self,
        tag: crate::object_runtime::VehicleNameTag,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        use crate::object_runtime::native_trunc_low32 as ftol;
        let fail = |reason: String| {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::VehicleNameTagDraw { reason },
            )
        };
        let initials = self
            .effects
            .as_ref()
            .and_then(|effects| effects.network_initials)
            .and_then(|table| table.get(usize::from(tag.slot)).copied())
            .ok_or_else(|| fail(format!("slot {}'s initials (0054E71C)", tag.slot)))?;
        // 004205F7..00420664: the point from the eye, 10 up, wrapped to the
        // world's coordinate width, and its depth along the view.
        let environment = &self.scene.environment;
        let shift = u32::from(environment.coordinate_shift) & 31;
        let eye = environment.view.eye;
        let row = |index: usize| f64::from(f32::from_bits(environment.view.basis.words[index]));
        let relative = |axis: usize, lift: i32| {
            let value = i32::from(tag.position[axis] as i16)
                .wrapping_sub(eye[axis])
                .wrapping_add(lift);
            (value << shift) >> shift
        };
        let [x, y, z] = [relative(0, 0), relative(1, 10), relative(2, 0)].map(f64::from);
        let depth = ftol(y * row(8) + z * row(9) + x * row(7));
        // 00420A70 and 00420AA0, then 00420AD0's projection past 64.
        let camera = [
            ftol(x * row(1) + y * row(2) + z * row(3)),
            ftol(x * row(4) + y * row(5) + z * row(6)),
        ];
        let point = if depth < 0x40 {
            camera.map(|value| value as i16)
        } else {
            let projection = environment.projection;
            let scaled = |axis: usize| {
                ftol(f64::from(projection.focal[axis]) * f64::from(camera[axis]) / f64::from(depth))
            };
            let mut screen = [
                scaled(0).wrapping_add(projection.center[0]),
                projection.center[1].wrapping_sub(scaled(1)),
            ];
            let mut size = screen[0].wrapping_abs() | screen[1].wrapping_abs();
            while size >= 0x7fff {
                size >>= 1;
                screen = screen.map(|value| value >> 1);
            }
            screen.map(|value| value as i16)
        };
        // 004206C5..00420736: the key, unless the sprite 0x32 sorts by its
        // texture.
        let resources = self.scene.resources;
        let sprite = resources
            .resolve(crate::render_resources::RenderResourceRequest {
                kind: crate::render_resources::RenderResourceKind::Sprite,
                argument: 0x32,
            })
            .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?;
        let mut key = depth.wrapping_sub(0x100) as u32;
        if SoftwareScene::material_sorting(self.scene) {
            let crate::render_resources::RenderResourceData::Sprite(entry) = sprite.data else {
                return Err(fail("sprite 0x32 is not a sprite".into()));
            };
            let selector = entry.selectors.mode0;
            let flagged = selector == 255
                || entry
                    .variants
                    .get(usize::from(selector))
                    .ok_or_else(|| fail(format!("sprite 0x32's variant {selector}")))?
                    .flags
                    .raw()
                    & 0x39
                    != 0;
            key = if flagged {
                (key >> 1).wrapping_add(0xc000_0000)
            } else {
                self.scene
                    .texture_identities
                    .resolve(sprite)
                    .map_err(|error| fail(error.to_string()))?
                    .sort_word(selector)
                    .map_err(|error| fail(error.to_string()))?
            };
        }
        // 0042079B: "%5.5s", right-aligned in five.
        let name: Vec<u8> = initials
            .iter()
            .take(5)
            .take_while(|&&byte| byte != 0)
            .copied()
            .collect();
        let mut text = vec![b' '; 5 - name.len()];
        text.extend(name);
        let color = crate::menu_text::MenuTextResources::style(&resources, tag.style)
            .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?;
        let font = crate::menu_text::MenuTextResources::font(&resources, 0)
            .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?;
        let mut request = crate::menu_text::LineTextRequest {
            text: &text,
            font,
            position_pixels: point.map(i32::from),
            color: crate::menu::MenuColor::from_word(color),
            base_color: crate::menu::MenuColor::from_word(
                crate::menu_text::INITIAL_GLYPH_BASE_WORD,
            ),
            context_origin_pixels: [0, 0],
        };
        let mut glyphs = QueuedGlyphs {
            scene: self.scene,
            resources,
            key,
        };
        crate::menu_text::draw_line_text(&mut request, &resources, &mut glyphs)
            .map_err(|error| fail(error.to_string()))
    }

    fn network_settings(&mut self) -> Option<[u32; 20]> {
        self.effects
            .as_ref()
            .map(|effects| effects.factory_process.settings)
    }

    fn resident_models(&mut self) -> Option<&crate::ResidentRuntime> {
        self.effects.as_ref().map(|effects| effects.residency)
    }

    fn network_team_flag(&mut self) -> Option<u8> {
        self.effects
            .as_ref()
            .and_then(|effects| effects.network_team_flag)
    }

    fn style_word(&mut self, argument: i32) -> Result<u32, ObjectDrawServiceError<Self::Error>> {
        crate::menu_text::MenuTextResources::style(&self.scene.resources, argument)
            .map_err(|error| ObjectDrawServiceError::Backend(error.into()))
    }

    fn gate_light(
        &mut self,
        style: u16,
    ) -> Result<SceneCommand, ObjectDrawServiceError<Self::Error>> {
        let terrain = match self.terrain {
            ModelTerrainRead::Live(terrain) => terrain,
            ModelTerrainRead::RendererEpoch => {
                return Err(ObjectDrawServiceError::UnownedCallback(
                    ObjectDrawCallbackBoundary::GateLight,
                ))
            }
        };
        let [x, y, z] = terrain.light_direction_raw();
        let style =
            crate::menu_text::MenuTextResources::style(&self.scene.resources, i32::from(style))
                .map_err(|error| ObjectDrawServiceError::Backend(error.into()))?;
        let [r, g, b, _] = style.to_le_bytes();
        Ok(SceneCommand::DirectionalLight {
            direction: crate::terrain_runtime::normalize_terrain_light([x / 4, y, z / 4]),
            rgb: [r, g, b],
        })
    }

    type Error = SoftwareSceneError;

    fn draw_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        ribbon: crate::object_runtime::ObjectId,
        context: &ObjectDrawContext,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        self.scene
            .draw_ribbon(arena, ribbon, context, self.executable)
    }

    fn draw_force_field(
        &mut self,
        arena: &mut ObjectArena,
        field: crate::object_runtime::ObjectId,
        context: &ObjectDrawContext,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        let unowned = |reason: &str| {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::Selector18ForceFieldDraw {
                    reason: reason.to_owned(),
                },
            )
        };
        let effects = self
            .effects
            .as_ref()
            .ok_or_else(|| unowned("the dome needs the world's draw services"))?;
        let custom = effects
            .custom
            .as_ref()
            .ok_or_else(|| unowned("the dome needs the world's terrain and flags"))?;
        let world = super::force_field::ForceFieldWorld {
            terrain: custom.terrain,
            world_flags: custom.flags,
            detail: effects
                .settings_70
                .ok_or_else(|| unowned("the dome needs settings +70"))?,
            clock: self.animation.animation_value(),
        };
        self.scene.draw_force_field(arena, field, context, world)
    }

    fn draw_laser(
        &mut self,
        arena: &mut ObjectArena,
        laser: crate::object_runtime::ObjectId,
        context: &ObjectDrawContext,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        let mut rng = self.scene.process_rng().ok_or_else(|| {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::Selector31LaserDraw {
                    reason: "the jitter needs the process RNG".into(),
                },
            )
        })?;
        let drawn = self.scene.draw_laser(arena, laser, context, &mut rng);
        self.scene.set_process_rng(Some(rng));
        drawn
    }

    fn draw_gate(
        &mut self,
        arena: &mut ObjectArena,
        gate: crate::object_runtime::ObjectId,
        context: &ObjectDrawContext,
        posts: &[([u16; 3], [u16; 3])],
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        self.scene.draw_gate(arena, gate, context, posts)
    }

    fn draw_barrier(
        &mut self,
        arena: &mut ObjectArena,
        barrier: crate::object_runtime::ObjectId,
        context: &ObjectDrawContext,
        bolts: &[crate::object_runtime::Selector69Bolt],
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        let unowned = |reason: &str| {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::Selector69BarrierDraw {
                    reason: reason.to_owned(),
                },
            )
        };
        let Self {
            scene,
            effects,
            executable,
            ..
        } = self;
        let effects = effects
            .as_mut()
            .ok_or_else(|| unowned("the sparks need the process particles"))?;
        let mut rng = scene
            .process_rng()
            .ok_or_else(|| unowned("the sparks need the process RNG"))?;
        let process = effects.factory_process;
        let phase = effects.phase;
        let particles = &mut *effects.particles;
        let executable = *executable;
        // 00411777..00411789: 004894A0(0, point, 005469FC's zero vector, 0, 0).
        let drawn = scene.draw_barrier(arena, barrier, context, bolts, executable, |point| {
            particles
                .birth_material_sprite(
                    wheel_particle(process, phase, point, [0; 3]),
                    false,
                    executable,
                    &mut rng,
                )
                .map(|_| ())
                .map_err(|error| format!("004894A0 barrier spark: {error}"))
        });
        scene.set_process_rng(Some(rng));
        drawn
    }

    fn draw_zone(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::ObjectId,
        points: &[[u16; 3]],
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        let unowned = |reason: &str| {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::Selector74ZoneDraw {
                    reason: reason.to_owned(),
                },
            )
        };
        let Self {
            scene,
            effects,
            executable,
            ..
        } = self;
        let effects = effects
            .as_mut()
            .ok_or_else(|| unowned("the points need the process particles"))?;
        let mut rng = scene
            .process_rng()
            .ok_or_else(|| unowned("the points need the process RNG"))?;
        let process = effects.factory_process;
        let phase = effects.phase;
        // 0042D91F: 00489A90(2, point, 0) per point, in order.
        let mut born = Ok(());
        for &position in points {
            born = effects
                .particles
                .birth_pickup_flash(
                    crate::process_particles::births::ParticleBirthRequest {
                        particle_type: crate::object_runtime::SELECTOR74_PARTICLE,
                        position,
                        velocity: None,
                        flags: 0,
                        inputs: crate::process_particles::births::ParticleBirthInputs {
                            network_active: process.network_flag != 0,
                            phase,
                            process,
                        },
                    },
                    *executable,
                    &mut rng,
                )
                .map(|_| ())
                .map_err(|error| unowned(&format!("00489A90 zone point: {error:?}")));
            if born.is_err() {
                break;
            }
        }
        scene.set_process_rng(Some(rng));
        born
    }

    fn draw_timed_pickup(
        &mut self,
        arena: &mut ObjectArena,
        object: crate::object_runtime::ObjectId,
        context: &mut ObjectDrawContext,
        process: &mut crate::object_runtime::ObjectDrawProcessState,
        lighting: &crate::object_runtime::ObjectDrawLighting<'_>,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        self.draw_pickup(arena, object, context, process, lighting)
    }

    fn drain_weapon_fire(
        &mut self,
        arena: &mut ObjectArena,
        actor: crate::object_runtime::ObjectId,
    ) -> Result<usize, ObjectDrawServiceError<Self::Error>> {
        let services = self.effects.as_mut().ok_or_else(|| {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::MissingWeaponFiringServices,
            )
        })?;
        let tables = services.tables.as_deref_mut().ok_or_else(|| {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::MissingWeaponFiringServices,
            )
        })?;
        let sounds = services.sounds.as_deref_mut().ok_or_else(|| {
            ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::MissingWeaponFiringServices,
            )
        })?;
        let mut rng = self.scene.process_rng().ok_or_else(|| {
            ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::WeaponFire {
                reason: "0048C890 requires the live object-pass RNG loan".into(),
            })
        })?;
        let result = arena.drain_actor_firing(
            actor,
            &mut crate::object_runtime::WeaponFiringContext {
                tables,
                rng: &mut rng,
            },
            &mut crate::object_runtime::WeaponFiringRuntime {
                executable: self.executable,
                factory_process: services.factory_process,
                particles: services.particles,
                statistics: services.statistics.as_deref_mut(),
                sounds,
                residency: services.residency,
                phase: services.phase,
                children: services.children.as_deref_mut().map(
                    |children| -> &mut dyn crate::object_runtime::ActorChildHost<
                        Error = crate::object_runtime::Selector36ChildServiceError,
                    > { children },
                ),
                pulse: services
                    .pulse
                    .as_deref_mut()
                    .map(|pulse| -> &mut dyn crate::object_runtime::DetonatorPulseHost { pulse }),
            },
        );
        // Restore even on failure: the reached factory/effect RNG prefix is
        //not rolled back, and world.rng is still the caller's stale copy.
        self.scene.set_process_rng(Some(rng));
        if let Err(error) = &result {
            if !matches!(
                error,
                crate::object_runtime::WeaponFiringDrainError::PreviouslyStopped(_)
            ) {
                eprintln!("Weapon firing stopped after reached writes: {actor:?}: {error}");
            }
        }
        result.map(|summary| summary.emitted).map_err(|error| {
            ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::WeaponFire {
                reason: error.to_string(),
            })
        })
    }

    fn scene(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        crate::scene_resources::ResidentSceneHost {
            resources: self.scene.resources,
            backend: &mut *self.scene,
        }
        .submit(command)
        .map_err(|error| match error {
            crate::scene_resources::SceneResourceError::Resource(error) => error.into(),
            crate::scene_resources::SceneResourceError::Backend(error) => error,
        })
    }

    fn attract_demo(&mut self) -> u8 {
        u8::from(*self.attract_demo)
    }

    fn material_color(
        &mut self,
        argument: i32,
    ) -> Result<u32, ObjectDrawServiceError<Self::Error>> {
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument,
        };
        let resource = self.scene.resources.resolve(request).map_err(|error| {
            ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::MaterialFeature {
                argument,
                reason: error.to_string(),
            })
        })?;
        let RenderResourceData::Material(material) = resource.data else {
            return Err(ObjectDrawServiceError::Backend(SoftwareSceneError::Custody));
        };
        Ok(self.scene.color_layout().convert_material(material))
    }

    fn model(
        &mut self,
        request: ObjectModelDrawSubmission<'_>,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        let root_was_open = self.scene.queue.has_only_implicit_root();
        let sine = &self.scene.tables.sine_table;
        let effects = self.effects.as_mut().and_then(|effects| {
            Some(ObjectCustomEffectServices {
                particles: &mut *effects.particles,
                factory_process: effects.factory_process,
                phase: effects.phase,
                world: effects.custom?,
                sine: Arc::clone(sine),
            })
        });
        let result = self.scene.draw_object_model_with_effects(
            SoftwareObjectModelDrawRequest {
                terrain: self.terrain,
                packet: request.packet,
                root_mode: request.root,
                context: request.context,
                arena: request.arena,
                animation: self.animation,
                executable: self.executable,
            },
            effects,
        );
        let Err(error) = result else {
            return Ok(());
        };
        self.model_error(request.packet.model_slot, root_was_open, error)
    }
}

impl SoftwareObjectDrawHost<'_, '_, '_> {
    /// Complete object-feature recovery policy. An unfinished explicit group
    /// cannot acquire subsequent independent objects or be auto-closed at flush.
    pub(super) fn model_error(
        &mut self,
        model_slot: i16,
        root_was_open: bool,
        error: SoftwareSceneError,
    ) -> Result<(), ObjectDrawServiceError<SoftwareSceneError>> {
        // Scratch custody is hard before any model-feature normalization.
        if error.is_model_scratch_fault() {
            return Err(ObjectDrawServiceError::Backend(error));
        }
        // A missing/dead/foreign source is arena custody, not an omitted
        // feature. Preserve the actual model error and keep the scene stopped.
        if matches!(
            &error,
            SoftwareSceneError::Model(ModelExecutionError::Vertex(
                crate::model_vertices::ModelVertexError::Custom(
                    crate::model_vertices::ModelCustomVertexError::Host(
                        ModelCommandError::Service(SoftwareSceneServiceError::ObjectCustomVertex {
                            reason: crate::object_runtime::ObjectCustomVertexError::Object(_)
                                | crate::object_runtime::ObjectCustomVertexError::BehaviorGroup { .. },
                            ..
                        })
                    )
                )
            ))
        ) {
            return Err(ObjectDrawServiceError::Backend(error));
        }
        if matches!(
            &error,
            SoftwareSceneError::Model(ModelExecutionError::Value(ModelValueError::Callback(
                ModelCommandError::Service(SoftwareSceneServiceError::ObjectModelCallback(
                    crate::object_runtime::ObjectModelCallbackError::Object(_)
                ))
            )))
        ) {
            return Err(ObjectDrawServiceError::Backend(error));
        }
        let reason = match error {
            SoftwareSceneError::Model(ModelExecutionError::Value(ModelValueError::Callback(
                ModelCommandError::Service(SoftwareSceneServiceError::ObjectModelCallback(error)),
            ))) => ObjectDrawCallbackBoundary::ModelValue(error),
            SoftwareSceneError::Model(ModelExecutionError::Vertex(
                crate::model_vertices::ModelVertexError::Custom(
                    crate::model_vertices::ModelCustomVertexError::Host(
                        ModelCommandError::Service(SoftwareSceneServiceError::ObjectCustomVertex {
                            object,
                            selector,
                            reason,
                        }),
                    ),
                ),
            )) => ObjectDrawCallbackBoundary::CustomVertex {
                object,
                selector,
                reason,
            },
            SoftwareSceneError::Model(ModelExecutionError::Host(ModelCommandError::Primitive(
                error @ crate::model_primitives::PrimitiveError::Unsupported { .. },
            ))) => ObjectDrawCallbackBoundary::ModelFeature {
                model_slot,
                reason: error.to_string(),
            },
            SoftwareSceneError::Model(ModelExecutionError::Host(error)) => {
                return Err(ObjectDrawServiceError::Backend(SoftwareSceneError::Model(
                    ModelExecutionError::Host(error),
                )));
            }
            SoftwareSceneError::Model(error) => ObjectDrawCallbackBoundary::ModelFeature {
                model_slot: model_slot,
                reason: error.to_string(),
            },
            SoftwareSceneError::Resource(error) => ObjectDrawCallbackBoundary::ModelFeature {
                model_slot: model_slot,
                reason: error.to_string(),
            },
            error => return Err(ObjectDrawServiceError::Backend(error)),
        };
        // Never reset corrupt queue or aborted presentation state. An enclosing
        // approximation can omit this object while retaining its reached prefix.
        if self.failure_policy == crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation {
            if self.scene.aborted_presentation || self.scene.queue.state().stopped {
                return Err(ObjectDrawServiceError::Backend(SoftwareSceneError::Stopped));
            }
            if !root_was_open || self.scene.queue.state().open_groups == 0 {
                return Err(ObjectDrawServiceError::Backend(
                    SoftwareSceneError::ObjectModelGroupPrefix {
                        model_slot,
                        root_was_open,
                        open_groups: self.scene.queue.state().open_groups,
                        reason,
                    },
                ));
            }
            // ObjectDrawGroupClosureApproximation: native end_group owns the
            // actual sort/copy/pop. Close only groups created by this failed
            // object above its authenticated implicit-root entry. No callback,
            // payload replay/reset or fabricated model completion occurs here.
            let groups = self.scene.queue.state().open_groups - 1;
            while self.scene.queue.state().open_groups > 1 {
                if let Err(error) = self.scene.queue.end_group() {
                    return Err(ObjectDrawServiceError::Backend(error.into()));
                }
            }
            if groups != 0 {
                if self.scene.stats.object_group_prefix_closures == 0 {
                    eprintln!("ObjectDrawGroupClosureApproximation: closed {groups} reached groups for incomplete model {model_slot} after {reason:?}");
                }
                self.scene.stats.object_group_prefix_closures += groups;
            }
            if let Err(hard) = self.scene.continue_after_feature_error() {
                return Err(ObjectDrawServiceError::Backend(hard));
            }
        }
        Err(ObjectDrawServiceError::UnownedCallback(reason))
    }
}

#[cfg(test)]
mod tests;
