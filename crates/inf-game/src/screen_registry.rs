//! Typed ownership for native scheduler attachments and screen membership.
//!
//! `004944D0`, `00494630`, `004946F0`, and `00437F90` prepend entries to
//! their lists. A context owner's linked screen is a separate field from a
//! session's newest screen: `00494760/004947B0` do not restore that link on
//! destruction. Generation checks make the resulting stale link an error.
//!
//! Screen payloads, input-context destruction, and backend callbacks remain
//! with their actual owners. This registry holds their identities and backend
//! value, with no successful placeholder callbacks or platform pointers.

use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

static NEXT_REGISTRY: AtomicU64 = AtomicU64::new(1);

/// Lifecycle of a concrete screen owner: metadata only, stopped after a
/// partial or failed operation, ready, or torn down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenLifecycle {
    New,
    Stopped,
    Ready,
    Destroyed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    registry: u64,
    slot: usize,
    generation: u64,
}

/// Scheduler identity, corresponding to a `004944D0` owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(Key);

/// Stable second-pass owner identity; its backend value can be replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContextOwnerId(Key);

/// Screen identity shared by the scheduler-member and session-screen lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScreenId(Key);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum RegistryError {
    #[error("unknown or expired session {0:?}")]
    UnknownSession(SessionId),
    #[error("unknown or expired context owner {0:?}")]
    UnknownContextOwner(ContextOwnerId),
    #[error("unknown or expired screen {0:?}")]
    UnknownScreen(ScreenId),
    #[error("session {0:?} is dispatching its tick; screen attachment is forbidden")]
    SessionBusy(SessionId),
    #[error("context owner {0:?} is dispatching; screen attachment is forbidden")]
    ContextBusy(ContextOwnerId),
    #[error("context owner {owner:?} still links to removed screen {screen:?}")]
    StaleLinkedScreen {
        owner: ContextOwnerId,
        screen: ScreenId,
    },
    #[error("screen {0:?} has not installed an input context")]
    MissingInputContext(ScreenId),
    #[error("screen {screen:?} cannot join session screen membership from {membership:?}")]
    ScreenMembership {
        screen: ScreenId,
        membership: ScreenMembership,
    },
}

#[derive(Debug, Error)]
pub enum RegistryCallbackError<E> {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("registry callback failed: {0}")]
    Callback(E),
}

/// The two distinct native owners passed to `00437F90` / `004946F0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenAttachment {
    pub session: SessionId,
    /// Stored at screen `+E8` and linked base `+14`, not backend `+4`.
    pub context_owner: ContextOwnerId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRegistration<B> {
    pub attachment: ScreenAttachment,
    /// Installation is separate from attachment (`004947E0`).
    pub input_context: Option<B>,
    pub membership: ScreenMembership,
}

/// The base scheduler attaches before 00437F90 publishes session +48 screen
/// membership. 00438080 removes that membership before input/base teardown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenMembership {
    Pending,
    Linked,
    Unlinked,
}

#[derive(Debug)]
struct Slot<T> {
    generation: u64,
    value: Option<T>,
}

#[derive(Debug)]
struct Arena<T> {
    registry: u64,
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
}

