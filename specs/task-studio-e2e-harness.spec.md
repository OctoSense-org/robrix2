spec: task
name: "Studio E2E harness — trustworthy evidence and real widget interactions"
inherits: project
tags: [ux-harness, studio, e2e]
---

## Intent

Make the existing ux-harness report actual protocol, capture and process failures and drive Robrix through Makepad Studio. Preserve the static UX gate while recording explicit GUI outcomes for the operator-approved local three-layer E2E.

## Constraints

- Do not run cargo fmt or weaken existing UX thresholds or tests.
- Matrix traffic is restricted to local Docker Palpo at 127.0.0.1:8008.
- Preserve existing user sessions; input and shutdown target only the selected test build/process.
- GUI dispatch uses actual member-picker selection and input events; direct API message injection is not GUI evidence.
- Protocol failures, missing state and stale images never become successful evidence.

## Decisions

- Keep serde/serde_json/png for the existing harness. This task approves direct harness dependencies on the already-locked tungstenite 0.26.2, matching Makepad protocol/micro-serde libraries and the already-approved proptest test dependency; no unrelated dependency upgrades.
- `ROBRIX_DATA_DIR` selects an absolute test profile directory; unset preserves the existing directory and invalid relative/empty overrides fail explicitly.
- `decode` preserves a valid empty widget snapshot but emits an explicit protocol error for malformed widget data or a missing request ID.
- Capture success requires a fresh response/frame and a successfully written artifact. Process cleanup runs on success and early error.
- `unique_match(widgets, selector)` requires exactly one visible match, optionally within an observed container. `input_center(widgets, target, windowed)` converts desktop snapshot coordinates only in windowed mode. Actions wait for an explicit state predicate with a bounded deadline.
- Studio `click`, `scroll`, `type` and `key` steps require an `expect` selector and a 1..60000 ms deadline (default 10000). The CLI validates the entire step plan before input and records success only after a correlated snapshot satisfies the predicate. A preexisting predicate proves observed state, not event acknowledgement or causality; choose a changed value or newly visible control when testing transitions.
- Studio output uses a newly created directory. An existing output path fails before connection or input and is never cleared or overwritten, keeping each run's artifacts separate.
- `validate_capture` checks request identity and PNG content; `Diagnostics::push` bounds retained bytes; `resolve_app_data_dir` isolates the selected profile. Studio binary input uses the official `StudioToAppVec` envelope.
- Business results remain separate from UX scores and require independent Matrix/HAFleet/artifact evidence.

## Boundaries

