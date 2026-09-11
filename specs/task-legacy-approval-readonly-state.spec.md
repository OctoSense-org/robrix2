spec: task
name: "Legacy approval cards — corroborated read-only state"
inherits: project
tags: [approval, frontend, legacy, provenance]
---

## Intent

Show canonical legacy approval status on its positively matched original card without making that card actionable. Preserve the old request payload and native approval admission rules while requiring exact Matrix sender, event relation, room, owner and request tuple evidence for display.

## Constraints

- Every legacy record remains non-actionable and status-only; do not synthesize a native request, owner, publisher or revision for old request content.
- Native binding, revision, claim and account isolation rules remain unchanged.
- Do not add a cache, backend/API/provider calls, public private-locator fields or new approval authority.
- Do not run formatters or change dependencies. Reuse existing localized approval state and runtime-consumed/task-unknown copy.

## Decisions

- Retain an optional original event ID from the legacy status reply relation in ApprovalSession's existing record/view. Conflicting original relations fail closed, including at the same revision.
- `legacy_approval_status_for_original(view, account, room, event_id, actual_original_sender, original_content)` in src/home/room_screen/octos_actions.rs is the pure display predicate. It uses the existing old-request parser and compares the exact relation, tuple, full sender, current owner and room; missing proof returns None.
- Actual status sender is already checked against its publisher by the strict model parser. The display predicate requires that publisher to equal the original event's actual sender. Edited effective content cannot establish proof.
- Rendering uses its existing original_content, source_event_id and original_sender inputs. No legacy request cache or native-request backfill is added; status-first and request-first histories produce the same read-only display.
- Canonical consumed means the runtime used the verdict, with task outcome unknown. Legacy pending remains disabled even if unexpired; terminal state does not enable any control.

## Boundaries

### Allowed Changes
- src/approval_state.rs
- src/home/room_screen/octos_actions.rs
- specs/task-legacy-approval-readonly-state.spec.md

### Forbidden
- Do not edit backend, transport, App lifecycle, marker discovery, dependency or live runtime files.
- Do not relax native request requirements or permit a different publisher to relabel an old request.

## Acceptance Criteria

<!-- display(s,e,a,r) iff legacy(s) AND not_conflicted(s) AND
     original_relation(s)=event_id(e) AND actual_sender(e)=validated_sender(s) AND
     tuple(e)=tuple(s) AND owner(s)=a AND room(s)=r. legacy(s) => not_actionable(s). -->

### Rule: corroborated-read-only — only the exact original card receives canonical legacy state

Scenario: Reject conflicting original relations at the same revision
  Test: legacy_approval_relation_conflict_is_not_a_duplicate
  Given two legacy statuses with equal state revision and tuple but different original event relations
  When the model folds them
  Then it marks the record conflicted instead of treating the second as a duplicate

Scenario: Match the real old request shape in either arrival order
  Test: legacy_approval_old_builder_displays_readonly_state_in_both_orders
  Given an old request without owner publisher and revision and a complete related legacy status
  When request-first and status-first histories are folded and rendered through the display predicate
  Then the canonical consumed state is selected in both orders with its preserved decision
  And the record remains status-only and every verdict claim is denied

Scenario: Reject wrong sender relation tuple owner room or edited content
  Test: legacy_approval_display_rejects_mismatched_original_evidence
  Given original-card evidence differs from a valid legacy status in one identity field
  When the display predicate evaluates it
  Then it returns None and the card remains unavailable

Scenario: Generated identity mismatches never select a legacy state
  Test: prop_legacy_approval_display_requires_exact_evidence
  Given generated unequal identity fields and original relations
  When they are substituted into otherwise valid original-card evidence
  Then no mismatched event receives a canonical legacy state

Scenario: All canonical legacy states remain disabled
  Test: legacy_approval_states_never_create_actionable_requests
  Given pending approved denied expired and consumed legacy statuses
  When the matching old request is displayed
  Then canonical state is retained and no approval becomes actionable

Scenario: Shared epochs notify both panes without consuming invalidation
  Test: legacy_approval_epoch_is_shared_by_two_pane_observers
  Given two pane observers hold the same room epoch before a legacy status arrives
  When the model accepts the status
  Then both observers detect the new epoch and querying one does not clear the other's notification

Scenario: Preserve native terminal claims and epochs
  Test: terminal_status_retires_claim_and_advances_room_epoch
  Given an actionable native request with an existing claim
  When a valid native terminal status arrives
  Then the claim is retired and the room epoch advances

## Out of Scope

- Backend historical event retrieval/attestation, credential generation recovery, publication or migration.
- Live GUI acceptance, provider execution and changing canonical backend approval decisions.
