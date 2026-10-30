//! The phase-7 map screen, the in-world computer's "Map and Objectives":
//! factory `00450FC0`, constructor `00451020`, vtable `005255C8`.
//!
//! Native lifecycle (see the phase-7 section of
//! [gameplay runtime](../../../docs/re/GAMEPLAY_RUNTIME.md)):
//!
//! - Constructor `00451020` builds the screen over base `00437F90` (which
//!   zeroes the age `+C0`), sets the zoom `+F8` to 1, clears the held bits
//!   `+EC`, installs the phase-7 tables and selects its context. The map's
//!   centre `+F0/+F2` starts at the first player's vehicle (`0046B030`
//!   resolves the player's `+D4`), its X and Z WORDs, or zero; the
//!   velocities `+F4/+F6` start at zero.
//! - Its callbacks take the press value in EDX: `004520A0` requests phase 4
//!   (`00438380(4)` on `+E4`) on a press; the six others set a held bit on a
//!   press and clear it on a release.
//! - Tick `00451F30` adds the raw elapsed microseconds to `+C0`, steers and
//!   damps the centre and steps the zoom; it runs no base input.
//! - Draw `00451250` draws the map in the right three quarters, a frame of
//!   four quadrants, the legend's three pulsing blips and the text.
//! - Destructor `004511D0` (`004511F0`) releases the binding sets
//!   (`00439D50`) and the base (`00438080`).
//!
//! The vtable's class query `004511B0` returns 7 and its reuse `004511C0`
//! accepts only phase 7. Its presentation is the shared `0040D1D0`.

use thiserror::Error;

use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::input::{InputCallback, InputContextId, InputRuntime, InputStateError};
use crate::scene_sprite::{DirectSpriteMode, DirectSpriteQuad};
use crate::scheduler::SchedulerTick;
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::session::{SessionPhase, SessionState};

pub(crate) mod software;

/// Vtable `005255C8` slot `+18` (`004511B0`).
pub const MAP_CLASS_ID: u8 = 7;
/// Opaque callback identity, distinct from every other screen context.
pub const MAP_CALLBACK_CONTEXT: u32 = 9;
/// `004520A0`: Esc, Space, Tab, Return, M and the joystick buttons.
pub const MAP_RETURN_CALLBACK: u32 = 0x0045_20a0;
/// The held callbacks and their `+EC` bits: Up and Down steer `+F6`, Left
/// and Right `+F4`, Insert zooms in and Delete out. The census starts
/// Insert's at `0045217C`, in the padding before its body.
pub const MAP_HELD_CALLBACKS: [(u32, u32); 6] = [
    (0x0045_20c0, 0x02),
    (0x0045_20f0, 0x04),
    (0x0045_2120, 0x08),
    (0x0045_2150, 0x10),
    (0x0045_2180, 0x40),
    (0x0045_21b0, 0x80),
];
/// `004F0140` readings beyond this push the centre (`00452025..0045208E`).
pub const MAP_ANALOG_THRESHOLD: i32 = 3000;

/// `004511C0`: the screen serves only requested phase 7.
pub fn map_reuses_phase(requested: SessionPhase) -> bool {
    requested == SessionPhase::Phase7
}

#[derive(Clone, Copy, Debug)]
pub struct MapScreenConfig {
    pub attachment: ScreenAttachment,
    pub callback_context: u32,
}

pub struct MapFactoryServices<'a, D> {
    pub registry: &'a mut ScreenRegistry<D, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub assets: &'a FrontendAssets,
}

pub trait MapFactoryHost {
    type Error;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
}

