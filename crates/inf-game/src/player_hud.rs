//! The players' HUD pass that ends the gameplay draw (`00440F00`,
//! `00441456..004414E5`) on the software renderer. Each vehicle draws its
//! own HUD `0041C320` ([`crate::vehicle_hud`] plans it) and then the
//! player's shown menu and message boxes (`00413820` at the start of
//! `0046BE00`). The rest of `0046BE00` draws only under a world flag no
//! retail world sets or online. Under V's free look the vehicle draws
//! `0041B230` ([`crate::free_look_view`]) instead of its HUD, and a held
//! missile draws its guided view `00456CF0` ([`crate::missile_view`]). The
//! HUD's own omissions and other units' virtual +84 are not ported; each is
//! logged once and skipped.

use crate::{
    factory_shop_draw::{ShopDraw, ShopDrawState, ShopDrawTables},
    free_look_view::{
        free_look_ops, plan_free_look, FreeLookCall, FreeLookFurniture, FreeLookInputs,
        FreeLookTarget,
    },
    frontend_scene::{
        ModelCallbackPolicy, ModelFailurePolicy, ModelPrimitiveContext, ModelSurfaceTableOrigin,
        SceneCommand, SceneFade, SceneModelSubmission, SceneProjection, SceneSetup, SceneView,
    },
    frontend_screen::FrontendProcessState,
    loading_draw::LoadingDrawResources,
    menu::{MenuBasis, MenuColor, MenuDrawInput},
    menu_basis::{MenuBasisMath, RotationPlane},
    menu_text::{
        draw_line_text, measure_text_width, LineTextRequest, MenuTextResources,
        INITIAL_GLYPH_BASE_WORD,
    },
    missile_view::{
        missile_view_ops, plan_missile_view, surface_static_quads, MissileViewCall,
        MissileViewInputs,
    },
    object_runtime::{
        FreeLookPick, HudReticleSettings, MessageBoxKind, ObjectId, PlayerCommandContext,
        PlayerCommandOperation, PlayerCommandRequest, PlayerHudUnit, PlayerId, ShownMenuOwner,
    },
    player_message_draw::{draw_message_box, MessageFrameInputs},
    process_particles::factories::ParticleFactoryProcessState,
    render_resources::RenderResources,
    scene_resources::{ResidentSceneHost, ResolvedSceneCommand, SceneResourceBackend},
    shown_menu_draw::{draw_shown_menu, ComputerDraw, ShownMenuDrawContext, ShownMenuOmissions},
    software_menu::{SoftwareMenu, SoftwareMenuRequest, SoftwareMenuState},
    software_scene::SoftwareScene,
    vehicle_hud::{
        plan_vehicle_hud, HudFrame, HudInputs, HudOp, HudProjection, HudStyles, HudText,
        HudTextPosition, HudWorld, STYLE_BAR, STYLE_COUNTERS, STYLE_FRAME, STYLE_TURBO,
        WEAPON_AMBIENT, WEAPON_LIGHT_DIRECTION, WEAPON_LIGHT_RGB, WEAPON_MODEL_POSITION,
    },
    world_backend::WorldSession,
    world_captions::{plan_world_captions, WorldCaptionInputs},
};

pub struct PlayerHudInputs<'a, 'r> {
    /// Menu colours and the animation clock.
    pub process: &'a FrontendProcessState,
    /// The process null handle `0046B090` compares against.
    pub factory_process: &'a ParticleFactoryProcessState,
    /// The shop's tables and the player commands' texts.
    pub executable: Option<inf_formats::FrontendExecutable<'a>>,
    /// The shared context template `0043B4A0` copies.
    pub template_projection: SceneProjection,
    pub sine_quarter: &'a [u16; 4096],
    /// The "HUD" (+60) and "First Person" (+68) settings.
    pub hud_setting: u32,
    pub first_person_setting: u32,
    /// Settings +7C, which the target's refresh reads.
    pub settings_7c: u32,
    /// World-manager byte G+5C5, the special view: the fire direction the
    /// sight takes (`0041FF20`) then frees the target.
    pub special_view: bool,
    /// The live settings the in-world menu's Options rows show.
    pub settings: Option<&'a crate::frontend_settings::FrontendSettings>,
    /// The selected video device's name, for the Options' device row.
    pub device_name: Option<Vec<u8>>,
    /// The database's second residency (a port fix): models the current
    /// world has not loaded.
    pub database_resources: Option<RenderResources<'r>>,
    /// The world map's jitter `+24` (G+1B4), which jams the radar; None
    /// without a constructed map.
    pub radar_jitter: Option<i32>,
    /// The process map `0054ED78` the terrain radar samples.
    pub radar_map: Option<&'a crate::terrain_map::TerrainMap>,
    /// The world draw's process state and light grid, which a carried
    /// actor's own draw reads.
    pub object_draw_process: Option<&'a crate::object_runtime::ObjectDrawProcessState>,
    pub light_grid: Option<&'a crate::world_lighting::WorldLightGrid>,
}

/// The first error of an array of results, else its values.
fn fallible<const N: usize>(values: [Result<u32, String>; N]) -> Result<[u32; N], String> {
    let mut out = [0; N];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = value?;
    }
    Ok(out)
}

/// `00488446..00488477`: the world's fog colour, H7 0x74 converted.
const WORLD_FAR_COLOR_SLOT: i32 = 0x74;

fn log_once(world: &mut WorldSession, key: String, message: impl FnOnce() -> String) {
    if world.draw_omissions_logged.insert(key) {
        eprintln!("{}", message());
    }
}

/// The world captions `0044121E..00441456` ([`crate::world_captions`]),
/// just before the HUD pass.
pub fn draw_world_captions<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    attract: bool,
) {
    let mut draw = || -> Result<(), String> {
        let mission = &world.mission;
        let unwritten = |field: u32| format!("mission W+{field:X} is unwritten");
        let count = mission.word_28.ok_or_else(|| unwritten(0x28))?;
        let player = world
            .arena
            .first_player()
            .ok_or("00488AA0 has no first player")?;
        let caption_inputs = WorldCaptionInputs {
            surface: scene.surface_pixels().map_err(|error| error.to_string())?,
            bonus_base: world.bonus_time_00,
            clock: inputs.process.animation().value(),
            message_shown: world
                .arena
                .player_message(player)
                .map_err(|error| error.to_string())?
                .shown_00()
                != 0,
            attract,
            doom_stage: mission.byte_18.ok_or_else(|| unwritten(0x18))?,
            countdown: mission.dword_24.ok_or_else(|| unwritten(0x24))?,
            count,
            count_text: match count {
                0 => 0,
                _ => mission.word_2a.ok_or_else(|| unwritten(0x2a))?,
            },
        };
        let font = resources.font(0).map_err(|error| error.to_string())?;
        let captions = plan_world_captions(
            &caption_inputs,
            |slot| {
                resources
                    .optional_loading_string(slot)
                    .map(|text| text.map(<[u8]>::to_vec))
                    .map_err(|error| error.to_string())
            },
            |text| measure_text_width(font, text).map_err(|error| error.to_string()),
        )?;
        let mut executor = HudExecutor {
            scene: &mut *scene,
            resources,
            menu_state: &mut *menu_state,
            inputs,
            math: MenuBasisMath::new(*inputs.sine_quarter),
            panel_texts: Vec::new(),
        };
        for caption in captions {
            executor.text_pass(
                &HudText::Bytes(caption.text),
                HudTextPosition::At(caption.position),
                &[(0, 0)],
            )?;
        }
        Ok(())
    };
    if let Err(error) = draw() {
        log_once(world, format!("world captions {error}"), || {
            format!("World captions (00440F00) stopped: {error}")
        });
    }
}