impl<T> Arena<T> {
    fn new(registry: u64) -> Self {
        Self {
            registry,
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    fn insert(&mut self, value: T) -> Key {
        let slot = if let Some(slot) = self.free.pop() {
            self.slots[slot].value = Some(value);
            slot
        } else {
            self.slots.push(Slot {
                generation: 1,
                value: Some(value),
            });
            self.slots.len() - 1
        };
        Key {
            registry: self.registry,
            slot,
            generation: self.slots[slot].generation,
        }
    }

    fn get(&self, key: Key) -> Option<&T> {
        let slot = self.slots.get(key.slot)?;
        if key.registry != self.registry || key.generation != slot.generation {
            return None;
        }
        slot.value.as_ref()
    }

    fn get_mut(&mut self, key: Key) -> Option<&mut T> {
        let slot = self.slots.get_mut(key.slot)?;
        if key.registry != self.registry || key.generation != slot.generation {
            return None;
        }
        slot.value.as_mut()
    }

    fn remove(&mut self, key: Key) -> Option<T> {
        let slot = self.slots.get_mut(key.slot)?;
        if key.registry != self.registry || key.generation != slot.generation {
            return None;
        }
        let value = slot.value.take()?;
        // Never wrap a generation into a previously issued identity.
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(key.slot);
        }
        Some(value)
    }
}

#[derive(Debug, Default)]
struct Session {
    scheduler_members: Vec<ScreenId>,
    screens: Vec<ScreenId>,
    busy: bool,
}

#[derive(Debug)]
struct ContextOwner<D> {
    backend: D,
    linked_screen: Option<ScreenId>,
    busy: bool,
}

/// Registry metadata independent of screen implementation and input decoding.
///
/// `D` is the backend value stored by the second-pass owner; `B` is an identity
/// supplied by the input owner. Both are distinct from screen/context handles.
/// Removing a session or context does not cascade screen destruction, matching
/// the base destructors. Invalidated parents are detected when resolved.
#[derive(Debug)]
pub struct ScreenRegistry<D, B> {
    sessions: Arena<Session>,
    contexts: Arena<ContextOwner<D>>,
    screens: Arena<ScreenRegistration<B>>,
    session_order: Vec<SessionId>,
    context_order: Vec<ContextOwnerId>,
    selected_screen: Option<ScreenId>,
}

impl<D, B: Copy + Eq> Default for ScreenRegistry<D, B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D, B: Copy + Eq> ScreenRegistry<D, B> {
    pub fn new() -> Self {
        let registry = NEXT_REGISTRY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("screen registry identity exhausted");
        Self {
            sessions: Arena::new(registry),
            contexts: Arena::new(registry),
            screens: Arena::new(registry),
            session_order: Vec::new(),
            context_order: Vec::new(),
            selected_screen: None,
        }
    }

    pub fn register_session(&mut self) -> SessionId {
        let id = SessionId(self.sessions.insert(Session::default()));
        self.session_order.insert(0, id);
        id
    }

    pub fn register_context(&mut self, backend: D) -> ContextOwnerId {
        let id = ContextOwnerId(self.contexts.insert(ContextOwner {
            backend,
            linked_screen: None,
            busy: false,
        }));
        self.context_order.insert(0, id);
        id
    }

    pub fn session_order(&self) -> &[SessionId] {
        &self.session_order
    }

    pub fn context_order(&self) -> &[ContextOwnerId] {
        &self.context_order
    }

    /// Complete attachment for callers with no intervening base initialization.
    /// Concrete constructors use the two stages below around their embedded
    /// input context and analog-consumer construction at 00437F90.
    pub fn attach_screen(
        &mut self,
        attachment: ScreenAttachment,
    ) -> Result<ScreenId, RegistryError> {
        let screen = self.attach_scheduler_member(attachment)?;
        self.link_screen_membership(screen)?;
        Ok(screen)
    }

    /// 004946F0: prepend session +1C scheduler membership and replace context
    /// owner +18. Session +48 screen membership is not published yet.
    pub fn attach_scheduler_member(
        &mut self,
        attachment: ScreenAttachment,
    ) -> Result<ScreenId, RegistryError> {
        let session = self.session(attachment.session)?;
        if session.busy {
            return Err(RegistryError::SessionBusy(attachment.session));
        }
        let context = self.context(attachment.context_owner)?;
        if context.busy {
            return Err(RegistryError::ContextBusy(attachment.context_owner));
        }
        // Retail checks context busy after member insertion. Validate both
        // first so an attachment rejected by this Rust API leaves no orphan.
        let screen = ScreenId(self.screens.insert(ScreenRegistration {
            attachment,
            input_context: None,
            membership: ScreenMembership::Pending,
        }));
        self.sessions
            .get_mut(attachment.session.0)
            .expect("validated session")
            .scheduler_members
            .insert(0, screen);
        self.contexts
            .get_mut(attachment.context_owner.0)
            .expect("validated context owner")
            .linked_screen = Some(screen);
        Ok(screen)
    }

    /// 00438029..00438040, after context, +CC and +D0 initialization. This
    /// publication neither selects input nor changes the context-owner link.
    pub fn link_screen_membership(&mut self, screen: ScreenId) -> Result<(), RegistryError> {
        let registration = self.screen(screen)?;
        if registration.membership != ScreenMembership::Pending {
            return Err(RegistryError::ScreenMembership {
                screen,
                membership: registration.membership,
            });
        }
        self.sessions
            .get_mut(registration.attachment.session.0)
            .ok_or(RegistryError::UnknownSession(
                registration.attachment.session,
            ))?
            .screens
            .insert(0, screen);
        self.screens
            .get_mut(screen.0)
            .expect("validated screen")
            .membership = ScreenMembership::Linked;
        Ok(())
    }