#[derive(Debug, Error)]
pub enum MapScreenError<E> {
    #[error("map screen requires {expected:?}, currently {actual:?}")]
    Lifecycle {
        expected: ScreenLifecycle,
        actual: ScreenLifecycle,
    },
    #[error("00450FC0 serves requested phase 7, not {0:?}")]
    RequestedPhase(SessionPhase),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Bindings(#[from] FrontendBindingError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error("map callback context {actual} does not match owner {expected}")]
    Context { expected: u32, actual: u32 },
    #[error("map input callback {0:#010x} has no owner")]
    UnknownCallback(u32),
    #[error("required map effect failed")]
    Host(E),
}

/// What a callback asks of its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapInputAction {
    /// `004520A0`'s press: `00438380(4)`, first-request-wins.
    RequestGameplay,
    None,
}

/// The screen's state, at its native offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapState {
    /// `+C0`: the raw elapsed microseconds the tick has added.
    pub age_c0: i32,
    /// `+EC`: the held bits.
    pub held_ec: u32,
    /// `+F0/+F2`: the map's centre, X and Z WORDs.
    pub center: [u16; 2],
    /// `+F4/+F6`: their velocities.
    pub velocity: [i16; 2],
    /// `+F8`: the zoom, odd from -1 to 129.
    pub zoom_f8: i32,
}

#[derive(Debug)]
pub struct MapScreen {
    config: MapScreenConfig,
    lifecycle: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    state: MapState,
    /// Port reporting: a failed draw has been reported.
    draw_failure_noted: bool,
}

impl MapScreen {
    /// Metadata only; `initialize` performs the ordered native acquisitions.
    pub fn new(config: MapScreenConfig, assets: &FrontendAssets) -> Self {
        Self {
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            config,
            lifecycle: ScreenLifecycle::New,
            id: None,
            state: MapState {
                age_c0: 0,
                held_ec: 0,
                center: [0; 2],
                velocity: [0; 2],
                zoom_f8: 0,
            },
            draw_failure_noted: false,
        }
    }

    pub fn lifecycle(&self) -> ScreenLifecycle {
        self.lifecycle
    }
    pub fn id(&self) -> Option<ScreenId> {
        self.id
    }
    pub fn callback_context(&self) -> u32 {
        self.config.callback_context
    }
    pub fn bindings(&self) -> &FrontendBindings {
        &self.bindings
    }
    pub fn state(&self) -> &MapState {
        &self.state
    }

    /// Port reporting only: true the first time a draw fails.
    pub fn note_draw_failure(&mut self) -> bool {
        !std::mem::replace(&mut self.draw_failure_noted, true)
    }

    /// Whether a draw has failed (and been reported).
    pub fn note_draw_failure_reported(&self) -> bool {
        self.draw_failure_noted
    }