pub fn draw_player_huds<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    mut network: Option<&mut crate::network::link::NetworkLink>,
) {
    let units = match world.arena.player_hud_units(Some(inputs.factory_process)) {
        Ok(units) => units,
        Err(error) => {
            log_once(world, format!("hud units {error}"), || {
                format!("Player HUD pass stopped: {error}")
            });
            return;
        }
    };
    for (player, unit) in units {
        let owner = match unit {
            PlayerHudUnit::Other(unit) => {
                log_once(world, format!("hud other {unit:?}"), || {
                    format!("Player HUD: unit {unit:?}'s virtual +84 is not ported; skipped")
                });
                continue;
            }
            PlayerHudUnit::Vehicle {
                vehicle,
                owner: Some(owner),
                drawable: true,
                alternate_hud,
            } => {
                if alternate_hud {
                    draw_free_look(world, scene, resources, menu_state, inputs, vehicle);
                } else {
                    draw_vehicle_hud(
                        world,
                        scene,
                        resources,
                        menu_state,
                        inputs,
                        vehicle,
                        owner,
                        network.as_deref(),
                    );
                }
                owner
            }
            PlayerHudUnit::Missile { missile, owner } => {
                draw_missile_view(world, scene, resources, menu_state, inputs, missile, owner);
                continue;
            }
            PlayerHudUnit::Vehicle { .. } => continue,
            PlayerHudUnit::Camera {
                owner: Some(owner), ..
            } => owner,
            PlayerHudUnit::Camera { owner: None, .. } => continue,
        };
        draw_player_message(world, scene, resources, menu_state, inputs, owner);
        // The rest of 0046BE00 has two gates: H14 world flag 0x800
        // (0046BEF4, 0048EDE0) shows a count of the world's type 0x12/0x13
        // objects as "%s %d" with H0 0x112, which no retail world sets
        // (leaves 1 and 3..43), and online play (0055F448, 0046C010) the
        // network texts.
        if inputs.factory_process.network_flag != 0 {
            draw_network_texts(
                world,
                scene,
                resources,
                menu_state,
                inputs,
                owner,
                network.as_deref_mut(),
            );
        }
        if world.world_flags & 0x800 != 0 {
            log_once(world, "hud player counter".into(), || {
                format!(
                    "Player HUD: 0046BE00's world-flag 0x800 counter (player {player:?}) is \
                     not ported; skipped"
                )
            });
        }
    }
}

/// `0046C016..0046C5F1`, online: the message lines (`0045C1D0`: each a
/// tick nearer its end, the live ones in H6 style 0x2A), then the race's
/// status or Powerball's ([`crate::network_hud`]).
fn draw_network_texts<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    player: crate::object_runtime::PlayerId,
    network: Option<&mut crate::network::link::NetworkLink>,
) {
    let draw = || -> Result<(), String> {
        let link = network.ok_or("the network texts need the session's link")?;
        let surface = scene.surface_pixels().map_err(|error| error.to_string())?;
        let mut executor = HudExecutor {
            scene: &mut *scene,
            resources,
            menu_state: &mut *menu_state,
            inputs,
            math: MenuBasisMath::new(*inputs.sine_quarter),
            panel_texts: Vec::new(),
        };
        for (index, position) in link.frame.lines.tick() {
            let text = &link.frame.lines.lines[index].text;
            let end = text
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(text.len());
            executor.text_pass(
                &HudText::Bytes(text[..end].to_vec()),
                HudTextPosition::At(position),
                &[(crate::network::lines::LINE_STYLE, 0)],
            )?;
        }
        let Some(race) = world.arena.race() else {
            if inputs.factory_process.settings[0x10 / 4] != 4 {
                return Ok(());
            }
            use crate::network_hud::PowerballStatus;
            let clock = inputs.process.animation().value();
            let host = inputs.factory_process.host_flag != 0;
            let Some(status) =
                crate::network_hud::powerball_status(world.arena.powerball_goals(), host, clock)
            else {
                return Ok(());
            };
            let text = match status {
                PowerballStatus::Line(id) => HudText::String(id),
                PowerballStatus::Wins(team) => {
                    let string = |slot: u16| {
                        resources
                            .optional_loading_string(i32::from(slot))
                            .map(|text| text.map(<[u8]>::to_vec))
                            .map_err(|error| error.to_string())
                    };
                    let format =
                        string(0x82)?.ok_or_else(|| crate::free_look_view::null_format(0x82))?;
                    let name = string(team)?;
                    HudText::Bytes(crate::free_look_view::sprintf(&format, &[name.as_deref()])?)
                }
            };
            let point = [surface[0] / 2, surface[1] / 2];
            executor.text_pass(
                &text,
                HudTextPosition::CentredOn(point),
                &[(crate::network_hud::STATUS_STYLE, 0)],
            )?;
            return Ok(());
        };
        let countdown = world
            .arena
            .race_state(race)
            .map_err(|error| error.to_string())?
            .countdown_2d4;
        let state = world
            .arena
            .world_player(player)
            .map_err(|error| error.to_string())?;
        let status = crate::network_hud::RaceStatus {
            countdown,
            wrong_way_19c: state.dword_19c,
            missed_198: state.dword_198,
            host: inputs.factory_process.host_flag != 0,
        };
        let clock = inputs.process.animation().value();
        for (text, point) in crate::network_hud::race_status_texts(status, clock, surface) {
            executor.text_pass(
                &HudText::String(text),
                HudTextPosition::CentredOn(point),
                &[(crate::network_hud::STATUS_STYLE, 0)],
            )?;
        }
        Ok(())
    };
    if let Err(error) = draw() {
        log_once(world, format!("network texts {error}"), || {
            format!("Player HUD: the network texts stopped: {error}; skipped")
        });
    }
}

/// `00442080`'s network section after the world draw (`004421E6..00442B2E`),
/// online: the game's end, the bars and the scoreboards
/// ([`crate::network_hud::plan_network_overlay`]). A stop keeps what it drew
/// and is logged once.
pub fn draw_network_overlay<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    network: Option<&crate::network::link::NetworkLink>,
    scores_held: bool,
) {
    use crate::network_hud::{plan_network_overlay, OverlayInputs, OverlayOp, OverlayResources};
    let process = inputs.factory_process;
    if process.network_flag == 0 {
        return;
    }
    let mut ops = Vec::new();
    let mut plan = || -> Result<(), String> {
        let link = network.ok_or("the network overlay needs the session's link")?;
        let mode = process.settings[0x10 / 4];
        let carriers = match mode {
            0 => {
                let null = process
                    .process_null_handle
                    .ok_or("the banners' carriers need the process null handle 0054E894")?;
                world
                    .arena
                    .banner_carriers(null)
                    .map_err(|error| error.to_string())?
            }
            _ => Vec::new(),
        };
        let race = match world.arena.race() {
            Some(race) => Some(
                world
                    .arena
                    .race_state(race)
                    .map_err(|error| error.to_string())?,
            ),
            None => None,
        };
        let ball_holder = match world.arena.powerball() {
            Some(ball) => Some(
                world
                    .arena
                    .ball(ball)
                    .map_err(|error| error.to_string())?
                    .holder_14c
                    .raw(),
            ),
            None => None,
        };
        let font = resources.font(0).map_err(|error| error.to_string())?;
        plan_network_overlay(
            &OverlayInputs {
                surface: scene.surface_pixels().map_err(|error| error.to_string())?,
                clock: inputs.process.animation().value(),
                mode,
                host: process.host_flag != 0,
                local_slot: process.local_slot,
                scores_held,
                game: &link.game,
                race,
                carriers: &carriers,
                goals: world.arena.powerball_goals(),
                ball_holder,
            },
            &OverlayResources {
                string: |slot| {
                    resources
                        .optional_loading_string(slot)
                        .map(|text| text.map(<[u8]>::to_vec))
                        .map_err(|error| error.to_string())
                },
                width: |text: &[u8]| {
                    measure_text_width(font, text).map_err(|error| error.to_string())
                },
                style: |slot| resources.style(slot).map_err(|error| error.to_string()),
            },
            &mut ops,
        )
    };
    let planned = plan();
    let mut executor = HudExecutor {
        scene: &mut *scene,
        resources,
        menu_state: &mut *menu_state,
        inputs,
        math: MenuBasisMath::new(*inputs.sine_quarter),
        panel_texts: Vec::new(),
    };
    let drawn = ops.into_iter().try_for_each(|op| match op {
        OverlayOp::Quad(quad) => executor
            .scene
            .draw_sprite_direct(quad)
            .map_err(|error| error.to_string()),
        OverlayOp::Text {
            text,
            position,
            color_word,
        } => executor.radar_label(&text, position, color_word),
    });
    if let Err(error) = drawn.and(planned) {
        log_once(world, format!("network overlay {error}"), || {
            format!("Network overlay: stopped: {error}; kept what it drew")
        });
    }
}

