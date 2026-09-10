spec: task
name: "Check composed task changes without intersecting unrelated boundaries"
inherits: project
tags: [ci, contracts, regression]
---

## Intent

Validate a PR containing multiple independently scoped tasks without requiring every task to own every changed file. Preserve file coverage, explicit prohibitions, complete task tests and readable failure reporting.

## Decisions

- For two or more changed task contracts, `partition_changes(changes, documents)` assigns each path to every active contract whose allowed path pattern matches. Every changed file must have an owner; an explicit forbidden path in any active contract fails the composition. No existing task boundary is expanded.
- Read boundary categories through `agent-spec parse --format json`. Scoped paths still go through the real agent-spec lifecycle boundary verifier and all bound tests. Single-contract verification continues to receive the complete change set; unchanged contracts remain regression checks.
- Parse lifecycle and verification JSON structurally. Invalid reports fail closed. Print bounded diagnostics without pipelines that terminate the producer early, and continue checking other contracts after a failure.
- Preserve the existing manual-scenario skip policy and capability/ADR gates. Report skips separately; they never become passing E2E evidence.
- [platform-specific] The Bash CI entry point and its Python 3 integration tests run on the existing macOS/Linux tooling hosts. Cargo workspace tests bind the Python regression suite through the ux-harness package.

## Boundaries

### Allowed Changes
- scripts/spec-guard.sh
- scripts/spec_guard.py
- scripts/test_spec_guard.py
- tools/ux-harness/tests/spec_guard.rs
- specs/task-spec-guard-composition.spec.md

### Forbidden
- Do not broaden existing task boundaries or remove acceptance scenarios.
- Do not change product code, Cargo dependencies, workflow skip conditions or UX thresholds.
- Do not accept files outside all active contracts or hide failed test results.

## Acceptance Criteria

<!-- Composed acceptance requires coverage of every changed path and successful verification of every active task's owned paths. -->
### Rule: coverage — every changed path has an accountable task

Scenario: Independent tasks verify only their declared paths
  Test:
    Package: ux-harness
    Filter: spec_guard_composition_regressions
  Given two changed contracts with disjoint allowed files and one unchanged contract
  When the gate checks the combined PR
  Then each changed contract receives only its own files
  And both task lifecycles and the unchanged regression execute

Scenario: Unowned and forbidden files fail the combined gate
  Test:
    Package: ux-harness
    Filter: spec_guard_composition_regressions
  Given a file outside all active boundaries or explicitly forbidden by an active contract
  When another contract permits the remaining files
  Then the gate fails and identifies the uncovered or forbidden path

Scenario: Shared files are checked by every declaring task
  Test:
    Package: ux-harness
    Filter: prop_spec_guard_composition_keeps_all_owners
  Given generated sets of active contracts sharing one changed file
  When the gate partitions the changes
  Then every declaring contract receives the shared file
  And an additional unowned file always fails

### Rule: truthful-failure — diagnostics cannot erase failures or stop later checks

Scenario: Malformed reports and failed tests remain failures
  Test:
    Package: ux-harness
    Filter: spec_guard_composition_regressions
  Given invalid JSON or a task with many failed scenarios
  When the gate prints its diagnostics
  Then the final exit is nonzero
  And later contracts are still checked
  And the final failure summary is printed without a broken pipe

Scenario: Single-contract boundaries and manual skips retain their meaning
  Test:
    Package: ux-harness
    Filter: spec_guard_composition_regressions
  Given one changed contract with an out-of-scope file or a manual skipped scenario
  When the gate runs the original full-change lifecycle check
  Then the out-of-scope change fails
  And a permitted manual skip is reported separately from passes

## Out of Scope

- Changing agent-spec's boundary language or Rust test runner.
- Certifying manual UI scenarios or complete business E2E.