    /// 004380A3..004380CC: remove only session +48 screen membership BEFORE
    /// input teardown. Screen identity, scheduler membership, selected identity
    /// and the context-owner link remain live. Repeated cleanup is harmless.
    pub fn unlink_screen_membership(&mut self, screen: ScreenId) -> Result<(), RegistryError> {
        let registration = self
            .screens
            .get_mut(screen.0)
            .ok_or(RegistryError::UnknownScreen(screen))?;
        registration.membership = ScreenMembership::Unlinked;
        if let Some(session) = self.sessions.get_mut(registration.attachment.session.0) {
            session.screens.retain(|&member| member != screen);
        }
        Ok(())
    }

    pub fn scheduler_members(&self, session: SessionId) -> Result<&[ScreenId], RegistryError> {
        Ok(&self.session(session)?.scheduler_members)
    }

    pub fn screens(&self, session: SessionId) -> Result<&[ScreenId], RegistryError> {
        Ok(&self.session(session)?.screens)
    }

    pub fn current_screen(&self, session: SessionId) -> Result<Option<ScreenId>, RegistryError> {
        Ok(self.session(session)?.screens.first().copied())
    }

    pub fn screen(&self, screen: ScreenId) -> Result<ScreenRegistration<B>, RegistryError> {
        self.screens
            .get(screen.0)
            .copied()
            .ok_or(RegistryError::UnknownScreen(screen))
    }

    /// Resolve the second-pass target independently of session membership.
    /// Removing that target leaves a stale link until another attachment
    /// replaces it; silently returning the preceding screen is not native.
    pub fn linked_screen(&self, owner: ContextOwnerId) -> Result<Option<ScreenId>, RegistryError> {
        let screen = self.context(owner)?.linked_screen;
        if let Some(screen) = screen {
            if self.screens.get(screen.0).is_none() {
                return Err(RegistryError::StaleLinkedScreen { owner, screen });
            }
        }
        Ok(screen)
    }

    pub fn backend(&self, owner: ContextOwnerId) -> Result<&D, RegistryError> {
        Ok(&self.context(owner)?.backend)
    }

    /// `00438370` changes owner `+4` without changing any screen's `+E8`.
    /// Return the old backend to its owner for the appropriate teardown.
    pub fn replace_backend(
        &mut self,
        owner: ContextOwnerId,
        backend: D,
    ) -> Result<D, RegistryError> {
        let context = self
            .contexts
            .get_mut(owner.0)
            .ok_or(RegistryError::UnknownContextOwner(owner))?;
        Ok(std::mem::replace(&mut context.backend, backend))
    }

    /// Final scheduler unlink, with no busy check (`00494760/004947B0`).
    /// Concrete teardown first calls `unlink_screen_membership`, then destroys
    /// its embedded input context/references, then calls this method. Callers
    /// using condensed teardown may leave membership linked; it is also removed
    /// here. This never destroys `B` or restores a preceding context link.
    pub fn remove_screen(
        &mut self,
        screen: ScreenId,
    ) -> Result<ScreenRegistration<B>, RegistryError> {
        self.unlink_screen_membership(screen)?;
        let registration = self
            .screens
            .remove(screen.0)
            .ok_or(RegistryError::UnknownScreen(screen))?;
        if self.selected_screen == Some(screen) {
            self.selected_screen = None;
        }
        if let Some(session) = self.sessions.get_mut(registration.attachment.session.0) {
            session.scheduler_members.retain(|&member| member != screen);
        }
        Ok(registration)
    }

    /// Base scheduler destruction unlinks its global entry without recursively
    /// destroying members. Existing screens retain an expired session handle.
    pub fn remove_session(&mut self, session: SessionId) -> Result<(), RegistryError> {
        self.sessions
            .remove(session.0)
            .ok_or(RegistryError::UnknownSession(session))?;
        self.session_order.retain(|&member| member != session);
        Ok(())
    }

    /// `00494670/004946B0` unlink the context owner only. Backend teardown is
    /// separate; ownership of its value is returned to the caller.
    pub fn remove_context(&mut self, owner: ContextOwnerId) -> Result<D, RegistryError> {
        let context = self
            .contexts
            .remove(owner.0)
            .ok_or(RegistryError::UnknownContextOwner(owner))?;
        self.context_order.retain(|&member| member != owner);
        Ok(context.backend)
    }

