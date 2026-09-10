spec: task
name: "Pin compatible Makepad fixes for owner approval GUI verification"
inherits: project
tags: [makepad, studio, e2e]
---

## Intent

Validate the canonical approval frontend with the reviewed Makepad Dock visibility, widget path query and PortalList scrolling repairs. Keep the separate pull requests and their dependency relationships explicit.

## Constraints

- Keep the original source trees, user profiles and unrelated running applications unchanged.
- Use only the isolated local Docker Palpo service for subsequent business E2E.
- Preserve strict Studio selectors and explicit action postconditions.
- Do not run Rust formatters, relax gates or reinterpret manual skips as passes.

## Decisions

- This task explicitly authorizes changing the two Makepad fork patches, the harness's two direct Makepad protocol dependencies, and their associated lock entries from dev commit 139d751442608653df2afcef0d0f4f060a10343e to reviewed Makepad PR 10 commit 45e826c2814fccd2c69d558b27775858444c907d, stacked on PRs 8 and 9. All four declarations must agree so the harness and app cannot load separate protocol versions.
- Pin all app and harness Cargo declarations to the same Makepad commit. The previously built Studio hub at 5ab3161938841f905581377a0afff9b8670b44f4 is compatible with this pin: the intervening changes affect only widgets/src/dock.rs, widgets/src/widget_tree.rs and widgets/src/portal_list.rs, with no protocol changes. Record the separate hub, app and harness binaries and require correlated live commands before input; do not infer compatibility for future revisions.
- `unique_match(widgets, selector)` in tools/ux-harness/src/locator.rs remains the pure selection rule. It accepts exactly one visible positive-area match and rejects absent or ambiguous targets.
- No Cargo patch with a personal filesystem path may be introduced.

## Boundaries

### Allowed Changes
- ./Cargo.toml
- ./Cargo.lock
- tools/ux-harness/Cargo.toml
- specs/task-e2e-dock-runtime-alignment.spec.md

### Forbidden
- Do not change frontend behavior, harness selection rules, HAFleet code, live credentials or provider settings in this alignment task.
- Do not update unrelated dependencies or substitute remote homeservers.

## Acceptance Criteria

<!-- Select(W, s) succeeds iff exactly one matching widget is visible and has positive area. -->

### Rule: strict-selection — dependency alignment preserves unambiguous input

Scenario: Reject missing hidden and duplicate targets
  Test:
    Package: ux-harness
    Filter: locator_requires_one_visible_match
  Given missing hidden unique and duplicate widget candidates
  When the runtime harness resolves its target
  Then only the unique visible candidate may receive input

Scenario: Generated candidate counts never select an arbitrary duplicate
  Test:
    Package: ux-harness
    Filter: prop_locator_never_picks_an_ambiguous_widget
  Given generated numbers of matching widgets
  When the harness resolves the selector
  Then it succeeds exactly when one candidate is eligible

Scenario: Verify the aligned real Dock and approval runtime
  Test: manual_test_aligned_dock_approval_runtime
  Given the app and harness use the recorded Makepad pin and the Studio hub has verified unchanged wire types
  When the isolated local E2E exercises cached and split panes with native approval controls
  Then hidden duplicate widgets never receive input
  And canonical approval state remains shared by every visible request copy
  And saved evidence distinguishes Studio results from Computer Use capture limitations

## Out of Scope

- Publishing this integration checkout as a replacement for the separate reviewed PRs
- Inferring full three-layer completion from build or selector tests
