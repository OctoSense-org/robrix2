//! Matrix mini-app cards: native article packages and separate WebKit link apps.

use makepad_widgets::*;
use ruma::{
    OwnedTransactionId, OwnedUserId, TransactionId,
    events::room::message::{MessageType, RoomMessageEventContent},
};
use serde::{Deserialize, Serialize};
use crate::{
    home::rooms_list::RoomsListRef,
    shared::{
        navigation_bar_button::{NavigationBarButtonWidgetRefExt, NavigationBarButtonWidgetExt},
        popup_list::{PopupKind, enqueue_popup_notification},
    },
    sliding_sync::{current_user_id, get_client, spawn_async_task, TimelineKind},
    utils::RoomNameId,
};

pub const MSGTYPE: &str = "rs.robius.robrix.mini_app";
const BROWSER: LiveId = live_id!(robrix_web_mini_app);
const CAPTION_PADDING: f64 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };

pub fn is_timeline_event(event: &ruma::events::AnySyncTimelineEvent) -> bool {
    let ruma::events::AnySyncTimelineEvent::MessageLike(event) = event else {
        return false;
    };
    event.original_content().is_some_and(|content| matches!(content,
        ruma::events::AnyMessageLikeEventContent::RoomMessage(message) if matches!(message.msgtype.msgtype(), MSGTYPE | crate::article_app::MSGTYPE)))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WebMiniApp {
    pub version: u8,
    pub title: String,
    pub url: String,
}

impl WebMiniApp {
    pub fn new(title: &str, address: &str) -> Result<Self, String> {
        let address = address.trim();
        if address.len() > 4096 || address.chars().any(char::is_control) {
            return Err(
                crate::i18n::tr("Use a web address shorter than 4096 bytes, without control characters.").into(),
            );
        }
        let url = url::Url::parse(address)
            .map_err(|_| "Enter a complete http:// or https:// address.")?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(crate::i18n::tr("Mini apps need an HTTP or HTTPS web address.").into());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(crate::i18n::tr("Remove the username or password from the web address.").into());
        }
        let title = title.trim();
        if title.chars().count() > 120 || title.chars().any(char::is_control) {
            return Err(crate::i18n::tr("Use a title of at most 120 characters on one line.").into());
        }
        Ok(Self {
            version: 1,
            title: if title.is_empty() {
                url.host_str().unwrap().to_owned()
            } else {
                title.to_owned()
            },
            url: url.to_string(),
        })
    }

    pub fn from_message(message: &MessageType) -> Result<Self, String> {
        if message.msgtype() != MSGTYPE {
            return Err(crate::i18n::tr("Not a web mini app").into());
        }
        let data = message.data();
        let raw: Self =
            serde_json::from_value(data.get("mini_app").cloned().ok_or(crate::i18n::tr("Missing mini app"))?)
                .map_err(|_| crate::i18n::tr("Invalid mini app fields"))?;
        if raw.version != 1 {
            return Err(crate::i18n::tr("Unsupported mini app version").into());
        }
        Self::new(&raw.title, &raw.url)
    }

    pub fn message(&self) -> RoomMessageEventContent {
        let mut data = serde_json::Map::new();
        data.insert("mini_app".into(), serde_json::to_value(self).unwrap());
        RoomMessageEventContent::new(
            MessageType::new(
                MSGTYPE,
                crate::i18n::format("[Mini app] {0}\n{1}", &[("0", (self.title).to_string()), ("1", (self.url).to_string())]),
                data,
            )
            .unwrap(),
        )
    }

    fn origin(&self) -> String {
        url::Url::parse(&self.url)
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_default()
    }
}

/// Shared native packages and web cards keep distinct launch paths.
#[derive(Clone, Debug)]
pub enum SharedMiniApp { Web(WebMiniApp), Article(crate::article_app::ArticlePackage) }
impl SharedMiniApp {
    pub fn from_message(message: &MessageType) -> Result<Self, String> {
        if message.msgtype() == crate::article_app::MSGTYPE {
            crate::article_app::ArticlePackage::from_message(message).map(Self::Article)
        } else { WebMiniApp::from_message(message).map(Self::Web) }
    }
    pub fn title(&self) -> &str { match self {Self::Web(app)=>&app.title, Self::Article(_)=>crate::i18n::tr("Article editor")} }
    fn origin(&self) -> String {match self {Self::Web(app)=>app.origin(),Self::Article(_)=>crate::i18n::tr("Markdown · Native preview").into()}}
}

