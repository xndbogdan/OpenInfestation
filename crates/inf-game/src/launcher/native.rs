//! The launcher's classic Win32 controls, after the V2000 port's. The file
//! check runs on a worker thread and reports through a channel; only the UI
//! thread touches window handles.

use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use inf_game::display_selection::{DisplayMode, DisplaySelection, MonitorModes};
use inf_game::frontend_assets::FrontendAssets;
use inf_game::frontend_settings::{repair, reset, FrontendSettings, SettingsHealth};
use inf_game::launcher_setup::{validate_installation, InstallationReport};
use inf_game::presentation::RendererChoice;

use super::choices::*;
use super::header_animation::HeaderAnimation;
use super::win32::*;
use super::LauncherRequest;

const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000f;
const WM_CLOSE: u32 = 0x0010;
const WM_SETFONT: u32 = 0x0030;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const DM_GETDEFID: u32 = 0x0400;
const DC_HASDEFID: u32 = 0x534b;
const BM_GETCHECK: u32 = 0x00f0;
const BM_SETCHECK: u32 = 0x00f1;
const CB_ADDSTRING: u32 = 0x0143;
const CB_GETCURSEL: u32 = 0x0147;
const CB_RESETCONTENT: u32 = 0x014b;
const CB_SETCURSEL: u32 = 0x014e;
const CBN_SELCHANGE: usize = 1;
const TBM_GETPOS: u32 = 0x0400;
const TBM_SETPOS: u32 = 0x0405;
const TBM_SETRANGE: u32 = 0x0406;
const TBS_AUTOTICKS: u32 = 0x0001;
const TBS_TOOLTIPS: u32 = 0x0100;
const ICC_BAR_CLASSES: u32 = 0x0004;
const IDCANCEL: usize = 2;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
/// Caption, system menu and a fixed border.
const WINDOW_STYLE: u32 = 0x00c8_0000;
const EX_CONTROL_PARENT: u32 = 0x0001_0000;
const EX_CLIENT_EDGE: u32 = 0x0000_0200;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_GROUPBOX: u32 = 0x0007;
/// `CBS_DROPDOWNLIST | CBS_HASSTRINGS | WS_VSCROLL`.
const COMBO_STYLE: u32 = 0x0020_0203;
/// `ES_AUTOHSCROLL | ES_READONLY`.
const PATH_STYLE: u32 = 0x0880;
/// `ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY | WS_VSCROLL`.
const DETAILS_STYLE: u32 = 0x0020_0844;
const DPI_AWARENESS_CONTEXT_SYSTEM_AWARE: isize = -2;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
const ENUM_CURRENT_SETTINGS: u32 = u32::MAX;
const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
const SW_SHOW: i32 = 5;
const SW_HIDE: i32 = 0;
/// `SetWindowPos` flags: keep the size, the Z order and the activation.
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const GWLP_USERDATA: i32 = -21;
const GW_OWNER: u32 = 4;
const MB_ICONERROR: u32 = 0x10;
const MB_ICONWARNING: u32 = 0x30;
const MB_ICONINFORMATION: u32 = 0x40;
/// `HALFTONE`: averages the downscaled banner instead of dropping pixels.
const HALFTONE: i32 = 4;
const SRCCOPY: u32 = 0x00cc_0020;
const DIB_RGB_COLORS: u32 = 0;

const TITLE: &str = "Infestation";
const MAIN_CLASS: &str = "InfestationPortLauncher";
const OPTIONS_CLASS: &str = "InfestationPortLauncherOptions";
const ID_PLAY: usize = 101;
const ID_OPTIONS: usize = 102;
const ID_QUIT: usize = 103;
const ID_CHANGE: usize = 104;
const ID_VERIFY: usize = 105;
const ID_SAVE: usize = 106;
const ID_CANCEL: usize = 107;
const ID_SETTINGS_FIX: usize = 108;
const ID_DISPLAY: usize = 201;
const ID_RESOLUTION: usize = 202;
const ID_RENDERER: usize = 203;
const ID_SCALING: usize = 204;
const ID_EFFECTS: usize = 205;
const ID_MUSIC: usize = 206;
const ID_SKIP: usize = 207;
const ID_CONTROLLER: usize = 208;
const ID_CAMERA: usize = 228;
const ID_FILTERING: usize = 209;
const ID_RAY_TRACING: usize = 220;
const ID_UPSCALER: usize = 221;
const ID_UPSCALING: usize = 222;
const ID_ANTI_ALIASING: usize = 223;
const ID_LIGHTING: usize = 224;
const ID_AMBIENCE: usize = 226;
const ID_HEADPHONES: usize = 227;
const ID_FIXES: usize = 210;
const WORKER_TIMER: usize = 1;
/// The banner's flipbook: one baked frame a 20 ms frontend clock tick.
const HEADER_TIMER: usize = 2;
/// Logical sizes of the two windows' client areas.
const MAIN_SIZE: [i32; 2] = [660, 439];
const OPTIONS_SIZE: [i32; 2] = [560, 1030];
/// The Vulkan group's height: hiding it lifts everything below by this,
/// and the Options window shrinks by it.
const VULKAN_GROUP_LIFT: i32 = 178;

struct WindowHost(RefCell<WindowState>);

#[derive(Default)]
struct MainControls {
    root: Handle,
    change: Handle,
    details: Handle,
    progress: Handle,
    play: Handle,
    options: Handle,
    /// Repair or Reset settings, in Play's and Options' place while only the
    /// settings file stops the game.
    settings_fix: Handle,
}

#[derive(Default)]
struct OptionControls {
    display: Handle,
    resolution: Handle,
    renderer: Handle,
    filtering: Handle,
    ray_tracing: Handle,
    upscaler: Handle,
    upscaling: Handle,
    anti_aliasing: Handle,
    lighting: Handle,
    scaling: Handle,
    effects: Handle,
    music: Handle,
    ambience: Handle,
    headphones: Handle,
    controller: Handle,
    camera: Handle,
    fixes: [Handle; FIXES],
    skip: Handle,
    progress: Handle,
    verify: Handle,
    save: Handle,
    cancel: Handle,
    /// The Vulkan group box, its note and its six row labels: hidden unless
    /// Vulkan runs. The combos stay in their fields above.
    vulkan_group: Handle,
    vulkan_note: Handle,
    vulkan_labels: [Handle; 5],
    /// Everything the lift moves: the Sound, Fixes and Cheats groups, their
    /// rows' labels, and the window's bottom rows.
    sound_group: Handle,
    effects_label: Handle,
    music_label: Handle,
    headphones_label: Handle,
    controller_label: Handle,
    ambience_label: Handle,
    camera_label: Handle,
    fixes_group: Handle,
    cheats_group: Handle,
    skip_note: Handle,
}

/// Options' choices while it is open: the game's settings as loaded, the
/// primary monitor's modes and the pending Display and Resolution.
struct PendingOptions {
    settings: FrontendSettings,
    modes: MonitorModes,
    display: DisplaySelection,
    /// Settings `+3C`'s Filtering as last chosen: kept through a Renderer
    /// change that cannot draw it, so a Vulkan choice survives a look at
    /// OpenGL.
    filtering: u32,
    /// The Upscaler as last chosen: Ray Reconstruction shows as DLSS while
    /// Ray Tracing is Off and stays chosen, as the Video menu's row keeps it.
    upscaler: inf_game::modern_settings::Upscaler,
    /// What this machine's Vulkan device runs: its rows list only that.
    support: inf_game::modern_settings::ModernSupport,
}

enum WorkerEvent {
    Checked {
        report: InstallationReport,
        show_result: bool,
    },
    Failure(String),
}

