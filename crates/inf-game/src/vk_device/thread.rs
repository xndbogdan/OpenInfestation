//! The Vulkan device on a thread of its own. The main thread records each
//! frame's calls ([`VkFrameHandle`]) in chunks, which the device's thread
//! replays on its [`VkFrame`] as they arrive: the device's recording and the
//! driver's work run beside the game and the next frame's draw. Nothing the
//! game reads comes back from the device: a frame's answers
//! ([`VkFrameHandle::shades_water`] and the like) follow from the settings
//! and the frame's own calls, and only reads (snapshots, captures) wait for
//! the device's thread. At most one frame waits there behind the one the
//! main thread records.

use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
    sync::{mpsc, Arc, Condvar, Mutex, PoisonError},
    thread::JoinHandle,
};

use sdl2::video::Window;

use super::{raster_aa, DeviceStaged, Murk, VkDevice, VkDeviceError, VkFrame};
use crate::{
    crystal_flares::CrystalSpark,
    d3d_texture::CpuTexture,
    d3d_textured::{D3dTextureBlend, D3dTextureFog},
    d3d_viewport::D3dTlVertex,
    fast_hash::FastMap,
    frame_device::{DeviceDraw, DeviceTexturing},
    modern_settings::{ModernSettings, ModernSupport},
    presentation::PresentationRect,
    raster_metal::MetalFace,
    raster_motion::ViewMotion,
    scene_feed::{SceneCamera, SceneFeed},
    software_frame::D3dTextureSampling,
};

/// Calls a chunk holds before it goes to the device's thread.
const CHUNK_CALLS: usize = 512;

/// Presents the device's thread keeps a texture no frame drew: twice the
/// device's own retirement, after which a frame drawing it sends it again.
const TEXTURE_FORGET_PRESENTS: u64 = 2 * super::TEXTURE_IDLE_PRESENTS;

#[derive(Clone, Copy)]
struct Texturing {
    key: u64,
    sampling: D3dTextureSampling,
    perspective: bool,
    fog: D3dTextureFog,
}

#[derive(Clone, Copy)]
struct Draw {
    blend: D3dTextureBlend,
    depth_test: bool,
    depth_write: bool,
    texture: Option<Texturing>,
}

struct RasterBegin {
    camera: SceneCamera,
    water_reflect: Option<f32>,
    murk: Option<Murk>,
    light: Option<SceneFeed>,
    sky: bool,
}

/// One [`VkFrame`] call, its fan's corners in the chunk's vertices.
enum FrameCall {
    Fan { start: u32, len: u32, draw: Draw },
    ClearColor([u8; 4]),
    ClearDepth,
    ReplaceSurface(Vec<u8>),
    FinishTracedWorld,
    FinishThermal(bool),
    BeginRasterWorld(Box<RasterBegin>),
    RasterLamps(Box<SceneFeed>),
    SetDrawMetal(Option<Arc<MetalFace>>),
    DrawRasterSky,
    SetDrawBand(bool),
    SetDrawMotion(Option<Box<ViewMotion>>),
    SetDrawWater(bool),
    SetDrawGround(Option<[u8; 3]>),
    SetDrawCovered(bool),
    SetDrawTint(Option<Box<CrystalSpark>>),
    SetWorldCamera(Option<Box<SceneCamera>>),
    FinishRasterWorld,
    SetTraceInfrared(bool),
    PrepareTrace,
    TraceWorld(Box<SceneFeed>, Vec<u64>),
    ReadRgba(mpsc::Sender<Vec<u8>>),
    StageRgba,
    Present([u32; 2], PresentationRect),
}

enum Call {
    Begin {
        frame: u64,
        native: [u32; 2],
        presented: [u32; 2],
        clear: [u8; 4],
    },
    Frame(u64, FrameCall),
    End(u64),
    /// A texture frames draw from now on, by its content key.
    Texture(u64, Box<CpuTexture>),
    Forget(Vec<u64>),
    SetFiltering(u32),
    SetModern(ModernSettings),
    ReadOutput([u32; 2], mpsc::Sender<Vec<u8>>),
    ReadStaged(mpsc::Sender<Vec<u8>>),
    /// Answered once everything sent before is done.
    Settle(mpsc::Sender<Vec<u8>>),
    Swap([u32; 2]),
}

