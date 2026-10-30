//! A player's shown menu (+0C) on the native font renderer. `00413820`
//! draws it before the message boxes, suppressed or not, with the
//! controller draw `00453610`; the type-3 controller's virtual +0C
//! (`004141B0`) frames a viewport narrower than the screen with a panel,
//! then the current menu draws its rows (`00452570`).
//!
//! Text rows (`00453A80`) draw as every menu's do; a factory's menu draws
//! its own rows through [`crate::factory_shop_draw`] (its draw `004316E0`
//! is the base `00452570` while its factory `+5C` is set, and `00431000`
//! never leaves it null); the in-world menu's
//! Options draw their value rows and sliders from the live settings, as the
//! frontend's do. Any other row callback is skipped and reported once, so
//! the rest of the menu still draws.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::{
    factory_shop_draw::{draw_shop_row, ShopDraw, ShopDrawError, ShopDrawHost, ShopPreview},
    frontend_scene::{
        FrontendSceneHost, ModelCallbackPolicy, ModelFailurePolicy, ModelPrimitiveContext,
        ModelSurfaceTableOrigin, SceneCommand, SceneFade, SceneModelSubmission, SceneSetup,
        SceneView,
    },
    frontend_screen::FrontendProcessState,
    menu::{
        MenuBasis, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuDrawInput,
        MenuDrawOutcome, MenuError, MenuRowDescriptor, MenuRowDrawRequest, MenuState,
    },
    menu_basis::MenuBasisMath,
    menu_text::{MenuRowTextRequest, MenuTextPlacement, MenuTextResources},
    object_runtime::{PlayerId, ShownMenu},
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    scene_resources::ResidentSceneHost,
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
    software_menu::{SoftwareMenu, SoftwareMenuError},
    software_scene::SoftwareSceneError,
};

/// The default text row, which the software menu draws.
const TEXT_ROW: u32 = 0x0045_3a80;
const OPTIONS_TITLE_ROW: u32 = 0x0045_f520;
const DEVICE_ROW: u32 = 0x0046_2de0;
/// `00414C20`, the hint list's row: hint `+5C`'s text.
const HINT_ROW: u32 = 0x0041_4c20;
/// `00414C80`, the database's row: the entry's text and spinning model.
const DATABASE_ROW: u32 = 0x0041_4c80;
/// `00415090`, the Cheats Menu's Skip to: "%s %s" (`0052FE2C`) of the row's
/// text and the name of the world in its argument 1's WORD (`0052F880`),
/// H0 `0xE0` on, left-aligned.
const SKIP_TO_ROW: u32 = 0x0041_5090;
const SKIP_WORLD_CELL: u32 = 0x0052_f880;
const WORLD_NAMES: u16 = 0xe0;
/// The port's Cheats Menu rows (`session_platform::cheats`): Teleport to
/// world, and each world of its list by its number and name. Addresses no
/// retail code has.
pub(crate) const TELEPORT_ROW: u32 = 0xf000_0001;
pub(crate) const WORLD_ROW: u32 = 0xf000_0003;
/// What the list calls the worlds whose name slots hold no name ("Cheat
/// Accepted" and "Completed").
const UNNAMED_WORLDS: [&[u8]; 2] = [b"Alien Brain map", b"Unused prototype"];
/// The ACAM's model, which the database shows in the entry's form.
const VEHICLE_MODEL: u16 = 0x75;

/// What the computer's database and hint list draw from: their tables, the
/// player's grant mask (`+9C`'s words) and the process RNG `0048F060`.
pub struct ComputerDraw<'a, 'r> {
    pub database: &'a [crate::object_runtime::DatabaseEntry],
    pub hint_texts: &'a [u16],
    pub grants: &'a [u32],
    pub rng: &'a mut u32,
    /// A second residency's resources for models the current world has not
    /// loaded (the port's database model fix); None draws retail's way.
    pub models: Option<RenderResources<'r>>,
    /// The entries' models that stopped this draw (slot, reason), for the
    /// caller's log; the menu's other rows still draw.
    pub model_failures: &'a mut Vec<(i16, String)>,
    /// `0052F880`, the world the Cheats Menu's Skip to names.
    pub skip_world: u32,
}

