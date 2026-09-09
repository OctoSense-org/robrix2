# ux-harness

A UX audit harness for Robrix2. It drives the real app through **makepad's
internal event channel**, captures the frames the app actually renders, and
turns both into a scored report with per-finding evidence.

The headless mode uses Makepad's CPU renderer over stdin/stdout and works over
SSH and in CI. The Studio mode attaches to a real running desktop build for
widget queries and event injection. Computer Use can independently check the
same window's focus, popup visibility and rendered appearance.

## What it measures

| Dimension | Weight | How |
|---|---:|---|
| Layout integrity | 1.5 | post-layout widget rects: unreachable, overflowing, collapsed, or stacked controls |
| Target size | 1.0 | every enabled control against `RBX_TAP_MIN` (mobile) / 28dp (desktop) |
| Legibility & contrast | 1.5 | WCAG contrast of the design tokens **and** of real text sampled from rendered pixels |
| Feedback & responsiveness | 1.5 | click → frame diff + widget-tree diff; how long the UI keeps redrawing |
| Keyboard & focus | 1.5 | Tab presses that change no pixels = no visible focus indicator |
| State coverage | 1.5 | blank-screen detection; presence of an actionable control |
| Visual consistency | 1.0 | raw hex literals outside the token layer |
| Internationalization | 1.5 | untranslated screens, missing keys, byte-identical translations |

Each dimension starts at 10 and loses 3.0 / 1.5 / 0.6 / 0.2 per
critical / high / medium / low finding. The overall score is the weighted mean
of the dimensions that actually ran — a dimension whose rules did not run is
reported as **not run**, never as a pass.

## Usage

### Static only (runs anywhere, no build needed)

```bash
cargo run -q -p ux-harness -- static --repo . --out target/ux-audit
```

Scores token contrast, token discipline, and translation coverage from the
source tree.

### Gate (CI pass/fail)

Scores are informative; the **gate** is the verdict. `--gate <policy.json>`
sums each finding's `count` per rule and compares it with the policy:

```bash
cargo run -q -p ux-harness -- static --repo . --out target/ux-audit --gate tools/ux-harness/gate.json
# exit 0 = passed, 3 = gate failed (table printed and written to <out>/ux-gate.md)
```

`gate.json` is a **ratchet**: `max` records today's baseline for rules that
still have debt (`i18n.untranslated-screen`, `i18n.untranslated-value`) and is
`0` for rules that must never regress (`i18n.missing-key`,
`visual.hardcoded-color`, `legibility.token-contrast`). Lower a `max` when you
pay debt down; a PR that raises one must say why. `"unlisted": "fail"` means a
new rule with findings has to be triaged into the policy before CI goes green.
CI runs this in the `spec_gate` job on every PR.

### Full audit (drives the app)

Build the app with makepad's headless CPU renderer, then point the harness at
the binary:

```bash
MAKEPAD=headless CARGO_TARGET_DIR=target-headless cargo build --profile fast

cd tools/ux-harness
cargo run -- run \
  --repo ../.. \
  --app ../../target-headless/fast/robrix \
  --out ../../target/ux-audit
```

Outputs `ux-report.md`, `ux-report.json`, and every captured frame under
`frames/` (including `scene_*.png`, one per measurement point).

`rustc` must be on `PATH`: the headless renderer JIT-compiles each shader into
a dylib on first use. The JIT cache lands in `--out/jit`, so a second run is
much faster than the first.

### Contrast helper

```bash
cargo run -- contrast --fg '#687283' --bg '#FFFFFF'
# #687283 on #FFFFFF = 4.86:1 (target 4.5:1)
# passes
```

When a pair fails it prints the nearest compliant color along the same hue, so
a palette fix keeps its hue relationships.

### Attach to a Studio build

Build Studio and Robrix with the same Makepad revision. Launch an isolated
Studio hub on loopback and start the test Robrix with an absolute
`ROBRIX_DATA_DIR`. Use a unique application bundle ID when another Robrix
instance is running, so Computer Use selects the same instance as Studio.

```bash
cargo build --release --locked -p ux-harness
target/release/ux-harness studio --address 127.0.0.1:18001 \
  --build-id 3 --app-mode windowed --steps steps.json --out target/ux-studio
```

`steps.json` is an array, for example:

```json
[
  {"action":"wait","selector":{"id":"home_screen_view"},"timeout_ms":10000},
  {"action":"click","selector":{"text":"e2e project room","within":{"id":"rooms_list"}},"expect":{"id":"at_mention_button"}},
  {"action":"click","selector":{"id":"at_mention_button"},"expect":{"id":"popup"}},
  {"action":"wait","selector":{"id":"popup"},"timeout_ms":10000},
  {"action":"capture","label":"member-picker"}
]
```

Selectors combine exact `id`, `text`, `widget_type`, `value`, `window_index`
and optional `within` geometry. Missing targets wait to a bounded deadline;
ambiguous targets fail. `type` injects text; `key` supports navigation, Enter,
Escape, Backspace and `a` with optional `logo`/`shift` modifiers. Every `click`,
`scroll`, `type` and `key` requires an `expect` selector. The harness polls fresh,
correlated snapshots until that expected state appears, with `timeout_ms` in
1..60000 (default 10000), and saves the exact matching snapshot. Polling never
replays input. The whole plan is validated before connection or input.
Choose a changed value or newly visible control to test a transition: a
predicate that was already true proves only observed state, not event
acknowledgement or causality. Event delivery alone is not a business result.

