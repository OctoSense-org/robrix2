//! Native article components. The embedding application supplies account,
//! persistence, image selection, navigation and publication services.
pub use article_core::document;
pub mod rich_input;
mod rich_layout;
pub fn script_mod(vm: &mut makepad_widgets::ScriptVm) {
    rich_input::script_mod(vm);
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod apple_fonts;
pub mod presentation;