    /// Constructor `00451020` in native order. `vehicle_words` are the first
    /// player's vehicle's X and Z WORDs (`+32/+36`), None without a player
    /// or with an unresolved `+D4`.
    pub fn initialize<D, H: MapFactoryHost>(
        &mut self,
        services: &mut MapFactoryServices<'_, D>,
        host: &mut H,
        state: &SessionState,
        vehicle_words: Option<[u16; 2]>,
    ) -> Result<(), MapScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::New)?;
        if !map_reuses_phase(state.requested_phase()) {
            return Err(MapScreenError::RequestedPhase(state.requested_phase()));
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        let id = services
            .registry
            .attach_scheduler_member(self.config.attachment)?;
        self.id = Some(id);
        self.bindings.initialize(services.input)?;
        services.registry.link_screen_membership(id)?;
        // 00451058..00451066.
        self.state.zoom_f8 = 1;
        self.state.held_ec = 0;
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(MapScreenError::Host)?;
        self.bindings.install(
            services.input,
            services.assets.phase7_input_tables(),
            &devices,
        )?;
        let context = self.bindings.context().expect("installed map context");
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => MapScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => MapScreenError::Input(error),
            })?;
        // 0045114B..00451183.
        self.state.center = vehicle_words.unwrap_or([0; 2]);
        self.state.velocity = [0; 2];
        self.lifecycle = ScreenLifecycle::Ready;
        Ok(())
    }

    /// Destructor `004511F0`: the binding sets (`00439D50`), then the base
    /// (`00438080`).
    pub fn destroy<D>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
    ) -> Result<(), MapScreenError<std::convert::Infallible>> {
        if self.lifecycle == ScreenLifecycle::Destroyed {
            return Ok(());
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        self.bindings.release_sets(input)?;
        if let Some(id) = self.id {
            if registry.screen(id)?.membership == ScreenMembership::Linked {
                registry.unlink_screen_membership(id)?;
            }
        }
        self.bindings.release_base(input)?;
        if let Some(id) = self.id {
            registry.remove_screen(id)?;
            self.id = None;
        }
        self.lifecycle = ScreenLifecycle::Destroyed;
        Ok(())
    }

    /// Tick `00451F30`.
    pub fn tick(
        &mut self,
        tick: SchedulerTick,
        input: &InputRuntime,
    ) -> Result<(), MapScreenError<std::convert::Infallible>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        let cc = self
            .bindings
            .consumer_cc()
            .ok_or(FrontendBindingError::MissingScreenConsumer(0xcc))?;
        let d0 = self
            .bindings
            .consumer_d0()
            .ok_or(FrontendBindingError::MissingScreenConsumer(0xd0))?;
        let state = &mut self.state;
        state.age_c0 = state.age_c0.wrapping_add(tick.elapsed_micros);
        let step = (tick.delta_micros >> 7) as i16;
        let held = state.held_ec;
        let [x, z] = &mut state.velocity;
        if held & 0x02 != 0 {
            *z = z.wrapping_add(step);
        }
        if held & 0x04 != 0 {
            *z = z.wrapping_sub(step);
        }
        if held & 0x08 != 0 {
            *x = x.wrapping_sub(step);
        }
        if held & 0x10 != 0 {
            *x = x.wrapping_add(step);
        }
        // 00451F95..00451FE5: move, then damp to 7/8 toward zero.
        for (center, velocity) in state.center.iter_mut().zip(state.velocity.iter_mut()) {
            *center = center.wrapping_add(*velocity as u16);
            *velocity = (i32::from(*velocity) * 7 / 8) as i16;
        }
        if held & 0x40 != 0 && state.zoom_f8 < 0x80 {
            state.zoom_f8 += 2;
        }
        if held & 0x80 != 0 && state.zoom_f8 > 0 {
            state.zoom_f8 -= 2;
        }
        // 0045201C..00452090: each consumer read twice, +CC for X, +D0 for Z.
        for (consumer, velocity) in [cc, d0].into_iter().zip(0..2) {
            if input.analog_value(consumer)? < -MAP_ANALOG_THRESHOLD {
                state.velocity[velocity] = state.velocity[velocity].wrapping_sub(step);
            }
            if input.analog_value(consumer)? > MAP_ANALOG_THRESHOLD {
                state.velocity[velocity] = state.velocity[velocity].wrapping_add(step);
            }
        }
        Ok(())
    }

    /// The screen's own callbacks; system chords belong to the router.
    pub fn digital_callback(
        &mut self,
        event: InputCallback,
    ) -> Result<MapInputAction, MapScreenError<std::convert::Infallible>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if event.context != self.config.callback_context {
            return Err(MapScreenError::Context {
                expected: self.config.callback_context,
                actual: event.context,
            });
        }
        if event.address == MAP_RETURN_CALLBACK {
            return Ok(if event.value != 0 {
                MapInputAction::RequestGameplay
            } else {
                MapInputAction::None
            });
        }
        let (_, bit) = MAP_HELD_CALLBACKS
            .into_iter()
            .find(|(address, _)| *address == event.address)
            .ok_or(MapScreenError::UnknownCallback(event.address))?;
        if event.value != 0 {
            self.state.held_ec |= bit;
        } else {
            self.state.held_ec &= !bit;
        }
        Ok(MapInputAction::None)
    }

    fn require_lifecycle<E>(&self, expected: ScreenLifecycle) -> Result<(), MapScreenError<E>> {
        if self.lifecycle == expected {
            Ok(())
        } else {
            Err(MapScreenError::Lifecycle {
                expected,
                actual: self.lifecycle,
            })
        }
    }
}

