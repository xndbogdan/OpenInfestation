//! Frontend scene and footer submission, 00483C60/004843D0.
//!
//! This executes scene arithmetic and submission order. Handler8 interpretation,
//! resource resolution and renderer operations require a host. No diagnostic
//! viewer transform or generated trigonometric table participates. The outer
//! 00484940 frame begin, initial renderer reset, menu gate and resource restore
//! remain the screen owner's responsibility.

use inf_formats::FrontendSceneModel;

use crate::{
    frontend_camera::AnimationClock,
    menu::MenuBasis,
    menu_basis::{MenuBasisMath, RotationPlane},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneView {
    pub eye: [i32; 3],
    /// Counter followed by X, Y, Z vectors, exactly as 0055F19C..0055F1C0.
    /// The CRT's initializer 0048F810 starts it, and the model bases
    /// 0055F1D0 and 0055F204, as identities; the port's scenes publish a
    /// view before drawing.
    pub basis: MenuBasis,
}

/// The projection/fade fields of the context copied by 0043B4A0. The host owns
/// the remaining context fields and must preserve them when applying setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneProjection {
    /// Original 00546F20/24 values. The scene doubles these with i32 wrap.
    pub focal: [i32; 2],
    pub viewport: [i32; 2],
    /// Context +64/+68, passed to 0048FB00.
    pub fade: [i32; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct SceneFrame {
    pub projection: SceneProjection,
    /// Stars precede this frame's camera publication. This must be the actual
    /// inherited renderer view, including on the first frontend frame.
    pub inherited_view: SceneView,
    pub camera_eye: [i32; 3],
    pub camera_look_at: [i32; 3],
    pub animation: AnimationClock,
    /// Screen +C0 and +120, respectively; native units are milliseconds.
    pub age_millis: i32,
    pub exit_millis: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSetup {
    pub focal: [i32; 2],
    pub viewport: [i32; 2],
    pub center: [i32; 2],
    pub fade: [i32; 2],
    pub fade_factor: i32,
}

/// 0048FB00 publishes these globals independently of the caller's context.
/// The briefing backdrop replaces them without changing context +64/+68.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneFade {
    pub bounds: [i32; 2],
    pub factor: i32,
}

impl SceneFade {
    pub fn new(bounds: [i32; 2]) -> Self {
        let [near, far] = bounds;
        Self {
            bounds,
            factor: if near < far {
                (0x100_0000_i64 / i64::from(far.wrapping_sub(near))) as i32
            } else {
                0x100_0000
            },
        }
    }
}

impl SceneSetup {
    /// The frontend doubles the context focal pair before publication
    /// (00483D41..00483D6A).
    fn new(projection: SceneProjection) -> Self {
        let setup = Self::context(projection);
        Self {
            focal: setup.focal.map(|value| value.wrapping_mul(2)),
            ..setup
        }
    }

    /// Context +38..+4F as 0043B4A0 writes them (focal as-is, viewport and
    /// its signed half) and the 0048FB00 fade factor of context +64/+68.
    pub fn context(projection: SceneProjection) -> Self {
        let fade = SceneFade::new(projection.fade);
        Self {
            focal: projection.focal,
            viewport: projection.viewport,
            center: projection.viewport.map(|value| value / 2),
            fade: projection.fade,
            fade_factor: fade.factor,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StarQuad {
    /// 00466170 command: clockwise screen corners, each with depth DWORD
    /// 0x7FFF_FFFF. Command +20 is zero; +24 is the first DWORD of
    /// the resolved color resource (00484048/0048404E), supplied by the host.
    pub corners: [[i16; 2]; 4],
    /// Full EDI passed to the lazy loader at 483F94/48403C. Pool bounds and
    /// indexing instead consume the signed low WORD, exposed by color_slot.
    pub color_argument: i32,
}

impl StarQuad {
    pub fn color_slot(&self) -> i16 {
        self.color_argument as i16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FooterQuad {
    pub sprite_slot: u16,
    pub corners: [[i16; 2]; 4],
    /// First DWORD of resource 14 from pool 0055EEC8. No palette/font type is
    /// inferred here: the renderer receives this exact descriptor word.
    pub style_word: u32,
    /// All four vertex DWORDs are 0x40; descriptor +20 is zero.
    pub vertex_attribute: u32,
}

/// Packed-value callback slot +0 of the submitted object's vtable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelCallbackPolicy {
    /// 00483C40, frontend object vtable 00528518: only selector 0 is nonzero.
    Frontend,
    /// Existing H8/0x116 sky adapter has no authenticated actor packet/source.
    /// Its static zero values remain replaceable; actual actor submissions use
    /// the separate fallible captured-object callback owner.
    WorldSkyStaticValuesApproximation,
    /// Loading object vtable00520494/+0=0040E0D0. Age is captured from
    /// signed loading-state micros/200, independently of AnimationClock.
    Loading {
        age_div_200: i32,
        selector: u16,
        pulse: u16,
    },
    /// 0043A270, name-entry object vtable 00522360: selector 1 reads the
    /// process word 00533760 (.data 1; gameplay stores 1..5), 2 returns
    /// 0xFFFF and 6 returns 1. 0043A150's descriptors end with this vtable
    /// too (the HUD's weapon icons, the shop's preview).
    NameEntry { selector_one: u16 },
    /// 00455360, panel object vtable 00520378. The panel owner computes
    /// selectors 1..3 from its native two-stage countdown and scale words.
    Panel {
        extent: u16,
        unfold: u16,
        scale: u16,
        flag: u8,
    },
    /// Base vtable 00520384: slots +0 and +4 are the CRT's `_purecall`
    /// (005116FF), so a model reading a value aborts natively. It has no
    /// value. `0043A150` installs it only transiently, ending with 00522360
    /// (see [`Self::NameEntry`]).
    PureVirtual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelSurfaceTableOrigin {
    /// 0043B4A0 copies template +50 ->00544740. Its +10/+14 callbacks are
    /// independent of the selected world's mutable table00546E60.
    TemplateDefault,
    /// Ordinary world/furniture and briefing decoration retain the selected
    /// renderer's +50 alias, including earlier 004501A0 callback writes.
    InheritedRenderer,
}

#[derive(Clone, Copy, Debug)]
pub struct SceneModelSubmission {
    pub model_slot: i16,
    pub position: [i32; 3],
    pub basis: MenuBasis,
    pub animation: AnimationClock,
    pub surface_table: ModelSurfaceTableOrigin,
    pub primitive_context: ModelPrimitiveContext,
    pub failure_policy: ModelFailurePolicy,
    pub policy: ModelCallbackPolicy,
}

/// How an explicitly unowned model value affects the enclosing scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelFailurePolicy {
    StopScene,
    /// Keep the completed command/RNG prefix when a model reads an unknown
    /// independent register. Only an unchanged, running implicit root group
    /// qualifies; every other model/backend/queue error still stops the scene.
    /// Native stack history remains unowned. This is not retail acceptance.
    UninitializedValuePrefixApproximation,
    /// LoadingModel117StackHistoryApproximation: the loading model's root
    /// starts register 3 at Infest01's stack value (see
    /// `ModelStackRegisterPolicy`); any other unknown register then keeps
    /// the prefix as above.
    LoadingModel117StackHistoryApproximation,
    /// The world objects' models drawn outside the world (the computer's
    /// database): the object draw's register seeds
    /// (`ModelStackRegisterPolicy::ObjectStackHistoryApproximations`), and
    /// any other unknown register keeps the prefix as above.
    ObjectStackHistoryApproximations,
    /// The factory shop's turning preview: the skimmer's register seed
    /// (`ModelStackRegisterPolicy::ShopPreviewModel127StackHistoryApproximation`),
    /// and any other unknown register keeps the prefix as above, so the menu
    /// still draws.
    ShopPreviewStackHistoryApproximation,
    /// The briefing preview drawn after the map panel's frame: its root
    /// starts with that frame's context bases
    /// (`ModelStackRegisterPolicy::BriefingPreviewMapFrameBases`); any
    /// failure then stops the scene as `StopScene`.
    BriefingPreviewMapFrameBases,
}

/// Context-owned packet values may differ from persistent renderer globals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelPrimitiveContext {
    Inherited,
    Caller { far_color: u32, mode: u32 },
}

impl SceneModelSubmission {
    /// The Handler8 consumer first masks its packed selector with 0xFF3F;
    /// that decoding remains the interpreter's job. These frontend/panel and
    /// explicitly static sky policies are independent of live actor callbacks.
    /// A pure virtual slot has no value.
    pub fn callback_value(&self, selector: u32) -> Option<u16> {
        Some(match self.policy {
            ModelCallbackPolicy::PureVirtual => return None,
            ModelCallbackPolicy::Frontend => self.animation.callback_value(selector),
            ModelCallbackPolicy::WorldSkyStaticValuesApproximation => 0,
            ModelCallbackPolicy::Loading {
                age_div_200,
                selector: selected,
                pulse,
            } => match selector {
                0 => age_div_200 as u16,
                1 => selected,
                2 => pulse,
                3 => (age_div_200 as u16).wrapping_mul(100),
                6 => 1,
                _ => 0,
            },
            ModelCallbackPolicy::NameEntry { selector_one } => match selector {
                0 => self.animation.value() as u16,
                1 => selector_one,
                2 => 0xffff,
                6 => 1,
                _ => 0,
            },
            ModelCallbackPolicy::Panel {
                extent,
                unfold,
                scale,
                flag,
            } => match selector {
                0 => self.animation.value() as u16,
                1 => extent,
                2 => unfold,
                3 => scale,
                4 => u16::from(flag),
                _ => 0,
            },
        })
    }
}

/// Concrete ordered operations. A backend must implement every operation it
/// accepts; recording these in tests does not establish rendered pixels.
#[derive(Clone, Copy, Debug)]
pub enum SceneCommand {
    /// Low byte of 0055F250; published before lighting/context setup.
    CoordinateShift(u8),
    DirectionalLight {
        direction: [f32; 3],
        rgb: [u8; 3],
    },
    /// 00490130 keeps the signed values (0055F258) and separately clamps each
    /// to a byte (0055F264).
    AmbientLight([i32; 3]),
    Setup(SceneSetup),
    /// Publishes only 0055F244/48/4C; focal, view and retained context survive.
    PublishFade(SceneFade),
    /// Context+6C, already converted to the display's material DWORD. This
    /// becomes the Far primitive packet color, distinct from0055F250.
    FarColor(u32),
    BeginBatch,
    FlushBatch,
    /// Establishes the identity object basis at world origin against the
    /// inherited view (0048FE60), before any new camera is published.
    StarView(SceneView),
    /// Host resolves color pool 0055EE68, uses context virtual +4 to select
    /// 0x40000000, and submits the 00466170 quad command. It must preserve the
    /// native z/attribute/trailing words documented on StarQuad. A negative or
    /// unavailable color_slot must return an error, never a fabricated color.
    Star(StarQuad),
    PublishView(SceneView),
    /// 0049FEA0: requires the actual Handler8 model interpreter/backend.
    Model(SceneModelSubmission),
    /// Renderer virtual +14(0), immediately after the first model's flush.
    ResetAfterFirstModel,
    /// Renderer virtual +A4, with the decoded sprite resource and descriptor.
    Footer(FooterQuad),
}

pub trait FrontendSceneHost {
    type Error;

    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error>;
    /// Resource 14 at pool 0055EEC8, fetched before any footer sprite lookup.
    fn footer_style_word(&mut self) -> Result<u32, Self::Error>;
    /// 004843F7/00484400 read live dimensions after the footer style lookup.
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error>;
    /// Unsigned words at sprite +14/+16 from pool 0055EDA8.
    fn sprite_dimensions(&mut self, slot: u16) -> Result<[u16; 2], Self::Error>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneDrawOutcome {
    pub generated_stars: u32,
    pub submitted_stars: u32,
    pub submitted_models: usize,
    /// The host also receives publication at the native point, so an error
    /// after publication must not roll that renderer state back.
    pub last_view: Option<SceneView>,
}

#[derive(Clone, Debug)]
pub struct FrontendScene {
    models: Vec<FrontendSceneModel>,
    position_scale: i32,
    math: MenuBasisMath,
}

impl FrontendScene {
    pub fn new(
        models: Vec<FrontendSceneModel>,
        position_scale: i32,
        sine_table: [u16; 4096],
    ) -> Self {
        Self {
            models,
            position_scale,
            math: MenuBasisMath::new(sine_table),
        }
    }

    /// 00483C60 uses the renderer captured at its entry. Its final BeginBatch
    /// belongs to this renderer, even if the outer frame captured a different
    /// renderer for the following footer. The caller owns footer and flush.
    pub fn draw_scene<H: FrontendSceneHost>(
        &self,
        frame: SceneFrame,
        host: &mut H,
    ) -> Result<SceneDrawOutcome, H::Error> {
        let setup = SceneSetup::new(frame.projection);
        host.submit(SceneCommand::CoordinateShift(0))?;
        host.submit(SceneCommand::DirectionalLight {
            direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
            rgb: [255, 210, 170],
        })?;
        host.submit(SceneCommand::AmbientLight([10, 6, 5]))?;
        host.submit(SceneCommand::Setup(setup))?;
        host.submit(SceneCommand::BeginBatch)?;
        host.submit(SceneCommand::StarView(frame.inherited_view))?;
        let mut outcome = self.draw_stars(frame, setup, host)?;
        host.submit(SceneCommand::FlushBatch)?;

        for (index, model) in self.models.iter().enumerate() {
            // The direction always uses the screen camera, including model 0,
            // whose view origin alone is replaced with the fixed backdrop eye.
            let view = SceneView {
                eye: if index == 0 {
                    [0, 0, -70000]
                } else {
                    frame.camera_eye
                },
                basis: camera_basis(frame.camera_eye, frame.camera_look_at),
            };
            host.submit(SceneCommand::PublishView(view))?;
            outcome.last_view = Some(view);
            let submission = self.model_submission(model, frame.animation);
            host.submit(SceneCommand::BeginBatch)?;
            host.submit(SceneCommand::Model(submission))?;
            host.submit(SceneCommand::FlushBatch)?;
            outcome.submitted_models += 1;
            if index == 0 {
                host.submit(SceneCommand::ResetAfterFirstModel)?;
            }
        }
        host.submit(SceneCommand::BeginBatch)?;
        Ok(outcome)
    }

    fn model_submission(
        &self,
        model: &FrontendSceneModel,
        animation: AnimationClock,
    ) -> SceneModelSubmission {
        // 0048F5D0/48F630/0048F690 initialize the three copied global unit axes.
        let mut basis = MenuBasis::INITIAL;
        let planes = [RotationPlane::Yz, RotationPlane::Xz, RotationPlane::Xy];
        // Native order is ALL positive controls, then ALL negative controls;
        // the latter are clock shifts, not negative fixed angles.
        for animated in [false, true] {
            for index in (0..3).rev() {
                let control = model.rotation_controls[index];
                if (!animated && control > 0) || (animated && control < 0) {
                    let angle = if animated {
                        animation
                            .value()
                            .wrapping_shl(control.wrapping_neg() as u32)
                            as i32
                    } else {
                        control
                    };
                    self.math.rotate_in_plane(&mut basis, angle, planes[index]);
                }
            }
        }
        SceneModelSubmission {
            model_slot: model.model_slot,
            position: model
                .position
                .map(|value| i32::from(value).wrapping_mul(self.position_scale)),
            basis,
            animation,
            primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
            failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
            surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
            policy: ModelCallbackPolicy::Frontend,
        }
    }

    fn draw_stars<H: FrontendSceneHost>(
        &self,
        frame: SceneFrame,
        setup: SceneSetup,
        host: &mut H,
    ) -> Result<SceneDrawOutcome, H::Error> {
        let mut outcome = SceneDrawOutcome::default();
        let mut seed = 0u32;
        let translation = star_translation(frame.inherited_view);
        for _ in 0..self.position_scale.wrapping_mul(1000) {
            let point = std::array::from_fn(|_| {
                seed = seed.wrapping_mul(0x13d56d).wrapping_add(0x13d573);
                (seed as i32 >> 13).wrapping_mul(self.position_scale) / 10
            });
            outcome.generated_stars += 1;
            let transformed = star_transform(point, frame.inherited_view, translation);
            if let Some([x, y]) = project_star(transformed, setup) {
                let mut color = (seed as i32).wrapping_abs() % 12 + 37;
                let fade = if frame.age_millis < 2000 {
                    Some(2000i32.wrapping_sub(frame.age_millis))
                } else if frame.exit_millis != 0 {
                    Some(frame.exit_millis)
                } else {
                    None
                };
                if let Some(fade) = fade {
                    color = color.wrapping_sub(fade / 100).max(37);
                }
                host.submit(SceneCommand::Star(StarQuad {
                    corners: [
                        [x, y],
                        [x.wrapping_add(1), y],
                        [x.wrapping_add(1), y.wrapping_add(1)],
                        [x, y.wrapping_add(1)],
                    ],
                    color_argument: color,
                }))?;
                outcome.submitted_stars += 1;
            }
        }
        Ok(outcome)
    }
}

/// 004843D0 uses the outer frame's previously captured renderer. It neither
/// begins nor flushes a batch; 004849CE obtains a fresh renderer for the flush.
pub fn draw_footer<H: FrontendSceneHost>(host: &mut H) -> Result<(), H::Error> {
    let style = host.footer_style_word()?;
    let [width, height] = host.surface_pixels()?;
    let margin = width / 40;
    let bottom = height.wrapping_sub(height / 40);
    let [w, h] = scale_sprite(host.sprite_dimensions(1807)?, [width, height], [640, 480]);
    footer_quad(host, 1807, [margin, bottom.wrapping_sub(h)], [w, h], style)?;
    let [w, h] = scale_sprite(host.sprite_dimensions(1806)?, [width, height], [640, 480]);
    footer_quad(
        host,
        1806,
        [
            width.wrapping_sub(margin).wrapping_sub(w),
            bottom.wrapping_sub(h),
        ],
        [w, h],
        style,
    )?;

    let size = footer_strip_size(host.sprite_dimensions(1809)?, [width, height]);
    let center = width / 2;
    let top = bottom.wrapping_sub(size[1]);
    footer_quad(host, 1809, [center.wrapping_sub(size[0]), top], size, style)?;
    // 4846E5 subtracts the PREVIOUS sprite's width again. It does not center
    // the group by summing each asset's width, even when dimensions differ.
    let left = center.wrapping_sub(size[0].wrapping_mul(2));
    let size = footer_strip_size(host.sprite_dimensions(1808)?, [width, height]);
    footer_quad(host, 1808, [left, top], size, style)?;
    let size = footer_strip_size(host.sprite_dimensions(1810)?, [width, height]);
    footer_quad(host, 1810, [center, top], size, style)?;
    let right = center.wrapping_add(size[0]);
    let size = footer_strip_size(host.sprite_dimensions(1811)?, [width, height]);
    footer_quad(host, 1811, [right, top], size, style)
}

fn footer_strip_size(size: [u16; 2], viewport: [i32; 2]) -> [i32; 2] {
    if viewport[0] < 800 {
        scale_sprite(size, viewport, [800, 600])
    } else {
        size.map(i32::from)
    }
}

fn scale_sprite(size: [u16; 2], viewport: [i32; 2], reference: [i32; 2]) -> [i32; 2] {
    std::array::from_fn(|axis| i32::from(size[axis]).wrapping_mul(viewport[axis]) / reference[axis])
}

fn footer_quad<H: FrontendSceneHost>(
    host: &mut H,
    slot: u16,
    [x, y]: [i32; 2],
    [w, h]: [i32; 2],
    style: u32,
) -> Result<(), H::Error> {
    let [left, top, right, bottom] = [x, y, x.wrapping_add(w), y.wrapping_add(h)].map(|v| v as i16);
    host.submit(SceneCommand::Footer(FooterQuad {
        sprite_slot: slot,
        corners: [[left, top], [right, top], [right, bottom], [left, bottom]],
        style_word: style,
        vertex_attribute: 0x40,
    }))
}

/// 00490370(false), after the caller's integer camera-direction normalization.
/// Shared with the world spawn view; the world camera math is the follow-up.
pub(crate) fn camera_basis(eye: [i32; 3], look: [i32; 3]) -> MenuBasis {
    let difference: [i32; 3] = std::array::from_fn(|i| look[i].wrapping_sub(eye[i]));
    let x = f64::from(difference[0]);
    let y = f64::from(difference[1] as f32);
    let z = f64::from(difference[2]);
    let spilled_z = f64::from(difference[2] as f32);
    let length = ((z * spilled_z + y * y) + x * x).sqrt() as f32;
    let forward = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [x, y, spilled_z].map(|v| (v / f64::from(length)) as f32)
    };
    let right = cross([0.0, 1.0, 0.0], forward);
    let quantized = right.map(|v| integer_low32((f64::from(v) * 4096.0).round_ties_even()) as i16);
    let [x, y, z] = quantized.map(f64::from);
    let length = ((z * z + y * y) + x * x).sqrt() as f32;
    let right = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [x, y, z].map(|v| (v / f64::from(length)) as f32)
    };
    let up = cross(forward, right);
    let mut basis = MenuBasis::INITIAL;
    for (index, value) in right.into_iter().chain(up).chain(forward).enumerate() {
        basis.words[index + 1] = value.to_bits();
    }
    basis
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let [ax, ay, az] = a.map(f64::from);
    let [bx, by, bz] = b.map(f64::from);
    [
        (ay * bz - by * az) as f32,
        (az * bx - ax * bz) as f32,
        (by * ax - ay * bx) as f32,
    ]
}

/// 48FE60 FISTP uses nearest-even and different addition orders per output.
fn star_translation(view: SceneView) -> [i32; 3] {
    let [x, y, z] = view.eye.map(|v| f64::from(v.wrapping_neg()));
    let vectors: [[f64; 3]; 3] = std::array::from_fn(|v| {
        std::array::from_fn(|i| f64::from(f32::from_bits(view.basis.words[1 + v * 3 + i])))
    });
    let [a, b, c] = vectors;
    [
        (z * a[2] + y * a[1]) + x * a[0],
        (z * b[2] + x * b[0]) + y * b[1],
        (x * c[0] + z * c[2]) + y * c[1],
    ]
    .map(|v| integer_low32(v.round_ties_even()))
}

fn star_transform(point: [i32; 3], view: SceneView, translation: [i32; 3]) -> [i32; 3] {
    let [x, y, z] = point.map(f64::from);
    std::array::from_fn(|axis| {
        let v: [f64; 3] =
            std::array::from_fn(|i| f64::from(f32::from_bits(view.basis.words[1 + axis * 3 + i])));
        integer_low32(((z * v[2] + y * v[1]) + x * v[0]).trunc()).wrapping_add(translation[axis])
    })
}

/// 00485350's projection, coordinate reduction and unsigned viewport tests.
/// Its fog byte is dead for 00483C60's star command and is not submitted.
fn project_star([x, y, z]: [i32; 3], setup: SceneSetup) -> Option<[i16; 2]> {
    if z < 64 {
        return None;
    }
    let mut sx = integer_low32((f64::from(setup.focal[0]) * f64::from(x) / f64::from(z)).trunc())
        .wrapping_add(setup.center[0]);
    let mut sy = setup.center[1].wrapping_sub(integer_low32(
        (f64::from(setup.focal[1]) * f64::from(y) / f64::from(z)).trunc(),
    ));
    let mut magnitude = sx.wrapping_abs() | sy.wrapping_abs();
    while magnitude >= 32767 {
        magnitude >>= 1;
        sx >>= 1;
        sy >>= 1;
    }
    if (sx as u32) < setup.viewport[0] as u32 && (sy as u32) < setup.viewport[1] as u32 {
        Some([sx as i16, sy as i16])
    } else {
        None
    }
}

/// Both __ftol and direct FISTP return the low DWORD of a signed-qword result;
/// masked invalid/overflow conversions return integer-indefinite (low32 zero).
fn integer_low32(integral: f64) -> i32 {
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
        integral as i64 as i32
    } else {
        0
    }
}

#[cfg(test)]
#[path = "frontend_scene_tests.rs"]
mod tests;
