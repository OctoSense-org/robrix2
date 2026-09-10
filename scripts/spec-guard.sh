#!/usr/bin/env bash
# Spec regression gate for robrix2 (pre-commit + CI).
#
# What it enforces (and what it deliberately does not):
#   1. `agent-spec lint --min-score 0.7` on specs changed in this change set.
#      Legacy specs are not re-linted, so old lint debt does not block PRs.
#   2. Structural guards from specs/structure-guards.txt (`agent-spec check-structure`).
#   3. Capability specs (specs/capabilities/*.spec.md) must fully pass
#      (`agent-spec lifecycle`, no skips), then every ADR they satisfy must be
#      `honored` (`agent-spec trace --gate`).
#   4. Task specs. Robrix2 task specs bind UI / homeserver scenarios to
#      `manual_test_*` selectors, which agent-spec reports as `skip`, and the
#      boundary layer applies the whole change set to every spec, so a naive
#      `agent-spec guard` fails on every PR. Instead:
#        - a single changed contract receives the full change set;
#        - multiple active contracts collectively cover every changed path,
#          then each runs `lifecycle` with its owned paths (boundaries + tests).
#          Shared files go to every declaring task; explicit denies still fail;
#        - all other specs are regression checks: run `verify` WITHOUT a change
#          set (tests only), fail on failed>0. Skips are tolerated.
#
# Usage:
#   scripts/spec-guard.sh [--change-scope staged|worktree] [--base <git-ref>] [--fast]
#     --change-scope  local mode: git staged (default) or worktree changes
#     --base <ref>    CI mode: change set = `git diff --name-only <ref>...HEAD`
#     --fast          steps 1-2 only (cheap pre-commit path)
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

SCOPE="staged"; BASE=""; FAST=0
while [ $# -gt 0 ]; do
  case "$1" in
    --change-scope) SCOPE="$2"; shift 2;;
    --base) BASE="$2"; shift 2;;
    --fast) FAST=1; shift;;
    *) echo "unknown arg: $1" >&2; exit 64;;
  esac
done

if ! command -v agent-spec >/dev/null 2>&1; then
  echo "spec-guard: agent-spec CLI not found (cargo install agent-spec --locked)" >&2
  exit 127
fi

# ---- change set ---------------------------------------------------------------
if [ -n "$BASE" ]; then
  mapfile -t CHANGED < <(git diff --name-only "$BASE...HEAD" --diff-filter=ACMR)
elif [ "$SCOPE" = "worktree" ]; then
  mapfile -t CHANGED < <({ git diff --name-only --diff-filter=ACMR; git diff --name-only --cached --diff-filter=ACMR; git ls-files --others --exclude-standard; } | sort -u)
else
  mapfile -t CHANGED < <(git diff --name-only --cached --diff-filter=ACMR)
fi
CHANGED_SPECS=()
for f in "${CHANGED[@]:-}"; do
  case "$f" in
    specs/project.spec.md) ;;  # project-level spec has no scenarios; not held to the task lint bar
    specs/*.spec.md|specs/*/*.spec.md) [ -f "$f" ] && CHANGED_SPECS+=("$f");;
  esac
done

fail=0
say() { printf '\n== %s\n' "$*"; }

# ---- 1. lint changed specs -----------------------------------------------------
say "1/4 lint changed specs (${#CHANGED_SPECS[@]})"
for s in "${CHANGED_SPECS[@]:-}"; do
  [ -n "$s" ] || continue
  if ! agent-spec lint "$s" --min-score 0.7; then echo "spec-guard: lint failed: $s"; fail=1; fi
done

# ---- 2. structural guards ------------------------------------------------------
say "2/4 structural guards (specs/structure-guards.txt)"
if [ -f specs/structure-guards.txt ]; then
  while IFS='|' read -r forbid glob; do
    forbid="$(echo "$forbid" | sed 's/^ *//; s/ *$//')"; glob="$(echo "$glob" | sed 's/^ *//; s/ *$//')"
    [ -z "$forbid" ] && continue; case "$forbid" in \#*) continue;; esac
    if ! agent-spec check-structure --code . --forbid "$forbid" --in "$glob"; then fail=1; fi
  done < specs/structure-guards.txt
fi

if [ "$FAST" = "1" ]; then
  [ "$fail" = "0" ] && echo "spec-guard (fast): OK" || echo "spec-guard (fast): FAILED"
  exit "$fail"
fi

# ---- 3. capability specs + ADR liveness ---------------------------------------
say "3/4 capability specs must fully pass; satisfied ADRs must be honored"
mkdir -p .agent-spec/runs
for cap in specs/capabilities/*.spec.md; do
  [ -f "$cap" ] || continue
  if ! agent-spec lifecycle "$cap" --code . --run-log-dir .agent-spec/runs >/dev/null 2>&1; then
    echo "spec-guard: capability spec not fully passing: $cap"
    agent-spec lifecycle "$cap" --code . --run-log-dir .agent-spec/runs 2>&1 | grep -E '"verdict": "(fail|skip)"' -B6 | grep -E 'scenario_name|reason' | head -20 || true
    fail=1
  else
    echo "ok: $cap"
  fi
  # ADR ids from `satisfies:` frontmatter
  for adr in $(sed -n 's/^satisfies:[[:space:]]*\[\(.*\)\]/\1/p' "$cap" | tr ',' ' '); do
    if ! agent-spec trace "$adr" --gate; then echo "spec-guard: liveness gate failed for $adr"; fail=1; fi
  done
done

# ---- 4. task specs ---------------------------------------------------------------
# 4a. Changed specs own the change collectively, not by intersecting unrelated
#     allow-lists. The helper checks total path coverage and explicit denies,
#     then runs the native lifecycle boundary verifier and tests for each scope.
#     Manual skips stay visible; failures and uncertain results fail the gate.
# 4b. Every other spec is a regression check: verify WITHOUT a change set (no
#     boundary layer, since foreign files would trivially violate them) and fail
#     only on `failed > 0`.
say "4/4 task specs: changed = contract check (with change set), others = regression (tests only)"
CHANGE_ARGS=()
for f in "${CHANGED[@]:-}"; do [ -n "$f" ] && [ -e "$f" ] && CHANGE_ARGS+=(--change "$f"); done
if ! python3 scripts/spec_guard.py "${CHANGE_ARGS[@]}"; then fail=1; fi

if [ "$fail" = "0" ]; then echo; echo "spec-guard: OK"; else echo; echo "spec-guard: FAILED"; fi
exit "$fail"