/// The guided missile's view `00456CF0` ([`crate::missile_view`]): the
/// static draws from the world's RNG, and a held missile's player gets its
/// HUD twice, as retail draws it.
fn draw_missile_view<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    missile: ObjectId,
    owner: Option<PlayerId>,
) {
    let planned = (|| -> Result<_, String> {
        let source = world
            .arena
            .missile_view_source(missile)
            .map_err(|error| error.to_string())?;
        let style = |slot: i32| resources.style(slot).map_err(|error| error.to_string());
        let [x, height, z] = source.position;
        let ground = world.terrain.height_at([x, z]);
        // 00457038..00457054: the water at clock 0, waving under H14 flag 0x40.
        let water = world.terrain.water_surface_height(
            crate::terrain_runtime::TerrainWaterSurfaceRequest {
                xz: [x, z],
                wave_clock: 0,
                world_flags: world.world_flags,
                terrain_height: ground,
            },
            inputs.sine_quarter,
        );
        Ok(plan_missile_view(&MissileViewInputs {
            surface: scene.surface_pixels().map_err(|error| error.to_string())?,
            hud_setting: inputs.hud_setting,
            styles: [style(0)?, style(1)?],
            disrupted: source.disrupted,
            held: owner.is_some(),
            rates: source.rates,
            height: height as i16,
            ground,
            water,
            velocity: source.velocity,
            fuse: source.fuse,
        }))
    })();
    let calls = match planned {
        Ok(calls) => calls,
        Err(error) => {
            log_once(world, format!("hud missile plan {error}"), || {
                format!("Player HUD: the guided missile's view (00456CF0) stopped before drawing: {error}")
            });
            return;
        }
    };
    for call in calls {
        let result = match &call {
            MissileViewCall::Static { end } => surface_static_quads(*end, &mut world.rng, |slot| {
                resources
                    .sprite_dimensions(i32::from(slot))
                    .map(|[width, _]| width)
            })
            .map_err(|error| error.to_string())
            .and_then(|quads| {
                quads.into_iter().try_for_each(|quad| {
                    scene
                        .draw_sprite_direct(quad)
                        .map_err(|error| error.to_string())
                })
            }),
            MissileViewCall::Chevron { .. } | MissileViewCall::Bar { .. } => {
                missile_view_ops(&call, inputs.template_projection.focal)
                    .map_err(|error| error.to_string())
                    .and_then(|ops| {
                        ops.into_iter().try_for_each(|op| match op {
                            HudOp::Quad(quad) => scene
                                .draw_sprite_direct(quad)
                                .map_err(|error| error.to_string()),
                            other => Err(format!("unexpected {other:?}")),
                        })
                    })
            }
            MissileViewCall::SharedView | MissileViewCall::PlayerHud => {
                if let Some(owner) = owner {
                    draw_player_message(world, scene, resources, menu_state, inputs, owner);
                }
                Ok(())
            }
            MissileViewCall::Counter { text, centre } => HudExecutor {
                scene: &mut *scene,
                resources,
                menu_state: &mut *menu_state,
                inputs,
                math: MenuBasisMath::new(*inputs.sine_quarter),
                panel_texts: Vec::new(),
            }
            .text_pass(
                &HudText::Bytes(text.clone()),
                HudTextPosition::CentredOn(*centre),
                &[(0, 0)],
            ),
        };
        if let Err(error) = result {
            log_once(world, format!("hud missile {error}"), || {
                format!("Player HUD: the guided missile's view (00456CF0) stopped: {error}")
            });
            return;
        }
    }
}

/// Free look's view `0041B230` ([`crate::free_look_view`]) for `vehicle`:
/// its plan, executed in order. A failure keeps the pixels already drawn and
/// is logged once.
fn draw_free_look<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    vehicle: ObjectId,
) {
    let planned = (|| -> Result<_, String> {
        let source = world
            .arena
            .free_look_source(vehicle, inputs.settings_7c)
            .map_err(|error| error.to_string())?;
        let target = match source.target {
            FreeLookPick::Target(target) => target,
            FreeLookPick::FurnitureCell(xz) => {
                free_look_furniture(world, resources, inputs, vehicle, xz)?
            }
        };
        let frame = hud_frame(world, scene, resources, inputs, 0)?;
        let style = |slot: i32| resources.style(slot).map_err(|error| error.to_string());
        let font = resources.font(0).map_err(|error| error.to_string())?;
        let calls = plan_free_look(
            &FreeLookInputs {
                surface: frame.surface,
                focal: frame.focal,
                clock: frame.clock,
                styles: [style(0)?, style(0x21)?],
                side: source.side,
                dword_5c0: source.dword_5c0,
                target,
            },
            |slot| {
                resources
                    .optional_loading_string(slot)
                    .map(|text| text.map(<[u8]>::to_vec))
                    .map_err(|error| error.to_string())
            },
            |text| {
                Ok(i32::from(
                    measure_text_width(font, text).map_err(|error| error.to_string())? as i16,
                ))
            },
        )?;
        Ok((frame, calls))
    })();
    let (frame, calls) = match planned {
        Ok(planned) => planned,
        Err(error) => {
            log_once(world, format!("hud free look plan {error}"), || {
                format!("Player HUD: free look's view (0041B230) stopped before drawing: {error}")
            });
            return;
        }
    };
    let mut executor = HudExecutor {
        scene,
        resources,
        menu_state,
        inputs,
        math: MenuBasisMath::new(*inputs.sine_quarter),
        panel_texts: Vec::new(),
    };
    // 0041B36F: 004A4400 begins a batch, which the view never flushes.
    let mut result = executor
        .scene
        .submit(ResolvedSceneCommand::BeginBatch)
        .map_err(|error| error.to_string());
    for call in calls {
        if result.is_err() {
            break;
        }
        result = match call {
            FreeLookCall::Light => executor
                .scene
                .submit(ResolvedSceneCommand::DirectionalLight {
                    direction: world.presentation.terrain_light_direction,
                    rgb: [0x80; 3],
                })
                .map_err(|error| error.to_string()),
            FreeLookCall::Text {
                text,
                position,
                colour,
            } => executor.radar_label(&text, position, colour),
            FreeLookCall::Set5c0(value) => world
                .arena
                .set_vehicle_hud_5c0(vehicle, value)
                .map_err(|error| error.to_string()),
            call => free_look_ops(&call, &frame)
                .map_err(|error| error.to_string())
                .and_then(|ops| ops.into_iter().try_for_each(|op| executor.draw(world, op))),
        };
    }
    if let Err(error) = result {
        log_once(world, format!("hud free look draw {error}"), || {
            format!("Player HUD: free look's view (0041B230) stopped: {error}; drawn pixels kept")
        });
        // Port containment, as for the vehicle's HUD.
        if executor.scene.feature_continuation_allowed() {
            let _ = executor.scene.continue_after_feature_error();
        }
    }
}

