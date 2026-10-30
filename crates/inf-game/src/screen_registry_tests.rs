use super::*;

type Registry = ScreenRegistry<&'static str, u32>;

fn attachment(session: SessionId, context_owner: ContextOwnerId) -> ScreenAttachment {
    ScreenAttachment {
        session,
        context_owner,
    }
}

fn fixture() -> (Registry, SessionId, ContextOwnerId) {
    let mut registry = Registry::new();
    let session = registry.register_session();
    let owner = registry.register_context("initial backend");
    (registry, session, owner)
}

#[test]
fn all_global_and_member_lists_prepend_new_registrations() {
    let (mut registry, first_session, first_owner) = fixture();
    let second_session = registry.register_session();
    let second_owner = registry.register_context("second backend");
    assert_eq!(registry.session_order(), &[second_session, first_session]);
    assert_eq!(registry.context_order(), &[second_owner, first_owner]);

    let first = registry
        .attach_screen(attachment(first_session, first_owner))
        .unwrap();
    let second = registry
        .attach_screen(attachment(first_session, first_owner))
        .unwrap();
    let third = registry
        .attach_screen(attachment(first_session, second_owner))
        .unwrap();
    assert_eq!(
        registry.screens(first_session),
        Ok(&[third, second, first][..])
    );
    assert_eq!(registry.screens(second_session), Ok(&[][..]));
    assert_eq!(registry.current_screen(first_session), Ok(Some(third)));
    assert_eq!(registry.linked_screen(first_owner), Ok(Some(second)));
    assert_eq!(registry.linked_screen(second_owner), Ok(Some(third)));
}

#[test]
fn removing_linked_screen_never_reactivates_an_older_member() {
    let (mut registry, session, owner) = fixture();
    let oldest = registry.attach_screen(attachment(session, owner)).unwrap();
    let newest = registry.attach_screen(attachment(session, owner)).unwrap();
    registry.remove_screen(newest).unwrap();
    assert_eq!(registry.current_screen(session), Ok(Some(oldest)));
    assert_eq!(
        registry.linked_screen(owner),
        Err(RegistryError::StaleLinkedScreen {
            owner,
            screen: newest,
        })
    );
    let replacement = registry.attach_screen(attachment(session, owner)).unwrap();
    assert_eq!(newest.0.slot, replacement.0.slot);
    assert_ne!(newest, replacement);
    assert_eq!(
        registry.screen(newest),
        Err(RegistryError::UnknownScreen(newest))
    );
    assert_eq!(registry.linked_screen(owner), Ok(Some(replacement)));
    assert_eq!(registry.screens(session), Ok(&[replacement, oldest][..]));
}

#[test]
fn removing_nonlinked_member_does_not_change_current_link() {
    let (mut registry, session, owner) = fixture();
    let oldest = registry.attach_screen(attachment(session, owner)).unwrap();
    let newest = registry.attach_screen(attachment(session, owner)).unwrap();
    registry.remove_screen(oldest).unwrap();
    assert_eq!(registry.screens(session), Ok(&[newest][..]));
    assert_eq!(registry.linked_screen(owner), Ok(Some(newest)));
}

#[test]
fn backend_replacement_keeps_owner_and_screen_identity() {
    let mut registry = ScreenRegistry::<String, u32>::new();
    let session = registry.register_session();
    let owner = registry.register_context("old".to_owned());
    let screen = registry.attach_screen(attachment(session, owner)).unwrap();
    assert_eq!(
        registry.replace_backend(owner, "new".to_owned()).unwrap(),
        "old"
    );
    assert_eq!(registry.backend(owner).unwrap(), "new");
    assert_eq!(
        registry.screen(screen).unwrap().attachment.context_owner,
        owner
    );
    assert_eq!(registry.linked_screen(owner), Ok(Some(screen)));
    assert_eq!(registry.remove_context(owner).unwrap(), "new");
    assert!(
        matches!(registry.backend(owner), Err(RegistryError::UnknownContextOwner(id)) if id == owner)
    );
    // Backend/context destruction does not destroy the screen membership.
    assert_eq!(registry.current_screen(session), Ok(Some(screen)));
    registry.remove_screen(screen).unwrap();
}