struct Chunk {
    calls: Vec<Call>,
    vertices: Vec<D3dTlVertex>,
}

impl Chunk {
    fn new() -> Self {
        Self {
            calls: Vec::with_capacity(CHUNK_CALLS),
            vertices: Vec::with_capacity(CHUNK_CALLS * 6),
        }
    }
}

/// The swaps the device's thread has done; `u64::MAX` once it has stopped.
struct Swaps {
    done: Mutex<u64>,
    changed: Condvar,
}

/// The device, made on the main thread, on its way to its own.
struct Moved(Rc<VkDevice>);

// SAFETY: the one `Rc` moves whole: no clone stays behind, and from then on
// only the device's thread touches the device.
unsafe impl Send for Moved {}

impl Moved {
    fn into_inner(self) -> Rc<VkDevice> {
        self.0
    }
}

/// Marks the device's thread stopped when it ends, even by a panic, so a
/// main thread waiting at a swap goes on.
struct Stopped<'a>(&'a Swaps);

impl Drop for Stopped<'_> {
    fn drop(&mut self) {
        *self.0.done.lock().unwrap_or_else(PoisonError::into_inner) = u64::MAX;
        self.0.changed.notify_all();
    }
}

/// The per-fan states the device's thread was last told, in [`VkFrame`]'s
/// state when it is made; only changes are sent.
#[derive(Default)]
struct DrawState {
    motion: Option<ViewMotion>,
    tint: Option<CrystalSpark>,
    covered: bool,
    metal: Option<Arc<MetalFace>>,
    water: bool,
    ground: Option<[u8; 3]>,
    band: bool,
}

/// A window's Vulkan device on its own thread, as the main thread holds it.
pub struct VkHandle {
    description: String,
    support: ModernSupport,
    ray_tracing: bool,
    scalar_layout: bool,
    /// The modern mode as the device runs it.
    modern: Cell<ModernSettings>,
    chunk: RefCell<Chunk>,
    sender: RefCell<Option<mpsc::Sender<Chunk>>>,
    thread: RefCell<Option<JoinHandle<()>>>,
    swaps: Arc<Swaps>,
    swaps_sent: Cell<u64>,
    frames: Cell<u64>,
    presents: Cell<u64>,
    /// The textures the device's thread holds, each with the present a
    /// frame last drew it in.
    textures: RefCell<FastMap<u64, u64>>,
}

impl fmt::Debug for VkHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VkHandle")
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