/// `0041BA99..0041BD39`: furniture's grant at the target's cell
/// (`00436BC0`), its name (`00475320`'s WORD +0) and its side, flag 4 of the
/// cell's H12 record.
fn free_look_furniture(
    world: &mut WorldSession,
    resources: RenderResources<'_>,
    inputs: &PlayerHudInputs<'_, '_>,
    vehicle: ObjectId,
    xz: [u16; 2],
) -> Result<FreeLookTarget, String> {
    let executable = inputs
        .executable
        .ok_or("free look's furniture needs the executable's grant tables")?;
    let mut host = crate::session_platform::WorldGrantGridHost {
        residency: resources.resident_runtime(),
        executable,
        source: &world.terrain_source,
        prepared: &world.terrain,
        process: inputs.factory_process,
    };
    let grant = crate::object_runtime::terrain_furniture_grant(
        &mut world.arena,
        crate::object_runtime::FamilyTerrainGrantRequest { actor: vehicle, xz },
        &mut host,
    )
    .map_err(|error| error.to_string())?;
    if grant == 0 {
        return Ok(FreeLookTarget::Hidden);
    }
    let name = *inf_formats::parse_grant_texts(executable)
        .map_err(|error| error.to_string())?
        .get(usize::from(grant))
        .ok_or_else(|| format!("free look's grant {grant:#x} is past 00475320's tables"))?;
    // 0041BC77..0041BD30: the same cell, searched again for its record.
    let terrain = &world.terrain;
    let grid_mask = !((terrain.sampling().spacing.units() - 1) >> 7);
    let [x, z] = xz.map(|word| (word >> 7) & grid_mask);
    let record = terrain
        .cell_at_grid([x, z])
        .has_sparse_record()
        .then(|| {
            terrain
                .buckets()
                .get(usize::from(z))
                .and_then(|bucket| bucket.records.iter().find(|record| record.x == x))
        })
        .flatten()
        .ok_or("free look's furniture cell has no H12 record, where retail reads past one")?;
    Ok(FreeLookTarget::Furniture(FreeLookFurniture {
        name,
        alien: record.state & 4 != 0,
    }))
}

/// `00413820` for `0046BE00`'s player.
fn draw_player_message<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    player: crate::object_runtime::PlayerId,
) {
    let Some((visibility, math)) = world
        .arena
        .player_message_environment_mut()
        .map(|environment| (environment.visibility, environment.math.clone()))
    else {
        // Boxes need the environment the tick installs; none exist yet.
        return;
    };
    let surface = match scene.surface_pixels() {
        Ok(surface) => surface,
        Err(error) => {
            eprintln!("Player message draw: no surface ({error})");
            return;
        }
    };
    let input = MenuDrawInput {
        surface_pixels: surface,
        visibility,
        sine_quarter: inputs.sine_quarter,
    };
    // The shown menu +0C first, even when the message is suppressed.
    draw_player_shown_menu(
        world, scene, resources, menu_state, inputs, player, input, &math,
    );
    let draw = match world.arena.player_message_draw(player) {
        Ok(draw) => draw,
        Err(error) => {
            eprintln!("Player message draw: {error}");
            return;
        }
    };
    let mut failures = Vec::new();
    if let Some(boxes) = draw.boxes {
        for live in [boxes.lines.as_mut(), boxes.hints.as_mut()]
            .into_iter()
            .flatten()
        {
            let kind = live.kind;
            // Port widescreen: retail sizes a box's panel by the focal and
            // places and wraps its text by the surface, so on a wider frame
            // the text overran the panel. Each box draws on a 4:3 canvas
            // instead, the top-left lines against the left edge and the
            // hints in the centre.
            let canvas = match kind {
                MessageBoxKind::Lines => scene.pixels.frame.left_four_three_canvas(),
                MessageBoxKind::Hints => scene.pixels.frame.four_three_canvas(),
            };
            if let Some(canvas) = canvas {
                scene.enter_canvas(canvas);
            }
            let surface = match scene.surface_pixels() {
                Ok(surface) => surface,
                Err(error) => {
                    scene.leave_canvas();
                    failures.push((kind, format!("no surface ({error})")));
                    continue;
                }
            };
            let input = MenuDrawInput {
                surface_pixels: surface,
                ..input
            };
            let frame = MessageFrameInputs {
                projection: SceneProjection {
                    viewport: surface,
                    ..inputs.template_projection
                },
                animation: inputs.process.animation(),
                math: &math,
            };
            let result = SoftwareMenu::new(SoftwareMenuRequest {
                scene: &mut *scene,
                resources,
                focal: inputs.template_projection.focal,
                process: inputs.process,
                state: &mut *menu_state,
            })
            .map_err(crate::menu::MenuError::Host)
            .and_then(|mut menu| draw_message_box(live, input, &mut menu, frame));
            scene.leave_canvas();
            if let Err(error) = result {
                failures.push((kind, format!("{error:?}")));
            }
        }
    }
    for (kind, error) in failures {
        log_once(world, format!("hud message {kind:?}"), || {
            format!("Player message draw: the {kind:?} box stopped ({error}); skipped")
        });
    }
}

/// The shown menu `+0C` (`00453610`), with a factory's own rows when its
/// menu is current, then the player commands those rows set, which reach
/// the message before its boxes draw.
#[allow(clippy::too_many_arguments)]
fn draw_player_shown_menu<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    player: PlayerId,
    input: MenuDrawInput<'_>,
    math: &MenuBasisMath,
) {
    let shop = match shop_draw_inputs(world, inputs, player) {
        Ok(shop) => shop,
        Err(error) => {
            log_once(world, format!("hud shop state {error}"), || {
                format!("Player message draw: the shop's rows need its state ({error}); skipped")
            });
            None
        }
    };
    // The computer's tables and the player's grant mask, copied before the
    // menu's borrow; the process RNG is the world's.
    let computer_tables = world
        .arena
        .player_message_environment_mut()
        .map(|environment| {
            (
                environment.menus.database.clone(),
                environment.menus.hint_texts.clone(),
                environment.cheats.skip_world,
            )
        });
    let grants = world
        .arena
        .world_player(player)
        .map(|state| state.grants_9c)
        .ok();
    let Some(menu) = world.arena.player_shown_menu_mut(player) else {
        return;
    };
    let mut model_failures = Vec::new();
    let computer = match (&computer_tables, &grants) {
        (Some((database, hint_texts, skip_world)), Some(grants)) => Some(ComputerDraw {
            database,
            hint_texts,
            grants,
            rng: &mut world.rng,
            models: inputs.database_resources,
            model_failures: &mut model_failures,
            skip_world: *skip_world,
        }),
        _ => None,
    };
    let mut omissions = ShownMenuOmissions::new();
    let mut commands = Vec::new();
    let result = SoftwareMenu::new(SoftwareMenuRequest {
        scene: &mut *scene,
        resources,
        focal: inputs.template_projection.focal,
        process: inputs.process,
        state: &mut *menu_state,
    })
    .map_err(|error| crate::menu::MenuError::Host(error.into()))
    .and_then(|mut software| {
        draw_shown_menu(
            menu,
            input,
            &mut software,
            ShownMenuDrawContext {
                shop: shop
                    .as_ref()
                    .map(|(tables, state)| ShopDraw { tables, state }),
                settings: inputs.settings,
                device_name: inputs.device_name.as_deref(),
                computer,
                process: inputs.process,
                focal: inputs.template_projection.focal,
                math,
                commands: &mut commands,
            },
            &mut omissions,
        )
    });
    if let Err(error) = result {
        log_once(world, format!("hud shown menu draw {error:?}"), || {
            format!("Player message draw: the shown menu stopped ({error:?}); drawn pixels kept")
        });
        // Port containment: the menu's failure must not stop the frame's
        // later draws (the message boxes, the next frame).
        if scene.feature_continuation_allowed() {
            let _ = scene.continue_after_feature_error();
        }
    }
    for (slot, reason) in model_failures {
        log_once(world, format!("hud database model {slot} {reason}"), || {
            format!("Player message draw: the database's model {slot} stopped ({reason}); its drawn pixels kept")
        });
    }
    for omission in omissions {
        log_once(world, format!("hud shown menu {omission:x}"), || {
            format!("Player message draw: the shown menu's row callback {omission:08X} is not ported; skipped")
        });
    }
    for (player, raw_id) in commands {
        let context = PlayerCommandContext {
            process: Some(inputs.factory_process),
            executable: inputs.executable,
            resident: Some(resources.resident_runtime()),
        };
        let request = PlayerCommandRequest {
            player,
            operation: PlayerCommandOperation::Set { raw_id, silent: 0 },
        };
        if let Err(error) = world.arena.apply_player_command(request, &context) {
            log_once(world, format!("hud shop command {raw_id:x}"), || {
                format!("Player message draw: the shop row's command {raw_id:#x} stopped: {error}")
            });
        }
    }
}

