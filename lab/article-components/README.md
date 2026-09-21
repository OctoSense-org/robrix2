# Shared article components and Robrix integration

Implementation: [ADR 0003](../../docs/adr/0003-shared-article-components.md).
Robrix remains independently installable and does not require an OctoSense
process, account service or installation.

| Component | Shared responsibilities | Host responsibilities |
| --- | --- | --- |
| `article-core` | Documents, themes, undo/redo, normalized assets, storage, scoped grants and publication contract | Current account, login epoch, consent, trusted storage root, publication implementation |
| `article-makepad` | Native rich input, layout, presentation and Apple font setup | Screen flow, localization, image picker, target selection |
| `article-blitz` | Bounded HTML/CSS rendering and native bitmap view | Authorized resource bytes, worker scheduling, stale-result rejection and clipping notice |
| Robrix adapter | Matrix publication/read/update/withdrawal, encrypted media and mini-app sharing | Matrix login and credentials stay here |

`dependencies.json` records the independently resolved core/native/Blitz
dependency graphs. None includes Robrix, Matrix or the OctoSense application. Octoscript
L0 remains an optional static library. Existing account-hashed schema-2 storage
and Matrix publication metadata are preserved.

Run the independent local editor:

```sh
cargo build --locked --manifest-path crates/article-makepad/Cargo.toml --example standalone
crates/article-makepad/target/debug/examples/standalone --data-dir=/tmp/article-example
```

Run Robrix with the optional native CSS preview:

```sh
cargo run --locked --features agent_chat,article_blitz --bin robrix
```

In the mobile layout: Discover → Article editor → consent → open/create a draft
→ Full preview → HTML/CSS preview (experimental). In the desktop layout, the
conversation's `+` menu also opens the editor. The default build uses the
existing native editor/reader and omits the experimental preview button.

Validation commands (macOS, isolated local test profiles):

```sh
cargo test --locked --manifest-path crates/article-core/Cargo.toml --features l0
cargo test --locked --manifest-path crates/article-makepad/Cargo.toml --lib
cargo test --locked --features agent_chat,article_blitz --lib
cargo check --locked --lib
python3 tools/wechat-ux/check_i18n.py
python3 tools/wechat-ux/live/native_article_components.py
python3 tools/wechat-ux/live/native_article_v2.py
python3 tools/wechat-ux/live/native_article_v2_encrypted.py
python3 tools/wechat-ux/live/native_article_blitz.py
```

The last three commands require the isolated Palpo fixture described in
`lab/wechat-ux/IMPLEMENTATION.md`. They do not drive a personal account. Fixture
profiles and credentials remain ignored; only selected article screenshots and
credential-free results are committed. Image bytes are seeded from the existing
normalized illustration fixture; these tests do not automate the OS file picker.

`validation.json` records actual executed checks and binary/evidence hashes.
The combined suites passed 219 tests with two existing external integration
tests ignored. Eighteen native checks passed, including the editor/reader smoke
check with Blitz disabled (`evidence/robrix-without-blitz.png`).
Standalone editing/save/reopen screenshots are in `evidence/standalone-*.png`;
Robrix HTML/CSS preview screenshots are in `evidence/robrix-css-*.png`.
Blitz's independent 14-test renderer suite and Makepad scroll verification are
in [the renderer lab](../article-blitz/README.md).

Remaining boundaries: this is shared Rust code, not an OctoSense application
package. That host still needs its own adapters and runtime validation. The
Blitz view is a static bitmap: no selection, link interaction or accessibility
text. It displays the structured editor's generated HTML/CSS; arbitrary public
HTML import, style-preserving round trips and arbitrary HTML WYSIWYG editing are
not implemented. CPU work is on a worker, with input/resource/output budgets,
but is not a process sandbox. iOS/device and WeChat 9/10 visual parity are not
claimed by these tests.