`--out` must name a new directory. Existing paths are rejected before connection
and are never cleared or overwritten. Keep step files outside that directory;
choose a different output path for every run so stale artifacts cannot mix with
current evidence. Re-running a step file can repeat its mutations; inspect the
current app state and prepare only the remaining steps after a failure.

`windowed` converts desktop snapshot coordinates using the current Window
geometry; `embedded` uses desktop coordinates directly. The pinned Studio
revision ignores the input window index, so non-primary-window input fails
explicitly. The attachment never stops or clears builds. Screenshot responses
must match both query and build IDs, decode as PNG and persist successfully.
Failures save diagnostics and the last observed snapshot. Password input
values are redacted from snapshot artifacts; keep credential step files private
and remove them after use.

Keep Studio snapshots/captures, Computer Use screenshots and independent
Matrix/HAFleet/artifact checks as separate evidence. A static UX score does not
prove GUI behavior, agent work, or final message delivery.

### Combined desktop verification and current limits

The September 9 macOS run used one uniquely bundled Robrix test process for
both transports. A Cargo binary with only a new compile-time name was not
sufficient for Computer Use discovery; an actual `.app` with a unique
`CFBundleIdentifier` and a Studio-launched executable wrapper worked. Keep its
profile separate from every existing Robrix process.

Use Studio to find a unique visible control, inject text, assert its resulting
value and capture correlated evidence. Use Computer Use to activate the real
window, inspect visual layout and select candidates that the snapshot cannot
identify. Re-query Studio after each Computer Use action before injecting the
next event. For `@`, verify the chosen candidate in the composer before sending
and verify the structured Matrix mention afterward.

Use a container selector for scrolling, for example
`{"action":"scroll","selector":{"id":"threads_list"},"sx":0,"sy":500,"expect":{"text":"Expected thread title"}}`.
The harness resolves fresh geometry, injects the scroll at that container's
center and waits for the expected visible thread before saving its snapshot.
Replace the example title with the actual expected result. Positive `sy` scrolls down. Inspect
the new state before choosing a thread; a long first entry can fill the pane.

Observed limitations in the pinned fork:

- Dynamic member rows were visible on screen but lacked usable text/geometry
  in snapshots. Computer Use performed the real selection.
- Inactive Dock tabs retained `visible: true` input entries at identical
  coordinates. Treat ambiguity as a failure; do not choose the first entry.
- Focus affected member-popup behavior. Successful event injection alone did
  not establish that the popup remained visible.
- With the Mac locked, Computer Use returned `cgWindowNotFound` for both Chrome
  and Robrix while Studio snapshots still responded. Continue protocol checks,
  but keep real desktop/dashboard acceptance pending until an unlocked run.

These are separate evidence limitations, not a claim of full accessibility or
cross-platform GUI coverage. Preserve failed attempts when a later retry works.

## How it drives the app

Makepad's headless backend (`MAKEPAD=headless`, run with `--stdin-loop`) reads
`StudioToApp` messages on stdin and writes `AppToStudio` on stdout — the same
channel Makepad Studio uses to remote-control a running app.

- **Input**: `MouseDown/Up/Move`, `KeyDown/Up`, `TextInput`, `Scroll` are
  delivered as genuine `Event`s, through the same dispatch a real click takes.
- **Introspection**: `WidgetSnapshot` exposes widget geometry, visibility, enabled state and text. Coverage depends on each widget implementation; validate it against the visible UI.
- **Capture**: every draw cycle writes a PNG to `MAKEPAD_HEADLESS_OUT_DIR`.

Two details are load-bearing:

1. **The backend is pull-driven.** It only advances timers, next-frames and
   repaints when it receives a `Tick`, and answers with `RequestAnimationFrame`
   while it still has work. `Driver::settle` ticks until the app goes quiet,
   which is what makes a run reproducible rather than a race. A UI with a
   focused text field never fully idles — the caret blink asks for frames
   forever — so settle is also bounded by a wall clock.
2. **Makepad's JSON has trailing commas.** `makepad_micro_serde` emits
   `{"a":1,}` when a struct's last field is a `None` option, which its own
   parser accepts and `serde_json` rejects. `proto::strip_trailing_commas`
   handles it; without it every `WidgetSnapshot` response silently fails to
   parse.

## Adding a scene

Scenes live in `src/scenes.rs`. A scene is a short deterministic path ending in
a measurement:

```rust
driver.set_viewport(1280.0, 800.0, 1.0)?;
driver.settle(60)?;
let widgets = driver.widget_snapshot()?;
let frame = driver.capture("my_scene")?;
findings.extend(check_geometry(&ctx, &widgets));
```

Keep them small and independent: a failure should name one interaction, not
"the app".

## Rule discipline

A rule that cannot measure something confidently stays quiet, and says so as a
`harness.*` finding rather than blaming the app. Concretely:

- Widgets reported at `(0,0,0,0)` were never drawn (every closed modal in the
  tree looks like this) — no geometry rule may speak about them.
- Content below or right of the viewport is normal scrolling, not a layout bug;
  only negative coordinates are unreachable.
- Contrast is only sampled for widgets fully inside the captured frame, and
  only for rects shaped like a line of text.
- Makepad emits a key-focus rect after pointer input but never after a key
  event, so the *absence* of one proves nothing. The focus rule is pixel-based
  instead.