struct WindowState {
    request: LauncherRequest,
    window: Handle,
    options_window: Handle,
    host: *mut WindowHost,
    controls: MainControls,
    option_controls: OptionControls,
    /// [`inf_game::vk_device::VkDevice::probe_modern_support`], asked when
    /// Options first opens (a Vulkan start); everything without Vulkan.
    modern_support: Option<inf_game::modern_settings::ModernSupport>,
    font: Handle,
    banner_brush: Handle,
    header_animation: HeaderAnimation,
    header_last_tick: Instant,
    scale: f64,
    root: PathBuf,
    report: Option<InstallationReport>,
    options: Option<PendingOptions>,
    worker: Option<JoinHandle<()>>,
    sender: Sender<WorkerEvent>,
    receiver: Receiver<WorkerEvent>,
    busy: bool,
    close_requested: bool,
    selection: Option<PathBuf>,
    started: Instant,
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

unsafe fn text(window: Handle, value: &str) {
    SetWindowTextW(window, wide(value).as_ptr());
}

unsafe fn redraw_after_dialog(owner: Handle) {
    if owner != 0 {
        InvalidateRect(owner, null(), 1);
        let parent = GetWindow(owner, GW_OWNER);
        if parent != 0 {
            InvalidateRect(parent, null(), 1);
        }
    }
}

unsafe fn notice(owner: Handle, title: &str, message: &str, flags: u32) -> i32 {
    let result = MessageBoxW(owner, wide(message).as_ptr(), wide(title).as_ptr(), flags);
    // A modal dialog pumps paint messages while the state borrow is held;
    // queue a fresh paint for after it is released.
    redraw_after_dialog(owner);
    result
}

unsafe fn show_interactive_window(window: Handle) {
    // Windows applies STARTUPINFO's show mode to the first ShowWindow call,
    // even SW_SHOW: a host may start the process hidden, but the launcher
    // is always an interactive window.
    ShowWindow(window, SW_SHOW);
    ShowWindow(window, SW_SHOW);
    UpdateWindow(window);
}

pub fn show_error(message: &str) {
    unsafe {
        notice(0, TITLE, message, MB_ICONERROR);
    }
}

pub fn run(request: LauncherRequest) -> Result<Option<PathBuf>, String> {
    unsafe { run_native(request) }
}

pub(super) fn attach_parent_console() {
    unsafe {
        // A GUI-subsystem executable has no console of its own. Attach only
        // to a caller's (Explorer has none), never allocate one.
        // https://learn.microsoft.com/en-us/windows/console/attachconsole
        let redirected = [(-10_i32) as u32, (-11_i32) as u32, (-12_i32) as u32].map(|identifier| {
            let handle = GetStdHandle(identifier);
            let kind = if handle.is_null() || handle as isize == -1 {
                0
            } else {
                GetFileType(handle) & 0xffff
            };
            // A disk file or a pipe: the caller redirected this stream.
            (identifier, matches!(kind, 1 | 3).then_some(handle))
        });
        AttachConsole(u32::MAX); // ATTACH_PARENT_PROCESS
                                 // AttachConsole may replace the standard handles; keep redirections
                                 // with their owner (Command::output, pipelines, files).
        for (identifier, handle) in redirected {
            if let Some(handle) = handle {
                SetStdHandle(identifier, handle);
            }
        }
    }
}

pub(super) fn declare_dpi_awareness() {
    unsafe {
        let user32 = wide("user32.dll");
        let module = GetModuleHandleW(user32.as_ptr());
        let set = if module == 0 {
            null_mut()
        } else {
            GetProcAddress(module, c"SetProcessDpiAwarenessContext".as_ptr().cast())
        };
        if !set.is_null() {
            // Windows 10 1703 and later.
            let set: unsafe extern "system" fn(isize) -> i32 = std::mem::transmute(set);
            if set(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 {
                return;
            }
        }
        // Older Windows: aware of the system DPI at least.
        SetProcessDPIAware();
    }
}

unsafe fn run_native(request: LauncherRequest) -> Result<Option<PathBuf>, String> {
    let header_animation = HeaderAnimation::load()?;
    // The volume sliders are comctl32's trackbars.
    InitCommonControlsEx(&InitCommonControls {
        size: std::mem::size_of::<InitCommonControls>() as u32,
        classes: ICC_BAR_CLASSES,
    });
    // The process is per-monitor aware for the game; the launcher lays out
    // at the system DPI, scaled by GetDpiForSystem.
    let dpi_context = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
    // The folder chooser needs an apartment-threaded COM.
    let com_result = CoInitializeEx(null_mut(), 2);
    let dpi = GetDpiForSystem().max(96);
    // Every control and font shares one scale, fitted to the screen.
    let scale = (f64::from(dpi) / 96.0)
        .min(f64::from((GetSystemMetrics(0) - 32).max(1)) / f64::from(MAIN_SIZE[0]))
        .min(f64::from((GetSystemMetrics(1) - 80).max(1)) / f64::from(OPTIONS_SIZE[1]));
    let font = CreateFontW(
        -(13.0 * scale).round() as i32,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        wide("MS Sans Serif").as_ptr(),
    );
    let root = request.root.clone();
    let (sender, receiver) = mpsc::channel();
    let host = Box::new(WindowHost(RefCell::new(WindowState {
        request,
        window: 0,
        options_window: 0,
        host: null_mut(),
        controls: MainControls::default(),
        option_controls: OptionControls::default(),
        modern_support: None,
        font,
        banner_brush: CreateSolidBrush(0),
        header_animation,
        header_last_tick: Instant::now(),
        scale,
        root,
        report: None,
        options: None,
        worker: None,
        sender,
        receiver,
        busy: false,
        close_requested: false,
        selection: None,
        started: Instant::now(),
    })));
    let host_pointer = (&*host as *const WindowHost).cast_mut();
    host.0.borrow_mut().host = host_pointer;
    let result = (|| {
        register_class(MAIN_CLASS, main_procedure)?;
        register_class(OPTIONS_CLASS, options_procedure)?;
        let window = create_top_window(MAIN_CLASS, TITLE, MAIN_SIZE, 0, host_pointer, scale)?;
        {
            let mut state = host.0.borrow_mut();
            state.window = window;
            state.create_main_controls();
            state.check_files(false);
        }
        show_interactive_window(window);
        host.0.borrow_mut().header_last_tick = Instant::now();
        SetTimer(window, WORKER_TIMER, 100, null());
        SetTimer(window, HEADER_TIMER, 20, null());
        let mut message = Message::default();
        loop {
            let status = GetMessageW(&mut message, 0, 0, 0);
            if status == 0 {
                break;
            }
            if status < 0 {
                return Err(format!("Windows message loop failed ({})", GetLastError()));
            }
            let options = host.0.borrow().options_window;
            let dialog = if options != 0 { options } else { window };
            if IsDialogMessageW(dialog, &mut message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(host.0.borrow_mut().selection.take())
    })();
    let mut state = host.0.borrow_mut();
    if IsWindow(state.options_window) != 0 {
        DestroyWindow(state.options_window);
    }
    if IsWindow(state.window) != 0 {
        DestroyWindow(state.window);
    }
    if let Some(worker) = state.worker.take() {
        let _ = worker.join();
    }
    DeleteObject(state.font);
    DeleteObject(state.banner_brush);
    if com_result >= 0 {
        CoUninitialize();
    }
    if dpi_context != 0 {
        SetThreadDpiAwarenessContext(dpi_context);
    }
    result
}

unsafe fn register_class(name: &str, procedure: WindowProc) -> Result<(), String> {
    let name = wide(name);
    let instance = GetModuleHandleW(null());
    let class = WindowClass {
        size: size_of::<WindowClass>() as u32,
        style: 3,
        procedure: Some(procedure),
        class_extra: 0,
        window_extra: 0,
        instance,
        // The port's icon, embedded by build.rs as IDI_ICON1.
        icon: LoadIconW(instance, wide("IDI_ICON1").as_ptr()),
        cursor: LoadCursorW(0, 32512usize as *const u16),
        background: 16,
        menu_name: null(),
        class_name: name.as_ptr(),
        // NULL lets Windows pick the small image from the icon's group.
        small_icon: 0,
    };
    // 1410: the class is already registered.
    if RegisterClassExW(&class) == 0 && GetLastError() != 1410 {
        Err(format!(
            "Cannot register the launcher window ({})",
            GetLastError()
        ))
    } else {
        Ok(())
    }
}

unsafe fn create_top_window(
    class: &str,
    title: &str,
    size: [i32; 2],
    owner: Handle,
    host: *mut WindowHost,
    scale: f64,
) -> Result<Handle, String> {
    let mut bounds = Rect {
        left: 0,
        top: 0,
        right: (f64::from(size[0]) * scale).round() as i32,
        bottom: (f64::from(size[1]) * scale).round() as i32,
    };
    AdjustWindowRectEx(&mut bounds, WINDOW_STYLE, 0, EX_CONTROL_PARENT);
    let width = bounds.right - bounds.left;
    let height = bounds.bottom - bounds.top;
    let window = CreateWindowExW(
        EX_CONTROL_PARENT,
        wide(class).as_ptr(),
        wide(title).as_ptr(),
        WINDOW_STYLE,
        (GetSystemMetrics(0) - width) / 2,
        (GetSystemMetrics(1) - height) / 2,
        width,
        height,
        owner,
        0,
        GetModuleHandleW(null()),
        host.cast(),
    );
    if window == 0 {
        Err(format!(
            "Cannot create the launcher window ({})",
            GetLastError()
        ))
    } else {
        Ok(window)
    }
}

unsafe fn host_for_message(window: Handle, message: u32, lparam: isize) -> *mut WindowHost {
    if message == WM_CREATE {
        let create = &*(lparam as *const CreateStruct);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.parameter as isize);
    }
    GetWindowLongPtrW(window, GWLP_USERDATA) as *mut WindowHost
}

unsafe extern "system" fn main_procedure(
    window: Handle,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if message == WM_DESTROY {
        KillTimer(window, WORKER_TIMER);
        KillTimer(window, HEADER_TIMER);
        PostQuitMessage(0);
        return 0;
    }
    if message == DM_GETDEFID {
        return ((DC_HASDEFID << 16) | ID_PLAY as u32) as isize;
    }
    let host = host_for_message(window, message, lparam);
    if !host.is_null() {
        // Native dialogs pump nested messages. Skipping them while the state
        // is borrowed avoids aliasing it; the timer catches up afterwards.
        if let Ok(mut state) = (*host).0.try_borrow_mut() {
            match message {
                WM_COMMAND if wparam >> 16 == 0 => {
                    state.command(wparam & 0xffff);
                    return 0;
                }
                WM_TIMER if wparam == WORKER_TIMER => {
                    state.poll_worker();
                    return 0;
                }
                WM_TIMER if wparam == HEADER_TIMER => {
                    state.animate_header();
                    return 0;
                }
                WM_CLOSE => {
                    state.quit();
                    return 0;
                }
                WM_PAINT => {
                    state.paint_banner(window);
                    return 0;
                }
                _ => {}
            }
        }
    }
    DefWindowProcW(window, message, wparam, lparam)
}

unsafe extern "system" fn options_procedure(
    window: Handle,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if message == DM_GETDEFID {
        return ((DC_HASDEFID << 16) | ID_SAVE as u32) as isize;
    }
    let host = host_for_message(window, message, lparam);
    if !host.is_null() {
        if let Ok(mut state) = (*host).0.try_borrow_mut() {
            match message {
                WM_COMMAND if wparam >> 16 == 0 => {
                    state.command(wparam & 0xffff);
                    return 0;
                }
                WM_COMMAND if wparam >> 16 == CBN_SELCHANGE => {
                    state.display_choice_changed(wparam & 0xffff);
                    return 0;
                }
                WM_CLOSE => {
                    state.close_options();
                    return 0;
                }
                _ => {}
            }
        }
    }
    DefWindowProcW(window, message, wparam, lparam)
}

impl WindowState {
    fn pixel(&self, logical: i32) -> i32 {
        pixel(self.scale, logical)
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn control(
        &self,
        parent: Handle,
        class: &str,
        label: &str,
        style: u32,
        id: usize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Handle {
        let child = CreateWindowExW(
            if class == "EDIT" { EX_CLIENT_EDGE } else { 0 },
            wide(class).as_ptr(),
            wide(label).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            self.pixel(x),
            self.pixel(y),
            self.pixel(width),
            self.pixel(height),
            parent,
            id as Handle,
            GetModuleHandleW(null()),
            null_mut(),
        );
        // The classic look: no visual style.
        let empty = [0u16];
        SetWindowTheme(child, empty.as_ptr(), empty.as_ptr());
        SendMessageW(child, WM_SETFONT, self.font as usize, 1);
        child
    }

    unsafe fn label(
        &self,
        parent: Handle,
        label: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Handle {
        self.control(parent, "STATIC", label, 0, 0, x, y, width, height)
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn button(
        &self,
        parent: Handle,
        label: &str,
        id: usize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Handle {
        let default = if id == ID_PLAY || id == ID_SAVE {
            BS_DEFPUSHBUTTON
        } else {
            0
        };
        self.control(
            parent,
            "BUTTON",
            label,
            WS_TABSTOP | default,
            id,
            x,
            y,
            width,
            height,
        )
    }

    unsafe fn group(
        &self,
        parent: Handle,
        label: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Handle {
        self.control(parent, "BUTTON", label, BS_GROUPBOX, 0, x, y, width, height)
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn checkbox(
        &self,
        parent: Handle,
        label: &str,
        id: usize,
        x: i32,
        y: i32,
        width: i32,
        checked: bool,
    ) -> Handle {
        let control = self.control(
            parent,
            "BUTTON",
            label,
            WS_TABSTOP | BS_AUTOCHECKBOX,
            id,
            x,
            y,
            width,
            23,
        );
        SendMessageW(control, BM_SETCHECK, usize::from(checked), 0);
        control
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn combo(
        &self,
        parent: Handle,
        values: &[String],
        selection: usize,
        id: usize,
        x: i32,
        y: i32,
        width: i32,
    ) -> Handle {
        let control = self.control(
            parent,
            "COMBOBOX",
            "",
            WS_TABSTOP | COMBO_STYLE,
            id,
            x,
            y,
            width,
            280,
        );
        for value in values {
            SendMessageW(control, CB_ADDSTRING, 0, wide(value).as_ptr() as isize);
        }
        SendMessageW(control, CB_SETCURSEL, selection, 0);
        control
    }

    /// A trackbar over `0..=maximum` at `value`, with a tick at each step
    /// and a tooltip that shows the value while it is dragged.
    #[allow(clippy::too_many_arguments)]
    unsafe fn slider(
        &self,
        parent: Handle,
        value: u32,
        maximum: u32,
        id: usize,
        x: i32,
        y: i32,
        width: i32,
    ) -> Handle {
        let control = self.control(
            parent,
            "msctls_trackbar32",
            "",
            WS_TABSTOP | TBS_AUTOTICKS | TBS_TOOLTIPS,
            id,
            x,
            y,
            width,
            30,
        );
        SendMessageW(control, TBM_SETRANGE, 1, (maximum << 16) as isize);
        SendMessageW(control, TBM_SETPOS, 1, value.min(maximum) as isize);
        control
    }

    unsafe fn create_main_controls(&mut self) {
        let window = self.window;
        self.group(window, " Installation ", 16, 132, 628, 58);
        let root = self.control(
            window,
            "EDIT",
            &self.root.display().to_string(),
            WS_TABSTOP | PATH_STYLE,
            0,
            26,
            155,
            498,
            22,
        );
        let change = self.button(window, "&Change...", ID_CHANGE, 534, 154, 100, 24);
        self.group(window, " Game files ", 16, 200, 628, 141);
        let details = self.control(
            window,
            "EDIT",
            "Checking files...",
            WS_TABSTOP | DETAILS_STYLE,
            0,
            26,
            223,
            608,
            106,
        );
        let progress = self.label(window, "", 16, 351, 628, 20);
        let play = self.button(window, "&Play", ID_PLAY, 16, 381, 192, 42);
        let options = self.button(window, "&Options...", ID_OPTIONS, 234, 381, 192, 42);
        let settings_fix = self.button(window, "", ID_SETTINGS_FIX, 16, 381, 410, 42);
        self.button(window, "&Quit", ID_QUIT, 452, 381, 192, 42);
        self.controls = MainControls {
            root,
            change,
            details,
            progress,
            play,
            options,
            settings_fix,
        };
        EnableWindow(play, 0);
        EnableWindow(options, 0);
        ShowWindow(settings_fix, SW_HIDE);
    }

    /// What fixes the settings file, while it is all that stops the game.
    fn settings_fix(&self) -> Option<&SettingsHealth> {
        self.report
            .as_ref()
            .filter(|report| report.only_settings_broken())
            .map(|report| &report.settings)
    }

    /// Play and Options give way to Repair settings, or Reset settings when
    /// nothing can be read from the file: never both (2026-10-28).
    unsafe fn show_settings_fix(&self) {
        let fix = self.settings_fix();
        if let Some(health) = fix {
            let label = match health {
                SettingsHealth::Unreadable { .. } => "&Reset settings",
                _ => "&Repair settings",
            };
            text(self.controls.settings_fix, label);
        }
        let shown = |shown: bool| if shown { SW_SHOW } else { SW_HIDE };
        ShowWindow(self.controls.play, shown(fix.is_none()));
        ShowWindow(self.controls.options, shown(fix.is_none()));
        ShowWindow(self.controls.settings_fix, shown(fix.is_some()));
        EnableWindow(self.controls.settings_fix, i32::from(!self.busy));
    }

    /// Repairs or resets the settings file, then checks the files again. The
    /// old file is kept as `settings.json.broken`, so nothing is asked first.
    unsafe fn fix_settings(&mut self) {
        let Some(health) = self.settings_fix().cloned() else {
            return;
        };
        let path = FrontendSettings::default_path();
        let (done, result) = match health {
            SettingsHealth::Unreadable { .. } => ("reset", reset(&path)),
            _ => (
                "repaired",
                FrontendAssets::open(&self.root)
                    .map_err(|error| error.to_string())
                    .and_then(|assets| repair(&path, &assets).map(drop)),
            ),
        };
        if let Err(error) = result {
            notice(
                self.window,
                TITLE,
                &format!("{} could not be {done}:\n\n{error}", path.display()),
                MB_ICONERROR,
            );
        }
        self.check_files(false);
    }

    fn banner_bounds(&self) -> Rect {
        Rect {
            left: self.pixel(16),
            top: self.pixel(16),
            right: self.pixel(644),
            bottom: self.pixel(118),
        }
    }

    unsafe fn animate_header(&mut self) {
        let now = Instant::now();
        let elapsed_micros = now.duration_since(self.header_last_tick).as_micros() as u64;
        self.header_last_tick = now;
        if self.header_animation.advance(elapsed_micros) {
            // The opaque frame repaints only the banner, without erasing the
            // background or invalidating any control.
            InvalidateRect(self.window, &self.banner_bounds(), 0);
        }
    }

    /// The banner, composed off screen and copied to the window in one blit.
    /// Drawn straight onto the window, the compositor could show the black
    /// fill before the frame landed on it, and the lettering flickered.
    unsafe fn paint_banner(&self, window: Handle) {
        let mut paint: PaintStruct = zeroed();
        let dc = BeginPaint(window, &mut paint);
        let banner = self.banner_bounds();
        let width = banner.right - banner.left;
        let height = banner.bottom - banner.top;
        let memory = CreateCompatibleDC(dc);
        let surface = if memory != 0 {
            CreateCompatibleBitmap(dc, width, height)
        } else {
            0
        };
        if surface != 0 {
            let previous = SelectObject(memory, surface);
            self.draw_banner(
                memory,
                Rect {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
            );
            BitBlt(
                dc,
                banner.left,
                banner.top,
                width,
                height,
                memory,
                0,
                0,
                SRCCOPY,
            );
            SelectObject(memory, previous);
            DeleteObject(surface);
        } else {
            self.draw_banner(dc, banner);
        }
        if memory != 0 {
            DeleteDC(memory);
        }
        EndPaint(window, &paint);
    }

    /// The current baked frame on black inside `banner`, its aspect kept.
    unsafe fn draw_banner(&self, dc: Handle, mut banner: Rect) {
        FillRect(dc, &banner, self.banner_brush);
        let image = self.header_animation.frame();
        let height = self.pixel(image.height);
        let width = height * image.width / image.height;
        let bitmap = BitmapInfo {
            header: BitmapInfoHeader {
                size: size_of::<BitmapInfoHeader>() as u32,
                width: image.width,
                // Negative: top-down rows, as the PNG stores them.
                height: -image.height,
                planes: 1,
                bit_count: 32,
                ..BitmapInfoHeader::default()
            },
            colors: [0],
        };
        SetStretchBltMode(dc, HALFTONE);
        SetBrushOrgEx(dc, 0, 0, null_mut());
        StretchDIBits(
            dc,
            (banner.left + banner.right - width) / 2,
            (banner.top + banner.bottom - height) / 2,
            width,
            height,
            0,
            0,
            image.width,
            image.height,
            image.bgra.as_ptr().cast(),
            &bitmap,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        // EDGE_SUNKEN on every side.
        DrawEdge(dc, &mut banner, 0x000a, 0x000f);
    }

    fn can_play(&self) -> bool {
        !self.busy
            && self
                .report
                .as_ref()
                .is_some_and(InstallationReport::is_ready)
    }

    unsafe fn command(&mut self, id: usize) {
        match id {
            IDCANCEL => {
                if self.options_window != 0 {
                    self.close_options();
                } else {
                    self.quit();
                }
            }
            ID_PLAY if self.can_play() => self.play(),
            ID_OPTIONS if self.can_play() => self.open_options(),
            ID_QUIT => self.quit(),
            ID_SETTINGS_FIX if !self.busy => self.fix_settings(),
            ID_CHANGE if !self.busy => self.choose_installation(),
            ID_VERIFY if !self.busy => self.check_files(true),
            ID_SAVE if !self.busy => self.save_options(),
            ID_CANCEL => self.close_options(),
            id if (ID_FIXES..ID_FIXES + FIXES).contains(&id) => {
                if self.options_window != 0 {
                    Self::refresh_pelagia_row(&self.option_controls.fixes);
                }
            }
            _ => {}
        }
    }

    /// Pelagia's rework needs its gate: while the gate row is off, the
    /// rework row goes dark and off with it. Takes the row handles
    /// explicitly, so a fresh dialog syncs from its own rows.
    unsafe fn refresh_pelagia_row(fixes: &[Handle; FIXES]) {
        let prototype = fixes[FIX_PROTOTYPE_WORLD];
        let rework = fixes[FIX_PELAGIA_REWORK];
        if prototype == 0 || rework == 0 {
            return;
        }
        if checked(prototype) {
            EnableWindow(rework, 1);
        } else {
            SendMessageW(rework, BM_SETCHECK, 0, 0);
            EnableWindow(rework, 0);
        }
    }

    unsafe fn play(&mut self) {
        self.request.preferences.data_root = Some(self.root.clone());
        if let Err(error) = self
            .request
            .preferences
            .save(&self.request.preferences_path)
        {
            notice(
                self.window,
                TITLE,
                &format!(
                    "The game can start, but the launcher could not remember this installation:\n\n{error}"
                ),
                MB_ICONWARNING,
            );
        }
        self.selection = Some(self.root.clone());
        DestroyWindow(self.window);
    }

    unsafe fn quit(&mut self) {
        if self.busy {
            self.close_requested = true;
            self.set_progress("Finishing the file check before closing...");
        } else {
            DestroyWindow(self.window);
        }
    }

    unsafe fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        let ready = self.can_play();
        EnableWindow(self.controls.play, i32::from(ready));
        EnableWindow(self.controls.options, i32::from(ready));
        EnableWindow(self.controls.change, i32::from(!busy));
        self.show_settings_fix();
        for control in [self.option_controls.verify, self.option_controls.save] {
            if control != 0 {
                EnableWindow(control, i32::from(!busy));
            }
        }
    }

    unsafe fn set_progress(&self, message: &str) {
        text(self.controls.progress, message);
        if self.options_window != 0 {
            text(self.option_controls.progress, message);
        }
    }

    unsafe fn check_files(&mut self, show_result: bool) {
        let root = self.root.clone();
        self.set_busy(true);
        self.set_progress(
            "Checking Infest.exe, PreLoad.dat, every overlay, the settings, the title video and the music...",
        );
        let sender = self.sender.clone();
        self.worker = Some(thread::spawn(move || {
            let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                validate_installation(&root, &FrontendSettings::default_path())
            }));
            let event = match checked {
                Ok(report) => WorkerEvent::Checked {
                    report,
                    show_result,
                },
                Err(_) => WorkerEvent::Failure("The file check stopped unexpectedly.".to_string()),
            };
            let _ = sender.send(event);
        }));
    }

    unsafe fn poll_worker(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            match event {
                WorkerEvent::Checked {
                    report,
                    show_result,
                } => {
                    let ready = report.is_ready();
                    let summary = popup_description(&report);
                    text(
                        self.controls.details,
                        &report.description().replace('\n', "\r\n"),
                    );
                    let progress = match &report.settings {
                        _ if ready => "",
                        _ if !report.only_settings_broken() => {
                            "Choose the folder holding Infest.exe, PreLoad.dat and Overlay."
                        }
                        SettingsHealth::Unreadable { .. } => {
                            "settings.json cannot be read. Reset it to start from the original settings."
                        }
                        _ => "settings.json is malformed. Repair it to play.",
                    };
                    self.report = Some(report);
                    self.set_busy(false);
                    self.set_progress(progress);
                    if show_result && !self.close_requested {
                        let flags = if ready {
                            MB_ICONINFORMATION
                        } else {
                            MB_ICONWARNING
                        };
                        notice(
                            self.dialog_owner(),
                            "Infestation file check",
                            &summary,
                            flags,
                        );
                    }
                    if self.options_window == 0 {
                        if ready {
                            SetFocus(self.controls.play);
                        } else if self.settings_fix().is_some() {
                            SetFocus(self.controls.settings_fix);
                        }
                    }
                }
                WorkerEvent::Failure(error) => {
                    self.report = None;
                    text(self.controls.details, &error);
                    self.set_busy(false);
                    self.set_progress("");
                }
            }
        }
        if self
            .request
            .smoke_timeout
            .is_some_and(|timeout| self.started.elapsed() >= timeout)
        {
            self.close_requested = true;
        }
        if self.close_requested && !self.busy {
            self.close_options();
            DestroyWindow(self.window);
        }
    }

    fn dialog_owner(&self) -> Handle {
        if self.options_window != 0 {
            self.options_window
        } else {
            self.window
        }
    }

    unsafe fn choose_installation(&mut self) {
        let Some(root) = choose_folder(
            self.window,
            "Select the Infestation folder holding Infest.exe, PreLoad.dat and Overlay.",
            Some(&self.root),
        ) else {
            return;
        };
        self.root = root;
        text(self.controls.root, &self.root.display().to_string());
        text(self.controls.details, "Checking files...");
        self.report = None;
        self.check_files(false);
    }

    unsafe fn open_options(&mut self) {
        if self.options_window != 0 {
            SetFocus(self.options_window);
            return;
        }
        let settings = FrontendAssets::open(&self.root)
            .map_err(|error| error.to_string())
            .and_then(|assets| {
                FrontendSettings::load_or_defaults(FrontendSettings::default_path(), &assets)
                    .map_err(|error| error.to_string())
            });
        let settings = match settings {
            Ok(settings) => settings,
            Err(error) => {
                notice(
                    self.window,
                    "Infestation Options",
                    &format!("The game's settings cannot be read:\n\n{error}"),
                    MB_ICONERROR,
                );
                return;
            }
        };
        let window = match create_top_window(
            OPTIONS_CLASS,
            "Infestation Options",
            OPTIONS_SIZE,
            self.window,
            self.host,
            self.scale,
        ) {
            Ok(window) => window,
            Err(error) => {
                notice(self.window, "Infestation Options", &error, MB_ICONERROR);
                return;
            }
        };
        self.options_window = window;
        let modes = primary_monitor_modes();
        let display = settings.display_selection();
        self.group(window, " Display ", 12, 12, 536, 166);
        self.label(window, "&Display:", 24, 38, 106, 20);
        let display_combo = self.combo(
            window,
            &strings(&DISPLAY_LABELS),
            display.mode.index() as usize,
            ID_DISPLAY,
            136,
            34,
            194,
        );
        self.label(window, "&Resolution:", 24, 72, 106, 20);
        let resolution = self.combo(window, &[], 0, ID_RESOLUTION, 136, 68, 194);
        fill_resolutions(resolution, &modes, display);
        self.label(window, "Re&nderer:", 24, 106, 106, 20);
        let renderer = self.combo(
            window,
            &strings(&RENDERERS.map(|(_, label)| label)),
            RENDERERS
                .iter()
                .position(|&(choice, _)| choice == settings.renderer())
                .unwrap_or(0),
            ID_RENDERER,
            136,
            102,
            194,
        );
        self.label(window, "Image &scaling:", 24, 140, 106, 20);
        let scaling = self.combo(
            window,
            &strings(&SCALINGS.map(|(_, label)| label)),
            SCALINGS
                .iter()
                .position(|&(choice, _)| choice == settings.scaling())
                .unwrap_or(0),
            ID_SCALING,
            136,
            136,
            194,
        );
        self.label(window, "&Filtering:", 346, 38, 70, 20);
        let filtering = self.combo(window, &[], 0, ID_FILTERING, 420, 34, 112);
        let saved_filtering = settings.words()[inf_game::frontend_video::FILTERING_CELL];
        fill_filtering(filtering, settings.renderer(), saved_filtering);
        self.label(
            window,
            "Display settings apply when Play starts the game. Widescreen shows more of the world at the sides.",
            346,
            72,
            186,
            96,
        );
        // The Vulkan renderer's modern mode.
        let modern = settings.modern();
        let support = *self.modern_support.get_or_insert_with(|| {
            inf_game::vk_device::VkDevice::probe_modern_support().unwrap_or_default()
        });
        let vulkan_group = self.group(window, " Vulkan ", 12, 186, 536, 178);
        let vulkan_note = self.label(
            window,
            "Vanilla (OpenGL) draws Infestation as the original did. Remastered (Vulkan) \
             adds traced light, reflections, water, fog and thermal vision, as your \
             graphics card allows; these rows apply to it only. Ray tracing switched in \
             game affects gameplay from the next session.",
            24,
            206,
            512,
            52,
        );
        let ray_tracing_label = self.label(window, "Ray &tracing:", 24, 268, 106, 20);
        let ray_tracing = self.combo(
            window,
            &strings(&["Off", "Path tracing"][..if support.ray_tracing { 2 } else { 1 }]),
            usize::from(modern.ray_tracing && support.ray_tracing),
            ID_RAY_TRACING,
            136,
            264,
            130,
        );
        let upscaler_label = self.label(window, "&Upscaler:", 286, 268, 96, 20);
        let upscaler = self.combo(window, &[], 0, ID_UPSCALER, 386, 264, 146);
        fill_upscalers(upscaler, modern.ray_tracing, modern.upscaler, support);
        let upscaling_label = self.label(window, "Upscal&ing:", 24, 302, 106, 20);
        let upscaling = self.combo(
            window,
            &strings(&UPSCALE_QUALITIES.map(|(_, label)| label)),
            UPSCALE_QUALITIES
                .iter()
                .position(|&(quality, _)| quality == modern.quality)
                .unwrap_or(0),
            ID_UPSCALING,
            136,
            298,
            130,
        );
        let anti_aliasing_label = self.label(window, "&Anti-aliasing:", 286, 302, 96, 20);
        let anti_aliasing = self.combo(
            window,
            &strings(&ANTI_ALIASINGS.map(|(_, label)| label)),
            ANTI_ALIASINGS
                .iter()
                .position(|&(choice, _)| choice == modern.anti_aliasing)
                .unwrap_or(0),
            ID_ANTI_ALIASING,
            386,
            298,
            146,
        );
        let lighting_label = self.label(window, "&Lighting:", 24, 336, 106, 20);
        let lighting = self.combo(
            window,
            &strings(&LIGHTINGS.map(|(_, label)| label)),
            LIGHTINGS
                .iter()
                .position(|&(preset, _)| preset == modern.lighting)
                .unwrap_or(0),
            ID_LIGHTING,
            136,
            332,
            130,
        );
        // Words +00 and +04: the effects and music volumes, 0..15.
        let words = settings.words();
        let sound_group = self.group(window, " Sound, controller and camera ", 12, 372, 536, 126);
        let effects_label = self.label(window, "&Effects volume:", 24, 398, 106, 20);
        let effects = self.slider(window, words[0], 15, ID_EFFECTS, 130, 392, 84);
        let music_label = self.label(window, "&Music volume:", 222, 398, 92, 20);
        let music = self.slider(window, words[1], 15, ID_MUSIC, 314, 392, 84);
        // The placed sounds for each ear (Vulkan).
        let headphones_label = self.label(window, "&Headphones:", 406, 398, 76, 20);
        let headphones = self.combo(
            window,
            &strings(&["Off", "On"]),
            usize::from(modern.headphones),
            ID_HEADPHONES,
            482,
            392,
            58,
        );
        let controller_label = self.label(window, "&Controller layout:", 24, 432, 106, 20);
        let controller = self.combo(
            window,
            &strings(&CONTROLLER_LAYOUTS.map(|(_, label)| label)),
            CONTROLLER_LAYOUTS
                .iter()
                .position(|&(layout, _)| layout == settings.controller_layout())
                .unwrap_or(0),
            ID_CONTROLLER,
            136,
            428,
            240,
        );
        // The sound the port adds, on Vulkan only.
        let ambience_label = self.label(window, "Am&bience:", 390, 432, 64, 20);
        let ambience = self.slider(
            window,
            u32::from(modern.ambience),
            u32::from(inf_game::modern_settings::MOST_AMBIENCE),
            ID_AMBIENCE,
            454,
            426,
            86,
        );
        // Retail's Normal and Cinematic, and the port's Remastered: the camera
        // that rises over what blocks it, swings round while reversing and
        // sees wider (on every renderer).
        let camera_label = self.label(window, "Camera st&yle:", 24, 466, 106, 20);
        let camera = self.combo(
            window,
            &strings(&CAMERA_STYLES),
            (words[inf_game::camera_clearance::CAMERA_STYLE_CELL] as usize)
                .min(CAMERA_STYLES.len() - 1),
            ID_CAMERA,
            136,
            462,
            240,
        );
        let fixes_group = self.group(
            window,
            " Fixes for retail bugs (off restores retail) ",
            12,
            506,
            536,
            282,
        );
        let cheats_group = self.group(
            window,
            " Cheats and extras (off is retail) ",
            12,
            796,
            536,
            100,
        );
        let values = fix_values(settings.fixes());
        let mut fixes = [0; FIXES];
        for (index, control) in fixes.iter_mut().enumerate() {
            let y = if index < BUG_FIX_ROWS {
                528 + 25 * index as i32
            } else {
                818 + 25 * (index - BUG_FIX_ROWS) as i32
            };
            *control = self.checkbox(
                window,
                FIX_LABELS[index],
                ID_FIXES + index,
                24,
                y,
                512,
                values[index],
            );
        }
        let skip = self.checkbox(
            window,
            "S&kip the launcher while this installation is ready",
            ID_SKIP,
            16,
            904,
            528,
            self.request.preferences.skip_launcher,
        );
        let skip_note = self.label(
            window,
            "Start with --launcher to open it again. Missing game files still open it.",
            34,
            930,
            510,
            20,
        );
        let progress = self.label(window, "", 16, 958, 528, 20);
        let verify = self.button(window, "&Verify files", ID_VERIFY, 12, 986, 160, 32);
        let save = self.button(window, "Save", ID_SAVE, 308, 986, 112, 32);
        let cancel = self.button(window, "Cancel", ID_CANCEL, 436, 986, 112, 32);
        self.option_controls = OptionControls {
            display: display_combo,
            resolution,
            renderer,
            filtering,
            ray_tracing,
            upscaler,
            upscaling,
            anti_aliasing,
            lighting,
            scaling,
            effects,
            music,
            ambience,
            headphones,
            controller,
            camera,
            fixes,
            skip,
            progress,
            verify,
            save,
            cancel,
            vulkan_group,
            vulkan_note,
            vulkan_labels: [
                ray_tracing_label,
                upscaler_label,
                upscaling_label,
                anti_aliasing_label,
                lighting_label,
            ],
            sound_group,
            effects_label,
            music_label,
            headphones_label,
            controller_label,
            ambience_label,
            camera_label,
            fixes_group,
            cheats_group,
            skip_note,
        };
        Self::refresh_pelagia_row(&self.option_controls.fixes);
        layout_modern_rows(&self.option_controls, window, self.scale);
        self.options = Some(PendingOptions {
            settings,
            modes,
            display,
            filtering: saved_filtering,
            upscaler: modern.upscaler,
            support,
        });
        self.set_busy(self.busy);
        EnableWindow(self.window, 0);
        show_interactive_window(window);
        SetFocus(self.option_controls.display);
    }

    /// Display or Resolution changed: update the pending choice. A new
    /// Display lists its own resolutions, as the Video menu's row does.
    unsafe fn display_choice_changed(&mut self, id: usize) {
        let controls = &self.option_controls;
        let (window, scale) = (self.options_window, self.scale);
        let Some(pending) = self.options.as_mut() else {
            return;
        };
        match id {
            ID_DISPLAY => {
                let mode = DisplayMode::from_index(combo_selection(controls.display) as u32);
                pending.display = pending.modes.select_mode(pending.display, mode);
                fill_resolutions(controls.resolution, &pending.modes, pending.display);
            }
            ID_RESOLUTION => {
                let sizes = pending.modes.menu_sizes(pending.display.mode);
                // The combo lists largest first; the ascending row underneath
                // stays as the game steps it.
                if let Some(&size) = sizes.iter().rev().nth(combo_selection(controls.resolution)) {
                    pending.display.adopt(pending.display.mode, size);
                }
            }
            // The Filtering list follows the renderer; the choice stays.
            ID_RENDERER => {
                // First sight of Vulkan: SMAA and Anisotropic 8x, unless the
                // player already picked Vulkan levels (2026-10-27).
                // Path tracing and Remaster lighting are already the fresh
                // defaults, and a saved Off stays saved.
                if renderer_choice(controls.renderer) == RendererChoice::Vulkan {
                    if pending.filtering <= inf_game::frontend_video::FILTERING_RETAIL_MOST {
                        pending.filtering = inf_game::frontend_video::FILTERING_FRESH_DEFAULT;
                    }
                    let smaa = ANTI_ALIASINGS
                        .iter()
                        .position(|&(choice, _)| {
                            choice == inf_game::modern_settings::AntiAliasing::Smaa
                        })
                        .unwrap_or(0);
                    if ANTI_ALIASINGS
                        .get(combo_selection(controls.anti_aliasing))
                        .is_none_or(|&(choice, _)| {
                            choice == inf_game::modern_settings::AntiAliasing::Off
                        })
                    {
                        SendMessageW(controls.anti_aliasing, CB_SETCURSEL, smaa, 0);
                    }
                }
                fill_filtering(
                    controls.filtering,
                    renderer_choice(controls.renderer),
                    pending.filtering,
                );
                layout_modern_rows(controls, window, scale);
            }
            // Ray Reconstruction is listed only while Ray Tracing is on.
            ID_RAY_TRACING => {
                fill_upscalers(
                    controls.upscaler,
                    combo_selection(controls.ray_tracing) != 0,
                    pending.upscaler,
                    pending.support,
                );
                layout_modern_rows(controls, window, scale);
            }
            ID_UPSCALER => {
                let listed =
                    listed_upscalers(combo_selection(controls.ray_tracing) != 0, pending.support);
                pending.upscaler =
                    listed[combo_selection(controls.upscaler).min(listed.len() - 1)].0;
                layout_modern_rows(controls, window, scale);
            }
            ID_FILTERING => pending.filtering = combo_selection(controls.filtering) as u32,
            _ => {}
        }
    }

    unsafe fn close_options(&mut self) {
        if self.options_window != 0 {
            // Enable the owner first, so Windows activates it rather than
            // another application when the owned window goes.
            EnableWindow(self.window, 1);
            DestroyWindow(self.options_window);
            self.options_window = 0;
            self.option_controls = OptionControls::default();
            self.options = None;
            SetFocus(self.controls.options);
        }
    }

    unsafe fn save_options(&mut self) {
        let controls = &self.option_controls;
        let Some(pending) = self.options.as_mut() else {
            return;
        };
        let settings = &mut pending.settings;
        settings.set_desktop(pending.modes.desktop);
        settings.set_display_selection(pending.display);
        settings.set_renderer(renderer_choice(controls.renderer));
        settings.words_mut()[inf_game::frontend_video::FILTERING_CELL] = pending.filtering;
        settings.words_mut()[inf_game::camera_clearance::CAMERA_STYLE_CELL] =
            combo_selection(controls.camera).min(CAMERA_STYLES.len() - 1) as u32;
        settings.set_modern(inf_game::modern_settings::ModernSettings {
            // A GPU that cannot trace keeps the saved choice for one that can.
            ray_tracing: if pending.support.ray_tracing {
                combo_selection(controls.ray_tracing) != 0
            } else {
                settings.modern().ray_tracing
            },
            upscaler: pending.upscaler,
            quality: UPSCALE_QUALITIES
                [combo_selection(controls.upscaling).min(UPSCALE_QUALITIES.len() - 1)]
            .0,
            anti_aliasing: ANTI_ALIASINGS
                [combo_selection(controls.anti_aliasing).min(ANTI_ALIASINGS.len() - 1)]
            .0,
            lighting: LIGHTINGS[combo_selection(controls.lighting).min(LIGHTINGS.len() - 1)].0,
            ultra_ground: false,
            ambience: slider_position(controls.ambience)
                .min(u32::from(inf_game::modern_settings::MOST_AMBIENCE))
                as u8,
            headphones: combo_selection(controls.headphones) != 0,
            puddle_wheels: settings.modern().puddle_wheels,
        });
        settings.set_scaling(SCALINGS[combo_selection(controls.scaling).min(SCALINGS.len() - 1)].0);
        settings.set_controller_layout(
            CONTROLLER_LAYOUTS
                [combo_selection(controls.controller).min(CONTROLLER_LAYOUTS.len() - 1)]
            .0,
        );
        let words = settings.words_mut();
        words[0] = slider_position(controls.effects).min(15);
        words[1] = slider_position(controls.music).min(15);
        settings.set_fixes(fixes_from(controls.fixes.map(|control| checked(control))));
        if let Err(error) = settings.persist() {
            notice(
                self.options_window,
                "Save Infestation options",
                &format!("The game's settings could not be saved:\n\n{error}"),
                MB_ICONERROR,
            );
            return;
        }
        self.request.preferences.skip_launcher = checked(controls.skip);
        if let Err(error) = self
            .request
            .preferences
            .save(&self.request.preferences_path)
        {
            notice(
                self.options_window,
                "Save Infestation options",
                &format!(
                    "The game's settings were saved, but the launcher's preference could not be:\n\n{error}"
                ),
                MB_ICONERROR,
            );
            return;
        }
        self.close_options();
        self.set_progress("Options saved.");
    }
}

/// The Vulkan rows show under Vulkan only. While shown, Upscaling picks
/// the size an upscaler brings up, traced or not; Anti-aliasing has nothing
/// while an upscaler brings its own; Lighting and Ultra ground are the
/// traced world's. Otherwise the Vulkan group hides, the Headphones and
/// Ambience rows hide, everything below lifts by the group's height and the
/// window shrinks by it; hidden choices stay saved.
unsafe fn layout_modern_rows(controls: &OptionControls, window: Handle, scale: f64) {
    let vulkan = renderer_choice(controls.renderer) == RendererChoice::Vulkan;
    let upscaling = combo_selection(controls.upscaler) != 0;
    for control in [
        controls.ray_tracing,
        controls.upscaler,
        controls.lighting,
        controls.ambience,
        controls.headphones,
    ] {
        EnableWindow(control, i32::from(vulkan));
    }
    // Upscaling picks the size an upscaler brings up: none without one.
    EnableWindow(controls.upscaling, i32::from(vulkan && upscaling));
    EnableWindow(controls.anti_aliasing, i32::from(vulkan && !upscaling));
    let show = if vulkan { SW_SHOW } else { SW_HIDE };
    for control in [
        controls.vulkan_group,
        controls.vulkan_note,
        controls.headphones_label,
        controls.headphones,
        controls.ambience_label,
        controls.ambience,
    ]
    .into_iter()
    .chain(controls.vulkan_labels)
    .chain([
        controls.ray_tracing,
        controls.upscaler,
        controls.upscaling,
        controls.anti_aliasing,
        controls.lighting,
    ]) {
        ShowWindow(control, show);
    }
    let lift = if vulkan { 0 } else { VULKAN_GROUP_LIFT };
    // Everything below the Vulkan group, at its built coordinates.
    for (control, x, y) in [
        (controls.sound_group, 12, 372),
        (controls.effects_label, 24, 398),
        (controls.effects, 130, 392),
        (controls.music_label, 222, 398),
        (controls.music, 314, 392),
        (controls.controller_label, 24, 432),
        (controls.controller, 136, 428),
        (controls.camera_label, 24, 466),
        (controls.camera, 136, 462),
        (controls.fixes_group, 12, 506),
        (controls.cheats_group, 12, 796),
        (controls.skip, 16, 904),
        (controls.skip_note, 34, 930),
        (controls.progress, 16, 958),
        (controls.verify, 12, 986),
        (controls.save, 308, 986),
        (controls.cancel, 436, 986),
    ] {
        SetWindowPosW(
            control,
            0,
            pixel(scale, x),
            pixel(scale, y - lift),
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    for (index, control) in controls.fixes.iter().enumerate() {
        let y = if index < BUG_FIX_ROWS {
            528 + 25 * index as i32
        } else {
            818 + 25 * (index - BUG_FIX_ROWS) as i32
        };
        SetWindowPosW(
            *control,
            0,
            pixel(scale, 24),
            pixel(scale, y - lift),
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let mut bounds = Rect {
        left: 0,
        top: 0,
        right: pixel(scale, OPTIONS_SIZE[0]),
        bottom: pixel(scale, OPTIONS_SIZE[1] - lift),
    };
    AdjustWindowRectEx(&mut bounds, WINDOW_STYLE, 0, EX_CONTROL_PARENT);
    SetWindowPosW(
        window,
        0,
        0,
        0,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

fn pixel(scale: f64, logical: i32) -> i32 {
    (f64::from(logical) * scale).round() as i32
}

/// Fill the Upscaler list for a world `traced` or not, showing `choice` as
/// [`shown_upscaler`] does.
unsafe fn fill_upscalers(
    control: Handle,
    traced: bool,
    choice: inf_game::modern_settings::Upscaler,
    support: inf_game::modern_settings::ModernSupport,
) {
    let listed = listed_upscalers(traced, support);
    SendMessageW(control, CB_RESETCONTENT, 0, 0);
    for (_, label) in &listed {
        SendMessageW(control, CB_ADDSTRING, 0, wide(*label).as_ptr() as isize);
    }
    let shown = shown_upscaler(traced, choice, support);
    let index = listed
        .iter()
        .position(|&(listed, _)| listed == shown)
        .unwrap_or(0);
    SendMessageW(control, CB_SETCURSEL, index, 0);
}

unsafe fn renderer_choice(control: Handle) -> RendererChoice {
    RENDERERS[combo_selection(control).min(RENDERERS.len() - 1)].0
}

/// The Filtering list `renderer` draws (Point and Bilinear, and with
/// Vulkan Trilinear and Anisotropic 2x to 16x), showing `filtering` as the
/// renderer draws it: a Vulkan choice under another renderer shows as
/// Bilinear and stays the pending choice.
unsafe fn fill_filtering(control: Handle, renderer: RendererChoice, filtering: u32) {
    use inf_game::frontend_video::{filtering_text, FILTERING_RETAIL_MOST, FILTERING_VULKAN_MOST};
    let most = if renderer == RendererChoice::Vulkan {
        FILTERING_VULKAN_MOST
    } else {
        FILTERING_RETAIL_MOST
    };
    SendMessageW(control, CB_RESETCONTENT, 0, 0);
    for level in 0..=most {
        // The English text, Latin-1 bytes as the game's.
        let label: String = filtering_text(level, 0)
            .into_iter()
            .map(char::from)
            .collect();
        SendMessageW(control, CB_ADDSTRING, 0, wide(label).as_ptr() as isize);
    }
    SendMessageW(control, CB_SETCURSEL, filtering.min(most) as usize, 0);
}

/// The Resolution row for `display`'s mode, with its entry selected: the
/// size itself when listed, else the entry the game would start with.
/// Borderless shows the desktop it covers and cannot change.
unsafe fn fill_resolutions(control: Handle, modes: &MonitorModes, display: DisplaySelection) {
    SendMessageW(control, CB_RESETCONTENT, 0, 0);
    let sizes = modes.menu_sizes(display.mode);
    // Largest first, so 4K sits on top; the game still steps the ascending
    // row underneath.
    for [width, height] in sizes.iter().rev() {
        SendMessageW(
            control,
            CB_ADDSTRING,
            0,
            wide(format!("{width} x {height}")).as_ptr() as isize,
        );
    }
    let current = display.resolution(modes.desktop);
    let selected = sizes
        .iter()
        .rev()
        .position(|&size| size == current)
        .or_else(|| {
            let resolved = modes.resolve(display.mode, current)?;
            sizes.iter().rev().position(|&size| size == resolved)
        });
    if let Some(index) = selected {
        SendMessageW(control, CB_SETCURSEL, index, 0);
    }
    EnableWindow(
        control,
        i32::from(display.mode != DisplayMode::Borderless && !sizes.is_empty()),
    );
}

/// What the primary monitor offers; the game opens there unless its Video
/// menu chose another. Its 15- to 32-bit modes and the desktop, in physical
/// pixels whatever the thread's DPI awareness.
unsafe fn primary_monitor_modes() -> MonitorModes {
    let mut monitor: MonitorInfo = zeroed();
    monitor.size = size_of::<MonitorInfo>() as u32;
    let primary = MonitorFromPoint(Point::default(), MONITOR_DEFAULTTOPRIMARY);
    if GetMonitorInfoW(primary, &mut monitor) == 0 {
        return MonitorModes::default();
    }
    let device = monitor.device.as_ptr();
    let mode = |index: u32| {
        let mut settings: DevMode = zeroed();
        settings.size = size_of::<DevMode>() as u16;
        (EnumDisplaySettingsW(device, index, &mut settings) != 0).then_some(settings)
    };
    let desktop =
        mode(ENUM_CURRENT_SETTINGS).map(|settings| [settings.pels_width, settings.pels_height]);
    let reported = (0..)
        .map_while(mode)
        .filter(|settings| matches!(settings.bits_per_pel, 15 | 16 | 24 | 32))
        .map(|settings| [settings.pels_width, settings.pels_height]);
    MonitorModes::new(desktop, reported)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

/// An empty directory has many issues; keep the message box on screen. The
/// main window's scrolling list keeps them all.
fn popup_description(report: &InstallationReport) -> String {
    let description = report.description();
    let lines: Vec<&str> = description.lines().collect();
    if lines.len() <= 12 {
        description.replace('\n', "\r\n")
    } else {
        format!(
            "{}\r\n\r\n{} more lines are in the main window's list.",
            lines[..12].join("\r\n"),
            lines.len() - 12
        )
    }
}

unsafe fn combo_selection(control: Handle) -> usize {
    SendMessageW(control, CB_GETCURSEL, 0, 0).max(0) as usize
}

unsafe fn slider_position(control: Handle) -> u32 {
    SendMessageW(control, TBM_GETPOS, 0, 0).max(0) as u32
}

unsafe fn checked(control: Handle) -> bool {
    SendMessageW(control, BM_GETCHECK, 0, 0) == 1
}

unsafe extern "system" fn browse_callback(
    window: Handle,
    message: u32,
    _lparam: isize,
    initial: isize,
) -> i32 {
    // BFFM_INITIALIZED: select the initial folder (BFFM_SETSELECTIONW).
    if message == 1 && initial != 0 {
        SendMessageW(window, 0x0467, 1, initial);
    }
    0
}

unsafe fn choose_folder(owner: Handle, prompt: &str, initial: Option<&Path>) -> Option<PathBuf> {
    let title = wide(prompt);
    let initial = initial.map(wide);
    let mut display_name = vec![0u16; 32768];
    let mut info = BrowseInfo {
        owner,
        root: null(),
        display_name: display_name.as_mut_ptr(),
        title: title.as_ptr(),
        // File-system folders, an edit box and the resizable dialog.
        flags: 0x0051,
        callback: Some(browse_callback),
        parameter: initial.as_ref().map_or(0, |value| value.as_ptr() as isize),
        image: 0,
    };
    let id = SHBrowseForFolderW(&mut info);
    redraw_after_dialog(owner);
    if id.is_null() {
        return None;
    }
    let mut path = vec![0u16; 32768];
    let success = SHGetPathFromIDListEx(id, path.as_mut_ptr(), path.len() as u32, 0);
    CoTaskMemFree(id);
    if success == 0 {
        return None;
    }
    let length = path
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(path.len());
    Some(PathBuf::from(OsString::from_wide(&path[..length])))
}