#[test]
fn parent_removal_does_not_cascade_or_allow_reused_identity_aliases() {
    let (mut registry, session, owner) = fixture();
    let screen = registry.attach_screen(attachment(session, owner)).unwrap();
    registry.remove_session(session).unwrap();
    registry.remove_context(owner).unwrap();
    let next_session = registry.register_session();
    let next_owner = registry.register_context("replacement");
    assert_eq!(session.0.slot, next_session.0.slot);
    assert_eq!(owner.0.slot, next_owner.0.slot);
    assert_ne!(session, next_session);
    assert_ne!(owner, next_owner);
    assert_eq!(
        registry.current_screen(session),
        Err(RegistryError::UnknownSession(session))
    );
    assert_eq!(
        registry.linked_screen(owner),
        Err(RegistryError::UnknownContextOwner(owner))
    );
    assert_eq!(
        registry.screen(screen).unwrap().attachment,
        attachment(session, owner)
    );
    assert_eq!(registry.screens(next_session), Ok(&[][..]));
    assert_eq!(registry.linked_screen(next_owner), Ok(None));
    registry.remove_screen(screen).unwrap();
    assert_eq!(registry.screens(next_session), Ok(&[][..]));
}

#[test]
fn handles_from_another_registry_cannot_alias_live_objects() {
    let (mut first, first_session, first_owner) = fixture();
    let (mut second, second_session, second_owner) = fixture();
    let first_screen = first
        .attach_screen(attachment(first_session, first_owner))
        .unwrap();
    let second_screen = second
        .attach_screen(attachment(second_session, second_owner))
        .unwrap();
    assert_eq!(first_screen.0.slot, second_screen.0.slot);
    assert_eq!(
        second.screen(first_screen),
        Err(RegistryError::UnknownScreen(first_screen))
    );
    assert_eq!(
        second.remove_session(first_session),
        Err(RegistryError::UnknownSession(first_session))
    );
    assert_eq!(
        second.remove_context(first_owner),
        Err(RegistryError::UnknownContextOwner(first_owner))
    );
    assert_eq!(
        second.attach_screen(attachment(first_session, second_owner)),
        Err(RegistryError::UnknownSession(first_session))
    );
    assert_eq!(
        second.attach_screen(attachment(second_session, first_owner)),
        Err(RegistryError::UnknownContextOwner(first_owner))
    );
    assert_eq!(second.screens(second_session), Ok(&[second_screen][..]));
}

