//! Host-admitted Octoscript L0 article app. No browser, token bridge or downloaded code.
mod model;
pub mod document;
use article_makepad::rich_input;
mod host;
#[cfg(feature = "html_preview")]
mod preview;
mod storage;
pub(crate) mod backend;
pub mod ui;
pub use model::{ArticlePackage, MSGTYPE};
pub use ui::{ArticleAction, ArticlePanelWidgetRefExt};
pub(crate) use model::invalidate_sessions;
pub fn script_mod(vm:&mut makepad_widgets::ScriptVm) {
    article_makepad::script_mod(vm);
    #[cfg(feature = "html_preview")]
    makepad_html_renderer::makepad::script_mod(vm);
    #[cfg(not(feature = "html_preview"))]
    {
        use makepad_widgets::*;
        script_eval!(vm, {mod.widgets.HtmlView = mod.widgets.View{}});
    }
    ui::script_mod(vm);
}