/// `004141B0`'s sprite, vertex DWORD and descriptor.
const PANEL_SPRITE: u16 = 0x2a6;
const PANEL_VERTEX: u32 = 0x4000;
const PANEL_DESCRIPTOR: u32 = 2;

/// The row callbacks the draw skipped.
pub type ShownMenuOmissions = BTreeSet<u32>;

#[derive(Debug, Error)]
pub enum ShownMenuDrawError {
    #[error(transparent)]
    Software(#[from] SoftwareMenuError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error("resource: {0}")]
    Resource(String),
    #[error("model {slot}: {reason}")]
    Model { slot: i16, reason: String },
    #[error("shop row: {0}")]
    Shop(Box<ShopDrawError<ShownMenuDrawError>>),
    #[error("settings row: {0}")]
    SettingsRow(String),
}

/// What the draw needs beyond the menu and the renderer.
pub struct ShownMenuDrawContext<'a, 'r> {
    /// The current menu's shop, when a factory's menu is current.
    pub shop: Option<ShopDraw<'a>>,
    /// The live settings the Options' value rows and sliders read.
    pub settings: Option<&'a crate::frontend_settings::FrontendSettings>,
    /// The selected video device's name (`00462DE0`'s text).
    pub device_name: Option<&'a [u8]>,
    /// The computer's database and hint list.
    pub computer: Option<ComputerDraw<'a, 'r>>,
    /// The animation clock and word `00533760`.
    pub process: &'a FrontendProcessState,
    /// `00546F20/24`, the menu context's `+38/+3C`.
    pub focal: [i32; 2],
    pub math: &'a MenuBasisMath,
    /// The player commands the rows set (`0046C760(raw_id, 0)`), for the
    /// caller to apply before the message boxes draw.
    pub commands: &'a mut Vec<(PlayerId, u32)>,
}

/// `00453610` over the shown menu.
pub fn draw_shown_menu<'r>(
    menu: &mut ShownMenu,
    input: MenuDrawInput<'_>,
    software: &mut SoftwareMenu<'_, '_, 'r>,
    context: ShownMenuDrawContext<'_, 'r>,
    omissions: &mut ShownMenuOmissions,
) -> Result<MenuDrawOutcome, MenuError<ShownMenuDrawError>> {
    let viewport = menu.stack.spec().viewport;
    menu.stack.draw(
        input,
        &mut ShownMenuDrawHost {
            menu: software,
            surface: input.surface_pixels,
            viewport,
            context,
            omissions,
        },
    )
}

/// `004141B0`: around a viewport narrower than 0x400, four quarters of
/// sprite 0x2A6 over the viewport widened by 0x78 left, 0x12C up, 0xC8
/// right and 0x78 down, in pixels of the surface. The quarters meet at the
/// centre; each starts on the panel's edge.
///
/// `004141B0` writes neither X of the third quarter's first two corners:
/// they are what the second quarter's +9C left in the shared packet, which
/// Infest01's draws show at the right edge.
pub fn shown_menu_panel(viewport: [i16; 4], surface: [i32; 2]) -> Option<[DirectSpriteQuad; 4]> {
    if viewport[2] == 0x400 {
        return None;
    }
    let [width, height] = surface;
    let scale = |units: i32, pixels: i32| units.wrapping_mul(pixels) / 1024;
    let x0 = scale(i32::from(viewport[0]) - 0x78, width);
    let y0 = scale(i32::from(viewport[1]) - 0x12c, height);
    let x1 = scale(i32::from(viewport[2]) + 0xc8, width);
    let y1 = scale(i32::from(viewport[3]) + 0x78, height);
    let (cx, cy) = (x0.wrapping_add(x1) / 2, y0.wrapping_add(y1) / 2);
    let (half_width, half_height) = (x1.wrapping_sub(x0) / 2, y1.wrapping_sub(y0) / 2);
    let (left, right) = (cx.wrapping_sub(half_width), cx.wrapping_add(half_width));
    let (top, bottom) = (cy.wrapping_sub(half_height), cy.wrapping_add(half_height));
    let quarter = |corners: [[i32; 2]; 4]| DirectSpriteQuad {
        sprite_slot: PANEL_SPRITE,
        corners: corners.map(|[x, y]| [x as i16, y as i16]),
        vertex_depths: [PANEL_VERTEX; 4],
        descriptor_word: PANEL_DESCRIPTOR,
        mode: DirectSpriteMode::Textured,
    };
    Some([
        quarter([[left, cy], [left, top], [cx, top], [cx, cy]]),
        quarter([[cx, top], [right, top], [right, cy], [cx, cy]]),
        quarter([[right, cy], [right, bottom], [cx, bottom], [cx, cy]]),
        quarter([[cx, bottom], [left, bottom], [left, cy], [cx, cy]]),
    ])
}