impl VkHandle {
    /// The device for `window`, started on its own thread.
    pub fn new(window: &Window, hidden: bool) -> Result<Rc<Self>, VkDeviceError> {
        let device = VkDevice::new(window, hidden)?;
        let description = device.description().to_owned();
        let support = device.modern_support();
        let ray_tracing = device.ray_tracing();
        let scalar_layout = device.core.scalar_layout;
        let modern = device.state.borrow().modern;
        let swaps = Arc::new(Swaps {
            done: Mutex::new(0),
            changed: Condvar::new(),
        });
        let (sender, receiver) = mpsc::channel();
        let moved = Moved(device);
        let thread = std::thread::Builder::new()
            .name("vulkan".into())
            .spawn({
                let swaps = Arc::clone(&swaps);
                move || run(moved.into_inner(), &receiver, &swaps)
            })
            .map_err(|error| VkDeviceError::Thread(error.to_string()))?;
        Ok(Rc::new(Self {
            description,
            support,
            ray_tracing,
            scalar_layout,
            modern: Cell::new(modern),
            chunk: RefCell::new(Chunk::new()),
            sender: RefCell::new(Some(sender)),
            thread: RefCell::new(Some(thread)),
            swaps,
            swaps_sent: Cell::new(0),
            frames: Cell::new(0),
            presents: Cell::new(0),
            textures: RefCell::new(FastMap::default()),
        }))
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// Whether the device traces rays.
    pub fn ray_tracing(&self) -> bool {
        self.ray_tracing
    }

    pub fn modern_support(&self) -> ModernSupport {
        self.support
    }

    pub fn set_filtering(&self, level: u32) {
        self.push(Call::SetFiltering(level));
    }

    /// The player's modern mode, from the next frame on, as the device
    /// runs it.
    pub fn set_modern(&self, modern: ModernSettings) {
        self.modern.set(modern.effective(self.support));
        self.push(Call::SetModern(modern));
    }

    /// The presented output's pixels, as [`VkDevice::read_output`] reads
    /// them once the device's thread has done everything sent before.
    pub(crate) fn read_output(&self, output: [u32; 2]) -> Vec<u8> {
        self.request(|reply| Call::ReadOutput(output, reply))
            .unwrap_or_else(|| vec![0; output[0] as usize * output[1] as usize * 4])
    }

    /// Wait until the device's thread has done everything sent so far,
    /// its last swap with it: before the window changes under it.
    pub(crate) fn settle(&self) {
        self.request(Call::Settle);
    }

    pub(crate) fn drawable_size(window: &Window) -> [u32; 2] {
        VkDevice::drawable_size(window)
    }

    /// Show the presented frame in `window`. The main thread goes on while
    /// the device's thread swaps, up to the next frame's swap.
    pub(crate) fn swap(&self, window: &Window) {
        let _wait = crate::cpu_profile::scope("swap (the device's thread)");
        self.push(Call::Swap(VkDevice::drawable_size(window)));
        self.flush();
        let sent = self.swaps_sent.get() + 1;
        self.swaps_sent.set(sent);
        let mut done = self
            .swaps
            .done
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while done.saturating_add(1) < sent {
            done = self
                .swaps
                .changed
                .wait(done)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn push(&self, call: Call) {
        let mut chunk = self.chunk.borrow_mut();
        chunk.calls.push(call);
        if chunk.calls.len() >= CHUNK_CALLS {
            let full = std::mem::replace(&mut *chunk, Chunk::new());
            drop(chunk);
            self.send(full);
        }
    }

    fn push_fan(&self, frame: u64, polygon: &[D3dTlVertex], draw: Draw) {
        let mut chunk = self.chunk.borrow_mut();
        let start = chunk.vertices.len() as u32;
        chunk.vertices.extend_from_slice(polygon);
        chunk.calls.push(Call::Frame(
            frame,
            FrameCall::Fan {
                start,
                len: polygon.len() as u32,
                draw,
            },
        ));
        if chunk.calls.len() >= CHUNK_CALLS {
            let full = std::mem::replace(&mut *chunk, Chunk::new());
            drop(chunk);
            self.send(full);
        }
    }

    fn flush(&self) {
        let chunk = std::mem::replace(&mut *self.chunk.borrow_mut(), Chunk::new());
        if !chunk.calls.is_empty() {
            self.send(chunk);
        }
    }

    fn send(&self, chunk: Chunk) {
        if let Some(sender) = &*self.sender.borrow() {
            // A stopped thread logged why; the frames draw nothing more.
            let _ = sender.send(chunk);
        }
    }

    /// Pixels the device's thread answers `call` with, once it has done
    /// everything sent before; none when it has stopped.
    fn request(&self, call: impl FnOnce(mpsc::Sender<Vec<u8>>) -> Call) -> Option<Vec<u8>> {
        let (reply, answer) = mpsc::channel();
        self.push(call(reply));
        self.flush();
        answer.recv().ok()
    }

    /// `texture`'s content key, sent with the texture to the device's
    /// thread the first time a frame draws it.
    fn send_texture(&self, texture: &CpuTexture) -> u64 {
        let key = texture.content_key();
        let present = self.presents.get();
        if self.textures.borrow_mut().insert(key, present).is_none() {
            self.push(Call::Texture(key, Box::new(texture.clone())));
        }
        key
    }

    /// A present is done: now and then the textures no frame drew for a
    /// while are forgotten on both sides.
    fn presented(&self) {
        let present = self.presents.get() + 1;
        self.presents.set(present);
        if present % 64 != 0 {
            return;
        }
        let mut textures = self.textures.borrow_mut();
        let idle: Vec<u64> = textures
            .iter()
            .filter(|(_, last)| **last + TEXTURE_FORGET_PRESENTS < present)
            .map(|(key, _)| *key)
            .collect();
        if idle.is_empty() {
            return;
        }
        for key in &idle {
            textures.remove(key);
        }
        drop(textures);
        self.push(Call::Forget(idle));
    }
}

impl Drop for VkHandle {
    fn drop(&mut self) {
        // The main thread's frame table, before the device's thread reports
        // its own.
        crate::cpu_profile::report();
        self.flush();
        self.sender.get_mut().take();
        if let Some(thread) = self.thread.get_mut().take() {
            if thread.join().is_err() {
                eprintln!("Infestation: the Vulkan device's thread stopped");
            }
        }
    }
}

/// The device's thread: replay each chunk's calls until the main thread
/// lets go of the device.
fn run(device: Rc<VkDevice>, receiver: &mpsc::Receiver<Chunk>, swaps: &Swaps) {
    let _stopped = Stopped(swaps);
    let mut frames: FastMap<u64, VkFrame> = FastMap::default();
    let mut textures: FastMap<u64, CpuTexture> = FastMap::default();
    let mut staged: Option<DeviceStaged> = None;
    while let Ok(Chunk { calls, vertices }) = receiver.recv() {
        for call in calls {
            match call {
                Call::Begin {
                    frame,
                    native,
                    presented,
                    clear,
                } => match VkFrame::new(&device, native, presented, clear) {
                    Ok(made) => {
                        frames.insert(frame, made);
                    }
                    Err(error) => device.state.borrow_mut().fail(&error),
                },
                Call::Frame(frame, call) => {
                    if let Some(frame) = frames.get_mut(&frame) {
                        replay(frame, call, &vertices, &textures, &mut staged);
                    }
                }
                Call::End(frame) => {
                    frames.remove(&frame);
                }
                Call::Texture(key, texture) => {
                    textures.insert(key, *texture);
                }
                Call::Forget(keys) => {
                    for key in keys {
                        textures.remove(&key);
                    }
                }
                Call::SetFiltering(level) => device.set_filtering(level),
                Call::SetModern(modern) => device.set_modern(modern),
                Call::ReadOutput(output, reply) => {
                    let _ = reply.send(device.read_output(output));
                }
                Call::Settle(reply) => {
                    let _ = reply.send(Vec::new());
                }
                Call::ReadStaged(reply) => {
                    let _ = reply.send(staged.as_ref().map(DeviceStaged::read).unwrap_or_default());
                }
                Call::Swap(drawable) => {
                    device.swap_to(drawable);
                    *swaps.done.lock().unwrap_or_else(PoisonError::into_inner) += 1;
                    swaps.changed.notify_all();
                }
            }
        }
    }
    // The frames return their targets to the device before it goes.
    drop(frames);
    drop(staged);
    drop(device);
}

fn replay(
    frame: &mut VkFrame,
    call: FrameCall,
    vertices: &[D3dTlVertex],
    textures: &FastMap<u64, CpuTexture>,
    staged: &mut Option<DeviceStaged>,
) {
    match call {
        FrameCall::Fan { start, len, draw } => {
            let texture = match draw.texture {
                None => None,
                Some(texturing) => {
                    let Some(texture) = textures.get(&texturing.key) else {
                        debug_assert!(false, "a drawn texture is sent before its fan");
                        return;
                    };
                    Some(DeviceTexturing {
                        texture,
                        sampling: texturing.sampling,
                        perspective: texturing.perspective,
                        fog: texturing.fog,
                    })
                }
            };
            let start = start as usize;
            frame.fan(
                &vertices[start..start + len as usize],
                &DeviceDraw {
                    blend: draw.blend,
                    depth_test: draw.depth_test,
                    depth_write: draw.depth_write,
                    texture,
                },
            );
        }
        FrameCall::ClearColor(rgba) => frame.clear_color(rgba),
        FrameCall::ClearDepth => frame.clear_depth(),
        FrameCall::ReplaceSurface(rgba) => frame.replace_surface(&rgba),
        FrameCall::FinishTracedWorld => frame.finish_traced_world(),
        FrameCall::FinishThermal(ironbow) => frame.finish_thermal(ironbow),
        FrameCall::BeginRasterWorld(begin) => {
            frame.begin_raster_world(
                &begin.camera,
                begin.water_reflect,
                begin.murk,
                begin.light.as_ref(),
                begin.sky,
            );
        }
        FrameCall::RasterLamps(light) => frame.raster_lamps(&light),
        FrameCall::SetDrawMetal(metal) => frame.set_draw_metal(metal),
        FrameCall::DrawRasterSky => frame.draw_raster_sky(),
        FrameCall::SetDrawBand(band) => frame.set_draw_band(band),
        FrameCall::SetDrawMotion(motion) => frame.set_draw_motion(motion.map(|motion| *motion)),
        FrameCall::SetDrawWater(water) => frame.set_draw_water(water),
        FrameCall::SetDrawGround(ground) => frame.set_draw_ground(ground),
        FrameCall::SetDrawCovered(covered) => frame.set_draw_covered(covered),
        FrameCall::SetDrawTint(tint) => frame.set_draw_tint(tint.map(|tint| *tint)),
        FrameCall::SetWorldCamera(camera) => frame.set_world_camera(camera.map(|camera| *camera)),
        FrameCall::FinishRasterWorld => frame.finish_raster_world(),
        FrameCall::SetTraceInfrared(on) => frame.set_trace_infrared(on),
        FrameCall::PrepareTrace => frame.prepare_trace(),
        FrameCall::TraceWorld(feed, keys) => {
            let drawn: Vec<&CpuTexture> = keys.iter().filter_map(|key| textures.get(key)).collect();
            debug_assert_eq!(drawn.len(), keys.len(), "traced textures are sent first");
            frame.trace_world(&feed, &drawn);
        }
        FrameCall::ReadRgba(reply) => {
            let _ = reply.send(frame.read_rgba());
        }
        FrameCall::StageRgba => *staged = Some(frame.stage_rgba()),
        FrameCall::Present(output, destination) => frame.present(output, destination),
    }
}

/// A frame on the device's thread, as the main thread records it: the
/// calls of [`VkFrame`], sent on, and its answers, from the settings and the
/// frame's own calls as [`VkFrame`] gives them.
pub struct VkFrameHandle {
    device: Rc<VkHandle>,
    id: u64,
    presented: [u32; 2],
    trace_infrared: bool,
    world_camera: Option<SceneCamera>,
    raster_sky: bool,
    raster_metal: bool,
    /// The last fan's texture, sent and stamped this present.
    last_texture: Option<u64>,
    sent: DrawState,
}

impl fmt::Debug for VkFrameHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VkFrameHandle")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl VkFrameHandle {
    /// As [`VkFrame::new`]: a frame for `native`-pixel submissions shown at
    /// `presented` pixels, cleared to `clear`.
    pub(crate) fn new(
        device: &Rc<VkHandle>,
        native: [u32; 2],
        presented: [u32; 2],
        clear: [u8; 4],
    ) -> Self {
        let id = device.frames.get() + 1;
        device.frames.set(id);
        device.push(Call::Begin {
            frame: id,
            native,
            presented,
            clear,
        });
        Self {
            device: Rc::clone(device),
            id,
            presented,
            trace_infrared: false,
            world_camera: None,
            raster_sky: false,
            raster_metal: false,
            last_texture: None,
            sent: DrawState::default(),
        }
    }

    fn call(&self, call: FrameCall) {
        self.device.push(Call::Frame(self.id, call));
    }

    pub(crate) fn fan(&mut self, polygon: &[D3dTlVertex], draw: &DeviceDraw<'_>) {
        if polygon.len() < 3 {
            return;
        }
        let texture = draw.texture.as_ref().map(|texturing| {
            let key = texturing.texture.content_key();
            if self.last_texture != Some(key) {
                self.device.send_texture(texturing.texture);
                self.last_texture = Some(key);
            }
            Texturing {
                key,
                sampling: texturing.sampling,
                perspective: texturing.perspective,
                fog: texturing.fog,
            }
        });
        self.device.push_fan(
            self.id,
            polygon,
            Draw {
                blend: draw.blend,
                depth_test: draw.depth_test,
                depth_write: draw.depth_write,
                texture,
            },
        );
    }

    pub(crate) fn clear_color(&mut self, rgba: [u8; 4]) {
        self.call(FrameCall::ClearColor(rgba));
    }

    pub(crate) fn clear_depth(&mut self) {
        self.call(FrameCall::ClearDepth);
    }

    /// The size the frame is shown at, and of [`Self::read_rgba`]'s image.
    pub(crate) fn target(&self) -> [u32; 2] {
        self.presented
    }

    pub(crate) fn replace_surface(&mut self, rgba: &[u8]) {
        self.call(FrameCall::ReplaceSurface(rgba.to_vec()));
    }

    pub(crate) fn finish_traced_world(&mut self) {
        self.call(FrameCall::FinishTracedWorld);
    }

    pub(crate) fn finish_thermal(&mut self, ironbow: bool) {
        self.call(FrameCall::FinishThermal(ironbow));
    }

    /// As [`VkFrame::begin_raster_world`], whose outcome follows from the
    /// settings while the device runs: its modern sky where the world's
    /// tuning has one and the device writes the world's frame block, its
    /// metal where that block is written above the water out of infrared
    /// vision, and the anti-aliasing's following ([`raster_aa::follows`]).
    pub(crate) fn begin_raster_world(
        &mut self,
        camera: &SceneCamera,
        water_reflect: Option<f32>,
        murk: Option<Murk>,
        light: Option<&SceneFeed>,
        sky: bool,
    ) -> bool {
        self.world_camera = Some(*camera);
        self.raster_sky = false;
        self.raster_metal = false;
        self.call(FrameCall::BeginRasterWorld(Box::new(RasterBegin {
            camera: *camera,
            water_reflect,
            murk,
            light: light.cloned(),
            sky,
        })));
        if self.traces_rays() {
            return false;
        }
        if let Some(feed) = light {
            let written = self.device.scalar_layout;
            let modern_sky = crate::modern_tuning::current().sky(feed.world).style
                == crate::modern_tuning::SkyStyle::Modern;
            self.raster_sky = written && sky && modern_sky && !feed.infrared;
            self.raster_metal = written && murk.is_none() && !feed.infrared;
        }
        raster_aa::follows(self.device.modern.get())
    }

    pub(crate) fn raster_lamps(&mut self, light: &SceneFeed) {
        self.call(FrameCall::RasterLamps(Box::new(light.clone())));
    }

    /// Whether the device shades an untraced world's water itself (not
    /// lava, which [`Self::begin_raster_world`] is told of): Vulkan's
    /// raster, whose water's pass takes the frame block, which needs the
    /// scalar block layout. The world's begin writes that block; where it
    /// cannot, the device has failed and draws nothing more, so a world
    /// asked to show the murk never shows the view from above in its place.
    /// A capability, not this frame's outcome: the look under the water
    /// decides the game's fade.
    pub(crate) fn shades_water(&self) -> bool {
        !self.traces_rays() && self.device.scalar_layout
    }

    /// Whether this raster world's metal mirrors the sky.
    pub(crate) fn reflects_metal(&self) -> bool {
        self.raster_metal
    }

    pub(crate) fn set_draw_metal(&mut self, metal: Option<Arc<MetalFace>>) {
        let same = match (&self.sent.metal, &metal) {
            (None, None) => true,
            (Some(sent), Some(metal)) => Arc::ptr_eq(sent, metal),
            _ => false,
        };
        if !same {
            self.sent.metal.clone_from(&metal);
            self.call(FrameCall::SetDrawMetal(metal));
        }
    }

    /// Whether the device draws this raster world's modern sky.
    pub(crate) fn draws_raster_sky(&self) -> bool {
        self.raster_sky
    }

    pub(crate) fn draw_raster_sky(&mut self) {
        self.call(FrameCall::DrawRasterSky);
    }

    pub(crate) fn set_draw_band(&mut self, band: bool) {
        if self.sent.band != band {
            self.sent.band = band;
            self.call(FrameCall::SetDrawBand(band));
        }
    }

    pub(crate) fn set_draw_motion(&mut self, motion: Option<ViewMotion>) {
        if self.sent.motion != motion {
            self.sent.motion = motion;
            self.call(FrameCall::SetDrawMotion(motion.map(Box::new)));
        }
    }

    pub(crate) fn set_draw_water(&mut self, water: bool) {
        if self.sent.water != water {
            self.sent.water = water;
            self.call(FrameCall::SetDrawWater(water));
        }
    }

    pub(crate) fn set_draw_ground(&mut self, ground: Option<[u8; 3]>) {
        if self.sent.ground != ground {
            self.sent.ground = ground;
            self.call(FrameCall::SetDrawGround(ground));
        }
    }

    pub(crate) fn set_draw_covered(&mut self, covered: bool) {
        if self.sent.covered != covered {
            self.sent.covered = covered;
            self.call(FrameCall::SetDrawCovered(covered));
        }
    }

    pub(crate) fn set_draw_tint(&mut self, tint: Option<CrystalSpark>) {
        if self.sent.tint != tint {
            self.sent.tint = tint;
            self.call(FrameCall::SetDrawTint(tint.map(Box::new)));
        }
    }

    pub(crate) fn set_world_camera(&mut self, camera: Option<SceneCamera>) {
        self.world_camera = camera;
        self.call(FrameCall::SetWorldCamera(camera.map(Box::new)));
    }

    /// The world's camera while its draws come in, traced or raster.
    pub(crate) fn world_camera(&self) -> Option<SceneCamera> {
        self.world_camera
    }

    pub(crate) fn finish_raster_world(&mut self) {
        self.world_camera = None;
        self.raster_sky = false;
        self.raster_metal = false;
        self.call(FrameCall::FinishRasterWorld);
    }

    /// Whether this frame's world is traced: a device that traces rays,
    /// with the player's Ray Tracing on, or under infrared vision.
    pub(crate) fn traces_rays(&self) -> bool {
        self.device.ray_tracing && (self.device.modern.get().ray_tracing || self.trace_infrared)
    }

    pub(crate) fn set_trace_infrared(&mut self, on: bool) {
        self.trace_infrared = on;
        self.call(FrameCall::SetTraceInfrared(on));
    }

    pub(crate) fn prepare_trace(&mut self) {
        self.call(FrameCall::PrepareTrace);
    }

    pub(crate) fn trace_world(&mut self, feed: &SceneFeed, textures: &[&CpuTexture]) -> bool {
        if !self.traces_rays() {
            return false;
        }
        let keys = textures
            .iter()
            .map(|texture| self.device.send_texture(texture))
            .collect();
        self.call(FrameCall::TraceWorld(Box::new(feed.clone()), keys));
        true
    }

    /// The frame's pixels, once the device's thread has drawn everything
    /// sent before.
    pub(crate) fn read_rgba(&mut self) -> Vec<u8> {
        let id = self.id;
        self.device
            .request(|reply| Call::Frame(id, FrameCall::ReadRgba(reply)))
            .unwrap_or_else(|| vec![0; self.presented[0] as usize * self.presented[1] as usize * 4])
    }

    pub(crate) fn stage_rgba(&mut self) -> StagedRgba {
        self.call(FrameCall::StageRgba);
        StagedRgba {
            device: Rc::clone(&self.device),
            presented: self.presented,
        }
    }

    pub(crate) fn present(&mut self, output: [u32; 2], destination: PresentationRect) {
        self.call(FrameCall::Present(output, destination));
        self.last_texture = None;
        self.device.presented();
    }
}

impl Drop for VkFrameHandle {
    fn drop(&mut self) {
        self.device.push(Call::End(self.id));
    }
}

/// A frame's pixels copied on the device for a snapshot
/// ([`VkFrameHandle::stage_rgba`]), read back when it is saved.
pub(crate) struct StagedRgba {
    device: Rc<VkHandle>,
    presented: [u32; 2],
}

impl fmt::Debug for StagedRgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagedRgba")
            .field("presented", &self.presented)
            .finish_non_exhaustive()
    }
}

impl StagedRgba {
    /// The copied pixels at the size the frame is shown at, top row first.
    pub(crate) fn read(&self) -> Vec<u8> {
        self.device
            .request(Call::ReadStaged)
            .filter(|pixels| !pixels.is_empty())
            .unwrap_or_else(|| vec![0; self.presented[0] as usize * self.presented[1] as usize * 4])
    }
}
