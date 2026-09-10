spec: task
name: "Owner approval UI — canonical state, one-time input and private navigation"
inherits: project
tags: [approval, matrix, ux]
---

## Intent

Show HAFleet's canonical approval state on the original request card and let an owner reach an already joined private approval room from a redacted project notice. Preserve the three-layer distinction between an approval verdict, runtime consumption and a task result.

## Constraints

- HAFleet remains the sole verdict authority; Matrix events are its presentation and transport.
- No public approval notice contains a private destination, request ID, digest, tool input or verdict control.
- Use existing MatrixRequest submission, original event content and authenticated full MXIDs.
- Never retry an uncertain verdict with another action, content or transaction identity.
- Preserve unrelated sessions and local Docker Palpo isolation during GUI verification.
- Do not run formatters, change dependencies, lower gates or claim manual scenarios passed from unit tests.

## Decisions

- ApprovalSession and ApprovalMarkerIndex in src/approval_state.rs provide the shared pure state model; their separate task contract governs parsing, fold, claims and discovery evidence.
- AppState holds one ephemeral shared session for all cached and split panes and one tolerant persisted marker cache per account. A cached marker needs current-run authenticated revalidation before use.
- approval_action_notification_matches(source, slot, pane, item, context) in src/home/room_screen/octos_actions.rs matches both decimal widget identities before accepting a Splash notification. Event ID and slot alone are insufficient.
- Canonical state is folded before invalidating request renderings. Each pane observes a shared change epoch; one pane must not drain another pane's invalidation signal. A stale local deadline never replaces an observed terminal verdict.
- The normal Matrix worker carries a claim key and fixed transaction ID with a verdict. Errors before send are distinguishable from unknown results after send. Only confirmed pre-send failure can release a live pending claim.
- ApprovalDiscoverySchedule in src/approval_discovery.rs selects bounded due joined-room batches using account generations and request nonces. Its pure schedule and response acceptance functions reject stale account/membership responses. Authenticated full state preserves actual sender metadata; a content-only cache miss is insufficient. One room fetch selects empty-key marker v2 exclusively whenever present and otherwise permits v1 only below the persisted room protocol floor.
- Background refresh uses an App-owned timer, at most four rooms per tick and at most two concurrent full-state requests with ten-second deadlines. Confirmed negative results expire after sixty seconds; positive results refresh after one hundred twenty seconds. Missing or failed discovery is visible as unavailable navigation.
- The public CTA emits only agent plus its containing project room. The App resolves a unique current-owner, joined-and-loaded private mapping and emits `RoomsListAction::OpenPendingApprovals` with `SelectedRoom::JoinedRoom` for desktop and mobile. The addressed active main-room RoomScreen consumes an exact-room one-shot request after timeline items are ready and shows the latest item; cached threads and ordinary room selection preserve reading position. It never calls the join-capable generic room navigation helper.
- Mobile transitions coalesce one pending intent and revalidate its destination after transition completion.
- The room Info overlay acquires focus after its first visible draw, cancels that pending focus when another widget takes it, and restores prior focus on close only while it still owns focus. Escape remains routed through focused-area hit testing, and pointer handling respects existing capture by overlay modals and children.
- New strings use locale catalogs and existing approval card, badge, row and design tokens. Consumed means runtime received the verdict; the task result remains unknown here.

## Boundaries

