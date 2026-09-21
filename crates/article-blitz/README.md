# article-blitz

A reusable Rust HTML/CSS **preview renderer**, with an optional native Makepad texture view. It has no dependency on Robrix accounts, Matrix, OctoSense services, or Octoscript. Either application can embed the same library and independently supply authorized resources.

The tested engine is [Blitz e99fbdbd](https://github.com/DioxusLabs/blitz/tree/e99fbdbd1d03b9f0aa1622c3f810d95daac92042), not a WebView: html5ever → Stylo → Taffy/Parley → AnyRender/Vello CPU → RGBA. This first integration preserves the input HTML/CSS for rendering; it does **not** convert arbitrary HTML into the existing editable article document, or implement an HTML WYSIWYG editor.

```rust
use article_blitz::{render_html, RenderOptions, ResourceMap};

let mut resources = ResourceMap::default();
// Bytes have already been selected/authorized/downloaded by the host.
let stylesheet_url = resources.insert_css("theme.css", "p { color: #337b60 }")?;
let html = format!("<link rel='stylesheet' href='{stylesheet_url}'><p>你好，世界</p>");
let bitmap = render_html(&html, RenderOptions::default(), &resources)?;
assert_eq!(bitmap.rgba.len(), bitmap.width as usize * bitmap.height as usize * 4);
# Ok::<(), article_blitz::RenderError>(())
```

Run `render_html` on a worker. The host must check the active account, grant expiry/revocation and document generation again before presenting the result. A `ResourceMap` is a per-render snapshot, not a persistent authority token. Build a new map after a permission or account change. Never place access tokens, cookies or signed remote URLs in article HTML/CSS or resource identifiers.

## Resource boundary

Only exact URLs under `https://article.invalid/assets/<id>` which the host inserted can resolve. PNG/JPEG images are decoded with bounds before admission; CSS is UTF-8 and size limited. Relative references resolve into that namespace. Unmapped URLs, including HTTP(S), `file:`, `data:`, CSS `@import`, image/background URLs and `@font-face` URLs, receive empty responses without I/O. Completing denied responses matters because otherwise a denied stylesheet can leave upstream waiting indefinitely.

There is no `blitz-net`, request client, navigation integration, clipboard integration or JavaScript package. Active and subdocument elements (`script`, `iframe`, forms, object/embed), SVG and MathML are rejected. The document is parsed as standards-mode HTML. Event attributes are inert in this renderer; **this is not an HTML sanitizer for subsequent browser use**.

The default `system-fonts` feature intentionally enables OS font discovery and loading. That allows `font-family: "PingFang SC", system-ui, sans-serif` on Apple hosts; it is distinct from document-authorized file loading. `--no-default-features` disables OS font discovery, but then this initial API does not provide a custom font registration interface and text coverage is limited.

Budgets: 256 KiB HTML; conservative token/depth preflight (16,384 tokens, 64 explicit open tags); 48 approved resources, 8 MiB each and 24 MiB total; CSS 64 KiB per resource; images at most 4096 × 4096 with a 64 MiB decoded allocation limit; 128 resource requests; eight resolve passes; at most 16,777,216 output pixels. Output dimensions are capped at 16,384 pixels, below Vello CPU's 16-bit dimension limit. Long content reports `clipped`; hosts must expose that status or use a future paged/tiled implementation instead of silently treating a crop as the whole article.

These limits bound inputs, granted resources and output allocation. They do **not** bound all intermediate Stylo/Taffy/Vello allocations or provide a cancellable CPU deadline. A worker thread keeps the UI responsive but is not process isolation. Do not claim this beta engine is hardened against arbitrary hostile HTML/CSS; untrusted public imports need a separately supervised process and resource controls before broad deployment.

## Makepad adapter

Enable `features = ["makepad"]`, register `article_blitz::makepad::script_mod(vm)` after base Makepad widgets, and instantiate `mod.widgets.BlitzArticleView`. Bring `article_blitz::makepad::BlitzArticleViewWidgetRefExt` into Rust scope:

```rust,ignore
ui.blitz_article_view(cx, ids!(article_preview)).set_rendered(cx, &bitmap);
// On logout, grant revocation, or document replacement:
ui.blitz_article_view(cx, ids!(article_preview)).clear(cx);
```

The adapter converts RGBA to Makepad's `VecBGRAu8_32`, displays the result in a clipped native scroll view, and has no WebView. Makepad is pinned to `47837267faf6970a6cc36acedf9f83846b277307` in this crate's lockfile, using the same source/branch as Robrix to avoid duplicate widget types. Hosts should render at the measured view width and current DPI; changing the view width only scales the existing bitmap until the host renders again.

This adapter supplies a bitmap preview, not text selection, link hit testing, accessibility text or editing. Keep the native editable view available. Its CPU bitmap allocation, upload and full-document rendering costs make it a first integration, not the final interactive renderer.

## Reproduce

This crate has its own workspace/lockfile so it can be built independently from Robrix. Runtime tests used Rust 1.98.0 on aarch64 macOS. A locked build check also passed on Rust 1.94.0 with default features disabled.

```sh
CARGO_TARGET_DIR=target-blitz cargo +1.98.0 test --locked --manifest-path crates/article-blitz/Cargo.toml --features makepad
CARGO_TARGET_DIR=target-blitz cargo +1.98.0 run --locked --manifest-path crates/article-blitz/Cargo.toml --example render_fixture -- lab/article-blitz/evidence
CARGO_TARGET_DIR=target-blitz cargo +1.98.0 build --locked --manifest-path crates/article-blitz/Cargo.toml --features makepad --example native_preview
python3 lab/article-blitz/verify_native.py
```

The native script starts only the isolated fixture binary, uses an owned loopback bridge, captures and OCR-checks Chinese text and the table, scrolls the native view, then exits that process. It does not connect a Matrix account. `MAKEPAD_HIDE_WINDOWS=1` keeps this verification off the user's screen.

Evidence and limitations are in `lab/article-blitz/README.md`. Tests prove local behaviors, not a 9/10 WeChat visual score. Upstream remains beta and its CSS support is incomplete: [upstream status](https://blitz.is/status/css).
