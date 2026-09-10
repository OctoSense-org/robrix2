spec: task
name: "Studio native widget path selection"
inherits: project
tags: [ux-harness, studio, locator]
---

## Intent

Select repeated widget IDs only when the app's retained widget graph proves the requested ancestry and effective interaction visibility. Preserve ambiguity and transport uncertainty instead of guessing from geometry or response order.

## Constraints

- Path segments are exact named widget IDs. Unnamed intermediary nodes may be skipped; unexpected named nodes may not.
- Require a correlated native path response and matching snapshot state before input.
- Do not fall back to ID, geometry order, dump indexes, or the first match.
- Preserve the existing absolute deadlines and no-replay behavior.

## Boundaries

### Allowed Changes
- tools/ux-harness/src/locator.rs
- tools/ux-harness/src/studio.rs
- tools/ux-harness/README.md
- specs/task-studio-widget-path-selection.spec.md

### Forbidden
- Do not modify dependencies, application sources, or Makepad sources in this Robrix unit.
- Do not select an ambiguous native path or an unmatched snapshot rectangle.

## Acceptance Criteria

Scenario: Native ownership and window translation
  Test:
    Package: ux-harness
    Filter: path_match_uses_native_ownership_and_window_translation
  Given a native path result uses window-local coordinates
  When the snapshot carries desktop coordinates
  Then the harness matches only after translating the owning window

Scenario: Ambiguity and legacy responses fail closed
  Test:
    Package: ux-harness
    Filter: path_match_preserves_native_ambiguity_and_rejects_legacy_or_invalid_rows
  Given zero, multiple, or malformed native path rows
  When the harness resolves the selector
  Then it reports missing, ambiguous, or invalid state without a fallback

Scenario: Studio query correlation
  Test:
    Package: ux-harness
    Filter: studio_path_query_requires_exact_correlation_before_snapshot_join
  Given unrelated query and build responses arrive before the requested path result
  When the Studio driver waits for the native path query
  Then only the exact query ID, build ID, and query string are accepted

## Out of Scope

- Binary protocol changes, live GUI execution, geometry joins, and index-based selection.
