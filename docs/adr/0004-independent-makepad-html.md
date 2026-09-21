# ADR 0004: Independent makepad-html component

Status: accepted; supersedes the renderer ownership in ADR 0003.

## Decision

HTML/CSS engine work belongs to the independent
[OctoSense-org/makepad-html](https://github.com/OctoSense-org/makepad-html) repository.
It owns a standalone workspace, generic rendering API (`RenderedDocument`,
`RenderOptions`, `ResourceMap`), optional Makepad `HtmlView`, a native viewer,
headless tools, generic CSS regressions, pinned Blitz source/patches and HTML5 /
WeChat-derived compatibility suites.

Robrix links a pinned git revision with the Cargo alias `makepad-html-renderer`.
The alias distinguishes this renderer from the small `makepad-html` parser
already re-exported by Makepad widgets. `html_preview` enables the integration;
`article_blitz` remains a compatibility feature alias for existing commands.
No downstream Blitz Cargo patch overrides are necessary.

`src/article_app/preview.rs` remains an app-specific adapter: it converts validated
article documents to HTML, grants already-authorized asset bytes, schedules the
render, and checks the login/grant/request lifecycle before display.
Drafts, themes, Markdown, consent, Matrix credentials, storage, publication and
withdrawal remain outside makepad-html. The renderer does not depend on article-core,
article-makepad, Robrix, Matrix, OctoSense or Octoscript.

## Consequences

Other Makepad applications can use and test HTML rendering independently.
WeChat HTML/CSS is one compatibility suite, rather than the library's domain model.
Engine fixes and interaction work go to makepad-html; editor workflow changes stay
here. Removing the renderer feature still supports the regular native article UI.

This is an extraction of the current static bitmap renderer. It does not add
persistent DOM, lossless editable HTML, selection, nested scrolling, SVG admission
or complete WeChat compatibility. Existing resource and active-content restrictions
are retained.
