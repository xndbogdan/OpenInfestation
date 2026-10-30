//! Derived profile menu 00448220, vtable 00524F28. Store operations and the
//! draw/tick handshake are owned here; glyph rendering and phase effects have
//! explicit hosts. Native media-change callback 00449C80 is RET 0 on this port.
//! Its destructor 004482D0 (scalar deleting 004482B0) frees the rows 00449410
//! built (+3C), then runs the base menu's 00452420; here the rows drop with
//! the menu.

use thiserror::Error;

use crate::{
    menu::{MenuBasis, MenuCommand, MenuData, MenuDerivedData, MenuState, MenuStateError},
    profile_store::{ProfileControlToken, ProfileStore, ProfileStoreError, PROFILE_LIMIT},
};

#[path = "profile_menu_templates.rs"]
mod templates;
pub use templates::{ProfileMenuTemplates, ProfileTemplateError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ProfileMode {
    Load = 0,
    Copy = 1,
    Delete = 2,
}
impl ProfileMode {
    pub const fn native(self) -> i32 {
        self as i32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileTimerKind {
    LoadFailure,
    CopyFailure,
    InitialWarning,
    DeleteFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileTimer {
    pub kind: ProfileTimerKind,
    pub millis: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ProfileOperation {
    Load = 1,
    NewName = 2,
    Delete = 3,
    Copy = 4,
    Format = 5,
    SelectAndFade = 6,
}

#[derive(Debug)]
pub enum ProfileTickOutcome {
    Idle,
    Completed(ProfileOperation),
    /// Native setjmp catches these store failures and continues its timer/draw
    /// state machine. Preserve the concrete error for runtime diagnostics.
    OperationFailed {
        operation: ProfileOperation,
        error: ProfileStoreError,
    },
}

#[derive(Debug, Error)]
pub enum ProfileMenuError<E> {
    #[error(transparent)]
    State(#[from] MenuStateError),
    #[error(transparent)]
    Store(#[from] ProfileStoreError),
    #[error("profile-menu host callback failed")]
    Host(E),
    #[error("unsupported profile action callback {0:#010x}")]
    UnsupportedCallback(u32),
    #[error("profile callback has negative slot {0}")]
    InvalidSlot(i32),
    #[error("profile operation {0:?} reached native +80 before an index writer completed")]
    MissingPendingIndex(ProfileOperation),
}

pub trait ProfileMenuServices {
    type Error;
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error>;
    /// Clear screen held-input low WORD, then execute 004F0340 input reset.
    fn reset_profile_input(&mut self) -> Result<(), Self::Error>;
    fn select_camera(&mut self, preset: i32) -> Result<(), Self::Error>;
    fn set_transition_flag(&mut self) -> Result<(), Self::Error>;
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error>;
    fn play_sound(&mut self, slot: i16) -> Result<(), Self::Error>;
    /// Controller virtual +04 -> 004151C0, starting its fade rather than pop.
    fn begin_controller_fade(&mut self) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug)]
pub struct ProfileMenuState {
    busy: bool,
    populated: bool,
    suppress_next_draw: bool,
    queried: bool,
    timers: [i32; 4],
    mode: ProfileMode,
    choice: bool,
    format_state: i32,
    pending: Option<ProfileOperation>,
    remaining_millis: i32,
    // +80 has no constructor initialization; it only exists after a writer.
    pending_index: Option<usize>,
}

impl ProfileMenuState {
    pub fn mode(&self) -> ProfileMode {
        self.mode
    }
    pub fn choice(&self) -> bool {
        self.choice
    }
    pub fn format_state(&self) -> i32 {
        self.format_state
    }
    pub fn set_format_state(&mut self, state: i32) {
        self.format_state = state;
    }
    pub fn pending(&self) -> Option<ProfileOperation> {
        self.pending
    }
    pub fn remaining_millis(&self) -> i32 {
        self.remaining_millis
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn timer(&self) -> Option<ProfileTimer> {
        [
            ProfileTimerKind::LoadFailure,
            ProfileTimerKind::CopyFailure,
            ProfileTimerKind::InitialWarning,
            ProfileTimerKind::DeleteFailure,
        ]
        .into_iter()
        .zip(self.timers)
        .find_map(|(kind, millis)| (millis > 0).then_some(ProfileTimer { kind, millis }))
    }
    /// 00448330. Callers supply -1/+1, or -mode for selected empty rows.
    /// The bounded native inputs avoid its undefined negative mode cases.
    pub fn change_mode(
        &mut self,
        base: &mut MenuState,
        delta: i32,
        owner_flag: bool,
        count: usize,
    ) -> Result<(), MenuStateError> {
        if base.navigation_locked() || owner_flag {
            return Ok(());
        }
        let mut mode = self.mode.native();
        for _ in 0..3 {
            mode = mode.wrapping_add(3).wrapping_add(delta) % 3;
            self.mode = match mode {
                0 => ProfileMode::Load,
                1 => ProfileMode::Copy,
                2 => ProfileMode::Delete,
                _ => return Err(MenuStateError::NoSelectableItem),
            };
            if self.mode != ProfileMode::Copy || count != PROFILE_LIMIT {
                break;
            }
        }
        if self.mode == ProfileMode::Copy && count == PROFILE_LIMIT {
            return Err(MenuStateError::NoSelectableItem);
        }
        let mut data = base.data().records_mut();
        let count = data.items.len();
        let header = data
            .items
            .first_mut()
            .ok_or(MenuStateError::ItemOutOfBounds { index: 0, count })?;
        header.draw_arguments[0] = match self.mode {
            ProfileMode::Load => 0xcd,
            ProfileMode::Copy => 0xd0,
            ProfileMode::Delete => 0xcf,
        };
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileDrawGate {
    pub draw_rows: bool,
    pub timer: Option<ProfileTimer>,
}

#[derive(Clone, Debug)]
pub struct ProfileMenu {
    pub(crate) base: MenuState,
    pub(crate) state: ProfileMenuState,
    templates: ProfileMenuTemplates,
}

impl ProfileMenu {
    pub fn new(templates: ProfileMenuTemplates, basis: MenuBasis, initial_warning: bool) -> Self {
        Self {
            base: MenuState::new(data(None, None, 0), basis),
            templates,
            state: ProfileMenuState {
                busy: true,
                populated: false,
                suppress_next_draw: false,
                queried: false,
                timers: [0, 0, if initial_warning { 2000 } else { 0 }, 0],
                mode: ProfileMode::Load,
                choice: false,
                format_state: 0,
                pending: None,
                remaining_millis: 0,
                pending_index: None,
            },
        }
    }
    pub fn base(&self) -> &MenuState {
        &self.base
    }
    pub fn base_mut(&mut self) -> &mut MenuState {
        &mut self.base
    }
    pub fn state(&self) -> &ProfileMenuState {
        &self.state
    }
    pub fn split_mut(&mut self) -> (&mut MenuState, &mut ProfileMenuState) {
        (&mut self.base, &mut self.state)
    }

    /// Call after resolving draw's font/style, before the base row walk.
    /// Any failed resource or row draw must leave the handshake unacknowledged.
    pub fn prepare_draw(
        &mut self,
        store: &mut ProfileStore,
    ) -> Result<ProfileDrawGate, ProfileStoreError> {
        let mut draw_rows = false;
        if !self.state.busy
            && self.state.pending.is_none()
            && store.query(false)?.status as u32 != 0
        {
            if self.state.suppress_next_draw {
                self.state.suppress_next_draw = false;
            } else {
                draw_rows = true;
            }
        }
        Ok(ProfileDrawGate {
            draw_rows,
            timer: self.state.timer(),
        })
    }
    pub fn finish_draw(&mut self) {
        self.state.busy = false;
    }

    fn rebuild(
        &mut self,
        owner_flag: bool,
        store: &mut ProfileStore,
    ) -> Result<(), ProfileStoreError> {
        let directory = store.query(!self.state.queried)?;
        self.state.queried = true;
        self.rebuild_status(directory.status as u32, directory.count, owner_flag);
        Ok(())
    }

    // Direct status dispatch remains separate so source branches 5/6 can be
    // verified without claiming the desktop ProfileStore produces those media states.
    fn rebuild_status(&mut self, status: u32, count: usize, owner_flag: bool) {
        if status == 3 {
            let mut rows = Vec::with_capacity(15);
            rows.push(self.templates.rows[0]);
            for index in 0..PROFILE_LIMIT {
                let occupied = index < count;
                let mut row = self.templates.rows[if occupied { 1 } else { 2 }];
                row.draw_arguments[0] = index as u32;
                if occupied {
                    row.action_arguments[0] = index as u32;
                }
                rows.push(row);
            }
            {
                let mut layouts = self.templates.regular.records_mut();
                layouts[1].item_count = 14;
                layouts[1].visible_rows = 9;
            }
            self.base.replace_data(
                data(Some(rows), Some(self.templates.regular.clone()), 1),
                1,
                Some(0),
            );
            self.base.set_navigation_locked(false);
        } else {
            let format_prompt = status == 5 && owner_flag;
            let mut row = self.templates.rows[if format_prompt { 4 } else { 3 }];
            if !format_prompt {
                row.draw_arguments[0] = match status {
                    5 => 0x9d,
                    6 => 0x96,
                    _ => 0x99,
                };
            }
            self.base.replace_data(
                data(Some(vec![row]), Some(self.templates.bind_exceptional()), 0),
                0,
                None,
            );
            self.base.set_navigation_locked(format_prompt);
            if format_prompt {
                self.state.format_state = 0;
                self.state.choice = false;
            }
        }
        self.state.populated = true;
    }

    pub fn execute_callback<S: ProfileMenuServices>(
        &mut self,
        callback: u32,
        command: MenuCommand,
        arguments: [u32; 2],
        owner_flag: bool,
        store: &mut ProfileStore,
        services: &mut S,
    ) -> Result<(), ProfileMenuError<S::Error>> {
        match callback {
            0x0044_92e0 => {
                if matches!(
                    command,
                    MenuCommand::AdjustPrevious | MenuCommand::AdjustNext
                ) {
                    self.occupied(command, -1, owner_flag, store, services)?;
                }
            }
            0x0044_9300 => {
                self.occupied(command, arguments[0] as i32, owner_flag, store, services)?
            }
            0x0044_93f0 => self.exceptional(command, owner_flag, store, services)?,
            0x0044_9400 => {
                if command == MenuCommand::Activate {
                    self.empty(owner_flag, store, services)?;
                }
            }
            other => return Err(ProfileMenuError::UnsupportedCallback(other)),
        }
        Ok(())
    }

    fn schedule(&mut self, operation: ProfileOperation, millis: i32, index: Option<usize>) {
        self.state.pending = Some(operation);
        self.state.remaining_millis = millis;
        if let Some(index) = index {
            self.state.pending_index = Some(index);
        }
    }

    fn empty<S: ProfileMenuServices>(
        &mut self,
        owner_flag: bool,
        store: &mut ProfileStore,
        services: &mut S,
    ) -> Result<(), ProfileMenuError<S::Error>> {
        if self.state.pending.is_some() {
            return Ok(());
        }
        let count = store.cached_count();
        if owner_flag {
            self.schedule(ProfileOperation::SelectAndFade, 1, Some(count));
            return Ok(());
        }
        store.select_for_menu(count)?;
        services
            .set_transition_flag()
            .map_err(ProfileMenuError::Host)?;
        self.schedule(ProfileOperation::NewName, 1000, None);
        services.select_camera(9).map_err(ProfileMenuError::Host)?;
        if count == 0 {
            store.record_control_token(ProfileControlToken::Zero);
        }
        Ok(())
    }

    /// `00448800`, an occupied row's command (from `004492E0`/`00449300`):
    /// unlocked, the mode steps or Activate loads or locks; locked (+2C),
    /// the Yes/No choice +2D answers.
    fn occupied<S: ProfileMenuServices>(
        &mut self,
        command: MenuCommand,
        index: i32,
        owner_flag: bool,
        store: &mut ProfileStore,
        services: &mut S,
    ) -> Result<(), ProfileMenuError<S::Error>> {
        if !self.base.navigation_locked() {
            match command {
                MenuCommand::AdjustPrevious | MenuCommand::AdjustNext => self.state.change_mode(
                    &mut self.base,
                    if command == MenuCommand::AdjustPrevious {
                        -1
                    } else {
                        1
                    },
                    owner_flag,
                    store.cached_count(),
                )?,
                MenuCommand::Activate => {
                    if self.state.mode == ProfileMode::Load {
                        let slot = usize::try_from(index)
                            .map_err(|_| ProfileMenuError::InvalidSlot(index))?;
                        if store
                            .entry(slot)?
                            .is_some_and(|entry| entry.load_is_rejected())
                        {
                            services.play_sound(4).map_err(ProfileMenuError::Host)?;
                            return Ok(());
                        }
                        if self.state.pending.is_some() {
                            return Ok(());
                        }
                        if !owner_flag {
                            self.schedule(ProfileOperation::Load, 1000, None);
                            services
                                .set_transition_flag()
                                .map_err(ProfileMenuError::Host)?;
                            self.state.pending_index = Some(slot);
                            services.select_camera(9).map_err(ProfileMenuError::Host)?;
                            return Ok(());
                        }
                    }
                    self.base.set_navigation_locked(true);
                    self.state.choice = false;
                }
                _ => (),
            }
            return Ok(());
        }
        let yes = match command {
            MenuCommand::AdjustPrevious => {
                self.state.choice = true;
                return Ok(());
            }
            MenuCommand::AdjustNext => {
                self.state.choice = false;
                return Ok(());
            }
            MenuCommand::Activate => self.state.choice,
            MenuCommand::Auxiliary6 => true,
            MenuCommand::Auxiliary7 => false,
            _ => return Ok(()),
        };
        self.base.set_navigation_locked(false);
        if yes {
            let index = usize::try_from(index).map_err(|_| ProfileMenuError::InvalidSlot(index))?;
            let (operation, delay) = match self.state.mode {
                ProfileMode::Load => (ProfileOperation::SelectAndFade, 1),
                ProfileMode::Copy => (ProfileOperation::Copy, 0),
                ProfileMode::Delete => (ProfileOperation::Delete, 0),
            };
            self.schedule(operation, delay, Some(index));
        }
        Ok(())
    }

    /// 00448750, from the confirmation row's wrapper 004493F0.
    fn exceptional<S: ProfileMenuServices>(
        &mut self,
        command: MenuCommand,
        owner_flag: bool,
        store: &mut ProfileStore,
        services: &mut S,
    ) -> Result<(), ProfileMenuError<S::Error>> {
        if self.base.navigation_locked() {
            let yes = match command {
                MenuCommand::AdjustPrevious => {
                    self.state.choice = true;
                    return Ok(());
                }
                MenuCommand::AdjustNext => {
                    self.state.choice = false;
                    return Ok(());
                }
                MenuCommand::Activate => self.state.choice,
                MenuCommand::Auxiliary6 => true,
                MenuCommand::Auxiliary7 => false,
                _ => return Ok(()),
            };
            if yes {
                self.schedule(ProfileOperation::Format, 1, None);
                self.state.busy = true;
            } else {
                self.state.format_state = 3;
                self.base.set_navigation_locked(false);
            }
        } else if self.state.format_state == 1 {
            self.state.populated = false;
        } else if !owner_flag && command == MenuCommand::Activate {
            self.empty(owner_flag, store, services)?;
        } else {
            self.state.queried = false;
            self.state.populated = false;
            self.state.busy = true;
        }
        Ok(())
    }

    pub fn tick<S: ProfileMenuServices>(
        &mut self,
        delta_micros: i32,
        owner_flag: bool,
        store: &mut ProfileStore,
        services: &mut S,
    ) -> Result<ProfileTickOutcome, ProfileMenuError<S::Error>> {
        let elapsed = delta_micros / 1000;
        for timer in &mut self.state.timers {
            if *timer > 0 {
                self.state.busy = true;
                if elapsed < *timer {
                    *timer = timer.wrapping_sub(elapsed);
                    return Ok(ProfileTickOutcome::Idle);
                }
                *timer = 0;
            }
        }
        if self.state.busy {
            return Ok(ProfileTickOutcome::Idle);
        }
        let rebuilt = !self.state.populated;
        if rebuilt {
            self.rebuild(owner_flag, store)?;
            services
                .reset_profile_input()
                .map_err(ProfileMenuError::Host)?;
        }
        self.base
            .tick_with_rotation(delta_micros, |basis, angle| {
                services.rotate_menu_basis(basis, angle)
            })
            .map_err(ProfileMenuError::Host)?;
        if rebuilt {
            self.base.reset_ages();
        }
        let Some(operation) = self.state.pending else {
            return Ok(ProfileTickOutcome::Idle);
        };
        if elapsed <= self.state.remaining_millis {
            self.state.remaining_millis = self.state.remaining_millis.wrapping_sub(elapsed);
            return Ok(ProfileTickOutcome::Idle);
        }
        let index = self.state.pending_index;
        let mut failure = None;
        match operation {
            ProfileOperation::Load => {
                let index = index.ok_or(ProfileMenuError::MissingPendingIndex(operation))?;
                if index < store.cached_count() {
                    match store.load(index) {
                        Ok(_) => services.request_phase(3).map_err(ProfileMenuError::Host)?,
                        Err(error) => {
                            self.state.populated = false;
                            self.state.queried = false;
                            self.state.timers[0] = 2000;
                            failure = Some(error);
                        }
                    }
                } else {
                    self.state.populated = false;
                }
            }
            ProfileOperation::NewName => {
                services.request_phase(9).map_err(ProfileMenuError::Host)?
            }
            ProfileOperation::Delete | ProfileOperation::Copy => {
                let index = index.ok_or(ProfileMenuError::MissingPendingIndex(operation))?;
                let result = if operation == ProfileOperation::Delete {
                    store.delete_profile(index)
                } else {
                    store.copy_profile(index)
                };
                self.state.populated = false;
                self.state.mode = ProfileMode::Load;
                if let Err(error) = result {
                    self.state.queried = false;
                    self.state.timers[if operation == ProfileOperation::Delete {
                        3
                    } else {
                        1
                    }] = 2000;
                    failure = Some(error);
                }
            }
            ProfileOperation::Format => {
                store.reset_directory_after_native_format();
                self.state.format_state = 1;
                self.base.set_navigation_locked(false);
            }
            ProfileOperation::SelectAndFade => {
                store.select_for_menu(
                    index.ok_or(ProfileMenuError::MissingPendingIndex(operation))?,
                )?;
                store.record_control_token(ProfileControlToken::MinusOne);
                services
                    .begin_controller_fade()
                    .map_err(ProfileMenuError::Host)?;
            }
        }
        if operation == ProfileOperation::NewName {
            self.state.suppress_next_draw = true;
        } else if operation != ProfileOperation::SelectAndFade {
            self.state.busy = true;
        }
        self.state.pending = None;
        self.state.remaining_millis = 0;
        Ok(match failure {
            Some(error) => ProfileTickOutcome::OperationFailed { operation, error },
            None => ProfileTickOutcome::Completed(operation),
        })
    }
}

fn data(
    items: Option<Vec<inf_formats::FrontendMenuItem>>,
    layouts: Option<crate::menu::MenuLayouts>,
    initial_selection: i32,
) -> MenuData {
    MenuData::derived(MenuDerivedData {
        constructor: 0x0044_8220,
        vtable: 0x0052_4f28,
        items,
        layouts,
        initial_selection,
        flags: 0,
    })
}

#[cfg(test)]
#[path = "profile_menu_tests.rs"]
mod tests;