#[derive(Clone, Debug)]
pub enum MiniAppAction {
    Compose(TimelineKind),
    Open {
        app: WebMiniApp,
        timeline: TimelineKind,
    },
    Close,
}

#[derive(Debug)]
struct MiniAppSent {
    owner: OwnedUserId,
    transaction: OwnedTransactionId,
    result: Result<(), String>,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.MiniAppCard = #(MiniAppCard::register_widget(vm)) {
        ..mod.widgets.View
        visible: false width: Fill height: Fit
        open_mini_app := NavigationBarButton {
            width: Fill height: Fit flow: Down spacing: 10 padding: 4
            draw_bg +: {color_hover: #xf2f2f2 color_active: #xf2f2f2 border_radius: 4}
            card_title := Label {
                width: Fill flow: Flow.Right{wrap: true} max_lines: 3
                draw_text +: {color: #x191919 text_style: theme.font_bold{font_size: 14}}
            }
            card_origin := Label {
                width: Fill max_lines: 1 text_overflow: Ellipsis
                draw_text +: {color: #x777777 text_style: theme.font_regular{font_size: 10}}
            }
            View {width: Fill height: 1 show_bg: true draw_bg.color: #xe8e8e8}
            Label {text: #(crate::i18n::tr("◉  Mini app")) i18n_text: "◉  Mini app" draw_text +: {color: #x576b95 text_style: theme.font_regular{font_size: 10}}}
        }
    }

    mod.widgets.MiniAppPanel = #(MiniAppPanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down
        padding: Inset{top: mod.widgets.SAFE_INSET_PAD_TOP + #(CAPTION_PADDING) bottom: mod.widgets.SAFE_INSET_PAD_BOTTOM}
        draw_bg.color: #xf7f7f7
        header := View {
            width: Fill height: 54 flow: Right spacing: 8 padding: 8 align: Align{y: 0.5}
            mini_close := RobrixNeutralIconButton {text: #(crate::i18n::tr("Close")) i18n_text: "Close" height: 40}
            heading := Label {
                text: #(crate::i18n::tr("Share mini app")) i18n_text: "Share mini app" width: Fill max_lines: 1 text_overflow: Ellipsis
                draw_text +: {color: #x191919 text_style: theme.font_bold{font_size: 14}}
            }
            mini_share := RobrixNeutralIconButton {text: #(crate::i18n::tr("Share")) i18n_text: "Share" height: 40 visible: false}
        }
        form := View {
            width: Fill height: Fit flow: Down spacing: 12 padding: 20
            Label {text: #(crate::i18n::tr("Web address")) i18n_text: "Web address" draw_text +: {color: #x191919}}
            mini_url := TextInput {width: Fill height: 46 empty_text: #(crate::i18n::tr("https://example.com")) i18n_empty_text: "https://example.com"}
            Label {text: #(crate::i18n::tr("Card title")) i18n_text: "Card title" draw_text +: {color: #x191919}}
            mini_title := TextInput {width: Fill height: 46 empty_text: #(crate::i18n::tr("Name your mini app")) i18n_empty_text: "Name your mini app"}
            mini_recipient := Label {width: Fill flow: Flow.Right{wrap: true} draw_text.color: #x576b95}
            mini_choose_chat := RobrixNeutralIconButton {text: #(crate::i18n::tr("Choose chat")) i18n_text: "Choose chat" height: 44}
            View {
                width: Fill height: Fit flow: Right spacing: 12
                mini_preview := RobrixNeutralIconButton {text: #(crate::i18n::tr("Preview")) i18n_text: "Preview" height: 44}
                mini_send := RobrixNeutralIconButton {text: #(crate::i18n::tr("Send")) i18n_text: "Send" height: 44 draw_text.color: #x07a858}
            }
        }
        chat_picker := View {
            visible: false width: Fill height: Fill flow: Down spacing: 12 padding: 20
            mini_chat_search := TextInput {width: Fill height: 44 empty_text: #(crate::i18n::tr("Find a chat")) i18n_empty_text: "Find a chat"}
            chat_list := PortalList {
                width: Fill height: Fill
                Chat := NavigationBarButton {
                    width: Fill height: 54 padding: 10 align: Align{y: 0.5}
                    draw_bg +: {color_hover: #xe8e8e8 color_active: #xe8e8e8}
                    name := Label {width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.color: #x191919}
                }
            }
            mini_picker_cancel := RobrixNeutralIconButton {text: #(crate::i18n::tr("Cancel")) i18n_text: "Cancel" height: 44}
        }
        viewer := View {
            visible: false width: Fill height: Fill flow: Down
            mini_origin := Label {
                width: Fill height: Fit padding: 10 max_lines: 1 text_overflow: Ellipsis
                draw_text +: {color: #x576b95 text_style: theme.font_regular{font_size: 10}}
            }
            View {
                width: Fill height: 44 flow: Right spacing: 8 padding: Inset{left: 8 right: 8}
                mini_web_back := RobrixNeutralIconButton {text: #(crate::i18n::tr("Back")) i18n_text: "Back" height: 40}
                mini_reload := RobrixNeutralIconButton {text: #(crate::i18n::tr("Reload")) i18n_text: "Reload" height: 40}
                mini_external := RobrixNeutralIconButton {text: #(crate::i18n::tr("Open in browser")) i18n_text: "Open in browser" height: 40}
            }
            web_surface := SolidView {width: Fill height: Fill draw_bg.color: #xffffff}
        }
        mini_status := Label {
            width: Fill height: Fit padding: 16 flow: Flow.Right{wrap: true}
            draw_text +: {color: #x777777 text_style: theme.font_regular{font_size: 11}}
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct MiniAppCard {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    app: Option<SharedMiniApp>,
    #[rust]
    timeline: Option<TimelineKind>,
}

impl Widget for MiniAppCard {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.visible {
            return;
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            if self
                .navigation_bar_button(cx, ids!(open_mini_app))
                .clicked(actions)
            {
                if let (Some(app), Some(timeline)) = (&self.app, &self.timeline) {
                    match app {
                        SharedMiniApp::Web(app) => cx.action(MiniAppAction::Open { app: app.clone(), timeline: timeline.clone() }),
                        SharedMiniApp::Article(_) => cx.action(crate::article_app::ArticleAction::Open),
                    }
                }
            }
        }
    }
}

impl MiniAppCardRef {
    pub fn set_app(&self, cx: &mut Cx, app: Option<SharedMiniApp>, timeline: &TimelineKind) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        inner.visible = app.is_some();
        if let Some(app) = &app {
            inner.label(cx, ids!(card_title)).set_text(cx, app.title());
            inner
                .label(cx, ids!(card_origin))
                .set_text(cx, &app.origin());
        }
        inner.app = app;
        inner.timeline = Some(timeline.clone());
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct MiniAppPanel {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    timeline: Option<TimelineKind>,
    #[rust]
    current: Option<WebMiniApp>,
    #[rust]
    rooms: Vec<RoomNameId>,
    #[rust]
    browser_open: bool,
    #[rust]
    pending: bool,
    #[rust]
    transaction: Option<OwnedTransactionId>,
    #[rust]
    last_send: Option<(WebMiniApp, TimelineKind)>,
}

impl MiniAppPanel {
    fn status(&self, cx: &mut Cx, text: &str) {
        self.label(cx, ids!(mini_status)).set_text(cx, text);
    }

    fn close_browser(&mut self, cx: &mut Cx) {
        if self.browser_open {
            cx.system_browser(BROWSER).close();
            self.browser_open = false;
        }
    }

    fn form(&mut self, cx: &mut Cx) {
        self.close_browser(cx);
        self.view(cx, ids!(form)).set_visible(cx, true);
        self.view(cx, ids!(viewer)).set_visible(cx, false);
        self.view(cx, ids!(chat_picker)).set_visible(cx, false);
        self.button(cx, ids!(mini_share)).set_visible(cx, false);
        self.label(cx, ids!(heading)).set_text(cx, crate::i18n::tr("Share mini app"));
        let name = self.timeline.as_ref().and_then(|timeline| {
            cx.get_global::<RoomsListRef>()
                .get_room_name(timeline.room_id())
        });
        self.label(cx, ids!(mini_recipient)).set_text(
            cx,
            &name
                .map(|name| crate::i18n::format("To: {0}", &[("0", (name.display()).to_string())]))
                .unwrap_or_else(|| crate::i18n::tr("Choose a chat to send to").into()),
        );
        self.status(cx, crate::i18n::tr("Send a web page as a mini-app card."));
        self.redraw(cx);
    }

    fn read_form(&self, cx: &mut Cx) -> Result<WebMiniApp, String> {
        WebMiniApp::new(
            &self.text_input(cx, ids!(mini_title)).text(),
            &self.text_input(cx, ids!(mini_url)).text(),
        )
    }

    fn show_web(&mut self, cx: &mut Cx, app: WebMiniApp) {
        self.close_browser(cx);
        self.label(cx, ids!(heading)).set_text(cx, &app.title);
        // This is the shared entry URL, not a claim about subsequent page navigations.
        self.label(cx, ids!(mini_origin))
            .set_text(cx, &crate::i18n::format("Shared link: {0}", &[("0", (app.origin()).to_string())]));
        self.view(cx, ids!(form)).set_visible(cx, false);
        self.view(cx, ids!(chat_picker)).set_visible(cx, false);
        self.view(cx, ids!(viewer)).set_visible(cx, true);
        self.button(cx, ids!(mini_share)).set_visible(cx, true);
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            cx.system_browser(BROWSER).spawn(&app.url);
            self.browser_open = true;
            self.status(cx, "");
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        self.status(
            cx,
            crate::i18n::tr("Use Open in browser to view this mini app on this platform."),
        );
        self.current = Some(app);
        cx.hide_text_ime();
        self.redraw(cx);
    }

    fn choose_chat(&mut self, cx: &mut Cx) {
        self.close_browser(cx);
        self.rooms = cx.get_global::<RoomsListRef>().mini_app_share_rooms();
        self.view(cx, ids!(form)).set_visible(cx, false);
        self.view(cx, ids!(viewer)).set_visible(cx, false);
        self.view(cx, ids!(chat_picker)).set_visible(cx, true);
        self.text_input(cx, ids!(mini_chat_search)).set_text(cx, "");
        self.status(
            cx,
            if self.rooms.is_empty() {
                crate::i18n::tr("No joined chats are available.")
            } else {
                crate::i18n::tr("Choose a chat, then tap Send.")
            },
        );
        self.redraw(cx);
    }

    fn send(&mut self, cx: &mut Cx) {
        if self.pending {
            return;
        }
        let app = match self.read_form(cx) {
            Ok(app) => app,
            Err(error) => {
                self.status(cx, &error);
                return;
            }
        };
        let Some(timeline) = self.timeline.clone() else {
            self.status(cx, crate::i18n::tr("Choose a chat first."));
            return;
        };
        let Some(client) = get_client() else {
            self.status(cx, crate::i18n::tr("Sign in before sharing."));
            return;
        };
        let Some(owner) = self
            .owner
            .clone()
            .filter(|owner| client.user_id() == Some(owner))
        else {
            return;
        };
        let Some(room) = client
            .get_room(timeline.room_id())
            .filter(|room| room.state() == matrix_sdk::RoomState::Joined && !crate::moments::is_moments(room))
        else {
            self.status(cx, crate::i18n::tr("This chat is no longer joined."));
            return;
        };
        let send = (app.clone(), timeline.clone());
        if self.last_send.as_ref() != Some(&send) {
            self.transaction = Some(TransactionId::new());
            self.last_send = Some(send);
        }
        let transaction = self.transaction.clone().unwrap();
        self.pending = true;
        self.button(cx, ids!(mini_send)).set_enabled(cx, false);
        self.status(cx, crate::i18n::tr("Sending…"));
        spawn_async_task(async move {
            let result = async {
                let mut message = app.message();
                if let Some(root) = timeline.thread_root_event_id() {
                    message.relates_to = Some(ruma::events::room::message::Relation::Thread(
                        ruma::events::relation::Thread::plain(root.clone(), root.clone()),
                    ));
                }
                room.send(message)
                    .with_transaction_id(transaction.clone())
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            .await;
            Cx::post_action(MiniAppSent {
                owner,
                transaction,
                result,
            });
        });
    }
}

impl Widget for MiniAppPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.owner.as_ref() != current_user_id().as_ref() || matches!(event, Event::Shutdown) {
            self.close_browser(cx);
            cx.action(MiniAppAction::Close);
            return;
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(sent) = action.downcast_ref::<MiniAppSent>() {
                    if self.owner.as_ref() == Some(&sent.owner)
                        && self.transaction.as_ref() == Some(&sent.transaction)
                    {
                        self.pending = false;
                        self.button(cx, ids!(mini_send)).set_enabled(cx, true);
                        match &sent.result {
                            Ok(()) => {
                                enqueue_popup_notification(
                                    crate::i18n::tr("Mini app sent"),
                                    PopupKind::Success,
                                    Some(3.0),
                                );
                                cx.action(MiniAppAction::Close);
                            }
                            Err(error) => self
                                .status(cx, &crate::i18n::format("Could not send. Tap Send to retry. {error}", &[("error", (error).to_string())])),
                        }
                    }
                }
            }
            if self.button(cx, ids!(mini_close)).clicked(actions) {
                cx.action(MiniAppAction::Close);
            }
            if self.button(cx, ids!(mini_share)).clicked(actions) {
                self.form(cx);
            }
            if self.button(cx, ids!(mini_preview)).clicked(actions) {
                match self.read_form(cx) {
                    Ok(app) => self.show_web(cx, app),
                    Err(error) => self.status(cx, &error),
                }
            }
            if self.button(cx, ids!(mini_send)).clicked(actions) {
                self.send(cx);
            }
            if self.button(cx, ids!(mini_choose_chat)).clicked(actions) && !self.pending {
                self.choose_chat(cx);
            }
            if self.button(cx, ids!(mini_picker_cancel)).clicked(actions) {
                self.form(cx);
            }
            if self
                .text_input(cx, ids!(mini_chat_search))
                .changed(actions)
                .is_some()
            {
                let query = self
                    .text_input(cx, ids!(mini_chat_search))
                    .text()
                    .to_lowercase();
                self.rooms = cx
                    .get_global::<RoomsListRef>()
                    .mini_app_share_rooms()
                    .into_iter()
                    .filter(|room| room.display().to_lowercase().contains(&query))
                    .collect();
                self.portal_list(cx, ids!(chat_list))
                    .set_first_id_and_scroll(0, 0.0);
                self.redraw(cx);
            }
            for (index, item) in self
                .portal_list(cx, ids!(chat_list))
                .items_with_actions(actions)
            {
                if item.as_navigation_bar_button().clicked(actions) {
                    if let Some(room) = self.rooms.get(index) {
                        self.timeline = Some(TimelineKind::MainRoom {
                            room_id: room.room_id().clone(),
                        });
                        self.form(cx);
                    }
                    break;
                }
            }
            if self.button(cx, ids!(mini_web_back)).clicked(actions) {
                cx.system_browser(BROWSER).history_go(-1);
            }
            if self.button(cx, ids!(mini_reload)).clicked(actions) {
                if let Some(app) = self.current.clone() {
                    self.show_web(cx, app);
                }
            }
            if self.button(cx, ids!(mini_external)).clicked(actions) {
                if let Some(app) = &self.current {
                    if let Err(error) = robius_open::Uri::new(&app.url).open() {
                        self.status(cx, &crate::i18n::format("Could not open browser: {error}", &[("error", format!("{:?}", error))]));
                    }
                }
            }
        }
        if event.back_pressed()
            || matches!(
                event,
                Event::KeyUp(KeyEvent {
                    key_code: KeyCode::Escape,
                    ..
                })
            )
        {
            cx.action(MiniAppAction::Close);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.borrow_mut::<PortalList>() {
                list.set_item_range(cx, 0, self.rooms.len());
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(room) = self.rooms.get(index) else {
                        continue;
                    };
                    let row = list.item(cx, index, id!(Chat));
                    row.label(cx, ids!(name)).set_text(cx, &room.display());
                    row.draw_all(cx, scope);
                }
            }
        }
        if self.browser_open {
            let area = self.view(cx, ids!(web_surface)).area();
            cx.system_browser(BROWSER).update(area, true);
        }
        DrawStep::done()
    }
}

impl MiniAppPanelRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &MiniAppAction) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        if matches!(action, MiniAppAction::Close) {
            inner.close_browser(cx);
            inner.owner = None;
            inner.transaction = None;
            modal.close(cx);
            return;
        }
        inner.owner = current_user_id();
        inner.pending = false;
        inner.transaction = None;
        inner.last_send = None;
        inner.button(cx, ids!(mini_send)).set_enabled(cx, true);
        match action {
            MiniAppAction::Compose(timeline) => {
                inner.timeline = Some(timeline.clone());
                inner.current = None;
                inner.text_input(cx, ids!(mini_url)).set_text(cx, "");
                inner.text_input(cx, ids!(mini_title)).set_text(cx, "");
                inner.form(cx);
            }
            MiniAppAction::Open { app, timeline } => {
                inner.timeline = Some(timeline.clone());
                inner.text_input(cx, ids!(mini_url)).set_text(cx, &app.url);
                inner
                    .text_input(cx, ids!(mini_title))
                    .set_text(cx, &app.title);
                inner.show_web(cx, app.clone());
            }
            MiniAppAction::Close => unreachable!(),
        }
        modal.open(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_preserves_title_url_and_readable_fallback() {
        let app = WebMiniApp::new("中文 Mini app", "https://example.com/app?q=a%20b#tab").unwrap();
        let json = serde_json::to_value(app.message()).unwrap();
        assert_eq!(json["msgtype"], MSGTYPE);
        assert!(json["body"].as_str().unwrap().contains(&app.url));
        let message: RoomMessageEventContent = serde_json::from_value(json).unwrap();
        assert_eq!(WebMiniApp::from_message(&message.msgtype).unwrap(), app);
    }
    #[test]
    fn rejects_unsafe_schemes_credentials_and_oversized_input() {
        for address in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,test",
            "https://user:pass@example.com",
            "http://example.com/\nsecret",
            "example.com",
            "matrix:u/test:example.com",
        ] {
            assert!(
                WebMiniApp::new("test", address).is_err(),
                "accepted {address}"
            );
        }
        assert!(WebMiniApp::new(&"a".repeat(121), "https://example.com").is_err());
        assert!(
            WebMiniApp::new("test", &format!("https://example.com/{}", "a".repeat(4096))).is_err()
        );
        assert!(WebMiniApp::new("test\nline", "https://example.com").is_err());
        assert!(WebMiniApp::new("test", "http://127.0.0.1:8000/app").is_ok());
    }
    #[test]
    fn rejects_unknown_versions_and_malicious_received_metadata() {
        let app = WebMiniApp::new("Example", "https://example.com").unwrap();
        for (key, value) in [
            ("version", serde_json::json!(2)),
            ("url", serde_json::json!("javascript:alert(1)")),
        ] {
            let mut json = serde_json::to_value(app.message()).unwrap();
            json["mini_app"][key] = value;
            let message: RoomMessageEventContent = serde_json::from_value(json).unwrap();
            assert!(WebMiniApp::from_message(&message.msgtype).is_err());
        }
    }

    #[test]
    fn custom_cards_are_admitted_to_the_matrix_timeline() {
        let app = WebMiniApp::new("Example", "https://example.com").unwrap();
        let mut json = serde_json::json!({
            "type": "m.room.message", "event_id": "$card:example.org",
            "sender": "@alice:example.org", "origin_server_ts": 1,
            "content": app.message()
        });
        let event = serde_json::from_value(json.clone()).unwrap();
        assert!(is_timeline_event(&event));
        json["content"]["msgtype"] = serde_json::json!("com.example.unrelated");
        assert!(!is_timeline_event(&serde_json::from_value(json).unwrap()));
    }
}
