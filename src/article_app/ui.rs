use makepad_widgets::*;
use ruma::{
    OwnedTransactionId, OwnedUserId, TransactionId, events::room::message::RoomMessageEventContent,
};
use crate::{
    home::rooms_list::RoomsListRef,
    shared::navigation_bar_button::NavigationBarButtonWidgetRefExt,
    sliding_sync::{current_user_id, get_client, spawn_async_task},
    utils::RoomNameId,
};
use super::model::*;

#[derive(Clone, Debug)]
pub enum ArticleAction {
    Open,
    Close,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Page {
    #[default]
    Details,
    Consent,
    Edit,
    Preview,
    Rooms,
    Confirm,
    Success,
}
#[derive(Clone, Debug)]
struct Prepared {
    room: RoomNameId,
    message: RoomMessageEventContent,
    transaction: OwnedTransactionId,
    sharing: bool,
}
#[derive(Debug)]
struct Sent {
    instance: String,
    transaction: OwnedTransactionId,
    result: Result<(), String>,
}
const TOP: f64 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.ArticlePrimary = RobrixNeutralIconButton {
        width: Fill height: 48 align: Align{x: 0.5 y: 0.5} spacing: 0
        draw_bg +: {color: #x07c160 color_hover: #x06ad56 color_down: #x05984b border_radius: 6}
        draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff text_style: theme.font_bold{font_size: 14}}
    }
    mod.widgets.ArticleLabel = Label {
        width: Fill height: Fit flow: Flow.Right{wrap: true}
        draw_text +: {color: #x191919 text_style: theme.font_regular{font_size: 13}}
    }
    mod.widgets.ArticleHtml = Html {
        width: Fill height: Fit padding: 0
        font_size: 12 font_color: #x191919
        heading_margin: Inset{top: 0.6 bottom: 0.2}
        draw_text.color: #x191919
        text_style_normal: theme.font_regular{font_size: 12}
        text_style_bold: theme.font_bold{font_size: 12}
        text_style_italic: theme.font_italic{font_size: 12}
        text_style_bold_italic: theme.font_bold_italic{font_size: 12}
    }
    mod.widgets.ArticleInput = TextInput {
        draw_bg +: {
            color: #xffffff color_hover: #xffffff color_focus: #xffffff color_down: #xffffff color_empty: #xffffff
            color_2_hover: vec4(-1.0) color_2_focus: vec4(-1.0) color_2_down: vec4(-1.0) color_2_empty: vec4(-1.0)
            border_color: #xe3e5e8 border_color_hover: #xe3e5e8 border_color_focus: #x07c160 border_color_down: #xe3e5e8 border_color_empty: #xe3e5e8
        }
        draw_text +: {color: #x191919 color_hover: #x191919 color_focus: #x191919 color_down: #x191919}
    }
    mod.widgets.ArticlePanel = #(ArticlePanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down show_bg: true
        draw_bg +: {pixel: fn() {return #xffffff}}
        padding: Inset{top: mod.widgets.SAFE_INSET_PAD_TOP + #(TOP) bottom: mod.widgets.SAFE_INSET_PAD_BOTTOM}
        header := SolidView {
            width: Fill height: 52 flow: Overlay draw_bg.color: #xededed
            View {
                width: Fill height: Fill align: Align{x: 0.5 y: 0.5}
                article_heading := Label {max_lines: 1 draw_text +: {color: #x191919 text_style: theme.font_bold{font_size: 15}}}
            }
            View {
                width: Fill height: Fill flow: Right padding: Inset{left: 8 right: 12} align: Align{y: 0.5}
                article_back := RobrixNeutralIconButton {
                    width: 36 height: 40 padding: 12 align: Align{x: 0.5 y: 0.5} spacing: 0
                    draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000 border_size: 0}
                    draw_icon +: {svg: ICON_CHEVRON_LEFT color: #x191919}
                    icon_walk: Walk{width: 8 height: 14}
                }
                View {width: Fill height: 1}
                article_close := RobrixNeutralIconButton {
                    text: #(crate::i18n::tr("Close")) i18n_text: "Close" height: 38
                    draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000 border_size: 0}
                    draw_text.color: #x191919
                }
            }
        }
        details := View {
            width: Fill height: Fill flow: Down padding: 24 spacing: 20
            View {height: 20 width: Fill}
            View {
                width: Fill height: 90 align: Align{x: 0.5 y: 0.5}
                RoundedView {
                    width: 80 height: 80 draw_bg +: {color: #x07c160 border_radius: 16} align: Align{x: 0.5 y: 0.5}
                    Icon {draw_icon +: {svg: ICON_FILE color: #xffffff} icon_walk: Walk{width: 48 height: 48}}
                }
            }
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Article editor")) i18n_text: "Article editor" draw_text.text_style: theme.font_bold{font_size: 22}}
            mod.widgets.ArticleLabel {text: "OctoSense · 1.0" draw_text.color: #x777777}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Write in Markdown, preview your article, and publish to a chat.")) i18n_text: "Write in Markdown, preview your article, and publish to a chat."}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Included with Robrix. Each person authorizes their own account; sharing this app does not share drafts or permissions.")) i18n_text: "Included with Robrix. Each person authorizes their own account; sharing this app does not share drafts or permissions." draw_text.color: #x777777}
            View {width: Fill height: Fill}
            article_continue := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Continue with Robrix")) i18n_text: "Continue with Robrix"}
        }
        consent := View {
            visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 22
            consent_account := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 16}}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("This app requests permission to:")) i18n_text: "This app requests permission to:"}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("• Save this app’s drafts on this device\n\n• Publish to a chat only after your confirmation")) i18n_text: "• Save this app’s drafts on this device\n\n• Publish to a chat only after your confirmation"}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Your password, session token and chat history stay private. Permission lasts for this open session, up to one hour.")) i18n_text: "Your password, session token and chat history stay private. Permission lasts for this open session, up to one hour." draw_text.color: #x777777}
            View {width: Fill height: Fill}
            article_allow := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Allow and open")) i18n_text: "Allow and open"}
            article_cancel := RobrixNeutralIconButton {width: Fill height: 40 text: #(crate::i18n::tr("Cancel")) i18n_text: "Cancel"}
        }
        editor := View {
            visible: false width: Fill height: Fill flow: Down padding: 18 spacing: 12
            article_title := mod.widgets.ArticleInput {width: Fill height: 46 draw_text.text_style: theme.font_bold{font_size: 16}}
            View {
                width: Fill height: 36 flow: Right spacing: 12
                article_bold := RobrixNeutralIconButton {text: "B" width: 42 height: 34}
                article_italic := RobrixNeutralIconButton {text: "I" width: 42 height: 34}
                article_h2 := RobrixNeutralIconButton {text: "H2" width: 42 height: 34}
                article_list := RobrixNeutralIconButton {text: "•" width: 42 height: 34}
            }
            article_markdown := mod.widgets.ArticleInput {width: Fill height: Fill is_multiline: true flow: Flow.Right{wrap: true} draw_text.text_style: theme.font_regular{font_size: 14}}
            article_preview := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Preview")) i18n_text: "Preview"}
            View {
                width: Fill height: 42 flow: Right spacing: 12
                article_save := RobrixNeutralIconButton {width: Fill height: 40 text: #(crate::i18n::tr("Save draft")) i18n_text: "Save draft"}
                article_share := RobrixNeutralIconButton {width: Fill height: 40 text: #(crate::i18n::tr("Share app")) i18n_text: "Share app"}
            }
        }
        preview := View {
            visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 16
            article_preview_scroll := ScrollYView {width: Fill height: Fill flow: Down article_html := mod.widgets.ArticleHtml {}}
            article_publish := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Publish")) i18n_text: "Publish"}
        }
        rooms := View {
            visible: false width: Fill height: Fill flow: Down padding: 18 spacing: 12
            article_chat_search := TextInput {width: Fill height: 44 empty_text: #(crate::i18n::tr("Find a chat")) i18n_empty_text: "Find a chat"}
            article_rooms := PortalList {
                width: Fill height: Fill
                Chat := NavigationBarButton {
                    width: Fill height: 64 padding: 12 align: Align{y: 0.5}
                    draw_bg +: {color_hover: #xf2f2f2 color_active: #xf2f2f2}
                    name := mod.widgets.ArticleLabel {max_lines: 2}
                }
            }
        }
        confirm := View {
            visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 16
            publish_account := mod.widgets.ArticleLabel {draw_text.color: #x576b95}
            publish_room := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 16}}
            confirm_scroll := ScrollYView {width: Fill height: Fill flow: Down confirm_html := mod.widgets.ArticleHtml {}}
            article_confirm := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Confirm publish")) i18n_text: "Confirm publish"}
            article_change := RobrixNeutralIconButton {width: Fill height: 40 text: #(crate::i18n::tr("Back")) i18n_text: "Back"}
        }
        success := View {
            visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 24 align: Align{x: 0.5}
            View {width: Fill height: Fill}
            Label {text: "✓" draw_text +: {color: #x07c160 text_style: theme.font_bold{font_size: 60}}}
            success_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 23}}
            success_room := mod.widgets.ArticleLabel {}
            View {width: Fill height: Fill}
            article_edit := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Continue editing")) i18n_text: "Continue editing"}
            article_share_again := RobrixNeutralIconButton {width: Fill height: 44 text: #(crate::i18n::tr("Share app")) i18n_text: "Share app"}
        }
        article_status := mod.widgets.ArticleLabel {padding: Inset{left: 18 right: 18 top: 4 bottom: 8} draw_text +: {color: #x777777 text_style: theme.font_regular{font_size: 11}}}
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ArticlePanel {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    active: bool,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    grant: Option<Grant>,
    #[rust]
    draft: Draft,
    #[rust]
    page: Page,
    #[rust]
    sharing: bool,
    #[rust]
    rooms: Vec<RoomNameId>,
    #[rust]
    prepared: Option<Prepared>,
    #[rust]
    pending: bool,
}
impl ArticlePanel {
    fn status(&self, cx: &mut Cx, text: &str) {
        self.label(cx, ids!(article_status)).set_text(cx, text);
    }
    fn show(&mut self, cx: &mut Cx, page: Page) {
        self.page = page;
        for (id, p) in [
            (id!(details), Page::Details),
            (id!(consent), Page::Consent),
            (id!(editor), Page::Edit),
            (id!(preview), Page::Preview),
            (id!(rooms), Page::Rooms),
            (id!(confirm), Page::Confirm),
            (id!(success), Page::Success),
        ] {
            self.view(cx, &[id]).set_visible(cx, p == page);
        }
        let heading = match page {
            Page::Details => "App details",
            Page::Consent => "Authorize app",
            Page::Edit | Page::Success => "Article editor",
            Page::Preview => "Article preview",
            Page::Rooms => "Choose chat",
            Page::Confirm => {
                if self.sharing {
                    "Share app"
                } else {
                    "Confirm publish"
                }
            }
        };
        self.label(cx, ids!(article_heading))
            .set_text(cx, crate::i18n::tr(heading));
        self.status(cx, "");
        self.redraw(cx);
    }
    fn authorized(&self) -> bool {
        self.grant
            .as_ref()
            .is_some_and(|g| g.valid(current_user_id().as_deref()))
            && !crate::logout::logout_state_machine::is_logout_in_progress()
    }
    fn bind_editor(&self, cx: &mut Cx) -> Result<(), String> {
        let fields = realize_editor(
            &self.draft,
            crate::i18n::language() == crate::i18n::Language::Chinese,
        )?;
        for (field, id) in fields
            .iter()
            .zip([id!(article_title), id!(article_markdown)])
        {
            let input = self.text_input(cx, &[id]);
            input.set_text(cx, &field.text);
            input.set_empty_text(cx, field.placeholder.clone());
        }
        Ok(())
    }
    fn save(&self, cx: &mut Cx) {
        let result = self
            .grant
            .as_ref()
            .ok_or("Authorization expired".into())
            .and_then(|g| {
                save_draft(
                    &crate::app_data_dir(),
                    g,
                    current_user_id().as_deref(),
                    &self.draft,
                )
            });
        self.status(
            cx,
            &match result {
                Ok(()) => crate::i18n::tr("Draft saved on this device").into(),
                Err(e) => e,
            },
        );
    }
    fn choose(&mut self, cx: &mut Cx, sharing: bool) {
        if !self.authorized() {
            return;
        }
        self.sharing = sharing;
        self.prepared = None;
        self.text_input(cx, ids!(article_chat_search))
            .set_text(cx, "");
        self.rooms = cx.get_global::<RoomsListRef>().mini_app_share_rooms();
        self.portal_list(cx, ids!(article_rooms))
            .set_first_id_and_scroll(0, 0.0);
        self.show(cx, Page::Rooms);
        if self.rooms.is_empty() {
            self.status(cx, crate::i18n::tr("Join a chat before publishing."));
        }
    }
    fn back(&mut self, cx: &mut Cx) {
        if self.pending {
            return;
        }
        match self.page {
            Page::Details => cx.action(ArticleAction::Close),
            Page::Consent => self.show(cx, Page::Details),
            Page::Edit => {
                self.save(cx);
                cx.action(ArticleAction::Close);
            }
            Page::Preview | Page::Success => self.show(cx, Page::Edit),
            Page::Rooms => self.show(
                cx,
                if self.sharing {
                    Page::Edit
                } else {
                    Page::Preview
                },
            ),
            Page::Confirm => self.show(cx, Page::Rooms),
        }
    }
    fn send(&mut self, cx: &mut Cx) {
        if self.pending || self.page != Page::Confirm || !self.authorized() {
            return;
        }
        let (Some(grant), Some(prepared), Some(client)) =
            (self.grant.clone(), self.prepared.clone(), get_client())
        else {
            return;
        };
        if !grant.valid(client.user_id()) {
            return;
        }
        self.pending = true;
        self.button(cx, ids!(article_confirm))
            .set_enabled(cx, false);
        self.status(cx, crate::i18n::tr("Sending…"));
        spawn_async_task(async move {
            let result = async {
                let room = client
                    .get_room(prepared.room.room_id())
                    .ok_or("This chat is no longer joined.")?;
                if room.state() != matrix_sdk::RoomState::Joined
                    || crate::moments::is_moments(&room)
                {
                    return Err("This chat is no longer joined.".into());
                }
                let member = room
                    .get_member(&grant.owner)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or("Unable to confirm room membership.")?;
                if !member.can_send_message(ruma::events::MessageLikeEventType::RoomMessage) {
                    return Err("You cannot publish in this chat.".into());
                }
                // Recheck after async membership/power lookup, against the captured
                // client, login generation and revocation cell. SDK owns credentials.
                if !grant.valid(client.user_id())
                    || crate::logout::logout_state_machine::is_logout_in_progress()
                    || room.state() != matrix_sdk::RoomState::Joined
                {
                    return Err("Authorization expired".into());
                }
                room.send(prepared.message)
                    .with_transaction_id(prepared.transaction.clone())
                    .with_request_config(matrix_sdk::config::RequestConfig::new().retry_limit(0))
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
            .await;
            Cx::post_action(Sent {
                instance: grant.instance,
                transaction: prepared.transaction,
                result,
            });
        });
    }
}
impl Widget for ArticlePanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.active {
            return;
        }
        if self.owner.as_ref() != current_user_id().as_ref()
            || self
                .grant
                .as_ref()
                .is_some_and(|g| !g.valid(current_user_id().as_deref()))
            || matches!(event, Event::Shutdown)
        {
            cx.action(ArticleAction::Close);
            return;
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(sent) = action.downcast_ref::<Sent>() {
                    if self
                        .grant
                        .as_ref()
                        .is_some_and(|g| g.instance == sent.instance)
                        && self
                            .prepared
                            .as_ref()
                            .is_some_and(|p| p.transaction == sent.transaction)
                    {
                        self.pending = false;
                        self.button(cx, ids!(article_confirm)).set_enabled(cx, true);
                        match &sent.result {
                            Ok(()) => {
                                let prepared = self.prepared.as_ref().unwrap().clone();
                                self.show(cx, Page::Success);
                                self.label(cx, ids!(success_title)).set_text(
                                    cx,
                                    crate::i18n::tr(if prepared.sharing {
                                        "Mini app sent"
                                    } else {
                                        "Published"
                                    }),
                                );
                                self.label(cx, ids!(success_room))
                                    .set_text(cx, &prepared.room.display());
                            }
                            Err(error) => self.status(
                                cx,
                                &format!(
                                    "{} {}",
                                    crate::i18n::tr(
                                        "Could not send. Retry uses the same transaction."
                                    ),
                                    crate::i18n::tr(error)
                                ),
                            ),
                        }
                    }
                }
            }
            if self.button(cx, ids!(article_close)).clicked(actions)
                || self.button(cx, ids!(article_cancel)).clicked(actions)
            {
                cx.action(ArticleAction::Close);
                return;
            }
            if self.button(cx, ids!(article_back)).clicked(actions)
                || self.button(cx, ids!(article_change)).clicked(actions)
            {
                self.back(cx);
            }
            if self.page == Page::Details
                && self.button(cx, ids!(article_continue)).clicked(actions)
            {
                if let Some(owner) = current_user_id() {
                    self.owner = Some(owner.clone());
                    self.show(cx, Page::Consent);
                    self.label(cx, ids!(consent_account))
                        .set_text(cx, owner.as_str());
                } else {
                    self.status(
                        cx,
                        crate::i18n::tr("Sign in to Robrix to authorize this app."),
                    );
                }
            }
            if self.page == Page::Consent && self.button(cx, ids!(article_allow)).clicked(actions) {
                if let Some(owner) = current_user_id().filter(|o| Some(o) == self.owner.as_ref()) {
                    let grant = Grant::new(owner);
                    let draft =
                        read_draft(&crate::app_data_dir(), &grant, current_user_id().as_deref());
                    match draft {
                        Ok(draft) => {
                            self.grant = Some(grant);
                            self.draft = draft;
                            if let Err(e) = self.bind_editor(cx) {
                                self.status(cx, &e);
                                return;
                            }
                            self.show(cx, Page::Edit);
                        }
                        Err(e) => self.status(cx, &e),
                    }
                }
            }
            if self.page == Page::Edit && self.authorized() {
                for (id, event) in [
                    (id!(article_title), "title_changed"),
                    (id!(article_markdown), "markdown_changed"),
                ] {
                    if let Some(value) = self.text_input(cx, &[id]).changed(actions) {
                        match apply_input(&mut self.draft, event, &value) {
                            Ok(()) => {
                                self.prepared = None;
                                self.status(cx, crate::i18n::tr("Unsaved changes"));
                            }
                            Err(e) => {
                                let _ = self.bind_editor(cx);
                                self.status(cx, &e);
                            }
                        }
                    }
                }
                for (id, addition) in [
                    (id!(article_bold), "**text**"),
                    (id!(article_italic), "*text*"),
                    (id!(article_h2), "\n## "),
                    (id!(article_list), "\n- "),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.text_input(cx, ids!(article_markdown))
                            .set_key_focus(cx);
                        let input = self.text_input(cx, ids!(article_markdown));
                        let sel = input.selection();
                        let range = sel.anchor.index.min(sel.cursor.index)
                            ..sel.anchor.index.max(sel.cursor.index);
                        let _ = input.replace_range(
                            cx,
                            range,
                            addition,
                            makepad_widgets::text_input::UndoGroup::New,
                        );
                        let text = self.text_input(cx, ids!(article_markdown)).text();
                        if let Err(e) = apply_input(&mut self.draft, "markdown_changed", &text) {
                            let _ = self.bind_editor(cx);
                            self.status(cx, &e);
                        } else {
                            self.prepared = None;
                        }
                    }
                }
                if self.button(cx, ids!(article_save)).clicked(actions) {
                    self.save(cx);
                }
                if self.button(cx, ids!(article_preview)).clicked(actions) {
                    match self.draft.html() {
                        Ok(html) => {
                            self.html(cx, ids!(article_html)).set_text(cx, &html);
                            self.show(cx, Page::Preview);
                            self.save(cx);
                        }
                        Err(e) => self.status(cx, &e),
                    }
                }
                if self.button(cx, ids!(article_share)).clicked(actions) {
                    self.save(cx);
                    self.choose(cx, true);
                }
            }
            if self.page == Page::Preview && self.button(cx, ids!(article_publish)).clicked(actions)
            {
                self.choose(cx, false);
            }
            if self.page == Page::Success {
                if self.button(cx, ids!(article_edit)).clicked(actions) {
                    self.show(cx, Page::Edit);
                }
                if self.button(cx, ids!(article_share_again)).clicked(actions) {
                    self.choose(cx, true);
                }
            }
            if self.page == Page::Rooms && self.authorized() {
                if let Some(query) = self
                    .text_input(cx, ids!(article_chat_search))
                    .changed(actions)
                {
                    self.rooms = cx
                        .get_global::<RoomsListRef>()
                        .mini_app_share_rooms()
                        .into_iter()
                        .filter(|r| r.display().to_lowercase().contains(&query.to_lowercase()))
                        .collect();
                    self.portal_list(cx, ids!(article_rooms))
                        .set_first_id_and_scroll(0, 0.0);
                    self.redraw(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(article_rooms))
                    .items_with_actions(actions)
                {
                    if item.as_navigation_bar_button().clicked(actions) {
                        if let Some(room) = self.rooms.get(index).cloned() {
                            let message = if self.sharing {
                                Ok(ArticlePackage::builtin().message())
                            } else {
                                self.draft.message()
                            };
                            match message {
                                Ok(message) => {
                                    let html = if self.sharing {
                                        format!(
                                            "<h1>{}</h1><p>Markdown · OctoSense</p>",
                                            crate::i18n::tr("Article editor")
                                        )
                                    } else {
                                        self.draft.html().unwrap()
                                    };
                                    self.html(cx, ids!(confirm_html)).set_text(cx, &html);
                                    self.label(cx, ids!(publish_account))
                                        .set_text(cx, self.owner.as_ref().unwrap().as_str());
                                    self.label(cx, ids!(publish_room))
                                        .set_text(cx, &room.display());
                                    self.prepared = Some(Prepared {
                                        room,
                                        message,
                                        transaction: TransactionId::new(),
                                        sharing: self.sharing,
                                    });
                                    self.button(cx, ids!(article_confirm)).set_text(
                                        cx,
                                        crate::i18n::tr(if self.sharing {
                                            "Confirm share"
                                        } else {
                                            "Confirm publish"
                                        }),
                                    );
                                    self.show(cx, Page::Confirm);
                                }
                                Err(e) => self.status(cx, &e),
                            }
                        }
                        break;
                    }
                }
            }
            if self.button(cx, ids!(article_confirm)).clicked(actions) {
                self.send(cx);
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
            self.back(cx);
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
                    let item = list.item(cx, index, id!(Chat));
                    item.label(cx, ids!(name)).set_text(cx, &room.display());
                    item.draw_all(cx, &mut Scope::empty());
                }
            }
        }
        DrawStep::done()
    }
}
impl ArticlePanelRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &ArticleAction) {
        let Some(mut panel) = self.borrow_mut() else {
            return;
        };
        match action {
            ArticleAction::Open => {
                if let Some(g) = panel.grant.take() {
                    g.revoke();
                }
                panel.active = true;
                panel.owner = current_user_id();
                panel.draft = Draft::default();
                panel.pending = false;
                panel.prepared = None;
                panel
                    .button(cx, ids!(article_confirm))
                    .set_enabled(cx, true);
                panel.show(cx, Page::Details);
                modal.open(cx);
            }
            ArticleAction::Close => {
                if panel.grant.is_some() && panel.authorized() {
                    panel.save(cx);
                }
                if let Some(g) = panel.grant.take() {
                    g.revoke();
                }
                panel.active = false;
                panel.owner = None;
                panel.draft = Draft::default();
                panel.prepared = None;
                panel.rooms.clear();
                panel.text_input(cx, ids!(article_title)).set_text(cx, "");
                panel
                    .text_input(cx, ids!(article_markdown))
                    .set_text(cx, "");
                panel.html(cx, ids!(article_html)).set_text(cx, "");
                panel.html(cx, ids!(confirm_html)).set_text(cx, "");
                modal.close(cx);
            }
        }
    }
}