#[test]
fn session_tick_guard_blocks_only_attachments_to_that_session() {
    let (mut registry, session, owner) = fixture();
    let other_session = registry.register_session();
    registry
        .with_session_tick(session, |registry| {
            assert_eq!(
                registry.attach_screen(attachment(session, owner)),
                Err(RegistryError::SessionBusy(session))
            );
            assert!(matches!(
                registry.with_session_tick(session, |_| Ok::<_, ()>(())),
                Err(RegistryCallbackError::Registry(RegistryError::SessionBusy(id))) if id == session
            ));
            assert!(registry.screens(session).unwrap().is_empty());
            assert_eq!(registry.linked_screen(owner), Ok(None));
            registry.attach_screen(attachment(other_session, owner)).unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    registry.attach_screen(attachment(session, owner)).unwrap();
}

#[test]
fn context_dispatch_blocks_cross_session_attachment_but_not_removal() {
    let (mut registry, session, owner) = fixture();
    let other_session = registry.register_session();
    let other_owner = registry.register_context("other");
    let screen = registry.attach_screen(attachment(session, owner)).unwrap();
    registry
        .with_context_dispatch(owner, |registry| {
            assert_eq!(
                registry.attach_screen(attachment(other_session, owner)),
                Err(RegistryError::ContextBusy(owner))
            );
            assert!(registry.screens(other_session).unwrap().is_empty());
            assert_eq!(registry.linked_screen(owner), Ok(Some(screen)));
            registry.remove_screen(screen).unwrap();
            assert!(matches!(
                registry.linked_screen(owner),
                Err(RegistryError::StaleLinkedScreen { .. })
            ));
            registry
                .attach_screen(attachment(session, other_owner))
                .unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    registry.attach_screen(attachment(session, owner)).unwrap();
}

#[test]
fn both_busy_flags_preserve_native_session_check_precedence() {
    let (mut registry, session, owner) = fixture();
    registry
        .with_session_tick(session, |registry| {
            registry
                .with_context_dispatch(owner, |registry| {
                    assert_eq!(
                        registry.attach_screen(attachment(session, owner)),
                        Err(RegistryError::SessionBusy(session))
                    );
                    Ok::<_, ()>(())
                })
                .unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(registry.screens(session).unwrap().is_empty());
    assert_eq!(registry.linked_screen(owner), Ok(None));
}

#[test]
fn callback_errors_release_dispatch_guards_without_undoing_completed_changes() {
    let (mut registry, session, owner) = fixture();
    let screen = registry.attach_screen(attachment(session, owner)).unwrap();
    assert!(matches!(
        registry.with_session_tick(session, |registry| {
            registry.remove_screen(screen).unwrap();
            Err::<(), _>("tick failed")
        }),
        Err(RegistryCallbackError::Callback("tick failed"))
    ));
    let screen = registry.attach_screen(attachment(session, owner)).unwrap();
    assert!(matches!(
        registry.with_context_dispatch(owner, |_| Err::<(), _>("linked failed")),
        Err(RegistryCallbackError::Callback("linked failed"))
    ));
    assert_eq!(registry.linked_screen(owner), Ok(Some(screen)));
    registry.attach_screen(attachment(session, owner)).unwrap();
}

#[test]
fn removing_a_dispatch_owner_is_not_an_attachment_busy_error() {
    let (mut registry, session, owner) = fixture();
    let replacement_session = registry
        .with_session_tick(session, |registry| {
            registry.remove_session(session).unwrap();
            Ok::<_, ()>(registry.register_session())
        })
        .unwrap();
    assert_ne!(replacement_session, session);
    let replacement_owner = registry
        .with_context_dispatch(owner, |registry| {
            registry.remove_context(owner).unwrap();
            Ok::<_, ()>(registry.register_context("new backend"))
        })
        .unwrap();
    assert_ne!(replacement_owner, owner);
    registry
        .attach_screen(attachment(replacement_session, replacement_owner))
        .unwrap();
}

#[test]
fn input_installation_is_separate_and_deselecting_another_screen_clears_bindings() {
    let (mut registry, session, owner) = fixture();
    let first = registry.attach_screen(attachment(session, owner)).unwrap();
    let second = registry.attach_screen(attachment(session, owner)).unwrap();
    assert_eq!(registry.selected_screen(), None);
    assert!(matches!(
        registry.select_input(first, |_| -> Result<(), ()> { panic!("must not apply uninstalled context") }),
        Err(RegistryCallbackError::Registry(RegistryError::MissingInputContext(id))) if id == first
    ));

    let mut applied = Vec::new();
    registry
        .bind_input(first, 41, |context| {
            applied.push(context);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(registry.selected_screen(), Some(first));
    registry
        .deselect_input(second, |context| {
            applied.push(context);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(registry.selected_screen(), Some(first));
    registry
        .select_input(first, |context| {
            applied.push(context);
            Ok::<_, ()>(())
        })
        .unwrap();
    registry
        .deselect_input(first, |context| {
            applied.push(context);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(registry.selected_screen(), None);
    assert_eq!(applied, [Some(41), None, Some(41), None]);
}

#[test]
fn input_callback_failures_preserve_the_exact_identity_commit_order() {
    let (mut registry, session, owner) = fixture();
    let first = registry.attach_screen(attachment(session, owner)).unwrap();
    let second = registry.attach_screen(attachment(session, owner)).unwrap();
    registry.bind_input(first, 41, |_| Ok::<_, ()>(())).unwrap();
    assert!(matches!(
        registry.bind_input(second, 82, |context| {
            assert_eq!(context, Some(82));
            Err::<(), _>("selection failed")
        }),
        Err(RegistryCallbackError::Callback("selection failed"))
    ));
    assert_eq!(registry.screen(second).unwrap().input_context, Some(82));
    assert_eq!(registry.selected_screen(), Some(first));
    assert!(matches!(
        registry.deselect_input(first, |context| {
            assert_eq!(context, None);
            Err::<(), _>("clear failed")
        }),
        Err(RegistryCallbackError::Callback("clear failed"))
    ));
    assert_eq!(registry.selected_screen(), None);
}

#[test]
fn unlink_clears_only_matching_selected_identity_and_keeps_input_owner_custody() {
    let (mut registry, session, owner) = fixture();
    let first = registry.attach_screen(attachment(session, owner)).unwrap();
    let second = registry.attach_screen(attachment(session, owner)).unwrap();
    registry.bind_input(first, 41, |_| Ok::<_, ()>(())).unwrap();
    registry.remove_screen(second).unwrap();
    assert_eq!(registry.selected_screen(), Some(first));
    registry.remove_screen(first).unwrap();
    assert_eq!(registry.selected_screen(), None);
    assert!(matches!(
        registry.deselect_input(first, |_| -> Result<(), ()> { panic!("expired screen") }),
        Err(RegistryCallbackError::Registry(RegistryError::UnknownScreen(id))) if id == first
    ));
}

#[test]
fn base_construction_links_scheduler_before_publishing_session_screen_membership() {
    let (mut registry, session, owner) = fixture();
    let previous = registry.attach_screen(attachment(session, owner)).unwrap();
    let screen = registry
        .attach_scheduler_member(attachment(session, owner))
        .unwrap();
    assert_eq!(
        registry.scheduler_members(session),
        Ok(&[screen, previous][..])
    );
    assert_eq!(registry.screens(session), Ok(&[previous][..]));
    assert_eq!(registry.current_screen(session), Ok(Some(previous)));
    assert_eq!(registry.linked_screen(owner), Ok(Some(screen)));
    assert_eq!(
        registry.screen(screen).unwrap().membership,
        ScreenMembership::Pending
    );
    assert_eq!(registry.screen(screen).unwrap().input_context, None);
    assert_eq!(registry.selected_screen(), None);

    registry.link_screen_membership(screen).unwrap();
    assert_eq!(registry.screens(session), Ok(&[screen, previous][..]));
    assert_eq!(registry.current_screen(session), Ok(Some(screen)));
    assert_eq!(
        registry.screen(screen).unwrap().membership,
        ScreenMembership::Linked
    );
    assert_eq!(registry.selected_screen(), None);
    assert_eq!(
        registry.link_screen_membership(screen),
        Err(RegistryError::ScreenMembership {
            screen,
            membership: ScreenMembership::Linked,
        })
    );
    assert_eq!(registry.screens(session), Ok(&[screen, previous][..]));
}

#[test]
fn membership_unlink_keeps_scheduler_input_identity_and_link_live_until_final_removal() {
    let (mut registry, session, owner) = fixture();
    let previous = registry.attach_screen(attachment(session, owner)).unwrap();
    let screen = registry.attach_screen(attachment(session, owner)).unwrap();
    registry
        .bind_input(screen, 42, |_| Ok::<_, ()>(()))
        .unwrap();

    registry.unlink_screen_membership(screen).unwrap();
    assert_eq!(registry.screens(session), Ok(&[previous][..]));
    assert_eq!(registry.current_screen(session), Ok(Some(previous)));
    assert_eq!(
        registry.scheduler_members(session),
        Ok(&[screen, previous][..])
    );
    assert_eq!(registry.linked_screen(owner), Ok(Some(screen)));
    assert_eq!(registry.selected_screen(), Some(screen));
    assert_eq!(registry.screen(screen).unwrap().input_context, Some(42));
    assert_eq!(
        registry.screen(screen).unwrap().membership,
        ScreenMembership::Unlinked
    );
    assert_eq!(
        registry.link_screen_membership(screen),
        Err(RegistryError::ScreenMembership {
            screen,
            membership: ScreenMembership::Unlinked,
        })
    );
    registry.unlink_screen_membership(screen).unwrap();

    // Input owner performs its D0/CC/context destruction at this boundary.
    let registration = registry.remove_screen(screen).unwrap();
    assert_eq!(registration.input_context, Some(42));
    assert_eq!(registry.scheduler_members(session), Ok(&[previous][..]));
    assert_eq!(registry.selected_screen(), None);
    assert_eq!(
        registry.screen(screen),
        Err(RegistryError::UnknownScreen(screen))
    );
    assert_eq!(
        registry.linked_screen(owner),
        Err(RegistryError::StaleLinkedScreen { owner, screen })
    );
}

#[test]
fn failed_base_initialization_can_remove_scheduler_without_ever_publishing_a_screen() {
    let (mut registry, session, owner) = fixture();
    let previous = registry.attach_screen(attachment(session, owner)).unwrap();
    registry
        .bind_input(previous, 41, |_| Ok::<_, ()>(()))
        .unwrap();
    let pending = registry
        .attach_scheduler_member(attachment(session, owner))
        .unwrap();
    registry.remove_screen(pending).unwrap();
    assert_eq!(registry.screens(session), Ok(&[previous][..]));
    assert_eq!(registry.scheduler_members(session), Ok(&[previous][..]));
    assert_eq!(registry.selected_screen(), Some(previous));
    assert_eq!(
        registry.linked_screen(owner),
        Err(RegistryError::StaleLinkedScreen {
            owner,
            screen: pending
        })
    );
}

#[test]
fn membership_unlink_is_allowed_during_dispatch_and_never_clears_another_selection() {
    let (mut registry, session, owner) = fixture();
    let selected = registry.attach_screen(attachment(session, owner)).unwrap();
    let newest = registry.attach_screen(attachment(session, owner)).unwrap();
    registry
        .bind_input(selected, 41, |_| Ok::<_, ()>(()))
        .unwrap();
    registry
        .with_session_tick(session, |registry| {
            registry
                .with_context_dispatch(owner, |registry| {
                    registry.unlink_screen_membership(newest).unwrap();
                    assert_eq!(registry.selected_screen(), Some(selected));
                    assert_eq!(registry.linked_screen(owner), Ok(Some(newest)));
                    assert_eq!(registry.current_screen(session), Ok(Some(selected)));
                    assert_eq!(
                        registry.scheduler_members(session),
                        Ok(&[newest, selected][..])
                    );
                    registry.remove_screen(newest).unwrap();
                    Ok::<_, ()>(())
                })
                .unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(registry.selected_screen(), Some(selected));
    assert_eq!(
        registry.unlink_screen_membership(newest),
        Err(RegistryError::UnknownScreen(newest))
    );
}
