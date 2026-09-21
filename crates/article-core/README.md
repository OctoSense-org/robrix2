# article-core

Transport-independent article documents, editing, assets, scoped local storage,
and host interfaces. Robrix and OctoSense can both embed this library; neither
application is a dependency. No background service or network client is needed.

Implement `host::ArticleHost` with a trusted storage root, an opaque current
account identifier and a `SessionAuthority` owned by your login lifecycle.
Issue `ConsentGrant` only from trusted host code after consent. Invalidate the
authority on logout/account changes. Use `storage::LocalStore<Host, P, O>`;
`P` and `O` are your publication and outbox metadata, or `()` for a local editor.
The `ArticlePublisher` port lets a host retain its transport-specific operations
and receipts. It does not grant authority merely by being implemented.

Image import accepts host-selected bytes. Reading assets validates content
hashes. `assets::crop_cover` and `editing::EditHistory` are shared by hosts.
The optional `l0` feature provides the reviewed builtin field bindings from
`resources/app.card`; no arbitrary downloaded code is admitted.

```sh
cargo test --locked --manifest-path crates/article-core/Cargo.toml --features l0
```

The v2 model accepts a bounded Markdown subset and generates basic HTML.
Arbitrary HTML/CSS import and lossless WYSIWYG HTML editing are not implemented
by this extraction. See [ADR 0003](../../docs/adr/0003-shared-article-components.md).
