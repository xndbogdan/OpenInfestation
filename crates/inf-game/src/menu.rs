//! Generic authored menu ownership at 00452310..004537E0.
//!
//! The controller dispatches the base vtable 00525608 and profile vtable
//! 00524F28 through explicit owners. Rendering and native basis rotation require
//! a host. Callback addresses identify dispatch, never callable host pointers.
//! The base controller's table 0052561C destroys through 004533E0/00453400.

use thiserror::Error;

use crate::{
    network_menu::NetworkMenu, profile_menu::ProfileMenu, profile_store::ProfileStoreError,
};

#[path = "menu_data.rs"]
mod data;
#[path = "menu_draw.rs"]
mod draw;
#[path = "menu_navigation.rs"]
mod navigation;
pub use data::{
    MenuData, MenuDerivedData, MenuLayoutSource, MenuLayouts, MenuMetadata, MenuRecords,
    MenuRecordsMut, MenuSource,
};
pub use draw::{
    MenuColor, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuDrawInput,
    MenuDrawOutcome, MenuDrawPlacement, MenuRowDescriptor, MenuRowDrawRequest,
};

/// Native menu+4 uses the common counter/vector layout shared with models.
pub use inf_formats::native_basis::NativeBasis as MenuBasis;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MenuCommand {
    Previous = 0,
    Next = 1,
    AdjustPrevious = 2,
    AdjustNext = 3,
    Activate = 4,
    SelectLast = 5,
    Auxiliary6 = 6,
    Auxiliary7 = 7,
}

/// Authored menu virtual +08 policies, independent of row/layout resources.
/// `0040D560` installs `00520394` for the briefing detail menu; its tick
/// `0040D5F0` has a distinct prefix before the shared `00452470` base. Its
/// destructor (`0040D580`, `0040D5A0`) is the base menu's `00452420`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuTickPolicy {
    #[default]
    Base,
    BriefingDetail,
}

impl MenuCommand {
    pub const fn native(self) -> u32 {
        self as u32
    }

    fn swapped_axes(self) -> Self {
        match self {
            Self::Previous => Self::AdjustPrevious,
            Self::Next => Self::AdjustNext,
            Self::AdjustPrevious => Self::Previous,
            Self::AdjustNext => Self::Next,
            other => other,
        }
    }
}

/// 00453360 derives visibility from gameplay owner+5C2, not network status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuVisibility {
    OwnerFlagClear,
    OwnerFlagSet,
}

impl MenuVisibility {
    pub const fn from_owner_flag(set: bool) -> Self {
        if set {
            Self::OwnerFlagSet
        } else {
            Self::OwnerFlagClear
        }
    }

