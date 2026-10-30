use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    profile_menu::{
        ProfileMenuError, ProfileMenuServices, ProfileMenuState, ProfileMenuTemplates, ProfileTimer,
    },
    profile_store::ProfileStore,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Probe {
    store: ProfileStore,
    calls: Vec<&'static str>,
    clips: Vec<[i16; 4]>,
    rows: Vec<MenuRowDrawRequest>,
    fail: Option<&'static str>,
    code: u32,
}

impl Probe {
    fn call(&mut self, name: &'static str) -> Result<(), u32> {
        self.calls.push(name);
        if self.fail == Some(name) {
            Err(self.code)
        } else {
            Ok(())
        }
    }
}

fn fixture(warning: bool) -> (MenuStack, Probe) {
    let assets = FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"))
        .expect("retail data required");
    let templates = ProfileMenuTemplates::parse(assets.executable_bytes()).unwrap();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../.tmp/profile-stack-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    let store = ProfileStore::open(directory.join("profiles.sav")).unwrap();
    let mut stack = MenuStack::new(MenuControllerSpec {
        fade_rate: 65535,
        viewport: [0, 0, 1024, 1024],
    });
    stack.push(ProfileMenu::new(templates, MenuBasis::INITIAL, warning));
    stack.fade = 65535;
    (
        stack,
        Probe {
            store,
            calls: vec![],
            clips: vec![],
            rows: vec![],
            fail: None,
            code: 0x777,
        },
    )
}

fn input(table: &[u16; 4096]) -> MenuDrawInput<'_> {
    MenuDrawInput {
        surface_pixels: [1024, 1024],
        visibility: MenuVisibility::OwnerFlagClear,
        sine_quarter: table,
    }
}

impl MenuDrawHost for Probe {
    type Error = u32;
    fn begin_controller(&mut self, _: MenuControllerDrawSetup) -> Result<(), u32> {
        self.call("controller")
    }
    fn publish_menu_basis(&mut self, _: MenuBasis) -> Result<(), u32> {
        self.call("basis")
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], u32> {
        self.call("read clip")?;
        Ok([-10, -20, 900, 700])
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), u32> {
        self.clips.push(clip);
        self.call("clip")
    }
    fn colors(&self) -> MenuDrawColors {
        MenuDrawColors {
            selected_start: 0,
            ordinary: 0,
            selected_end: 0,
        }
    }
    fn draw_row(&mut self, _: &mut MenuRowDrawRequest, _: &mut MenuState) -> Result<(), u32> {
        panic!("derived rows must retain their callback owner")
    }
    fn begin_profile_draw(&mut self) -> Result<(), MenuError<u32>> {
        self.call("resources").map_err(MenuError::Host)
    }
    fn profile_store(&mut self) -> Result<&mut ProfileStore, MenuError<u32>> {
        self.call("store").map_err(MenuError::Host)?;
        Ok(&mut self.store)
    }
    fn draw_profile_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        _: &mut MenuState,
        state: &mut ProfileMenuState,
    ) -> Result<(), MenuError<u32>> {
        self.rows.push(*request);
        self.call("row").map_err(MenuError::Host)?;
        // Deliberate callback observer changes verify shared row feedback and
        // that the adapter passes the actual derived state, not a snapshot.
        request.descriptor.position_pixels[1] += 45;
        state.set_format_state(3);
        Ok(())
    }
    fn draw_profile_status(
        &mut self,
        _: ProfileTimer,
        surface: [i32; 2],
    ) -> Result<(), MenuError<u32>> {
        assert_eq!(surface, [1024, 1024]);
        self.call("status").map_err(MenuError::Host)
    }
}

struct Services<'a> {
    calls: &'a mut Vec<&'static str>,
    active: &'a mut bool,
}
impl ProfileMenuServices for Services<'_> {
    type Error = u32;
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, _: i32) -> Result<(), u32> {
        self.calls.push("rotate");
        Ok(())
    }
    fn reset_profile_input(&mut self) -> Result<(), u32> {
        self.calls.push("reset input");
        Ok(())
    }
    fn select_camera(&mut self, _: i32) -> Result<(), u32> {
        Err(1)
    }
    fn set_transition_flag(&mut self) -> Result<(), u32> {
        Err(2)
    }
    fn request_phase(&mut self, _: u32) -> Result<(), u32> {
        Err(3)
    }
    fn play_sound(&mut self, _: i16) -> Result<(), u32> {
        Err(4)
    }
    fn begin_controller_fade(&mut self) -> Result<(), u32> {
        *self.active = false;
        Ok(())
    }
}

impl MenuHost for Probe {
    type Error = u32;
    fn action(&mut self, _: MenuActionRequest, _: &mut MenuStack) -> Result<(), u32> {
        Err(5)
    }
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, _: i32) -> Result<(), u32> {
        panic!("profile tick must own the base tick call")
    }
    fn tick_profile(
        &mut self,
        profile: &mut ProfileMenu,
        delta: i32,
        active: &mut bool,
    ) -> Result<(), MenuError<u32>> {
        self.calls.push("profile tick");
        let mut services = Services {
            calls: &mut self.calls,
            active,
        };
        profile
            .tick(delta, false, &mut self.store, &mut services)
            .map(|_| ())
            .map_err(|error| match error {
                ProfileMenuError::State(error) => MenuError::State(error),
                ProfileMenuError::Store(error) => MenuError::ProfileStore(error),
                ProfileMenuError::Host(error) => MenuError::Host(error),
                other => panic!("unexpected profile action error: {other:?}"),
            })
    }
}

