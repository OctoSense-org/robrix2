spec: task
name: "Studio transport — resumable upgrade and phase deadlines"
inherits: project
tags: [ux-harness, studio, transport]
---

## Intent

Make the Studio harness retain a partially completed WebSocket upgrade across temporary socket waits without extending its total deadline. Report the failing transport phase so an HTTP upgrade timeout is distinguishable from a missing Hello, command response or capture.

## Constraints

- Preserve the existing 5-second TCP connection, 5-second upgrade, 5-second Hello and 25-second capture budgets. Snapshot and locator operations retain their existing caller deadline.
- Retry only temporary socket waits inside the current connection and operation; do not reconnect, replay input or submit another capture request after uncertainty.
- Preserve query/build correlation, strict unique locators, fresh capture validation and failure evidence files.
- Do not run formatters, upgrade dependencies or change application, Matrix, Studio server or operating-system settings.

## Decisions

- Keep the pinned tungstenite transport. Resume its interrupted MidHandshake with its buffered state and the original absolute upgrade deadline.
- The pure function `transport_wait(deadline: Instant, now: Instant) -> Option<Duration>` in tools/ux-harness/src/studio.rs returns the positive remaining duration capped at a 50 ms socket wait, or None at/after the deadline. A narrow TCP stream adapter applies this decision before each underlying read/write, including tungstenite's internal fragment loops; operation loops retain the same deadline.
- Error messages identify the phase as Studio upgrade, Studio Hello, Studio command or Studio capture, and distinguish deadline expiration from protocol/disconnection errors. Existing failure.txt records the returned error unchanged.
- Synthetic loopback servers test the real StudioDriver APIs without starting an app or sending traffic to the owned live Studio. Fragment progress beyond five seconds must not extend the upgrade budget; progress within budget is accepted on the original connection.
- These changes do not establish the cause of the intermittent live Studio handshake failures or clear the separate Computer Use window-discovery blocker. No product UI or i18n strings change.

## Boundaries

### Allowed Changes
- tools/ux-harness/src/studio.rs
- tools/ux-harness/README.md
- specs/task-studio-transport-deadlines.spec.md

### Forbidden
- Do not modify Cargo.toml, Cargo.lock, the application sources or Makepad sources.
- Do not reconnect or replay a control/capture command after a timeout.
- Do not weaken existing test assertions, locator uniqueness or UX gate thresholds.

## Acceptance Criteria

<!-- W(D,t) = None iff t >= D; otherwise 0 < W(D,t) <= min(50ms,D-t).
     The deadline D is fixed for each phase. Waiting never resubmits a command. -->

### Rule: fixed-phase-deadline — progress cannot reset the budget

Scenario: Reject an upgrade that keeps progressing beyond its total deadline
  Test:
    Package: ux-harness
    Filter: studio_transport_upgrade_progress_cannot_extend_deadline
  Given a real server fragments its upgrade response over six seconds
  When StudioDriver connects with the existing five-second upgrade budget
  Then it reports Studio upgrade deadline expired before accepting Hello
  And no replacement connection is opened

Scenario: Accept fragmented upgrade progress within the deadline
  Test:
    Package: ux-harness
    Filter: studio_transport_fragmented_upgrade_succeeds_on_original_connection
  Given upgrade fragments arrive across multiple temporary socket waits
  When the complete response and Hello arrive within the budget
  Then the original connection succeeds without restarting the upgrade

Scenario: Reject an expired wait without a zero socket timeout
  Test:
    Package: ux-harness
    Filter: studio_transport_wait_boundary
  Given the current time equals or exceeds the phase deadline
  When transport_wait calculates the next socket wait
  Then it returns None instead of an indefinite or extended timeout

Scenario: Generated waits preserve the remaining deadline
  Test:
    Package: ux-harness
    Filter: prop_studio_transport_wait_never_extends_deadline
  Given generated phase budgets and elapsed times
  When transport_wait calculates the next socket wait
  Then it returns None exactly when elapsed time reaches the budget
  And every positive wait is at most fifty milliseconds and the remaining budget

Scenario: Distinguish a missing Hello from an upgrade failure
  Test:
    Package: ux-harness
    Filter: studio_transport_hello_timeout_names_phase
  Given the HTTP upgrade succeeds but no Hello arrives
  When the five-second Hello deadline expires
  Then the error identifies Studio Hello deadline expired

Scenario: Distinguish a silent command response
  Test:
    Package: ux-harness
    Filter: studio_transport_command_timeout_names_phase_without_replay
  Given a server accepts one snapshot request and sends no response
  When the caller deadline expires
  Then the error identifies Studio command deadline expired
  And exactly one request was sent

Scenario: Distinguish an absent capture without creating success evidence
  Test:
    Package: ux-harness
    Filter: studio_transport_capture_timeout_names_phase_without_replay
  Given a server accepts one screenshot request and sends no response
  When the twenty-five-second capture deadline expires
  Then the error identifies Studio capture deadline expired
  And exactly one capture request was sent and no capture file was created

Scenario: Reject continuously progressing response fragments after the deadline
  Test:
    Package: ux-harness
    Filter: studio_transport_command_fragments_cannot_extend_deadline
  Given a correlated snapshot frame arrives one byte every twenty milliseconds
  When the 150 ms command deadline expires before the complete frame
  Then the command fails within 300 ms instead of accepting the late snapshot

Scenario: Preserve a partially written command through temporary backpressure
  Test:
    Package: ux-harness
    Filter: studio_transport_slow_command_write_keeps_one_frame
  Given a server stops reading for 250 ms after the first bytes of an input arrive
  When the harness writes the remainder inside its existing command budget
  Then the receiver obtains exactly one complete input frame
  And no command is recreated after a partial write

Scenario: Delayed state observation does not replay the input
  Test:
    Package: ux-harness
    Filter: studio_waits_for_delayed_postcondition_without_replaying_input
  Given one input produces the expected state after several snapshots
  When the harness waits for its correlated postcondition
  Then it observes the state after sending that input exactly once

Scenario: Failed attachment preserves the CLI evidence
  Test:
    Package: ux-harness
    Filter: studio_connection_failure_preserves_cli_evidence
  Given the server disconnects before Hello
  When the CLI run fails to attach
  Then failure.txt contains the actual returned transport error

## Out of Scope

- New timeout flags, automatic reconnection, command replay or changes to Studio protocol schemas.
- Diagnosing the live Studio process stall, GPU behavior, Computer Use access or operating-system permissions.
- Provider, Matrix, native approval and live GUI execution; the root agent owns that validation separately.