struct ShownMenuDrawHost<'a, 'm, 'f, 'r> {
    menu: &'a mut SoftwareMenu<'m, 'f, 'r>,
    surface: [i32; 2],
    viewport: [i16; 4],
    context: ShownMenuDrawContext<'a, 'r>,
    omissions: &'a mut ShownMenuOmissions,
}

impl MenuDrawHost for ShownMenuDrawHost<'_, '_, '_, '_> {
    type Error = ShownMenuDrawError;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        Ok(self.menu.begin_controller(setup)?)
    }

    /// `004141B0`, at any fade.
    fn draw_controller_frame(&mut self) -> Result<(), Self::Error> {
        for quad in shown_menu_panel(self.viewport, self.surface)
            .into_iter()
            .flatten()
        {
            self.menu.scene_mut().draw_sprite_direct(quad)?;
        }
        Ok(())
    }

    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        Ok(self.menu.publish_menu_basis(basis)?)
    }

    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Ok(self.menu.clip_rect()?)
    }

    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        Ok(self.menu.set_clip_rect(clip)?)
    }

    fn colors(&self) -> MenuDrawColors {
        self.menu.colors()
    }

    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        state: &mut MenuState,
    ) -> Result<(), Self::Error> {
        if request.callback_address == TEXT_ROW {
            return Ok(self.menu.draw_row(request, state)?);
        }
        // 0045F520, the Options title: 00485320 selects a frontend camera
        // only under a class-2 screen (`+1C(2)`), never the gameplay or
        // phase-5 screen, then the text row 00453A80 draws it.
        if request.callback_address == OPTIONS_TITLE_ROW {
            request.callback_address = TEXT_ROW;
            return Ok(self.menu.draw_row(request, state)?);
        }
        // 00452C3C publishes the row projection before any callback.
        self.menu
            .publish_row_origin(request.context_origin_pixels)?;
        if let Some(settings) = self.context.settings {
            if let Some(content) = crate::settings_row::settings_row_content(
                settings,
                request.callback_address,
                request.arguments,
            ) {
                use crate::settings_row::{
                    draw_identity_row, draw_port_row, draw_settings_row, draw_volume_row,
                    SettingsRowContent,
                };
                let content = content.map_err(|address| {
                    ShownMenuDrawError::SettingsRow(format!(
                        "cell {address:#010x} is outside the settings state"
                    ))
                })?;
                let resources = self.menu.resources();
                let host = &mut *self.menu;
                return match content {
                    SettingsRowContent::Value(value) => {
                        draw_settings_row(request, value, &resources, host)
                    }
                    SettingsRowContent::Volume(value) => {
                        draw_volume_row(request, value, &resources, host)
                    }
                    SettingsRowContent::Text(label, text) => {
                        draw_identity_row(request, label, &text, &resources, host)
                    }
                    SettingsRowContent::Port(label, value) => {
                        if crate::frontend_video::modern_row_locked(
                            &settings.modern(),
                            settings.modern_support(),
                            request.arguments,
                        ) {
                            request.descriptor.color =
                                crate::frontend_video::dimmed_row_color(request.descriptor.color);
                        }
                        let mut records = state.data().records_mut();
                        crate::frontend_video::refresh_disabled_rows(
                            &mut records.items,
                            &settings.modern(),
                            settings.modern_support(),
                        );
                        draw_port_row(request, &label, &value, &resources, host)
                    }
                }
                .map_err(|error| ShownMenuDrawError::SettingsRow(error.to_string()));
            }
        }
        if request.callback_address == HINT_ROW && self.context.computer.is_some() {
            return self.hint_row(request);
        }
        if request.callback_address == DATABASE_ROW && self.context.computer.is_some() {
            return self.database_row(request, state);
        }
        if request.callback_address == SKIP_TO_ROW
            && request.arguments[1] == SKIP_WORLD_CELL
            && self.context.computer.is_some()
        {
            return self.skip_to_row(request);
        }
        if request.callback_address == TELEPORT_ROW {
            return self.literal_row(request, b"Teleport to world");
        }
        if request.callback_address == WORLD_ROW {
            let world = request.arguments[0];
            // World 1 reads both ways at once: Pelagia, retail's unused
            // prototype, on or off.
            if world == crate::pelagia::WORLD {
                let text = [
                    format!("{world:>2}  ").as_bytes(),
                    b"Pelagia - Unused prototype".as_slice(),
                ]
                .concat();
                return self.literal_row(request, &text);
            }
            let named = crate::pelagia::world_name(world, request.arguments[1] == 1);
            let name = match named.or_else(|| UNNAMED_WORLDS.get(world as usize).copied()) {
                Some(name) => name.to_vec(),
                None => {
                    let resources = self.menu.resources();
                    let id = (world as u16).wrapping_add(WORLD_NAMES) as i16;
                    let name = resources.string(i32::from(id)).map_err(resource_error)?;
                    name.split(|&byte| byte == 0).next().unwrap_or(&[]).to_vec()
                }
            };
            let text = [format!("{world:>2}  ").as_bytes(), name.as_slice()].concat();
            return self.literal_row(request, &text);
        }
        // 00462DE0, the video device row, names the selected device.
        if request.callback_address == DEVICE_ROW
            && request.arguments[1] == crate::frontend_video::VIDEO_DEVICE_CELL
        {
            if let Some(name) = self.context.device_name {
                let resources = self.menu.resources();
                return crate::settings_row::draw_identity_row(
                    request,
                    request.arguments[0] as i32,
                    name,
                    &resources,
                    &mut *self.menu,
                )
                .map_err(|error| ShownMenuDrawError::SettingsRow(error.to_string()));
            }
        }
        if let Some(shop) = self.context.shop {
            let mut host = ShopRowHost {
                menu: &mut *self.menu,
                surface: self.surface,
                process: self.context.process,
                focal: self.context.focal,
                math: self.context.math,
                commands: &mut *self.context.commands,
            };
            let drawn = draw_shop_row(request, state, shop, &mut host)
                .map_err(|error| ShownMenuDrawError::Shop(Box::new(error)))?;
            if drawn {
                return Ok(());
            }
        }
        self.omissions.insert(request.callback_address);
        Ok(())
    }
}

