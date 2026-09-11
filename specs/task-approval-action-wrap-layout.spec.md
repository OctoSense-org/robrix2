spec: task
name: "Approval action rows retain inner responsive wrapping"
inherits: project
tags: [bugfix, makepad, layout, approval]
---

## Intent

Remove the repeated Makepad error caused by a Fill-width Splash directly inside a wrapping horizontal action row. Preserve responsive action-button wrapping and existing message/pane ownership in full, condensed and private approval cards.

## Decisions

- Change the two `action_button_row` containers in `src/home/room_screen/message.rs` and the `approval_action_button_row` container in `src/shared/approval_card.rs` to `flow: Down`; retain their existing Fill-width, Fit-height Splash children.
- The generated Splash body remains the owner of `Flow.Right{wrap: true}` and Fit-sized buttons or disabled labels. Preserve spacing, label text, action payloads, and pane/message ownership.
- This is a layout-only correction with two manual runtime checks. Source diff review verifies exactly three flow changes, with the inner Splash generator and event/ownership code unchanged. Build and runtime evidence are recorded separately; source inspection does not count as a runtime pass.

## Boundaries

### Allowed Changes
- src/home/room_screen/message.rs
- src/shared/approval_card.rs
- specs/task-approval-action-wrap-layout.spec.md

### Forbidden
- Do not modify dependencies, generated Splash bodies, approval state, action dispatch, or widget visibility policy.
- Do not suppress Makepad diagnostics or remove action buttons to silence the error.
- Do not run Rust formatters.

## Out of Scope

- Approval protocol or room-discovery changes.
- Native approval decisions or provider calls during layout verification.
- General timeline, typography or theme redesign.

## Completion Criteria

Scenario: Visible action rows do not repeat the unsupported Fill-width error
  Test: manual_test_approval_action_rows_no_wrap_fill_error
  Given the built app renders a full message action row, a condensed message action row and a private approval action row
  And a fresh log cursor excludes warnings from earlier binaries
  When the visible action rows redraw during a "30" second observation window including narrow and wide room panes
  Then the log contains "0" new occurrences of "flow: Right { wrap: true } does not support width: Fill"
  And each tested row has positive visible geometry

Scenario: Narrow action rows preserve wrapping and source ownership
  Test: manual_test_approval_action_rows_wrap_and_keep_ownership
  Given full and condensed message action rows and a private approval card with multiple action buttons are visible
  And a disabled action row and multiline approval text are available
  When the room pane changes between widths of "360" and "800" pixels
  Then buttons that exceed the available line width continue on the next line within their own message
  And disabled labels and approval text remain readable within the pane
  And a regular action's captured pane and source-message identity match the row that contains it
  And no native approval decision or provider call is issued by this layout check