    pub fn selected_screen(&self) -> Option<ScreenId> {
        self.selected_screen
    }

    /// Store the screen's input context before selecting it (`004947E0`).
    /// This implements the base `+10` selection policy; a concrete screen with
    /// an overridden virtual needs its corresponding owner implementation.
    pub fn bind_input<E>(
        &mut self,
        screen: ScreenId,
        input_context: B,
        apply_bindings: impl FnOnce(Option<B>) -> Result<(), E>,
    ) -> Result<(), RegistryCallbackError<E>> {
        self.screens
            .get_mut(screen.0)
            .ok_or(RegistryError::UnknownScreen(screen))?
            .input_context = Some(input_context);
        self.select_input(screen, apply_bindings)
    }

    /// `004947F0` applies bindings first, then publishes selected identity.
    /// A callback error leaves earlier effects in place; it is not a rollback.
    pub fn select_input<E>(
        &mut self,
        screen: ScreenId,
        apply_bindings: impl FnOnce(Option<B>) -> Result<(), E>,
    ) -> Result<(), RegistryCallbackError<E>> {
        let input_context = self
            .screen(screen)?
            .input_context
            .ok_or(RegistryError::MissingInputContext(screen))?;
        apply_bindings(Some(input_context)).map_err(RegistryCallbackError::Callback)?;
        self.selected_screen = Some(screen);
        Ok(())
    }

    /// `00494810` clears matching selected identity before clearing bindings.
    /// The binding callback runs even when this screen was not selected.
    pub fn deselect_input<E>(
        &mut self,
        screen: ScreenId,
        apply_bindings: impl FnOnce(Option<B>) -> Result<(), E>,
    ) -> Result<(), RegistryCallbackError<E>> {
        self.screen(screen)?;
        if self.selected_screen == Some(screen) {
            self.selected_screen = None;
        }
        apply_bindings(None).map_err(RegistryCallbackError::Callback)?;
        Ok(())
    }

    /// Guard only session virtual `+04`: input dispatch and transition precede
    /// the session `+18` busy assignment at `004945FB`. Nested dispatch and attachments
    /// to this session are rejected. Removal remains permitted.
    ///
    /// Flags are released on ordinary callback success/error, a Rust failure
    /// containment policy rather than native nonlocal-jump recovery. A panic
    /// leaves the flag set; callers must not resume a panicked runtime.
    pub fn with_session_tick<T, E>(
        &mut self,
        session: SessionId,
        callback: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, RegistryCallbackError<E>> {
        let entry = self
            .sessions
            .get_mut(session.0)
            .ok_or(RegistryError::UnknownSession(session))?;
        if entry.busy {
            return Err(RegistryError::SessionBusy(session).into());
        }
        entry.busy = true;
        let result = callback(self);
        if let Some(entry) = self.sessions.get_mut(session.0) {
            entry.busy = false;
        }
        result.map_err(RegistryCallbackError::Callback)
    }

    /// Guard the complete linked-object second pass (`004943DA..0049440D`),
    /// including all three virtual callbacks, with the same error policy as
    /// `with_session_tick`. Resolve the linked screen/backend for each call.
    pub fn with_context_dispatch<T, E>(
        &mut self,
        owner: ContextOwnerId,
        callback: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, RegistryCallbackError<E>> {
        let entry = self
            .contexts
            .get_mut(owner.0)
            .ok_or(RegistryError::UnknownContextOwner(owner))?;
        if entry.busy {
            return Err(RegistryError::ContextBusy(owner).into());
        }
        entry.busy = true;
        let result = callback(self);
        if let Some(entry) = self.contexts.get_mut(owner.0) {
            entry.busy = false;
        }
        result.map_err(RegistryCallbackError::Callback)
    }

    fn session(&self, session: SessionId) -> Result<&Session, RegistryError> {
        self.sessions
            .get(session.0)
            .ok_or(RegistryError::UnknownSession(session))
    }

    fn context(&self, owner: ContextOwnerId) -> Result<&ContextOwner<D>, RegistryError> {
        self.contexts
            .get(owner.0)
            .ok_or(RegistryError::UnknownContextOwner(owner))
    }
}

#[cfg(test)]
#[path = "screen_registry_tests.rs"]
mod tests;