impl ShownMenuDrawHost<'_, '_, '_, '_> {
    /// `00415090`: the row's text, a space and the world's name.
    fn skip_to_row(&mut self, request: &mut MenuRowDrawRequest) -> Result<(), ShownMenuDrawError> {
        let world = self
            .context
            .computer
            .as_ref()
            .expect("checked by the caller")
            .skip_world;
        // 0041509A: the WORD at the argument, then each "%s" up to its NUL.
        let name = (world as u16).wrapping_add(WORLD_NAMES) as i16;
        let resources = self.menu.resources();
        let until_nul = |bytes: &[u8]| -> Vec<u8> {
            bytes
                .split(|&byte| byte == 0)
                .next()
                .unwrap_or(&[])
                .to_vec()
        };
        let label = until_nul(
            resources
                .string(i32::from(request.arguments[0] as i16))
                .map_err(resource_error)?,
        );
        let name = until_nul(resources.string(i32::from(name)).map_err(resource_error)?);
        let text = [label.as_slice(), b" ", name.as_slice()].concat();
        self.menu.draw_row_text(MenuRowTextRequest {
            descriptor: &mut request.descriptor,
            text: &text,
            placement: MenuTextPlacement::Left,
            context_origin_pixels: request.context_origin_pixels,
        })?;
        Ok(())
    }