### Allowed Changes
- tools/ux-harness/**
- src/lib.rs
- src/data_directory.rs
- ./Cargo.toml
- ./Cargo.lock
- src/app.rs
- specs/task-studio-e2e-harness.spec.md
- docs/superpowers/plans/2026-09-09-studio-e2e.md

### Forbidden
- Do not edit global agent configuration, user session databases or unrelated projects.
- Do not raise tools/ux-harness/gate.json thresholds.
- Do not write task done or fabricated inner-loop results from the GUI driver.

## Acceptance Criteria

### Rule: protocol-evidence — malformed protocol data is never valid empty state

Scenario: Reject malformed snapshot fields
  Test:
    Package: ux-harness
    Filter: malformed_widget_snapshot_is_error
  Given a WidgetSnapshot response whose widgets field is an invalid object
  When decode parses the response
  Then it reports a protocol error instead of an empty widget vector

Scenario: Reject a missing correlation ID
  Test:
    Package: ux-harness
    Filter: missing_snapshot_request_id_is_error
  Given a WidgetSnapshot response without request_id
  When decode parses the response
  Then it reports a protocol error instead of substituting zero

Scenario: Accept a valid empty snapshot
  Test:
    Package: ux-harness
    Filter: valid_empty_widget_snapshot_is_preserved
  Given a WidgetSnapshot response with request_id 17 and widgets []
  When decode parses the response
  Then it preserves request_id 17 and the empty vector

Scenario: Preserve Unicode through protocol cleanup
  Test:
    Package: ux-harness
    Filter: prop_protocol_cleanup_preserves_text
  Given generated Unicode labels including Chinese
  When protocol cleanup removes trailing delimiters
  Then valid JSON text remains byte-identical

Scenario: Correlate real websocket replies
  Test:
    Package: ux-harness
    Filter: studio_snapshot_requires_matching_build_and_query
  Given a local websocket server returning stale-query and foreign-build snapshots
  When the harness requests its selected build snapshot
  Then only its matching reply is accepted

Scenario: Fail on a disconnected Studio socket
  Test:
    Package: ux-harness
    Filter: studio_disconnect_is_error_not_empty_state
  Given a local websocket server closing before its reply
  When the harness requests a snapshot
  Then the transport returns an error

### Rule: capture-evidence — stale or unwritten captures fail

Scenario: Reject an old frame after capture deadline
  Test:
    Package: ux-harness
    Filter: capture_deadline_never_reuses_old_file_or_response
  Given only the frame that existed before a screenshot request
  When the capture deadline expires
  Then capture returns an error and no success artifact

Scenario: Reject a capture artifact write failure
  Test:
    Package: ux-harness
    Filter: capture_write_failure_is_error
  Given a valid fresh frame and an unwritable destination
  When capture writes its evidence
  Then it returns the write error

Scenario: Generate request identities
  Test:
    Package: ux-harness
    Filter: prop_capture_requires_matching_request
  Given generated expected and received request identities
  When a valid PNG is checked
  Then acceptance is equivalent to matching request identities

### Rule: lifecycle-evidence — error paths retain diagnostics and cleanup

Scenario: Retain bounded stderr diagnostics
  Test:
    Package: ux-harness
    Filter: stderr_diagnostics_are_bounded_and_retained
  Given an owned child emitting stderr beyond the diagnostic capacity
  When the harness records diagnostics
  Then the newest error is retained and storage remains bounded

Scenario: Reclaim owned process after early error
  Test:
    Package: ux-harness
    Filter: owned_child_is_reaped_on_early_return
  Given an owned test child and a failing startup operation
  When the driver scope ends before explicit shutdown
  Then the child is terminated and reaped

Scenario: Generate bounded diagnostic streams
  Test:
    Package: ux-harness
    Filter: prop_diagnostics_storage_never_exceeds_capacity
  Given generated capacities and Unicode log streams
  When each line is appended
  Then retained bytes never exceed capacity

### Rule: gui-evidence — business acceptance uses the actual UI

Scenario: Preserve default directory and isolate explicit profiles
  Test: data_directory_override_is_explicit
  Given the default profile and an absolute test profile
  When the override is absent or explicitly supplied
  Then absence preserves the default and an absolute override selects only the test profile
  And relative and empty overrides return an error

Scenario: Reject ambiguous locator matches
  Test:
    Package: ux-harness
    Filter: prop_locator_never_picks_an_ambiguous_widget
  Given a generated number of visible matching widgets
  When unique_match selects a target
  Then selection succeeds if and only if the count is one

Scenario: Correct independent-window coordinates
  Test:
    Package: ux-harness
    Filter: prop_windowed_coordinates_are_translation_invariant
  Given a target and window translated by generated offsets
  When input_center converts the observed geometry
  Then the window-local target remains unchanged

Scenario: Send the event envelope decoded by the app
  Test:
    Package: ux-harness
    Filter: studio_input_uses_the_apps_vector_envelope
  Given a real local websocket connection
  When a Chinese text event is sent
  Then the application protocol decodes exactly one event with unchanged text

Scenario: Scroll the observed container and record the resulting state
  Test:
    Package: ux-harness
    Filter: studio_scroll_targets_observed_container_and_records_new_state
  Given a uniquely located scrollable container in an independent window
  When a scroll step runs
  Then the official event targets its window-local center
  And the step evidence records a new snapshot after the scroll

Scenario: Reject mutations without an expected state
  Test:
    Package: ux-harness
    Filter: studio_mutations_require_explicit_postconditions
  Given click, scroll, type and key steps without an expect selector
  When the step plan is decoded
  Then each mutation is rejected before input

Scenario: Reject a reused output directory without altering old evidence
  Test:
    Package: ux-harness
    Filter: studio_rejects_existing_output_without_touching_its_evidence
  Given an output directory containing a previous run's failure record
  When a Studio run selects that directory
  Then it fails before connecting
  And the previous evidence remains unchanged

Scenario: Validate later steps before any earlier mutation is sent
  Test:
    Package: ux-harness
    Filter: studio_validates_the_complete_plan_before_any_input
  Given a valid first mutation followed by an invalid deadline, selector or key
  When the CLI validates the complete step plan
  Then it identifies the invalid step before connecting or creating output

Scenario: Fail when an ignored event never reaches its postcondition
  Test:
    Package: ux-harness
    Filter: studio_ignored_input_cannot_pass_its_postcondition
  Given a local Studio server that accepts an input event but returns unchanged snapshots
  When a mutation waits for the expected new value
  Then it fails within its bounded deadline and preserves failure evidence
  And the input is sent exactly once

Scenario: Wait for a delayed postcondition without replaying input
  Test:
    Package: ux-harness
    Filter: studio_waits_for_delayed_postcondition_without_replaying_input
  Given a local Studio server that returns old state before the expected new value
  When a mutation runs
  Then it records only the snapshot that satisfies the expected value as step evidence
  And the input is sent exactly once

Scenario: Studio drives real mentions and verifies the returned result
  Test: manual_test_studio_local_three_layer_e2e
  Given matching Studio and Robrix builds and local E2E agents
  When Studio and Computer Use select the actual member candidate and send a unique task
  Then the matching Matrix mention routes to the intended middle agent
  And the middle independently monitors and verifies lower work
  And task done, dispatch completed and the unique final same-thread reply have independent evidence

## Out of Scope

- AccessKit and system accessibility integration
- Claiming Windows/Linux GUI coverage from macOS execution
- Replacing Herdr or implementing a new agent orchestrator inside ux-harness
