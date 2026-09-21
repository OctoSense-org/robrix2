//! Host-admitted Octoscript L0 article app. No browser, token bridge or downloaded code.
mod model;
pub mod ui;
pub use model::{ArticlePackage, MSGTYPE};
pub use ui::{ArticleAction, ArticlePanelWidgetRefExt, script_mod};
pub(crate) use model::invalidate_sessions;
