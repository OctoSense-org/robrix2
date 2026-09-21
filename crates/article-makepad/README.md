# article-makepad

Reusable native article widgets and presentation. Depends on `article-core` and
Makepad; it has no Matrix, Robrix or OctoSense application dependency.

Call `article_makepad::script_mod(vm)` after registering Makepad widgets, then
use `mod.widgets.ArticleRichInput`. `presentation::style_input` and `style_html`
apply the same document styles in every host. Apple hosts can call
`apple_fonts::install(vm)` after theme selection and before widget registration
to select installed PingFang Regular/Semibold with bundled fallback.

The rich input retains Makepad's selection, clipboard and IME implementation.
Derived code is attributed in `src/rich_input.rs` and `MAKEPAD-LICENSE.txt`.
Consumers must use one compatible Makepad source graph; this repository's lock
files select `47837267faf6970a6cc36acedf9f83846b277307`.

Run the local-only example from the repository root:

```sh
cargo build --locked --manifest-path crates/article-makepad/Cargo.toml --example standalone
crates/article-makepad/target/debug/examples/standalone --data-dir=/tmp/article-example
```

It exercises native editing, selected-text formatting, themes, preview, saving
and reopening without either host application or a server. It is an integration
example, not the full Robrix publication workflow or an OctoSense package.
