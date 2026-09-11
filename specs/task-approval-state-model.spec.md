spec: task
name: "Model canonical owner approval state"
inherits: project
tags: [approval, matrix, security, frontend]
---

## Intent

Provide one pure, account-scoped model for strict private approval parsing, canonical state folding, private room discovery, and one-time local verdict claims. UI and Matrix I/O remain separate consumers of this model.

## Decisions

- `ApprovalSession::ingest_message(account_mxid, room_id, event_id, actual_sender, original_content, now_ms)` in `src/approval_state.rs` is the sole request/status admission and fold point.
- `ApprovalMarkerIndex::observe_v2(...)`, `ingest_marker(account_mxid, room_id, event_id, event_type, actual_sender, state_key, original_content, now_ms)`, and `resolve(...)` are the sole marker protocol-floor, admission, and navigation decision points.
- `ApprovalSession::is_actionable(...)`, `try_claim(...)`, and `record_send_result(...)` are the sole local verdict eligibility and claim transition points.
- The session is ephemeral and shared by all panes for an account. Marker cache format v2 migrates persisted v1 single-agent entries into `(agent, project)` tuples with validation cleared; unknown formats fail closed. Persisted entries require fresh authenticated state revalidation before resolution.
- An observed empty-key marker v2 establishes a durable per-room protocol floor. V2 is never downgraded to v1, including after malformed, duplicate, absent, or lower-generation observations.
- A v2 manifest admits at most 64 unique `(agent, project_room_id)` tuples atomically. Different agents may share a project and approval room; one tuple resolving to multiple rooms remains ambiguous.
- This module performs no UI or Matrix I/O and accepts original raw event content plus actual sender/room/account context as plain values.

## Boundaries

### Allowed Changes
- ./src/lib.rs
- ./src/approval_state.rs
- ./src/approval_discovery.rs
- ./src/app.rs
- ./src/sliding_sync.rs
- ./src/home/room_screen/**
- ./src/home/rooms_list.rs
- ./src/shared/approval_card.rs
- ./resources/i18n/**
- ./specs/task-approval-state-model.spec.md
- ./specs/task-approval-state-navigation.spec.md

### Forbidden

- Cargo/dependency changes, provider/backend changes, protocol transport changes, or live Matrix actions.

## Acceptance Criteria

<!--
A(e,t) iff valid_request(e) AND account(e)=owner(e) AND state(e,t)=pending AND t<expiry(e) AND no_claim(e).
F(S,e) preserves the greatest valid monotonic canonical revision; conflicting equal revisions make the request non-actionable.
R(M,a,g,p,t)=Unique(r) iff exactly one fresh, revalidated, active authenticated marker maps (a,g,p) to joined room r.
C(k) permits at most one live claim; only confirmed failure before send can release a still-actionable claim.
-->

### Rule: strict-original-admission — Private approval events bind original content to Matrix context

Scenario: Strict request and status parsing
  Test: approval_state_accepts_exact_original_bindings_and_rejects_sender_owner_digest_and_action_mismatches
  Given original private approval content and actual room sender and account context
  When the model ingests the event
  Then only a complete valid binding is retained and only its exact owner can act

Scenario: Generated invalid bindings fail closed
  Test: prop_invalid_binding_never_becomes_actionable
  Given generated malformed binding fields
  When the model ingests them
  Then no malformed request becomes actionable

### Rule: monotonic-canonical-fold — Canonical state never regresses or forks

Scenario: Status can precede request and terminal state wins
  Test: approval_status_before_request_folds_to_the_highest_terminal_revision
  Given reordered request and status history
  When valid projections are ingested
  Then every pane query observes the same greatest valid terminal state

Scenario: Revision order is invariant
  Test: prop_status_order_preserves_highest_terminal
  Given generated valid terminal revisions
  When they arrive in different orders
  Then the folded terminal result is the same

### Rule: authenticated-marker-resolution — Public navigation resolves only fresh private marker state

Scenario: Marker mappings fail closed on ambiguity staleness and non-membership
  Test: approval_navigation_requires_unique_joined_loaded_room
  Given private marker state for an account agent and project room
  When the model resolves a public navigation request
  Then only one fresh authenticated already-joined room is returned

Scenario: Generated competing markers never choose the first
  Test: prop_approval_marker_resolution_is_unique
  Given generated distinct approval rooms for one association
  When both markers are active
  Then resolution is ambiguous

Scenario: Shared-room v2 preserves every agent and project tuple
  Test: approval_marker_v2_shared_room_resolves_all_four_bindings
  Given three agents and two projects produce four active tuples for one owner room
  When the authenticated v2 manifest is ingested
  Then both project-two agent routes resolve to that same joined room
  And an unknown agent membership observation does not remove a tuple

Scenario: V2 protocol evidence cannot fall back to v1
  Test: approval_marker_v2_floor_blocks_v1_rollback_and_malformed_fallback
  Given a room has exposed an empty-key v2 marker
  When a v1 marker or malformed lower-generation v2 is later observed
  Then the cached room is unavailable and no v1 route resolves

Scenario: Persisted v1 marker cache migrates without authority
  Test: persisted_v1_marker_cache_migrates_to_agent_project_tuple_unvalidated
  Given a serialized cache-format-v1 single-agent marker
  When it is loaded by the cache-format-v2 model
  Then its association includes the legacy agent
  And it remains stale until authenticated current-state revalidation

### Rule: shared-one-time-claim — All panes share one conservative verdict claim

Scenario: Ambiguous transport remains locked
  Test: approval_verdict_outcome_keeps_unknown_claim_locked
  Given an actionable request claimed by one pane
  When transport is successful ambiguous or definitively fails before send
  Then only the definitive pre-send failure can restore eligibility

Scenario: Generated repeats admit one live claim
  Test: prop_repeated_claims_admit_at_most_one
  Given a generated number of pane attempts for one request
  When every pane attempts to claim
  Then at most one live claim is granted

Scenario: Generated late pre-send failures cannot unlock unknown outcomes
  Test: prop_approval_claim_never_releases_unknown_send
  Given an approval claim whose send outcome is unknown
  When generated late pre-send failures are reported
  Then the claim remains locked

## Out of Scope

- Rendering, localized copy, Matrix timeline extraction, sending verdicts, persistent AppState wiring, background marker revalidation, and HAFleet backend behavior.
- This implementation task owns only `src/approval_state.rs`, its `src/lib.rs` inclusion, and this contract. The wider allowed boundary is reserved for the planned UI integration in the same approval feature changeset.