    /// A port row's text, left-aligned.
    fn literal_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        text: &[u8],
    ) -> Result<(), ShownMenuDrawError> {
        self.menu.draw_row_text(MenuRowTextRequest {
            descriptor: &mut request.descriptor,
            text,
            placement: MenuTextPlacement::Left,
            context_origin_pixels: request.context_origin_pixels,
        })?;
        Ok(())
    }

    /// `00414C20`: the text `0046D350` gives the row's hint, left-aligned.
    fn hint_row(&mut self, request: &mut MenuRowDrawRequest) -> Result<(), ShownMenuDrawError> {
        let computer = self
            .context
            .computer
            .as_ref()
            .expect("checked by the caller");
        let hint = request.arguments[0] as usize;
        let text = *computer.hint_texts.get(hint).ok_or_else(|| {
            ShownMenuDrawError::Resource(format!("hint {hint} is past 0053A27C's table"))
        })?;
        let text = self
            .menu
            .resources()
            .string(i32::from(text))
            .map_err(resource_error)?
            .to_vec();
        self.menu.draw_row_text(MenuRowTextRequest {
            descriptor: &mut request.descriptor,
            text: &text,
            placement: MenuTextPlacement::Left,
            context_origin_pixels: request.context_origin_pixels,
        })?;
        Ok(())
    }

    /// `00414C80`: the entry's text (H0 `text + 1`), then, for an entry the
    /// player knows or one draw in three of one it does not (`0048F060`),
    /// its model. The model spins with the menu's basis at three times its
    /// extent plus 500 in front of an eye raised two fifths of that, centred
    /// on the controller's viewport top; the ACAM's shows the entry's form.
    /// A model the current world has not loaded draws nothing.
    fn database_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        state: &mut MenuState,
    ) -> Result<(), ShownMenuDrawError> {
        let index = request.arguments[0] as usize;
        let (entry, known) = {
            let computer = self
                .context
                .computer
                .as_ref()
                .expect("checked by the caller");
            let entry = *computer.database.get(index).ok_or_else(|| {
                ShownMenuDrawError::Resource(format!("database entry {index} is past its table"))
            })?;
            // 004752E0: entry 0 is always known.
            let known = index == 0
                || computer
                    .grants
                    .get(index >> 5)
                    .is_some_and(|word| word & 1 << (index & 31) != 0);
            (entry, known)
        };
        let text = self
            .menu
            .resources()
            .string(i32::from(entry.text) + 1)
            .map_err(resource_error)?
            .to_vec();
        self.menu.draw_row_text(MenuRowTextRequest {
            descriptor: &mut request.descriptor,
            text: &text,
            placement: MenuTextPlacement::Left,
            context_origin_pixels: request.context_origin_pixels,
        })?;
        if !known {
            let computer = self
                .context
                .computer
                .as_mut()
                .expect("checked by the caller");
            let draw = crate::name_entry::name_entry_rng_next(computer.rng) & 0xffff;
            if draw % 3 != 0 {
                return Ok(());
            }
        }
        if entry.model == 0 {
            return Ok(());
        }
        let model = entry.model as i16;
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: i32::from(model),
        };
        let current = self.menu.resources();
        if model < 0 || usize::from(entry.model) > current.pool_capacity(RenderResourceKind::Model)
        {
            return Ok(());
        }
        // The lazy load finds only the current world's models; the port's
        // fix draws another world's from the second residency.
        let (resources, resource, borrowed) = match current.resolve(request) {
            Ok(resource) => (current, resource, false),
            Err(_) => {
                let models = self
                    .context
                    .computer
                    .as_ref()
                    .expect("checked by the caller")
                    .models;
                match models.map(|models| (models, models.resolve(request))) {
                    Some((models, Ok(resource))) => (models, resource, true),
                    _ => return Ok(()),
                }
            }
        };
        let RenderResourceData::Model(record) = resource.data else {
            return Err(ShownMenuDrawError::Model {
                slot: model,
                reason: "the pool slot holds no model".into(),
            });
        };
        let depth = (record.header.visibility_extent as i32)
            .wrapping_mul(3)
            .wrapping_add(500);
        let eye_y = depth.wrapping_mul(2) / 5;
        let mut height = 0;
        if entry.model == VEHICLE_MODEL {
            height = 0x32;
            self.context.process.set_object_selector_one(match index {
                2 => 2,
                0xa5 => 4,
                0xa6 => 5,
                0xa7 => 3,
                _ => 1,
            });
        }
        let [width, surface_height] = self.surface;
        let shifted = |value: i32| (value + ((value >> 31) & 0x3ff)) >> 10;
        let center = [
            shifted(
                width.wrapping_mul(i32::from(self.viewport[0]) + i32::from(self.viewport[2])) / 2,
            ),
            shifted(surface_height.wrapping_mul(i32::from(self.viewport[1]))),
        ];
        let fade = [depth.wrapping_add(2000), depth.wrapping_add(2500)];
        let saved = self.menu.clip_rect()?;
        self.menu
            .set_clip_rect([0, 0, width as i16, surface_height as i16])?;
        let commands = [
            SceneCommand::Setup(SceneSetup {
                focal: self.context.focal,
                viewport: self.surface,
                center,
                fade,
                fade_factor: SceneFade::new(fade).factor,
            }),
            SceneCommand::PublishView(SceneView {
                eye: [0, eye_y, 0],
                basis: MenuBasis::INITIAL,
            }),
            SceneCommand::Model(SceneModelSubmission {
                model_slot: model,
                position: [0, height, depth],
                basis: state.basis,
                animation: self.context.process.animation(),
                surface_table: ModelSurfaceTableOrigin::TemplateDefault,
                primitive_context: ModelPrimitiveContext::Inherited,
                // The world's objects drawn outside the world: the object draw's
                // register seeds (the ACAM's forms), and an unknown register
                // stops only the model's remainder; retail holds stack history.
                failure_policy: ModelFailurePolicy::ObjectStackHistoryApproximations,
                policy: ModelCallbackPolicy::NameEntry {
                    selector_one: self.context.process.object_selector_one(),
                },
            }),
            SceneCommand::FlushBatch,
        ];
        let drawn = if borrowed {
            // Another world's model draws on a scene lent over the second
            // residency; its batch reset stands for 004A4400.
            self.menu
                .scene_mut()
                .with_current_resources(
                    crate::software_scene::CurrentResourceDraw {
                        resources,
                        batch_policy: crate::software_scene::ResourceBatchPolicy::CompletedResourceBatchRetirementApproximation,
                    },
                    |loan| -> Result<(), String> {
                        loan.reset_batch_40e6fd().map_err(|error| error.to_string())?;
                        let scene = loan.scene_after_reset().map_err(|error| error.to_string())?;
                        let mut host = ResidentSceneHost {
                            resources,
                            backend: scene,
                        };
                        for command in commands {
                            host.submit(command).map_err(|error| format!("{error:?}"))?;
                        }
                        Ok(())
                    },
                )
                .map_err(|error| match error {
                    crate::software_scene::ResourceLoanError::Callback(reason)
                    | crate::software_scene::ResourceLoanError::DeferredCommands {
                        callback_error: Some(reason),
                        ..
                    } => reason,
                    error => error.to_string(),
                })
        } else {
            let mut host = ResidentSceneHost {
                resources,
                backend: self.menu.scene_mut(),
            };
            std::iter::once(SceneCommand::BeginBatch)
                .chain(commands)
                .try_for_each(|command| host.submit(command))
                .map_err(|error| format!("{error:?}"))
        };
        // Port containment: a model that stops keeps the pixels it reached;
        // the row's clip still returns and the menu's other rows still draw
        // (retail's models complete, so its row always gets here).
        if let Err(reason) = drawn {
            let scene = self.menu.scene_mut();
            if !scene.feature_continuation_allowed() {
                return Err(ShownMenuDrawError::Model {
                    slot: model,
                    reason,
                });
            }
            scene.continue_after_feature_error()?;
            self.context
                .computer
                .as_mut()
                .expect("checked by the caller")
                .model_failures
                .push((model, reason));
        }
        self.menu.set_clip_rect(saved)?;
        Ok(())
    }
}