    pub const fn mask(self) -> u32 {
        match self {
            Self::OwnerFlagClear => 4,
            Self::OwnerFlagSet => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuInput {
    pub command: MenuCommand,
    pub visibility: MenuVisibility,
    /// Native EDX forwarded to the row callback (usually the owning screen).
    pub context: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuActionRequest {
    pub menu_source: MenuSource,
    pub item_index: usize,
    pub callback_address: u32,
    pub command: MenuCommand,
    pub arguments: [u32; 2],
    pub context: u32,
}

pub trait MenuHost {
    type Error;

    /// 00452E20 returns immediately after this call. The action can replace or
    /// remove its menu through `stack`; no old menu reference survives it.
    fn action(
        &mut self,
        request: MenuActionRequest,
        stack: &mut MenuStack,
    ) -> Result<(), Self::Error>;

    /// Execute 00491530 against this persistent native basis, including its
    /// rotation counter/normalization. `MenuBasisMath` supplies the concrete
    /// arithmetic when loaded with the original table; an unimplemented host
    /// must report failure instead of accepting the rotation without execution.
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error>;

    /// Derived virtual +08 at 00449690. The split controller borrow permits
    /// operation 6 to start its fade at the actual callback boundary.
    fn tick_profile(
        &mut self,
        _profile: &mut ProfileMenu,
        _delta_micros: i32,
        _active: &mut bool,
    ) -> Result<(), MenuError<Self::Error>> {
        Err(MenuStateError::UnsupportedProfileHook { hook: "tick" }.into())
    }

    /// The network menus' ticks (`0045FDE0`, `0045FF80`).
    fn tick_network(
        &mut self,
        _network: &mut NetworkMenu,
        _delta_micros: i32,
    ) -> Result<(), MenuError<Self::Error>> {
        Err(MenuStateError::UnsupportedProfileHook {
            hook: "network tick",
        }
        .into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommandOutcome {
    NoMenu,
    NoAction,
    NavigationLocked,
    Selection { previous: i32, current: i32 },
    ActionDispatched,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MenuStateError {
    #[error("menu item index {index} is outside {count} records")]
    ItemOutOfBounds { index: i32, count: usize },
    #[error("menu selection {selection} has no authored layout")]
    MissingLayout { selection: i32 },
    #[error("menu navigation cycled without a selectable item")]
    NoSelectableItem,
    #[error("menu navigation selects record {index} past the {count} items, which native would draw as a row")]
    SelectsAdjacentRecord { index: i32, count: usize },
    #[error("menu record count exceeds native signed 32-bit indexing")]
    RecordCountOverflow,
    #[error("menu layout index {index} is outside the authored records")]
    LayoutOutOfBounds { index: usize },
    #[error("menu draw has a zero {field} divisor")]
    ZeroDrawDivisor { field: &'static str },
    #[error("menu draw signed division overflow")]
    DrawDivisionOverflow,
    #[error("menu draw traversal cycled without reaching a visible row")]
    DrawTraversalCycle,
    #[error("menu host does not implement the required profile {hook} hook")]
    UnsupportedProfileHook { hook: &'static str },
}

#[derive(Debug, Error)]
pub enum MenuError<E> {
    #[error(transparent)]
    State(#[from] MenuStateError),
    #[error("menu host callback failed")]
    Host(E),
    #[error(transparent)]
    ProfileStore(#[from] ProfileStoreError),
}

#[derive(Clone, Debug)]
pub struct MenuState {
    data: MenuData,
    pub basis: MenuBasis,
    selection: i32,
    scroll_start: i32,
    age_millis: i32,
    navigation_locked: bool,
    tick_policy: MenuTickPolicy,
}

impl MenuState {
    /// 00452310 resets shared ages/scroll animation, but preserves row heights.
    /// The selection and scroll index remain instance state.
    pub fn new(data: MenuData, basis: MenuBasis) -> Self {
        let selection = data.records().initial_selection;
        let mut state = Self {
            data,
            basis,
            selection,
            scroll_start: 0,
            age_millis: 0,
            navigation_locked: false,
            tick_policy: MenuTickPolicy::Base,
        };
        for layout in state.data.records_mut().layouts.iter_mut() {
            layout.scroll_animation = 0;
        }
        state.reset_ages();
        state
    }

    /// Select the recovered virtual tick owner at construction. Presentation
    /// resources and mutable layouts retain their existing shared lifetime.
    pub fn with_tick_policy(mut self, policy: MenuTickPolicy) -> Self {
        self.tick_policy = policy;
        self
    }

    pub fn data(&self) -> &MenuData {
        &self.data
    }
    pub fn selection(&self) -> i32 {
        self.selection
    }
    pub fn scroll_start(&self) -> i32 {
        self.scroll_start
    }
    pub fn age_millis(&self) -> i32 {
        self.age_millis
    }
    pub fn navigation_locked(&self) -> bool {
        self.navigation_locked
    }
    pub fn set_navigation_locked(&mut self, locked: bool) {
        self.navigation_locked = locked;
    }

    /// 00449410 rebinds derived rows without reconstructing the base owner.
    /// Basis, age and shared layout animation survive. Exceptional rebuilds
    /// leave +50 alone; the ordinary profile list explicitly writes it.
    pub(crate) fn replace_data(
        &mut self,
        data: MenuData,
        selection: i32,
        scroll_start: Option<i32>,
    ) {
        self.data = data;
        self.selection = selection;
        if let Some(scroll_start) = scroll_start {
            self.scroll_start = scroll_start;
        }
    }

    /// 00452440 does not reset selection, scroll start, row heights, or the
    /// layout animation word. Camera movement and stack pop both call it.
    pub fn reset_ages(&mut self) {
        self.age_millis = 0;
        for item in self.data.records_mut().items.iter_mut() {
            item.age = 0;
        }
    }

    fn prepare_command(&mut self, input: MenuInput) -> Result<PreparedCommand, MenuStateError> {
        let shared = self.data.clone();
        let mut definition = shared.records_mut();
        if !definition.has_items() {
            return Ok(PreparedCommand::Complete(MenuCommandOutcome::NoAction));
        }
        let command = if definition.flags & 1 != 0 {
            input.command.swapped_axes()
        } else {
            input.command
        };
        let previous = self.selection;
        match command {
            MenuCommand::Previous | MenuCommand::Next => {
                if self.navigation_locked {
                    return Ok(PreparedCommand::Complete(
                        MenuCommandOutcome::NavigationLocked,
                    ));
                }
                navigation::navigate(self, &mut definition, command, input.visibility)?;
            }
            MenuCommand::SelectLast => {
                self.scroll_start = 0;
                self.selection = i32::try_from(definition.items.len())
                    .map_err(|_| MenuStateError::RecordCountOverflow)?
                    - 1;
            }
            _ => {
                let index = navigation::item_index(self.selection, definition.items.len())?;
                let item = &definition.items[index];
                if item.action_callback == 0 {
                    return Ok(PreparedCommand::Complete(MenuCommandOutcome::NoAction));
                }
                // No visibility/nonselectable/lock check on the action path.
                return Ok(PreparedCommand::Action(MenuActionRequest {
                    menu_source: definition.source,
                    item_index: index,
                    callback_address: item.action_callback,
                    command,
                    arguments: item.action_arguments,
                    context: input.context,
                }));
            }
        }
        Ok(PreparedCommand::Complete(MenuCommandOutcome::Selection {
            previous,
            current: self.selection,
        }))
    }

    /// 00452470: signed microseconds, per-call truncated milliseconds (there is
    /// no fractional accumulator), then persistent basis rotation.
    pub(crate) fn tick<H: MenuHost>(
        &mut self,
        delta_micros: i32,
        host: &mut H,
    ) -> Result<(), MenuError<H::Error>> {
        if self.tick_policy == MenuTickPolicy::BriefingDetail {
            // 0040D5F0: the DWORD +44 age advances before both bytes of each
            // layout's signed WORD +0E are cleared. 00452470 then advances
            // +44 again, while row ages and basis rotation execute once.
            self.age_millis = self.age_millis.wrapping_add(delta_micros / 1000);
            for layout in self.data.records_mut().layouts.iter_mut() {
                layout.scroll_animation = 0;
            }
        }
        self.tick_with_rotation(delta_micros, |basis, angle| {
            host.rotate_menu_basis(basis, angle)
        })
        .map_err(MenuError::Host)
    }

    /// Shared 00452470 base phase, also called directly by 00449690. This
    /// deliberately excludes the selected virtual tick prefix: native derived
    /// callers own that prefix before invoking their base. The rotation service
    /// executes without constructing an unrelated action host.
    pub(crate) fn tick_with_rotation<F, E>(&mut self, delta_micros: i32, rotate: F) -> Result<(), E>
    where
        F: FnOnce(&mut MenuBasis, i32) -> Result<(), E>,
    {
        let elapsed = delta_micros / 1000;
        self.age_millis = self.age_millis.wrapping_add(elapsed);
        {
            let mut data = self.data.records_mut();
            for layout in data.layouts.iter_mut() {
                let scroll = i32::from(layout.scroll_animation);
                if scroll == 0 {
                    continue;
                }
                let product = i64::from(scroll * 5 / 4) * i64::from(delta_micros.wrapping_shl(13));
                // IMUL then SHL EAX / RCL EDX: signed product >>31, narrowed.
                let step = (product >> 31) as i32;
                let step = if scroll > 0 {
                    step.max(1)
                } else {
                    step.min(-1)
                };
                layout.scroll_animation = layout.scroll_animation.wrapping_sub(step as i16);
            }
            for item in data.items.iter_mut() {
                let age = i32::from(item.age).wrapping_add(elapsed);
                item.age = age.min(65535) as u16;
            }
        }
        rotate(&mut self.basis, delta_micros / 100)
    }
}

enum PreparedCommand {
    Complete(MenuCommandOutcome),
    Action(MenuActionRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuControllerSpec {
    /// 00453380 first argument, stored at +20 (not a resource identifier).
    pub fade_rate: i32,
    pub viewport: [i16; 4],
}

#[derive(Clone, Debug)]
pub enum MenuOwner {
    Authored(MenuState),
    Profile(ProfileMenu),
    Network(NetworkMenu),
}

impl MenuOwner {
    pub fn base(&self) -> &MenuState {
        match self {
            Self::Authored(menu) => menu,
            Self::Profile(profile) => profile.base(),
            Self::Network(network) => network.base(),
        }
    }

    pub fn base_mut(&mut self) -> &mut MenuState {
        match self {
            Self::Authored(menu) => menu,
            Self::Profile(profile) => profile.base_mut(),
            Self::Network(network) => network.base_mut(),
        }
    }
}

impl From<MenuState> for MenuOwner {
    fn from(menu: MenuState) -> Self {
        Self::Authored(menu)
    }
}

impl From<ProfileMenu> for MenuOwner {
    fn from(profile: ProfileMenu) -> Self {
        Self::Profile(profile)
    }
}

impl From<NetworkMenu> for MenuOwner {
    fn from(network: NetworkMenu) -> Self {
        Self::Network(network)
    }
}

/// Both supported owners have a proven RET at virtual +10 (00401310).
/// Profile destruction releases its instance rows; shared layouts survive.
#[derive(Clone, Debug)]
pub struct MenuStack {
    menus: Vec<MenuOwner>,
    spec: MenuControllerSpec,
    fade: i32,
    active: bool,
    drawn: bool,
}

impl MenuStack {
    pub fn new(spec: MenuControllerSpec) -> Self {
        Self {
            menus: Vec::new(),
            spec,
            fade: 0,
            active: true,
            drawn: false,
        }
    }
    pub fn spec(&self) -> MenuControllerSpec {
        self.spec
    }
    /// `00414140` re-sets a type-3 controller's viewport when it is shown
    /// again.
    pub fn set_viewport(&mut self, viewport: [i16; 4]) {
        self.spec.viewport = viewport;
    }
    pub fn len(&self) -> usize {
        self.menus.len()
    }
    pub fn is_empty(&self) -> bool {
        self.menus.is_empty()
    }
    pub fn current(&self) -> Option<&MenuState> {
        self.menus.last().map(MenuOwner::base)
    }
    pub fn current_mut(&mut self) -> Option<&mut MenuState> {
        self.menus.last_mut().map(MenuOwner::base_mut)
    }
    pub fn current_owner(&self) -> Option<&MenuOwner> {
        self.menus.last()
    }
    pub fn current_owner_mut(&mut self) -> Option<&mut MenuOwner> {
        self.menus.last_mut()
    }
    pub fn current_owner_and_active_mut(&mut self) -> Option<(&mut MenuOwner, &mut bool)> {
        self.menus.last_mut().map(|owner| (owner, &mut self.active))
    }
    pub fn fade(&self) -> i32 {
        self.fade
    }
    pub fn drawn(&self) -> bool {
        self.drawn
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// 00453610 sets +25 after the real controller draw, even before full fade.
    pub fn mark_drawn(&mut self) {
        self.drawn = true;
    }

    /// 00453440 inserts at the intrusive-list head; current is newest.
    pub fn push(&mut self, menu: impl Into<MenuOwner>) {
        self.menus.push(menu.into());
        self.drawn = false;
    }

    /// 00453480 destroys the old current before linking the replacement.
    pub fn replace(&mut self, menu: impl Into<MenuOwner>) {
        self.menus.pop();
        self.push(menu);
    }

    /// 004534D0 resets the newly exposed menu even when count is zero/negative.
    pub fn pop(&mut self, count: i32) {
        for _ in 0..count.max(0) {
            if self.menus.pop().is_none() {
                break;
            }
        }
        self.reset_current_ages();
        self.drawn = false;
    }

    pub fn reset_current_ages(&mut self) {
        if let Some(menu) = self.current_mut() {
            menu.reset_ages();
        }
    }

    pub fn command<H: MenuHost>(
        &mut self,
        input: MenuInput,
        host: &mut H,
    ) -> Result<MenuCommandOutcome, MenuError<H::Error>> {
        let Some(menu) = self.current_mut() else {
            return Ok(MenuCommandOutcome::NoMenu);
        };
        match menu.prepare_command(input)? {
            PreparedCommand::Complete(outcome) => Ok(outcome),
            PreparedCommand::Action(request) => {
                host.action(request, self).map_err(MenuError::Host)?;
                Ok(MenuCommandOutcome::ActionDispatched)
            }
        }
    }

    /// 00453560, reached through controller virtual+10. Returns the native
    /// alive result; reaching zero destroys all menus, including on a zero-delta
    /// first tick. Hosts must not manufacture a draw to unlock menu ticking.
    pub fn tick<H: MenuHost>(
        &mut self,
        delta_micros: i32,
        host: &mut H,
    ) -> Result<bool, MenuError<H::Error>> {
        let directed = if self.active {
            delta_micros
        } else {
            delta_micros.wrapping_neg()
        };
        let step = (directed / 128).wrapping_mul(self.spec.fade_rate);
        self.fade = self.fade.wrapping_add(step).clamp(0, 65535);
        if self.fade == 65535 {
            if self.drawn {
                if let Some((owner, active)) = self.current_owner_and_active_mut() {
                    match owner {
                        MenuOwner::Authored(menu) => menu.tick(delta_micros, host)?,
                        MenuOwner::Profile(profile) => {
                            host.tick_profile(profile, delta_micros, active)?
                        }
                        MenuOwner::Network(network) => host.tick_network(network, delta_micros)?,
                    }
                }
            }
        } else if self.fade == 0 {
            self.menus.clear();
            return Ok(false);
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "menu_profile_tests.rs"]
mod profile_tests;