fn profile(stack: &MenuStack) -> &ProfileMenu {
    let MenuOwner::Profile(profile) = stack.current_owner().unwrap() else {
        panic!("profile owner required")
    };
    profile
}

fn ready_rows(stack: &mut MenuStack, probe: &mut Probe) {
    stack.draw(input(&[0; 4096]), probe).unwrap();
    stack.tick(16_000, probe).unwrap();
    probe.calls.clear();
    probe.clips.clear();
    assert_eq!(stack.current().unwrap().data().records().items.len(), 15);
}

#[test]
fn profile_stack_keeps_real_draw_before_rebuild_and_routes_live_derived_rows() {
    let (mut stack, mut probe) = fixture(false);
    stack.tick(16_000, &mut probe).unwrap();
    assert!(probe.calls.is_empty());
    let first = stack.draw(input(&[0; 4096]), &mut probe).unwrap();
    assert_eq!(first.rows, 0);
    assert!(first.menu_drawn);
    assert_eq!(probe.calls, ["controller", "resources", "store"]);
    assert!(!profile(&stack).state().busy());
    probe.calls.clear();
    stack.tick(16_000, &mut probe).unwrap();
    assert_eq!(probe.calls, ["profile tick", "reset input", "rotate"]);
    assert_eq!(stack.current().unwrap().age_millis(), 0);
    let result = stack.draw(input(&[0; 4096]), &mut probe).unwrap();
    assert!(result.rows > 0);
    assert!(probe.rows.iter().all(|request| matches!(
        request.menu_source,
        MenuSource::Derived {
            constructor: 0x448220,
            vtable: 0x524f28
        }
    )));
    assert_eq!(profile(&stack).state().format_state(), 3);
    assert_eq!(probe.clips.last(), Some(&[-10, -20, 900, 700]));
}

#[test]
fn resource_and_status_failures_leave_first_draw_handshake_unacknowledged() {
    for (failure, expected) in [
        ("resources", vec!["controller", "resources"]),
        ("status", vec!["controller", "resources", "store", "status"]),
    ] {
        let (mut stack, mut probe) = fixture(true);
        probe.fail = Some(failure);
        assert!(matches!(
            stack.draw(input(&[0; 4096]), &mut probe),
            Err(MenuError::Host(0x777))
        ));
        assert_eq!(probe.calls, expected);
        assert!(profile(&stack).state().busy());
        assert!(!stack.drawn());
    }
}

#[test]
fn derived_row_failure_preserves_native_301_clip_asymmetry_and_original_error() {
    for (code, restored) in [(0x301, false), (0x777, true)] {
        let (mut stack, mut probe) = fixture(false);
        ready_rows(&mut stack, &mut probe);
        probe.fail = Some("row");
        probe.code = code;
        assert!(
            matches!(stack.draw_catching(input(&[0; 4096]), &mut probe, |error| Some(*error)), Err(MenuError::Host(found)) if found == code)
        );
        assert_eq!(probe.clips.last() == Some(&[-10, -20, 900, 700]), restored);
        assert!(!probe.calls.contains(&"status"));
    }
}

struct BaseOnly;
impl MenuHost for BaseOnly {
    type Error = u32;
    fn action(&mut self, _: MenuActionRequest, _: &mut MenuStack) -> Result<(), u32> {
        panic!()
    }
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, _: i32) -> Result<(), u32> {
        panic!("no base tick substitution")
    }
}
impl MenuDrawHost for BaseOnly {
    type Error = u32;
    fn begin_controller(&mut self, _: MenuControllerDrawSetup) -> Result<(), u32> {
        Ok(())
    }
    fn publish_menu_basis(&mut self, _: MenuBasis) -> Result<(), u32> {
        panic!()
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], u32> {
        panic!()
    }
    fn set_clip_rect(&mut self, _: [i16; 4]) -> Result<(), u32> {
        panic!()
    }
    fn colors(&self) -> MenuDrawColors {
        panic!()
    }
    fn draw_row(&mut self, _: &mut MenuRowDrawRequest, _: &mut MenuState) -> Result<(), u32> {
        panic!()
    }
}

#[test]
fn unavailable_profile_hooks_are_strict_and_controller_partial_fade_keeps_its_gate() {
    let (mut stack, _) = fixture(false);
    stack.fade = 10;
    assert!(
        !stack
            .draw(input(&[0; 4096]), &mut BaseOnly)
            .unwrap()
            .menu_drawn
    );
    assert!(profile(&stack).state().busy());
    assert!(matches!(
        stack.tick(16_000, &mut BaseOnly),
        Err(MenuError::State(MenuStateError::UnsupportedProfileHook {
            hook: "tick"
        }))
    ));
    assert!(matches!(
        stack.draw(input(&[0; 4096]), &mut BaseOnly),
        Err(MenuError::State(MenuStateError::UnsupportedProfileHook {
            hook: "draw resources"
        }))
    ));
    assert!(profile(&stack).state().busy());
    let (_, active) = stack.current_owner_and_active_mut().unwrap();
    *active = false;
    assert!(!stack.tick(16_000, &mut BaseOnly).unwrap());
    assert!(stack.is_empty());
}
