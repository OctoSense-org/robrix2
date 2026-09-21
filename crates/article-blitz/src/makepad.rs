//! Optional native texture adapter. The host computes [`RenderedArticle`] on a
//! worker, checks its own account/session generation, then uploads on the UI thread.
//! This bitmap preview deliberately does not pretend to provide text selection,
//! links, accessibility text, or rich-text editing.
use crate::RenderedArticle;
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.BlitzArticleView = #(BlitzArticleView::register_widget(vm)) {
        ..mod.widgets.View
        width: Fill height: Fill flow: Down clip_x: true clip_y: true
        viewport := ScrollYView {
            width: Fill height: Fill flow: Down clip_x: true clip_y: true
            article_bitmap := Image {width: Fill height: Fit fit: ImageFit.Horizontal}
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct BlitzArticleView {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
}
impl Widget for BlitzArticleView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}
impl BlitzArticleView {
    pub fn set_rendered(&mut self, cx: &mut Cx, rendered: &RenderedArticle) {
        let texture = Texture::new_with_format(
            cx,
            TextureFormat::VecBGRAu8_32 {
                width: rendered.width as usize,
                height: rendered.height as usize,
                data: Some(rendered.to_bgra_u32()),
                updated: TextureUpdated::Full,
            },
        );
        self.view
            .image(cx, ids!(article_bitmap))
            .set_texture(cx, Some(texture));
        self.view.redraw(cx);
    }
    pub fn clear(&mut self, cx: &mut Cx) {
        self.view
            .image(cx, ids!(article_bitmap))
            .set_texture(cx, None);
        self.view.redraw(cx);
    }
}
impl BlitzArticleViewRef {
    pub fn set_rendered(&self, cx: &mut Cx, rendered: &RenderedArticle) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_rendered(cx, rendered);
        }
    }
    pub fn clear(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.clear(cx);
        }
    }
}
