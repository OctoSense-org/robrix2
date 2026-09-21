//! Isolated native fixture viewer: no Robrix login, profile, or external services.
pub use makepad_widgets;
use makepad_widgets::*;
use article_blitz::{render_html, RenderOptions, ResourceMap, makepad::BlitzArticleViewWidgetRefExt};
app_main!(App);
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: vec2(440, 880)
                body +: {
                    flow: Down
                    title := Label {width: Fill height: 44 padding: 12 text: "Blitz native article preview"}
                    article := BlitzArticleView {width: Fill height: Fill}
                    status := Label {width: Fill height: 30 text: "Loading fixture…"}
                }
            }
        }
    }
}
#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
}
impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let mut resources = ResourceMap::default();
        resources
            .insert_css("article.css", include_str!("../tests/fixtures/article.css"))
            .unwrap();
        resources
            .insert_image(
                "cover.png",
                include_bytes!("../tests/fixtures/cover.png").to_vec(),
            )
            .unwrap();
        let article = render_html(
            include_str!("../tests/fixtures/article.html"),
            RenderOptions {
                width_css: 440,
                scale: 2.0,
                ..Default::default()
            },
            &resources,
        )
        .unwrap();
        self.ui
            .blitz_article_view(cx, ids!(article))
            .set_rendered(cx, &article);
        self.ui
            .label(cx, ids!(status))
            .set_text(cx, "Offline fixture · Native Makepad texture");
    }
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        article_blitz::makepad::script_mod(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
