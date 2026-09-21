//! Host-admitted Octoscript L0 article app. No browser, token bridge or downloaded code.
mod model;
pub mod document;
mod rich_input;
mod rich_layout;
mod storage;
pub(crate) mod backend;
pub mod ui;
pub use model::{ArticlePackage, MSGTYPE};
pub use ui::{ArticleAction, ArticlePanelWidgetRefExt};
pub(crate) use model::invalidate_sessions;
pub fn script_mod(vm:&mut makepad_widgets::ScriptVm) {
    rich_input::script_mod(vm);
    ui::script_mod(vm);
}
