spec: task
name: "Studio Splash compatibility for Robrix widgets"
inherits: project
tags: [makepad, studio, splash, compatibility]
---

## Intent

Restore Robrix startup compatibility with Makepad `5ab3161` after the aligned Studio runtime reported a reserved `self` shader parse error and eleven removed `PortalList.max_pull_down` properties. Preserve the affected widgets' visual behavior while keeping the repair limited to the proven compatibility sites.

## Constraints

- Use Makepad 2.0 property-form shader syntax `pixel: fn()` with an implicit `self` receiver.
- Replace each removed zero pull-down limit with `bounce_at_start: false`, preserving disabled start-edge bounce.
- Do not run formatters, add dependencies, or change unrelated shaders, fonts, updater behavior, approval behavior, or end-edge bounce.
- Runtime acceptance requires the exact aligned Robrix and Makepad heads and correlated Studio evidence; source inspection alone cannot establish runtime acceptance.

## Decisions

- Convert only `RoomFilterSearchResultItem.draw_bg.pixel` from the legacy named-method form to the Makepad 2.0 property callback form.
- Apply the keyboard selection animator through the typed result item's contained View; a custom result widget cannot be borrowed as a standalone View.
- Bind the selected background color as a draw instance and read it through `self` in the shader so deferred shader compilation resolves it.
- Convert only the eleven observed `max_pull_down: 0.0` assignments to `bounce_at_start: false`.
- Verify through the existing Studio harness and real aligned application instead of adding source-string or mirrored implementation tests.

## Boundaries

### Allowed Changes
- src/shared/room_filter_search_results.rs
- src/settings/devices_settings.rs
- src/home/search_messages.rs
- src/home/space_lobby.rs
- src/home/spaces_bar.rs
- src/home/directory_screen.rs
- src/home/room_settings_modal.rs
- src/home/global_message_search.rs
- src/home/room_screen/dsl.rs
- src/home/room_screen/threads_pane.rs
- src/home/room_screen/room_info_pane.rs
- specs/task-studio-splash-compatibility.spec.md

### Forbidden
- Cargo dependency or lockfile changes
- Makepad source changes
- Approval state, Matrix, font, updater, or Dock behavior changes

## Acceptance Criteria

Scenario: Aligned Studio startup has no repaired DSL diagnostics
  Test: manual_test_aligned_studio_startup_has_no_compatibility_diagnostics
  Given Robrix commit `9114bf638b6f534f66b4a3702e60456b0f3be193` plus this patch is built against Makepad `5ab3161`
  When the owned application is launched through the existing Studio harness
  Then its build log contains no reserved `self`, frozen vector, or undefined `max_pull_down` diagnostic

Scenario: Aligned application renders after compatibility repair
  Test: manual_test_aligned_studio_capture_after_compatibility_repair
  Given the owned aligned application has connected to Studio
  When the harness requests a widget snapshot and PNG for the exact build and query identifiers
  Then it receives a nonempty correlated widget tree containing the login or authenticated Robrix view
  And it writes a correlated nonempty PNG

Scenario: Search result selection remains visible
  Test: manual_test_room_filter_selected_highlight_after_shader_conversion
  Given the room filter search results contain at least two entries
  When keyboard selection moves between the entries
  Then only the selected entry renders the existing selected background color

Scenario: Start-edge bounce remains disabled at repaired lists
  Test: manual_test_repaired_portal_lists_disable_start_edge_bounce
  Given each repaired PortalList contains enough rows to scroll
  When the operator pulls beyond the start edge
  Then no pull-down bounce animation is shown
  And normal list scrolling remains usable

## Out of Scope

- Repairing unrelated runtime diagnostics or Makepad resources
- Changing end-edge scrolling behavior
- Approval workflow validation beyond proving the application renders