/// The current shown menu's shop: the executable tables and the world
/// state its rows read. None when no factory's menu is current.
fn shop_draw_inputs(
    world: &mut WorldSession,
    inputs: &PlayerHudInputs<'_, '_>,
    player: PlayerId,
) -> Result<Option<(ShopDrawTables, ShopDrawState)>, String> {
    let owner = world
        .arena
        .player_shown_menu(player)
        .and_then(|menu| menu.current_owner());
    let Some(ShownMenuOwner::Shop { factory }) = owner else {
        return Ok(None);
    };
    let executable = inputs
        .executable
        .ok_or("the shop's tables need the executable")?;
    let tables = ShopDrawTables::parse(executable).map_err(|error| error.to_string())?;
    let state = world
        .arena
        .shop_draw_state::<String>(
            factory,
            inputs.factory_process,
            inputs.process.animation().value(),
        )
        .map_err(|error| error.to_string())?;
    Ok(Some((tables, state)))
}

/// `0041C320` for the vehicle of `player`: its plan, executed in order.
/// A failure keeps the pixels already drawn and is logged once.
#[allow(clippy::too_many_arguments)]
fn draw_vehicle_hud<'r>(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, 'r>,
    resources: RenderResources<'r>,
    menu_state: &mut SoftwareMenuState,
    inputs: &PlayerHudInputs<'_, 'r>,
    vehicle: ObjectId,
    player: PlayerId,
    network: Option<&crate::network::link::NetworkLink>,
) {
    let plan = vehicle_hud_inputs(world, scene, resources, inputs, vehicle, player, network)
        .and_then(|hud| plan_vehicle_hud(&hud).map_err(|error| error.to_string()));
    let ops = match plan {
        Ok(ops) => ops,
        Err(error) => {
            log_once(world, format!("hud vehicle plan {error}"), || {
                format!("Player HUD: the vehicle's HUD (0041C320) stopped before drawing: {error}")
            });
            return;
        }
    };
    let mut executor = HudExecutor {
        scene,
        resources,
        menu_state,
        inputs,
        math: MenuBasisMath::new(*inputs.sine_quarter),
        panel_texts: Vec::new(),
    };
    // 0041C487: 004A4400 begins the batch the weapon models queue into.
    let mut result = executor
        .scene
        .submit(ResolvedSceneCommand::BeginBatch)
        .map_err(|error| error.to_string());
    for op in ops {
        if result.is_err() {
            break;
        }
        result = match op {
            HudOp::Omission(omission) => {
                log_once(world, format!("hud omission {omission:?}"), || {
                    format!("Player HUD: the vehicle HUD's {omission:?} is not ported; skipped")
                });
                Ok(())
            }
            HudOp::SetReticle5c0(value) => world
                .arena
                .set_vehicle_hud_5c0(vehicle, value)
                .map_err(|error| error.to_string()),
            HudOp::SetReticleCorners(corners) => world
                .arena
                .set_vehicle_hud_corners(vehicle, corners)
                .map_err(|error| error.to_string()),
            // 0041CD45: 0046C760(0x21, 0) on the vehicle's player.
            HudOp::PlayerCommand(raw_id) => {
                let context = PlayerCommandContext {
                    process: Some(executor.inputs.factory_process),
                    executable: executor.inputs.executable,
                    resident: Some(executor.resources.resident_runtime()),
                };
                let request = PlayerCommandRequest {
                    player,
                    operation: PlayerCommandOperation::Set { raw_id, silent: 0 },
                };
                world
                    .arena
                    .apply_player_command(request, &context)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
            HudOp::ClearNotice(index) => world
                .arena
                .clear_vehicle_hud_notice(vehicle, index)
                .map_err(|error| error.to_string()),
            HudOp::CarriedActor {
                actor,
                centre,
                distance,
                basis_angle,
            } => match executor.carried_actor(world, actor, centre, distance, basis_angle) {
                Ok(omissions) => {
                    for omission in omissions {
                        log_once(world, format!("hud carried {omission:?}"), || {
                            format!("Player HUD: a carried actor's draw is not ported ({omission:?}); skipped")
                        });
                    }
                    Ok(())
                }
                Err(error) => Err(error),
            },
            HudOp::TerrainRadar(op) => match executor.terrain_radar(&mut world.rng, &op) {
                Ok(0) => Ok(()),
                Ok(outside) => {
                    log_once(world, "hud terrain radar outside".into(), || {
                        format!("Player HUD: {outside} terrain radar pixels lie outside the canvas; skipped")
                    });
                    Ok(())
                }
                Err(error) => Err(error),
            },
            // 004723F0: 00472409 draws the shared process LCG 0048F060
            // once, then the walk draws while jammed and writes its blips.
            HudOp::RadarContents(pass) => {
                let blink = crate::name_entry::name_entry_rng_next(&mut world.rng);
                match pass {
                    Some(pass) => crate::radar_blips::radar_blips(
                        &pass,
                        executor.inputs.sine_quarter,
                        blink,
                        &mut world.rng,
                        crate::name_entry::name_entry_rng_next,
                    )
                    .map_err(|error| error.to_string())
                    .and_then(|blips| executor.radar_blips(&blips)),
                    None => Ok(()),
                }
            }
            op => executor.draw(world, op),
        };
    }
    if let Err(error) = result {
        log_once(world, format!("hud vehicle draw {error}"), || {
            format!("Player HUD: the vehicle's HUD (0041C320) stopped: {error}; drawn pixels kept")
        });
        // Port containment, as for the shown menu.
        if executor.scene.feature_continuation_allowed() {
            let _ = executor.scene.continue_after_feature_error();
        }
    }
}

/// The plan's inputs: [`hud_frame`] in the camera's layout and the
/// vehicle, player and mission state.
fn vehicle_hud_inputs(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, '_>,
    resources: RenderResources<'_>,
    inputs: &PlayerHudInputs<'_, '_>,
    vehicle: ObjectId,
    player: PlayerId,
    network: Option<&crate::network::link::NetworkLink>,
) -> Result<HudInputs, String> {
    // 00441499..004414BE: the camera's first-person mode passes the
    // "First Person" setting's BYTE; otherwise the layout is 0.
    let first_person = world
        .arena
        .camera_first_person()
        .map_err(|error| error.to_string())?;
    let layout = if first_person {
        inputs.first_person_setting as u8
    } else {
        0
    };
    let frame = hud_frame(world, scene, resources, inputs, layout)?;
    let view = world
        .last_draw
        .as_ref()
        .ok_or("the HUD needs the world draw's view record")?;
    let eye_words = [view.eye[0] as u16, view.eye[2] as u16];
    let (hud_vehicle, hud_player) = world
        .arena
        .vehicle_hud_sources(
            vehicle,
            player,
            HudReticleSettings {
                hud: inputs.hud_setting,
                settings_7c: inputs.settings_7c,
                special_view: inputs.special_view,
            },
        )
        .map_err(|error| error.to_string())?;
    let network = match network {
        _ if inputs.factory_process.network_flag == 0 => None,
        None => return Err("the online HUD needs the session's link".into()),
        Some(link) => Some(hud_network(
            world,
            inputs.factory_process,
            link,
            vehicle,
            hud_vehicle.team,
        )?),
    };
    Ok(HudInputs {
        frame,
        vehicle: hud_vehicle,
        player: hud_player,
        world: HudWorld {
            network,
            milestone_total: world.briefing_milestone_count,
            seconds: world.mission.save.elapsed_seconds,
            eye_words,
            annotations: world
                .arena
                .hud_annotations()
                .map_err(|error| error.to_string())?,
            radar_actors: world
                .arena
                .hud_radar_actors()
                .map_err(|error| error.to_string())?,
            radar_jitter: inputs
                .radar_jitter
                .ok_or("the radar needs the world map's jitter")?,
        },
    })
}

/// `0041E7A3..0041E9DB`'s score line and Powerball's ball, for a vehicle on
/// `team`.
fn hud_network(
    world: &WorldSession,
    process: &ParticleFactoryProcessState,
    link: &crate::network::link::NetworkLink,
    vehicle: crate::object_runtime::ObjectId,
    team: u32,
) -> Result<crate::vehicle_hud::HudNetwork, String> {
    use crate::vehicle_hud::{HudNetwork, HudScore};
    let mode = process.settings[0x10 / 4];
    let slot = usize::from(process.local_slot);
    let scores = link
        .game
        .scores
        .get(slot)
        .ok_or("this machine's slot is past the scores")?;
    let score = if !(2..=17).contains(&(team as i32)) {
        HudScore::None
    } else {
        match mode {
            2 | 4 => HudScore::Own(scores[2]),
            0 => HudScore::Team(
                link.game
                    .teams
                    .get(team as usize)
                    .copied()
                    .flatten()
                    .ok_or("the vehicle's team has no task (0055F100)")?
                    .words_0c,
            ),
            _ => match world.arena.race() {
                None => HudScore::Unwritten,
                Some(race) => {
                    let state = world
                        .arena
                        .race_state(race)
                        .map_err(|error| error.to_string())?;
                    let record = state
                        .records
                        .get(slot)
                        .ok_or("this machine's slot is past the race's records")?;
                    let racing = record.flags_20 & 1 != 0;
                    HudScore::Race {
                        laps: if racing {
                            record.laps_18
                        } else {
                            state.laps_338
                        },
                        total: state.laps_338,
                        place: racing.then(|| 17 - i32::from(scores[2])),
                    }
                }
            },
        }
    };
    // 0041DCA6: the ball 005468D0, when there is one, held by this vehicle.
    let holds_ball = match world.arena.powerball() {
        Some(ball) => {
            let holder = world
                .arena
                .ball(ball)
                .map_err(|error| error.to_string())?
                .holder_14c;
            holder
                == world
                    .arena
                    .actor(vehicle)
                    .map_err(|error| error.to_string())?
                    .handle
        }
        None => false,
    };
    Ok(HudNetwork {
        mode,
        holds_ball,
        score,
    })
}

/// What the HUDs draw on: the live surface, the template focal, the clock,
/// the settings, font 0, the styles and the view record of the world draw.
fn hud_frame(
    world: &WorldSession,
    scene: &mut SoftwareScene<'_, '_>,
    resources: RenderResources<'_>,
    inputs: &PlayerHudInputs<'_, '_>,
    layout: u8,
) -> Result<HudFrame, String> {
    let surface = scene.surface_pixels().map_err(|error| error.to_string())?;
    let view = world
        .last_draw
        .as_ref()
        .ok_or("the HUD needs the world draw's view record")?;
    let words = view.basis_words;
    let view_basis = std::array::from_fn(|axis| {
        std::array::from_fn(|i| f32::from_bits(words[1 + axis * 3 + i]))
    });
    let font = resources.font(0).map_err(|error| error.to_string())?;
    let height = font.line_step_terms[0]
        .checked_div(font.coordinate_divisor)
        .ok_or("font 0's divisor is zero")?;
    let radar_size = resources
        .handler10_dimensions(0)
        .map_err(|error| error.to_string())?
        .map(|value| value as i32);
    let projection = HudProjection {
        view: SceneView {
            eye: view.eye,
            basis: MenuBasis {
                words: view.basis_words,
            },
        },
        projection: view.projection,
        coordinate_shift: view.coordinate_shift,
    };
    let style = |slot: i32| resources.style(slot).map_err(|error| error.to_string());
    let styles = HudStyles {
        frame: style(STYLE_FRAME)?,
        turbo: style(STYLE_TURBO)?,
        bar: style(STYLE_BAR)?,
        counters: [
            style(STYLE_COUNTERS[0])?,
            style(STYLE_COUNTERS[1])?,
            style(STYLE_COUNTERS[2])?,
        ],
    };
    Ok(HudFrame {
        surface,
        focal: inputs.template_projection.focal,
        layout,
        clock: inputs.process.animation().value() as i32,
        hud_setting: inputs.hud_setting,
        font_quarter_height: height / 4,
        projection,
        view_basis,
        sine_quarter: *inputs.sine_quarter,
        styles,
        radar_size,
        viewport: scene.pixels.pipeline.viewport,
    })
}

struct HudExecutor<'s, 'f, 'r, 'm, 'i, 'p> {
    scene: &'s mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    menu_state: &'m mut SoftwareMenuState,
    inputs: &'i PlayerHudInputs<'p, 'r>,
    math: MenuBasisMath,
    /// The switch panels' text packets, queued until the flush.
    panel_texts: Vec<(HudText, HudTextPosition)>,
}

impl HudExecutor<'_, '_, '_, '_, '_, '_> {
    fn draw(&mut self, world: &WorldSession, op: HudOp) -> Result<(), String> {
        match op {
            HudOp::Quad(quad) => self
                .scene
                .draw_sprite_direct(quad)
                .map_err(|error| error.to_string()),
            HudOp::Flat(quad) => self
                .scene
                .draw_flat_quad_direct(quad)
                .map_err(|error| error.to_string()),
            HudOp::Text { text, position } => self.text(text, position),
            HudOp::RadarLabel {
                text,
                position,
                color_word,
            } => self.radar_label(&text, position, color_word),
            // 0041D2BB..0041D355: 00490030, then 00490130 with each
            // channel shifted right two bits.
            HudOp::WeaponLight => {
                self.scene
                    .submit(ResolvedSceneCommand::DirectionalLight {
                        direction: WEAPON_LIGHT_DIRECTION,
                        rgb: WEAPON_LIGHT_RGB,
                    })
                    .map_err(|error| error.to_string())?;
                self.scene
                    .submit(ResolvedSceneCommand::AmbientLight(WEAPON_AMBIENT))
                    .map_err(|error| error.to_string())
            }
            HudOp::WeaponModel {
                model,
                centre,
                basis_angle,
                distance,
            } => self.weapon_model(world, model, centre, basis_angle, distance),
            HudOp::PanelText { text, position } => {
                self.panel_texts.push((text, position));
                Ok(())
            }
            // 0041E1A4: 004A4460 draws the queued weapon models, then the
            // panels' text packets, whose sort word follows every face.
            HudOp::Flush => {
                self.scene
                    .submit(ResolvedSceneCommand::FlushBatch)
                    .map_err(|error| error.to_string())?;
                for (text, position) in std::mem::take(&mut self.panel_texts) {
                    self.text_pass(&text, position, &[(0, 0)])?;
                }
                Ok(())
            }
            HudOp::Omission(_)
            | HudOp::SetReticle5c0(_)
            | HudOp::SetReticleCorners(_)
            | HudOp::PlayerCommand(_)
            | HudOp::ClearNotice(_)
            | HudOp::RadarContents(_)
            | HudOp::TerrainRadar(_)
            | HudOp::CarriedActor { .. } => Ok(()),
        }
    }

    /// `0041DBD8..0041DD25`: the view record's eye at the origin with the
    /// identity basis `00547590`, `0048FB00` with the record's fade, the
    /// projection centred on the icon (`0055F22C`), then `0049FEA0` with the
    /// model at `distance` (1000 for an icon), turned -0x1000 in YZ and the
    /// clock times 400 in XZ. The descriptor is `0043A150`'s, whose vtable
    /// `00522360` answers the model's values (`0043A270`). `0041A740` sets
    /// the same record and descriptor for each of a panel's models.
    fn weapon_model(
        &mut self,
        world: &WorldSession,
        model: i16,
        centre: [i32; 2],
        basis_angle: i32,
        distance: i32,
    ) -> Result<(), String> {
        let view = world
            .last_draw
            .as_ref()
            .ok_or("the weapon icon needs the world draw's view record")?;
        let fade = SceneFade::new(view.projection.fade);
        let setup = SceneSetup {
            focal: self.inputs.template_projection.focal,
            viewport: view.projection.viewport,
            center: centre,
            fade: view.projection.fade,
            fade_factor: fade.factor,
        };
        let mut basis = MenuBasis::INITIAL;
        self.math
            .rotate_in_plane(&mut basis, -0x1000, RotationPlane::Yz);
        self.math
            .rotate_in_plane(&mut basis, basis_angle, RotationPlane::Xz);
        let mut host = ResidentSceneHost {
            resources: self.resources,
            backend: &mut *self.scene,
        };
        let commands = [
            SceneCommand::Setup(setup),
            SceneCommand::PublishView(SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            }),
            SceneCommand::Model(SceneModelSubmission {
                model_slot: model,
                position: [WEAPON_MODEL_POSITION[0], WEAPON_MODEL_POSITION[1], distance],
                basis,
                animation: self.inputs.process.animation(),
                surface_table: ModelSurfaceTableOrigin::TemplateDefault,
                primitive_context: ModelPrimitiveContext::Inherited,
                failure_policy: ModelFailurePolicy::StopScene,
                policy: ModelCallbackPolicy::NameEntry {
                    selector_one: self.inputs.process.object_selector_one(),
                },
            }),
        ];
        for command in commands {
            crate::frontend_scene::FrontendSceneHost::submit(&mut host, command)
                .map_err(|error| format!("{error:?}"))?;
        }
        Ok(())
    }

    /// `0041D5E9..0041D8D5`: the view record's eye at the origin with the
    /// identity basis, `0048FB00` with the world's fade, the projection
    /// centred on the slot (`0055F23C`), then the actor's own draw `+2C`
    /// lent the position (0, 0, `distance`) and the identity basis turned
    /// `basis_angle` in XZ (`00491530`), with the world draw's light grid
    /// and fog colour. Returns the draw's port omissions.
    fn carried_actor(
        &mut self,
        world: &mut WorldSession,
        actor: crate::object_runtime::ObjectId,
        centre: [i32; 2],
        distance: u16,
        basis_angle: i32,
    ) -> Result<Vec<crate::object_runtime::Flag0ObjectDrawOmission>, String> {
        use crate::object_runtime::{
            Flag0ObjectDrawFailurePolicy, Flag0ObjectDrawRequest, ObjectDrawCone,
            ObjectDrawContext, ObjectDrawLighting,
        };
        // Its draw plays the game: the game's projection, not the view
        // shown's.
        let projection = world
            .last_draw
            .as_ref()
            .ok_or("a carried actor needs the world draw's view record")?
            .game_projection;
        let fade = SceneFade::new(projection.fade);
        let view = SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        };
        let mut host = ResidentSceneHost {
            resources: self.resources,
            backend: &mut *self.scene,
        };
        for command in [
            SceneCommand::Setup(SceneSetup {
                focal: self.inputs.template_projection.focal,
                viewport: projection.viewport,
                center: centre,
                fade: projection.fade,
                fade_factor: fade.factor,
            }),
            SceneCommand::PublishView(view),
        ] {
            crate::frontend_scene::FrontendSceneHost::submit(&mut host, command)
                .map_err(|error| format!("{error:?}"))?;
        }
        let mut basis = MenuBasis::INITIAL;
        self.math
            .rotate_in_plane(&mut basis, basis_angle, RotationPlane::Xz);
        let far_color = self.material_word(WORLD_FAR_COLOR_SLOT)?;
        let executable = self
            .inputs
            .executable
            .ok_or("a carried actor's draw needs the executable")?;
        let mut process = self
            .inputs
            .object_draw_process
            .ok_or("a carried actor's draw needs the object draw process")?
            .clone();
        let grid = self
            .inputs
            .light_grid
            .ok_or("a carried actor's draw needs the world's light grid")?;
        let WorldSession {
            arena,
            terrain,
            terrain_source,
            presentation,
            ..
        } = world;
        let mut context = ObjectDrawContext {
            view,
            near: projection.fade[0],
            far: projection.fade[1],
            far_color,
        };
        let network_flag = self.inputs.factory_process.network_flag;
        let selection = self.scene.select_default_surface_callbacks();
        let outcome = arena
            .draw_with_lent_pose(
                actor,
                [0, 0, distance],
                basis,
                &mut Flag0ObjectDrawRequest {
                    context: &mut context,
                    cone: ObjectDrawCone::Retail,
                    process: &mut process,
                    network_flag: &network_flag,
                    terrain: &mut crate::world_actor_terrain::WorldActorTerrainHost {
                        residency: self.resources.resident_runtime(),
                        source: terrain_source,
                        prepared: terrain,
                    },
                    lighting: ObjectDrawLighting {
                        grid,
                        tint_word: &mut presentation.tint_word,
                        direction: presentation.terrain_light_direction,
                        spacing: terrain.sampling().spacing,
                        executable,
                    },
                    host: &mut crate::software_scene::SoftwareObjectDrawHost {
                        terrain: crate::software_scene::ModelTerrainRead::Live(terrain),
                        scene: &mut *self.scene,
                        animation: self.inputs.process,
                        executable,
                        attract_demo: &false,
                        failure_policy:
                            Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
                        effects: None,
                    },
                    failure_policy:
                        Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
                },
            )
            .map_err(|error| error.to_string());
        self.scene.restore_surface_callbacks(selection);
        Ok(outcome?.omissions)
    }

    /// The walk's blips: each dot's pixels (`004719E0`) stored straight
    /// into the frame in its team's converted H7 colour, halved through
    /// renderer `+30` for a dying actor (`00472170`), and each marker as
    /// `004736E0`'s effect quad of sprite 7 in its H6 style.
    fn radar_blips(&mut self, blips: &[crate::radar_blips::RadarBlip]) -> Result<(), String> {
        use crate::radar_blips::{dot_pixels, RadarBlip};
        // Renderer +30 derives from the display's material shifts
        // (004A49B0), not from the H7 conversion's.
        let shifts = self.scene.pixels.renderer.material_shifts;
        let network = self.inputs.factory_process.network_flag != 0;
        for blip in blips {
            match *blip {
                RadarBlip::Dot {
                    position,
                    team,
                    dim,
                    pinned,
                } => {
                    let lookup =
                        crate::briefing_markers::briefing_marker_material_lookup(team, network);
                    let mut colour = None;
                    for slot in std::iter::once(lookup.initial_slot).chain(lookup.replacement_slot)
                    {
                        colour = Some(self.material_word(slot)?);
                    }
                    let mut colour = colour.expect("004721A0 resolves one slot at least");
                    if dim {
                        colour =
                            (colour >> 1) & (crate::briefing_markers::marker_dim_mask(shifts) >> 1);
                    }
                    let words: Vec<_> = dot_pixels(position, pinned)
                        .into_iter()
                        .map(|pixel| (pixel, colour as u16, 0xff))
                        .collect();
                    let outside = self.scene.pixels.frame.put_native_words(&words, shifts);
                    if outside != 0 {
                        return Err(format!(
                            "a radar dot at {position:?} writes {outside} pixels outside the canvas"
                        ));
                    }
                }
                RadarBlip::Marker {
                    position: [x, y],
                    half_size,
                    style,
                } => {
                    let color_word = self
                        .resources
                        .style(style)
                        .map_err(|error| error.to_string())?;
                    self.scene
                        .draw_sprite_direct(crate::scene_sprite::DirectSpriteQuad {
                            sprite_slot: 7,
                            corners: [
                                [x - half_size, y - half_size],
                                [x + half_size, y - half_size],
                                [x + half_size, y + half_size],
                                [x - half_size, y + half_size],
                            ]
                            .map(|corner| corner.map(|value| value as i16)),
                            vertex_depths: [0x40; 4],
                            descriptor_word: 0,
                            mode: crate::scene_sprite::DirectSpriteMode::Effect { color_word },
                        })
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(())
    }

    /// `00470170` on the frame: H10 global slots 0..2, its H7 colours and
    /// the process map, then its pixels stored straight into the frame,
    /// whole pixels first and then the rim pixels (they never overlap). A
    /// CPU frame reads each rim pixel's destination back as retail does; a
    /// device frame keeps no CPU pixels, so a rim pixel blends its share of
    /// samples over the destination ([`TerrainRadar::rim_blend`], a port
    /// approximation). Returns the pixels outside the canvas, skipped.
    ///
    /// [`TerrainRadar::rim_blend`]: crate::terrain_radar::TerrainRadar::rim_blend
    fn terrain_radar(
        &mut self,
        rng: &mut u32,
        op: &crate::vehicle_hud::TerrainRadarOp,
    ) -> Result<usize, String> {
        use crate::terrain_radar::{
            terrain_colours, terrain_radar, TerrainRadarColours, TerrainRadarGrids,
            TerrainRadarPass, TerrainRadarPixel, MASK_SLOT, NOISE_SLOTS, OVERLAY_SLOTS,
            TERRAIN_EXTRA_SLOTS, TERRAIN_MATERIAL_SLOTS,
        };
        use inf_formats::{SampleGridRole, SampleGridValues};
        let grid = |slot, role| {
            self.resources
                .handler10_sample_grid(slot, role)
                .map_err(|error| error.to_string())
        };
        let samples = grid(0, SampleGridRole::EightWordSamples)?;
        let masks = grid(1, SampleGridRole::FourSampleMask)?;
        let overlay = grid(2, SampleGridRole::MaterialIndices)?;
        let (
            SampleGridValues::SignedWords8(sample_values),
            SampleGridValues::SignedBytes(mask_values),
            SampleGridValues::SignedBytes(overlay_values),
        ) = (&samples.samples, &masks.samples, &overlay.samples)
        else {
            return Err("H10 slots 0..2 do not hold the terrain radar's layouts".into());
        };
        let shifts = self.scene.pixels.renderer.material_shifts;
        let colour = |slot: i32| self.material_word(slot);
        let mut overlay_colours = [0; 16];
        for (entry, slot) in overlay_colours.iter_mut().zip(OVERLAY_SLOTS) {
            *entry = colour(slot)?;
        }
        let colours = TerrainRadarColours {
            terrain: terrain_colours(
                fallible(TERRAIN_MATERIAL_SLOTS.map(colour))?,
                fallible(TERRAIN_EXTRA_SLOTS.map(colour))?,
                shifts,
            ),
            overlay: overlay_colours,
            noise: fallible(NOISE_SLOTS.map(colour))?,
            mask: colour(MASK_SLOT)?,
        };
        let map = self
            .inputs
            .radar_map
            .ok_or("the terrain radar needs the process map 0054ED78")?;
        let pass = TerrainRadarPass {
            grids: TerrainRadarGrids {
                size: [samples.width as i32, samples.height as i32],
                samples: sample_values,
                masks: mask_values,
                overlay: overlay_values,
            },
            colours,
            map: map.indices(),
            centre: op.centre,
            viewport: op.viewport,
            eye_words: op.eye_words,
            rotation: op.rotation,
            jitter: op.jitter,
        };
        let Some(radar) = terrain_radar(&pass, rng, crate::name_entry::name_entry_rng_next)
            .map_err(|error| error.to_string())?
        else {
            return Ok(0);
        };
        let frame = &mut self.scene.pixels.frame;
        let mut words = Vec::with_capacity(radar.pixels.len());
        let mut rims = Vec::new();
        for &(position, pixel) in &radar.pixels {
            match pixel {
                TerrainRadarPixel::Word(word) => words.push((position, word, 0xff)),
                TerrainRadarPixel::Rim(terms) => rims.push((position, terms)),
            }
        }
        let mut outside = frame.put_native_words(&words, shifts);
        words.clear();
        for (position, terms) in rims {
            if frame.has_cpu_pixels() {
                match frame.native_word(position, shifts) {
                    Some(destination) => {
                        words.push((position, radar.rim(terms, destination), 0xff))
                    }
                    None => outside += 1,
                }
            } else if let Some((word, alpha)) = radar.rim_blend(terms, shifts) {
                words.push((position, word, alpha));
            }
        }
        Ok(outside + frame.put_native_words(&words, shifts))
    }

    /// An H7 colour as the display converted it at load (`0050E690`).
    fn material_word(&self, slot: i32) -> Result<u32, String> {
        use crate::render_resources::{
            RenderResourceData, RenderResourceKind, RenderResourceRequest,
        };
        let resource = self
            .resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Material,
                argument: slot,
            })
            .map_err(|error| error.to_string())?;
        let RenderResourceData::Material(material) = resource.data else {
            return Err(format!("H7 slot {slot:#x} is not a material"));
        };
        let layout = crate::scene_resources::SceneResourceBackend::color_layout(&*self.scene);
        Ok(layout.convert_material(material))
    }

    /// `00472AEF`: one `004A3AB0` pass in the label's own colour.
    fn radar_label(
        &mut self,
        text: &[u8],
        position: [i32; 2],
        color_word: u32,
    ) -> Result<(), String> {
        let resources = self.resources;
        let font = resources.font(0).map_err(|error| error.to_string())?;
        let mut request = LineTextRequest {
            text,
            font,
            position_pixels: position,
            color: MenuColor::from_word(color_word),
            base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
            context_origin_pixels: self
                .inputs
                .template_projection
                .viewport
                .map(|value| value / 2),
        };
        let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut *self.scene,
            resources,
            focal: self.inputs.template_projection.focal,
            process: self.inputs.process,
            state: &mut *self.menu_state,
        })
        .map_err(|error| error.to_string())?;
        draw_line_text(&mut request, &resources, &mut menu).map_err(|error| format!("{error:?}"))
    }

    /// `00415650`: style 7 one pixel right and down, then style 0.
    fn text(&mut self, text: HudText, position: HudTextPosition) -> Result<(), String> {
        self.text_pass(&text, position, &[(7, 1), (0, 0)])
    }

    /// `004A3AB0` once per `(style, offset)` with H4 font 0.
    fn text_pass(
        &mut self,
        text: &HudText,
        position: HudTextPosition,
        passes: &[(i32, i32)],
    ) -> Result<(), String> {
        let resources = self.resources;
        let bytes = match text {
            HudText::Bytes(bytes) => bytes.as_slice(),
            HudText::String(id) => resources
                .string(i32::from(*id))
                .map_err(|error| error.to_string())?,
        };
        let font = resources.font(0).map_err(|error| error.to_string())?;
        let measure = |bytes: &[u8]| -> Result<i32, String> {
            Ok(i32::from(
                measure_text_width(font, bytes).map_err(|error| error.to_string())? as i16,
            ))
        };
        let width = || measure(bytes);
        let [x, y] = match position {
            HudTextPosition::At(point) => point,
            HudTextPosition::CentredOn([x, y]) => [x - width()? / 2, y],
            HudTextPosition::CentredUnder {
                point: [x, y],
                spacer,
            } => [x - width()? / 2, y + measure(spacer)?],
            HudTextPosition::ScreenCentred { y } => {
                let surface = self
                    .scene
                    .surface_pixels()
                    .map_err(|error| error.to_string())?;
                [(surface[0] - width()?) / 2, y]
            }
        };
        let centre = self
            .inputs
            .template_projection
            .viewport
            .map(|value| value / 2);
        for &(style, offset) in passes {
            let color =
                MenuColor::from_word(resources.style(style).map_err(|error| error.to_string())?);
            let mut request = LineTextRequest {
                text: bytes,
                font,
                position_pixels: [x + offset, y + offset],
                color,
                base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
                context_origin_pixels: centre,
            };
            let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
                scene: &mut *self.scene,
                resources,
                focal: self.inputs.template_projection.focal,
                process: self.inputs.process,
                state: &mut *self.menu_state,
            })
            .map_err(|error| error.to_string())?;
            draw_line_text(&mut request, &resources, &mut menu)
                .map_err(|error| format!("{error:?}"))?;
        }
        Ok(())
    }
}
