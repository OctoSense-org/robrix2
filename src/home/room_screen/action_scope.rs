//! Widget-instance ownership for room controls shared by multiple cached tabs.

use makepad_widgets::*;

pub(super) fn room_control_action<'a>(action: &'a Action, owners: &[WidgetUid]) -> Option<&'a WidgetAction> {
    let action = action.as_widget_action()?;
    (action.widget_uid.0 != 0 && owners.contains(&action.widget_uid)).then_some(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::room_screen::ThreadsPaneAction;
    use crate::home::search_messages::SearchMessagesAction;
    use matrix_sdk::ruma::OwnedEventId;
    use proptest::prelude::*;

    fn emitted_by(uid: u64, payload: impl WidgetActionTrait) -> Action {
        Box::new(WidgetAction {
            data: None,
            action: Box::new(payload),
            widget_uid: WidgetUid(uid),
            group: None,
        })
    }

    #[test]
    fn room_control_action_keeps_thread_selection_in_its_own_pane() {
        let root = OwnedEventId::try_from("$codex-root").unwrap();
        let action = emitted_by(11, ThreadsPaneAction::OpenThread(root.clone()));
        // Main and two thread tabs share a Matrix room, but never a widget UID.
        for (pane, uid) in [("Claude thread", 12), ("Codex thread", 13), ("other room", 21)] {
            assert!(room_control_action(&action, &[WidgetUid(uid)]).is_none(), "{pane} accepted another pane's jump");
        }
        let selected = room_control_action(&action, &[WidgetUid(11)]).unwrap();
        assert!(matches!(selected.cast_ref::<ThreadsPaneAction>(), ThreadsPaneAction::OpenThread(event) if event == &root));
        assert_eq!(action.as_widget_action().unwrap().widget_uid, WidgetUid(11));
    }

    #[test]
    fn room_control_action_accepts_local_search_controls_only() {
        let local = [WidgetUid(30), WidgetUid(31), WidgetUid(32)];
        let other = [WidgetUid(40), WidgetUid(41), WidgetUid(42)];
        for source in local {
            let action = emitted_by(source.0, SearchMessagesAction::OpenRequested);
            assert!(matches!(room_control_action(&action, &local).cast_ref(), SearchMessagesAction::OpenRequested));
            assert!(room_control_action(&action, &other).is_none());
        }
    }

    #[test]
    fn room_control_action_rejects_missing_or_zero_ownership() {
        let plain: Action = Box::new(());
        assert!(room_control_action(&plain, &[WidgetUid(1)]).is_none());
        assert!(room_control_action(&emitted_by(1, ThreadsPaneAction::CloseRequested), &[]).is_none());
        assert!(room_control_action(&emitted_by(0, ThreadsPaneAction::CloseRequested), &[WidgetUid(0)]).is_none());
    }

    proptest! {
        #[test]
        fn prop_room_control_action_matches_explicit_owner_set(
            source in prop_oneof![Just(0), any::<u64>()],
            mut owners in prop::collection::vec(any::<u64>(), 0..8),
            include_source in any::<bool>(),
        ) {
            if include_source {
                owners.push(source);
            }
            let expected = source != 0 && owners.iter().any(|owner| *owner == source);
            let action = emitted_by(source, ThreadsPaneAction::LoadMoreRequested);
            let owners = owners.into_iter().map(WidgetUid).collect::<Vec<_>>();
            prop_assert_eq!(room_control_action(&action, &owners).is_some(), expected);
        }
    }

}