/// The shop rows' services on the software menu.
struct ShopRowHost<'a, 'm, 'f, 'r> {
    menu: &'a mut SoftwareMenu<'m, 'f, 'r>,
    surface: [i32; 2],
    process: &'a FrontendProcessState,
    focal: [i32; 2],
    math: &'a MenuBasisMath,
    commands: &'a mut Vec<(PlayerId, u32)>,
}

fn resource_error(error: impl std::fmt::Display) -> ShownMenuDrawError {
    ShownMenuDrawError::Resource(error.to_string())
}

impl ShopDrawHost for ShopRowHost<'_, '_, '_, '_> {
    type Error = ShownMenuDrawError;

    fn surface(&self) -> [i32; 2] {
        self.surface
    }

    fn style(&self, slot: i32) -> Result<u32, Self::Error> {
        self.menu.resources().style(slot).map_err(resource_error)
    }

    fn string(&self, id: i32) -> Result<Vec<u8>, Self::Error> {
        self.menu
            .resources()
            .string(id)
            .map(<[u8]>::to_vec)
            .map_err(resource_error)
    }

    /// `0042FE16..0042FE53`: a slot past the pool's count draws nothing;
    /// the lazy load supplies any other.
    fn model_extent(&self, slot: i16) -> Result<Option<u32>, Self::Error> {
        let resources = self.menu.resources();
        if usize::from(slot as u16) > resources.pool_capacity(RenderResourceKind::Model) {
            return Ok(None);
        }
        let resource = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: i32::from(slot),
            })
            .map_err(|error| ShownMenuDrawError::Model {
                slot,
                reason: error.to_string(),
            })?;
        match resource.data {
            RenderResourceData::Model(entry) => Ok(Some(entry.header.visibility_extent)),
            _ => Err(ShownMenuDrawError::Model {
                slot,
                reason: "the pool slot holds no model".into(),
            }),
        }
    }

    fn text(
        &mut self,
        descriptor: &mut MenuRowDescriptor,
        origin: [i32; 2],
        left: Option<&[u8]>,
        right: Option<&[u8]>,
    ) -> Result<(), Self::Error> {
        for (text, placement) in [
            (left, MenuTextPlacement::Left),
            (right, MenuTextPlacement::Right),
        ] {
            let Some(text) = text else { continue };
            self.menu.draw_row_text(MenuRowTextRequest {
                descriptor: &mut *descriptor,
                text,
                placement,
                context_origin_pixels: origin,
            })?;
        }
        Ok(())
    }

    fn quad(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error> {
        Ok(self.menu.scene_mut().draw_sprite_direct(quad)?)
    }

    /// `0042FE59..0043003C`: the clip opened to the surface, a batch, the
    /// context published with its fade, far colour 0 and its identity view
    /// raised to the eye, `0049FEA0`, the flush, the clip restored.
    fn preview(&mut self, preview: ShopPreview) -> Result<(), Self::Error> {
        let saved = self.menu.clip_rect()?;
        let [width, height] = self.surface;
        self.menu
            .set_clip_rect([0, 0, width as i16, height as i16])?;
        let mut basis = MenuBasis::INITIAL;
        self.math.rotate(&mut basis, preview.angle);
        let mut host = ResidentSceneHost {
            resources: self.menu.resources(),
            backend: self.menu.scene_mut(),
        };
        for command in [
            SceneCommand::BeginBatch,
            SceneCommand::Setup(SceneSetup {
                focal: self.focal,
                viewport: self.surface,
                center: preview.center,
                fade: preview.fade,
                fade_factor: SceneFade::new(preview.fade).factor,
            }),
            SceneCommand::FarColor(0),
            SceneCommand::PublishView(SceneView {
                eye: [0, preview.eye_y, 0],
                basis: MenuBasis::INITIAL,
            }),
            SceneCommand::Model(SceneModelSubmission {
                model_slot: preview.model_slot,
                position: preview.position,
                basis,
                animation: self.process.animation(),
                surface_table: ModelSurfaceTableOrigin::TemplateDefault,
                primitive_context: ModelPrimitiveContext::Inherited,
                failure_policy: ModelFailurePolicy::ShopPreviewStackHistoryApproximation,
                policy: ModelCallbackPolicy::NameEntry {
                    selector_one: preview.selector_one,
                },
            }),
            SceneCommand::FlushBatch,
        ] {
            host.submit(command)
                .map_err(|error| ShownMenuDrawError::Model {
                    slot: preview.model_slot,
                    reason: format!("{error:?}"),
                })?;
        }
        self.menu.set_clip_rect(saved)?;
        Ok(())
    }

    fn selector_one(&self) -> u16 {
        self.process.object_selector_one()
    }

    fn set_selector_one(&mut self, value: u16) {
        self.process.set_object_selector_one(value);
    }

    fn command(&mut self, player: PlayerId, raw_id: u32) -> Result<(), Self::Error> {
        self.commands.push((player, raw_id));
        Ok(())
    }
}

#[cfg(test)]
#[path = "shown_menu_draw_tests.rs"]
mod tests;
