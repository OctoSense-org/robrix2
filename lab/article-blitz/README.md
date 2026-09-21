# Blitz article integration validation

This integration supplies native HTML/CSS article previews to independent hosts. It is not a migration of Robrix to Dioxus, an OctoSense runtime dependency, or an implementation of arbitrary HTML rich-text editing.

Source: `crates/article-blitz`. Engine commit: `e99fbdbd1d03b9f0aa1622c3f810d95daac92042`. Makepad commit: `47837267faf6970a6cc36acedf9f83846b277307`. Standalone lockfile records all resolved dependencies. Runtime tests: Rust 1.98.0, aarch64 macOS. Rust 1.94.0 `--no-default-features` build check also passed.

The fixtures are original synthetic articles, not captured WeChat pages. They exercise Chinese and English with the PingFang SC CSS preference, inline emphasis, nested sections, image resource grants, borders, backgrounds, spacing, CSS variables, class/inline cascade, responsive media rules, dark mode and a basic table. They are useful integration evidence, not a WeChat compatibility benchmark.

Generated evidence:

- `evidence/mobile-light.png`: 390 CSS px, 2× rendering.
- `evidence/desktop-light.png`: 760 CSS px, 1× rendering.
- `evidence/mobile-dark.png`: 390 CSS px, 2× rendering, authored dark-theme CSS.
- `evidence/native-top.png` and `native-bottom.png`: actual isolated Makepad window captures, including native scrolling of the Blitz texture.
- `evidence/render-results.json`: engine revision, dimensions, resource decisions and wall times.
- `evidence/native-validation.json`: native binary/image hashes, window dimensions, OCR assertions and process-isolation checks.
- `validation.json`: final test/check results and evidence hashes.

The renderer test suite checks CSS cascade/media variables through pixels, exact in-memory grants, denied remote/file/data resources, blocked nested imports, image decode/display, malformed resources, active elements, input/viewport/depth limits, capped requests, long-article clipping and Chinese responsive/dark fixtures. Native verification additionally checks the actual Makepad texture path and scroll behavior with screenshot OCR.

Known boundaries:

- No HTML-to-editable-document conversion, imported-style persistence schema, link hit testing, selection or accessibility text in this bitmap widget.
- CSS dark mode is based on the fixture's authored rules, not a claim to reproduce WeChat's automatic dark-color transformations.
- No real-device iOS validation or Android/Windows/Linux runtime validation.
- No arbitrary SVG, animated components or WeChat private tags; no JavaScript execution.
- System font discovery is explicitly enabled; no bundled redistribution of PingFang.
- No measured similarity to actual WeChat screenshots and no 9/10 claim.
- Output/resource budgets do not create a CPU or process sandbox. Upstream intermediate allocations and pathological CSS still require stronger containment before admitting arbitrary public documents.
- Long documents beyond the bitmap budget are explicitly reported as clipped; the host must display that condition. Tiled rendering is future work.

Reproduction commands and the host API are in `crates/article-blitz/README.md`. No personal Matrix credentials, profiles or accounts were used.

Robrix's optional `article_blitz` feature now uses this renderer for validated
local draft previews. Its integration checks, Palpo lifecycle checks and actual
Robrix screenshots are recorded separately in `lab/article-components/`.