/// The world's part of `00471F70`: the four tiles and their `00472D30`
/// marker passes, about `center` with each tile `size` pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapTilesRequest {
    pub center: [i32; 2],
    pub size: [i32; 2],
    /// `+F0/+F2`.
    pub camera_words: [u16; 2],
    /// `+C0 / 1000`, the markers' scan age.
    pub scan_age: i32,
    /// `0046BDC0(0x13)`: the Resource scanner draws the pickup spawners.
    pub resource_scanner: bool,
}

/// The draw's services. Each method is the native call it names.
pub trait MapDrawHost {
    type Error;
    /// `00439270`, then `00495900`/`00495910`.
    fn prepare(&mut self) -> Result<[i32; 2], Self::Error>;
    /// `004A4400`.
    fn begin_batch(&mut self) -> Result<(), Self::Error>;
    /// Renderer `+24`.
    fn clip(&mut self, rect: [i16; 4]) -> Result<(), Self::Error>;
    /// `0046BDC0(0x13)` on `00488AA0`'s player.
    fn resource_scanner(&mut self) -> Result<bool, Self::Error>;
    /// `00471F70` with H6 `0x3B` and H3 `0x5B7`.
    fn map(&mut self, request: MapTilesRequest) -> Result<(), Self::Error>;
    /// Renderer `+9C` or `+A4`.
    fn quad(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error>;
    /// An H6 style's first DWORD.
    fn style(&mut self, slot: i32) -> Result<u32, Self::Error>;
    /// An H7 material's first DWORD, converted to the display.
    fn material(&mut self, slot: i32) -> Result<u32, Self::Error>;
    /// Display `+24/+28/+2C`.
    fn material_shifts(&mut self) -> [u32; 3];
    /// An H0 string.
    fn string(&mut self, id: i32) -> Result<Vec<u8>, Self::Error>;
    /// `004A4240` with H4 font 0, narrowed to AX.
    fn text_width(&mut self, text: &[u8]) -> Result<i16, Self::Error>;
    /// `004A3B20` with H4 font 0, revealed from `age_millis` (descriptor
    /// flags 2), publishing its narrowed end position.
    fn raw_text(
        &mut self,
        text: &[u8],
        position: &mut [i32; 2],
        width: i32,
        color_word: u32,
        age_millis: i32,
    ) -> Result<(), Self::Error>;
    /// `004A3AB0` with H4 font 0.
    fn line_text(
        &mut self,
        text: &[u8],
        position: [i32; 2],
        color_word: u32,
    ) -> Result<(), Self::Error>;
}

/// H3 `0x29E`, the frame's quadrant sprite.
pub const MAP_FRAME_SPRITE: u16 = 0x29e;
/// H0 "Map of" and the world's name, "%s %s" (`0052FE2C`).
pub const MAP_TITLE_TEXTS: [i32; 2] = [0x11a, 0x36a];
/// H0 `0x380..0x383`: the world's four objectives.
pub const MAP_OBJECTIVE_TEXTS: std::ops::Range<i32> = 0x380..0x384;
/// The legend: "Key building", "Allied unit" and "Enemy unit", each with
/// the H7 colour of its blip and the H6 style of its text, at its
/// hundredths of the height (the blip's, then the text's).
pub const MAP_LEGEND: [MapLegendRow; 3] = [
    MapLegendRow {
        text: 0x11c,
        material: 0x59,
        style: 0x0e,
        blip_y: 84,
        text_y: 85,
    },
    MapLegendRow {
        text: 0x11d,
        material: 0x56,
        style: 0x2a,
        blip_y: 87,
        text_y: 88,
    },
    MapLegendRow {
        text: 0x11e,
        material: 0x57,
        style: 0x28,
        blip_y: 90,
        text_y: 91,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapLegendRow {
    pub text: i32,
    pub material: i32,
    pub style: i32,
    pub blip_y: i32,
    pub text_y: i32,
}

/// `004515D1..00451613`: the pulse, -256..=0, from the age in tens of
/// microseconds, as a signed WORD's 256ths.
pub fn map_pulse(age_c0: i32) -> i32 {
    -((age_c0 / 10) & 0xffff) >> 8
}

/// `00451633..00451712`: a legend blip's colour. Each low byte of the H6
/// word is scaled by the pulse and then by the material's display channel
/// (its top five bits, as a byte), both in signed 256ths; the high byte is
/// kept. A negative pulse wraps the bytes.
pub fn legend_blip_color(style: u32, material: u32, shifts: [u32; 3], pulse: i32) -> u32 {
    let channels = [
        (material >> (shifts[0].wrapping_add(1) & 31)) & 0xf8,
        (material >> (shifts[1].wrapping_add(1) & 31)) & 0xf8,
        (u32::from(material as u8) << (shifts[2].wrapping_sub(1) & 31)) & 0xf8,
    ];
    let mut bytes = style.to_le_bytes();
    for (byte, channel) in bytes.iter_mut().zip(channels) {
        let scaled = i32::from(*byte).wrapping_mul(pulse) / 256;
        *byte = (scaled.wrapping_mul(channel as i32) / 256) as u8;
    }
    u32::from_le_bytes(bytes)
}

/// The fixed positions of `00451250` on a `width` by `height` surface,
/// in its signed truncating arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapLayout {
    /// `[esp+1C]`: three eighths of the width, the text column.
    pub column: i32,
    /// The map's centre: 60% of the width, half the height.
    pub center: [i32; 2],
    /// `004512FF..00451315`: the right three quarters.
    pub map_clip: [i16; 4],
    pub full_clip: [i16; 4],
    /// The tiles: the zoom plus 0x80 times the column's and 11/24 of the
    /// height's doubled 128ths.
    pub tile: [i32; 2],
    /// The frame's half extents: four thirds of the column and of 11/24
    /// of the height.
    pub frame: [i32; 2],
}

pub fn map_layout([width, height]: [i32; 2], zoom_f8: i32) -> MapLayout {
    let column = width.wrapping_mul(3) / 8;
    let rows = height.wrapping_mul(11) / 24;
    let zoom = zoom_f8.wrapping_add(0x80);
    let tile = |extent: i32| zoom.wrapping_mul(extent).wrapping_shl(1) / 128;
    MapLayout {
        column,
        center: [width.wrapping_mul(6) / 10, height.wrapping_mul(5) / 10],
        map_clip: [
            width.wrapping_sub(column.wrapping_mul(2)) as i16,
            0,
            width.wrapping_sub(1) as i16,
            height.wrapping_sub(1) as i16,
        ],
        full_clip: [
            0,
            0,
            width.wrapping_sub(1) as i16,
            height.wrapping_sub(1) as i16,
        ],
        tile: [tile(column), tile(rows)],
        frame: [column.wrapping_mul(4) / 3, rows.wrapping_mul(4) / 3],
    }
}

/// `0045141C..004515CB`: four `+9C` quads of H3 `0x29E` at depth `0x6000`,
/// the top left, top right, bottom right and bottom left quadrants about
/// the centre. Each lists the centre last and its outer corner second.
pub fn map_frame_quads(center: [i32; 2], frame: [i32; 2]) -> [DirectSpriteQuad; 4] {
    let [x, y] = center;
    let [a, b] = frame;
    let quad = |corners: [[i32; 2]; 4]| DirectSpriteQuad {
        sprite_slot: MAP_FRAME_SPRITE,
        corners: corners.map(|corner| corner.map(|value| value as i16)),
        vertex_depths: [0x6000; 4],
        descriptor_word: 0,
        mode: DirectSpriteMode::Textured,
    };
    let (left, right, top, bottom) = (
        x.wrapping_sub(a),
        x.wrapping_add(a),
        y.wrapping_sub(b),
        y.wrapping_add(b),
    );
    [
        quad([[left, y], [left, top], [x, top], [x, y]]),
        quad([[x, top], [right, top], [right, y], [x, y]]),
        quad([[right, y], [right, bottom], [x, bottom], [x, y]]),
        quad([[x, bottom], [left, bottom], [left, y], [x, y]]),
    ]
}

/// Draw `00451250` in native order. `mission_seconds` is the mission clock
/// (`G+1E4`).
pub fn draw_map_screen<H: MapDrawHost>(
    state: &MapState,
    mission_seconds: u32,
    host: &mut H,
) -> Result<(), H::Error> {
    let surface = host.prepare()?;
    let [width, height] = surface;
    host.begin_batch()?;
    let layout = map_layout(surface, state.zoom_f8);
    host.clip(layout.map_clip)?;
    let resource_scanner = host.resource_scanner()?;
    host.map(MapTilesRequest {
        center: layout.center,
        size: layout.tile,
        camera_words: state.center,
        scan_age: state.age_c0 / 1000,
        resource_scanner,
    })?;
    host.clip(layout.full_clip)?;
    for quad in map_frame_quads(layout.center, layout.frame) {
        host.quad(quad)?;
    }
    // 004515D1..00451A99: the legend's blips, pulsing together.
    let pulse = map_pulse(state.age_c0);
    let radius = height.wrapping_mul(pulse) / 0x4000;
    let blip_x = width.wrapping_mul(52) / 100;
    for row in MAP_LEGEND {
        let material = host.material(row.material)?;
        let style = host.style(0x0e)?;
        let shifts = host.material_shifts();
        host.quad(crate::briefing_markers::blip(
            [blip_x, height.wrapping_mul(row.blip_y) / 100],
            radius,
            legend_blip_color(style, material, shifts, pulse),
        ))?;
    }
    // 00451AB2..00451BEE: "Map of" and the world's name, centred over the
    // whole width, 5% down, in style 5.
    let age_millis = state.age_c0 / 1000;
    let color = host.style(5)?;
    let name = host.string(MAP_TITLE_TEXTS[1])?;
    let map_of = host.string(MAP_TITLE_TEXTS[0])?;
    let mut title = terminated(&map_of).to_vec();
    title.push(b' ');
    title.extend_from_slice(terminated(&name));
    let measured = i32::from(host.text_width(&title)?);
    let top = height.wrapping_mul(5) / 100;
    let mut position = [width.wrapping_sub(measured) / 2, top];
    host.raw_text(&title, &mut position, width, color, age_millis)?;
    // 00451BF3..00451CA2: the objectives down the text column, each 3/32 of
    // the height below where the last one ended.
    let mut y = height / 8;
    for id in MAP_OBJECTIVE_TEXTS {
        let objective = host.string(id)?;
        let mut position = [width / 32, y];
        host.raw_text(
            terminated(&objective),
            &mut position,
            layout.column,
            color,
            age_millis,
        )?;
        y = position[1].wrapping_add(height.wrapping_mul(3) / 32);
    }
    // 00451CA4..00451D34: the mission time.
    let seconds = mission_seconds as i32;
    let time = format!(
        "{:02}:{:02}:{:02}",
        (seconds / 3600) % 100,
        (seconds / 60) % 60,
        seconds % 60
    );
    host.line_text(time.as_bytes(), [width.wrapping_mul(50) / 64, top], color)?;
    // 00451D39..00451F11: the legend's text.
    let text_x = width.wrapping_mul(55) / 100;
    for row in MAP_LEGEND {
        let text = host.string(row.text)?;
        let color = host.style(row.style)?;
        host.line_text(
            terminated(&text),
            [text_x, height.wrapping_mul(row.text_y) / 100],
            color,
        )?;
    }
    Ok(())
}

fn terminated(text: &[u8]) -> &[u8] {
    &text[..text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len())]
}

#[cfg(test)]
#[path = "map_screen_tests.rs"]
mod tests;
