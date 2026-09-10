spec: task
name: "Scope room pane navigation actions to their owning controls"
inherits: project
tags: [bugfix, threads, search, actions]
---

## Intent

Selecting a thread in one room screen must not start an event search in another
cached room or thread tab. Carry pane ownership through dynamic thread/search
rows and consume pane, toolbar, and header actions only in their owning screen.

## Constraints

- Preserve actions for other room screens and split views; do not discard all actions in inactive tabs.
- Keep Matrix requests, message actions, timeline lifecycle, and global app navigation unchanged.
- Do not change layout, visible strings, locale keys, dependencies, or formatter policy.
- Do not contact Matrix services in automated tests.

## Decisions

- The pure function `room_control_action(action: &Action, owners: &[WidgetUid]) -> Option<&WidgetAction>` in `src/home/room_screen/action_scope.rs` is the ownership decision used by room controls.
- Dynamic thread and search entries store their pane's WidgetUid at binding time and emit selection under that owner rather than their recycled row UID.
- Thread pane actions accept only the owning pane UID. Toolbar and RoomTopBar actions accept only their own control UID. Search actions accept the local pane, search button, and room-screen relay UIDs.
- Example and property tests verify WidgetAction ownership before building. Studio/CUA checks the rendered entry-to-pane wiring. The original blanket cast is retained only long enough to record a failing regression, then replaced by the ownership decision.
- The i18n review finds no new text or changed alignment; translation resources are unchanged.

## Boundaries

### Allowed Changes
- src/home/room_screen/action_scope.rs
- src/home/room_screen/mod.rs
- src/home/room_screen/threads_pane.rs
- src/home/room_screen/search.rs
- src/home/search_messages.rs
- specs/task-room-pane-action-ownership.spec.md

### Forbidden
- Do not edit src/app.rs, src/event_preview.rs, tools/ux-harness/**, or src/sliding_sync.rs.
- Do not run cargo fmt or rustfmt.
- Do not commit or create a PR before user testing.

## Acceptance Criteria

<!--
  pane-owner: accept(a, owners) <=> widget(a) exists and uid(a) != 0 and uid(a) in owners
  The owner is a widget instance, not a room ID: same-room thread tabs remain distinct.
  Filtering never consumes or rewrites another screen's action.
-->

### Rule: pane-owner — Room controls accept only an explicitly owned action

Scenario: Other thread panes cannot consume a root selection
  Test: room_control_action_keeps_thread_selection_in_its_own_pane
  Given main, Claude-thread, Codex-thread and other-room panes have distinct widget identities
  When the main pane emits a Codex root selection
  Then only the main pane accepts that action and the other panes retain no jump target

Scenario: Search pane and header relays stay local
  Test: room_control_action_accepts_local_search_controls_only
  Given search pane, search button and header relay identities belong to one room screen
  When each control emits a search action
  Then that screen accepts the original action and a different room screen rejects it

Scenario: Missing ownership fails closed
  Test: room_control_action_rejects_missing_or_zero_ownership
  Given a non-widget action, an empty owner list, or an unbound zero widget identity
  When ownership is checked
  Then no action is accepted

Scenario: Generated identities preserve action isolation
  Test: prop_room_control_action_matches_explicit_owner_set
  Given arbitrary source identities and owner sets
  When ownership is checked
  Then acceptance equals nonzero source membership in the owner set

## Out of Scope

- Repairing unrelated global-search routing in src/app.rs.
- Changing background pagination completion or Matrix history availability.
