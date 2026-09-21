//! Host-admitted Octoscript L0 article app. No browser, token bridge or downloaded code.
mod model;
pub mod document;
use article_makepad::rich_input;
mod host;
#[cfg(feature = "article_blitz")]
mod preview;
mod storage;
pub(crate) mod backend;
pub mod ui;
pub use model::{ArticlePackage, MSGTYPE};
pub use ui::{ArticleAction, ArticlePanelWidgetRefExt};
pub(crate) use model::invalidate_sessions;
pub fn script_mod(vm:&mut makepad_widgets::ScriptVm) {
    article_makepad::script_mod(vm);
    #[cfg(feature = "article_blitz")]
    article_blitz::makepad::script_mod(vm);
    #[cfg(not(feature = "article_blitz"))]
    {
        use makepad_widgets::*;
        script_eval!(vm, {mod.widgets.BlitzArticleView = mod.widgets.View{}});
    }
    ui::script_mod(vm);
}
