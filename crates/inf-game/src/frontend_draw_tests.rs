use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::frontend_camera::FrontendEntry;
use crate::menu::{MenuActionRequest, MenuData, MenuHost, MenuStack, MenuVisibility};
use std::path::Path;

#[derive(Default)]
struct Host {
    setup: usize,
    rows: Vec<MenuRowDrawRequest>,
    fail_row: bool,
    restored: usize,
}
impl MenuDrawHost for Host {
    type Error = &'static str;
    fn begin_controller(&mut self, _: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.setup += 1;
        Ok(())
    }
    fn publish_menu_basis(&mut self, _: MenuBasis) -> Result<(), Self::Error> {
        Ok(())
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Ok([0, 0, 800, 600])
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        self.restored += usize::from(clip == [0, 0, 800, 600]);
        Ok(())
    }
    fn colors(&self) -> MenuDrawColors {
        MenuDrawColors {
            selected_start: 0x0080_8080,
            ordinary: 0x00c8_c8c8,
            selected_end: 0x00ff_ffff,
        }
    }
    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        _: &mut MenuState,
    ) -> Result<(), Self::Error> {
        self.rows.push(*request);
        request.descriptor.position_pixels[1] += 60;
        if self.fail_row {
            Err("renderer unavailable")
        } else {
            Ok(())
        }
    }
}
impl MenuHost for Host {
    type Error = &'static str;
    fn action(&mut self, _: MenuActionRequest, _: &mut MenuStack) -> Result<(), Self::Error> {
        Err("unexpected action")
    }
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, _: i32) -> Result<(), Self::Error> {
        Err("unexpected rotation before first draw")
    }
}

fn assets() -> FrontendAssets {
    FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap()
}

fn control(assets: &FrontendAssets, entry: FrontendEntry) -> FrontendControl {
    let mut control = FrontendControl::new(FrontendControlConfig {
        camera_presets: assets.camera_presets().to_vec(),
        position_scale: assets.position_scale(),
        motion_scale: assets.motion_scale(),
        entry,
        context: 71,
    })
    .unwrap();
    control.menus_mut().push(MenuState::new(
        MenuData::new(assets.root_menu().clone()),
        MenuBasis::INITIAL,
    ));
    control
}

fn draw_input(assets: &FrontendAssets) -> MenuDrawInput<'_> {
    MenuDrawInput {
        surface_pixels: [800, 600],
        visibility: MenuVisibility::OwnerFlagClear,
        sine_quarter: &assets.sine_table().values,
    }
}

#[test]
fn retail_root_executes_wrapper_and_real_height_feedback_through_frontend_draw() {
    let assets = assets();
    let mut control = control(&assets, FrontendEntry::Returning);
    let mut host = Host::default();
    control.menus_mut().tick(16_000, &mut host).unwrap();
    let before = control.menus().current().unwrap().data().records().items[0].row_height;
    let result = control
        .draw_menu(draw_input(&assets), &mut host)
        .unwrap()
        .unwrap();
    assert!(result.menu_drawn);
    assert_eq!(result.rows, host.rows.len());
    assert_eq!(host.rows[0].callback_address, 0x4834f0);
    assert_eq!(assets.root_menu().items[0].draw_callback, 0x4835e0);
    assert_ne!(
        control.menus().current().unwrap().data().records().items[0].row_height,
        before
    );
    assert!(control.menus().drawn());
    assert_eq!(control.camera().selected_preset(), 0);
    assert_eq!(control.entry_state(), FrontendEntry::Returning);
    assert!(host.restored > 0);
}

#[test]
fn row_target_change_does_not_recheck_gate_and_unready_camera_skips_entire_backend() {
    let assets = assets();
    let mut control = control(&assets, FrontendEntry::Returning);
    let mut host = Host::default();
    control.menus_mut().tick(16_000, &mut host).unwrap();
    // This authored wrapper dispatches to the ordinary header renderer and
    // selects its second argument before the backend runs. Mutate only fixture
    // records to exercise it in the first visible row.
    {
        let menu = control.menus().current().unwrap();
        let mut records = menu.data().records_mut();
        records.items[0].draw_callback = 0x45f520;
        records.items[0].draw_arguments[1] = 9;
    }
    control.state.returning.set(false);
    control.state.accelerated = true;
    let result = control
        .draw_menu(draw_input(&assets), &mut host)
        .unwrap()
        .unwrap();
    assert!(result.rows > 1);
    assert_eq!(host.rows[0].callback_address, 0x453a80);
    assert_eq!(control.camera().selected_preset(), 9);
    assert_eq!(control.timer_0120_millis(), 1);
    assert!(!control.camera().ready());
    assert_eq!(control.entry_state(), FrontendEntry::Returning);
    assert!(!control.accelerated());
    let setup_count = host.setup;
    assert_eq!(
        control.draw_menu(draw_input(&assets), &mut host).unwrap(),
        None
    );
    assert_eq!(host.setup, setup_count);
}

#[test]
fn failed_backend_keeps_prior_camera_effect_but_never_publishes_draw_success() {
    let assets = assets();
    let mut control = control(&assets, FrontendEntry::Returning);
    let mut host = Host {
        fail_row: true,
        ..Host::default()
    };
    control.menus_mut().tick(16_000, &mut host).unwrap();
    {
        let menu = control.menus().current().unwrap();
        let mut records = menu.data().records_mut();
        records.items[0].draw_callback = 0x45f520;
        records.items[0].draw_arguments[1] = 9;
    }
    control.state.returning.set(false);
    assert!(matches!(
        control.draw_menu(draw_input(&assets), &mut host),
        Err(FrontendControlError::Host("renderer unavailable"))
    ));
    assert_eq!(control.camera().selected_preset(), 9);
    assert_eq!(control.entry_state(), FrontendEntry::First);
    assert!(control.stopped());
    assert!(!control.menus().drawn());
    assert_eq!(host.rows.len(), 1);
    assert!(host.restored > 0);
}