### Allowed Changes
- src/app.rs
- src/approval_discovery.rs
- src/approval_state.rs
- src/lib.rs
- src/sliding_sync.rs
- src/home/room_screen/**
- src/home/home_screen.rs
- src/home/main_desktop_ui.rs
- src/home/main_mobile_ui.rs
- src/home/rooms_list.rs
- src/shared/approval_card.rs
- src/i18n.rs
- resources/i18n/**
- specs/task-approval-state-navigation.spec.md
- specs/task-approval-state-model.spec.md

### Forbidden
- Do not add an HAFleet API, bearer token or approval authorization fallback to Robrix.
- Do not add private approval routing data or native verdict actions to public content.
- Do not change backend task state or infer task completion from consumed approvals.
- Do not alter user profiles, global agent settings or unrelated live instances.

## Acceptance Criteria

<!-- notify accepted implies exact pane AND item AND event AND slot;
     send attempted implies shared claim AND exact owner AND pending AND unexpired;
     navigation implies unique AND current account AND joined AND loaded;
     discovery insertion implies current generation AND nonce AND joined membership. -->

### Rule: scoped-notification — only the originating pane and item consume input

Scenario: Reject another cached pane or recycled item
  Test: approval_notification_requires_pane_and_item_ownership
  Given two panes displaying the same source event and slot
  When the source pane receives its notification and another pane receives the same payload
  Then only the matching pane and current item accept it

Scenario: Generated widget identities preserve exact ownership
  Test: prop_approval_notification_matches_all_owner_fields
  Given generated pane, item, event and slot identities
  When a notification is checked against a context
  Then it is accepted exactly when all four identities match

### Rule: discovery-response — late asynchronous results cannot restore stale mappings

Scenario: Reject responses after account change or room removal
  Test: approval_discovery_rejects_late_account_and_membership_results
  Given an in-flight private marker discovery
  When the account changes or the room ceases to be joined and loaded
  Then its result cannot update the marker cache

Scenario: Generated discovery batches remain bounded and fair
  Test: prop_approval_discovery_bounded_fair_batches
  Given generated joined-room sets and repeated refresh ticks
  When the scheduler selects due rooms
  Then no batch exceeds four rooms and each eligible room is eventually selected

Scenario: Full-state selection gives v2 strict precedence
  Test: approval_discovery_selects_v2_without_v1_fallback
  Given one authenticated room-state response contains valid v1 and malformed or duplicate v2 marker events
  When the discovery worker selects a marker
  Then it reports v2 observation and never returns the v1 event

Scenario: Malformed sibling state cannot erase an observed v2 floor
  Test: approval_discovery_raw_error_retains_observed_v2_floor
  Given an authenticated full-state response contains an empty-key v2 marker and a sibling with a non-string event type
  When raw event extraction rejects the response
  Then discovery remains unavailable and still reports the observed v2 protocol floor

### Rule: verdict-outcome — native controls preserve uncertain delivery

Scenario: Separate pre-send failure from unknown send outcome
  Test: approval_verdict_outcome_keeps_unknown_claim_locked
  Given a native verdict with one fixed transaction ID
  When preparation fails or a send response is lost
  Then only preparation failure may release its pending claim
  And the unknown result cannot enable a second decision

Scenario: Generated shared claims cannot emit conflicting verdicts
  Test: prop_approval_claim_never_releases_unknown_send
  Given generated repeated send outcomes and attempts from multiple panes
  When they update the shared approval session
  Then an unknown send retains the original one-time choice

### Rule: native-navigation — public notices only open a resolved joined room

Scenario: Reject absent, ambiguous or unjoined destinations
  Test: approval_navigation_requires_unique_joined_loaded_room
  Given a redacted public notice and private marker candidates
  When the owner activates the navigation control
  Then only one currently joined and loaded owner room can be selected

Scenario: Generated destination sets never choose the first ambiguous room
  Test: prop_approval_marker_resolution_is_unique
  Given generated active private destination candidates
  When the marker index resolves the project notice
  Then selection succeeds exactly for one eligible candidate

Scenario: Pending-approval navigation is deferred, room-bound and one-shot
  Test: latest_approval_navigation_is_room_bound_deferred_and_one_shot
  Given an existing approval room is scrolled to older content and another RoomScreen is cached
  When the public CTA selects the resolved room before its current timeline items are ready
  Then the matching RoomScreen consumes the request once after items arrive and shows its latest item
  And mismatched rooms, cached threads and ordinary room selection do not consume or inherit the request

Scenario: A deferred intent cannot move a reused thread
  Test: deferred_approval_navigation_rejects_same_room_thread_reuse
  Given the selected main room has no loaded items and retains a pending approval navigation
  When its RoomScreen is reused for a thread of the same room before loading completes
  Then the pending request is cleared without scrolling that thread
  And returning to the main room cannot restore the old request

Scenario: Verify real local three-layer approval and navigation
  Test: manual_test_canonical_approval_studio_computer_use
  Given local Docker Palpo, aligned Makepad Studio and the isolated E2E agents
  When Studio and Computer Use select an actual agent through the member picker and activate a private native verdict
  Then all request copies show HAFleet's increasing canonical state
  And consumed is distinct from the independently verified task result
  And the public CTA opens the correct existing private room on desktop and mobile
  And an already-open room is moved from older content to the latest approval while ordinary room selection retains its reading position
  And reconnect and cold history discovery preserve one-time verdicts
  And opening room Info followed by Escape closes it without an extra pane click
  And clicking or pressing Escape in an overlaid room search or invitation modal leaves the underlying room Info pane open

## Out of Scope

- AccessKit, new orchestration layers and remote homeservers
- Treating missing legacy request delivery as permission to create fresh actionable cards
- Inferring other-platform GUI coverage from macOS tests
