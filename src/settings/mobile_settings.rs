//! WeChat-shaped mobile settings backed by the existing account and preference APIs.
use makepad_widgets::*;
use matrix_sdk::{encryption::VerificationState, ruma::OwnedUserId};
use crate::{
    app::AppState,
    block_user_modal::{BlockUserModalAction, BlockUserRequest},
    home::navigation_tab_bar::{get_own_profile, NavigationBarAction},
    logout::logout_confirm_modal::{LogoutAction, LogoutConfirmModalAction},
    profile::user_profile::UserProfile,
    shared::{avatar::{AvatarState, AvatarWidgetExt}, navigation_bar_button::{NavigationBarButtonWidgetExt, NavigationBarButtonWidgetRefExt},
        popup_list::{enqueue_popup_notification, PopupKind}},
    sliding_sync::{current_user_id, get_client, get_blocked_users, submit_async_request, AccountDataAction, MatrixRequest},
    utils,
};
use super::{account_settings::{AccountManagementUrl, AccountSettingsAction, pick_profile_photo, confirm_remove_profile_photo},
    app_preferences::{AppPreferencesGlobal, MarkAsReadBehavior, ReadReceiptsPrivacy, ThumbnailMaxHeight, UiZoom}};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let Switch = ToggleFlat {
        width: 46 height: 28 padding: 0 text: "" label_walk: Walk{width: 0 height: 0}
        draw_bg +: {pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.box(0.0, 0.0, 46.0, 28.0, 14.0)
            sdf.fill((#xdcdcdc).mix(#x07c160, self.active))
            sdf.circle(14.0 + 18.0 * self.active, 14.0, 12.0)
            sdf.fill(#xffffff)
            return sdf.result
        }}
    }
    let SwitchRow = View {
        width: Fill height: 56 flow: Right padding: Inset{left: 20 right: 20} align: Align{y: 0.5}
        title := DetailLabel {width: Fill max_lines: 2}
        toggle := Switch {}
    }
    mod.widgets.MobileSettings = #(MobileSettings::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down show_bg: true draw_bg.color: #xededed
        header := DetailHeader {}
        pages := PageFlip {
            width: Fill height: Fill active_page: @settings
            settings := ScrollYView {
                width: Fill height: Fill flow: Down
                DetailSection {
                    account_row := DetailRow {title.text: #(crate::i18n::tr("Account and Security")) title.i18n_text: "Account and Security"}
                    DetailDivider {}
                    privacy_row := DetailRow {title.text: #(crate::i18n::tr("Privacy")) title.i18n_text: "Privacy"}
                    DetailDivider {}
                    general_row := DetailRow {title.text: #(crate::i18n::tr("General")) title.i18n_text: "General"}
                }
                DetailGap {}
                DetailSection {
                    help_row := DetailRow {title.text: #(crate::i18n::tr("Help & Feedback")) title.i18n_text: "Help & Feedback"}
                    DetailDivider {}
                    about_row := DetailRow {title.text: #(crate::i18n::tr("About Robrix")) title.i18n_text: "About Robrix"}
                }
                DetailGap {}
                DetailSection {logout_row := DetailAction {title +: {text: #(crate::i18n::tr("Log Out")) i18n_text: "Log Out" draw_text.color: #x191919}}}
            }
            personal := ScrollYView {
                width: Fill height: Fill flow: Down
                DetailSection {
                    photo_row := NavigationBarButton {
                        width: Fill height: 88 padding: Inset{left: 20 right: 20} spacing: 12 flow: Right align: Align{y: 0.5}
                        draw_bg +: {color_hover: #xe5e5e5 border_radius: 0}
                        DetailLabel {width: Fill text: #(crate::i18n::tr("Profile Photo")) i18n_text: "Profile Photo"}
                        photo_thumbnail := DetailAvatar {width: 60 height: 60}
                        Icon {icon_walk: Walk{width: 7 height: 12} draw_icon +: {svg: crate_resource("self://resources/icons/mobile_chevron_right.svg") color: #xb2b2b2}}
                    }
                    DetailDivider {}
                    name_row := DetailRow {title.text: #(crate::i18n::tr("Name")) title.i18n_text: "Name"}
                    DetailDivider {}
                    id_row := DetailRow {title.text: #(crate::i18n::tr("Matrix ID")) title.i18n_text: "Matrix ID"}
                    DetailDivider {}
                    more_row := DetailRow {title.text: #(crate::i18n::tr("More Info")) title.i18n_text: "More Info"}
                }
            }
            name := View {
                width: Fill height: Fill flow: Down
                DetailSection {
                    name_input := RobrixTextInput {
                        width: Fill height: 56 padding: Inset{left: 20 right: 20 top: 16 bottom: 16}
                        empty_text: #(crate::i18n::tr("Name")) i18n_empty_text: "Name" draw_text +: {color: #x191919 text_style: theme.font_regular {font_size: 12.5}}
                        draw_bg +: {color: #xffffff color_hover: #xffffff color_focus: #xffffff border_size: 0 border_radius: 0}
                    }
                }
                DetailNote {text: #(crate::i18n::tr("Your name is visible to people you chat with.")) i18n_text: "Your name is visible to people you chat with."}
            }
            photo := ScrollYView {
                width: Fill height: Fill flow: Down
                View {width: Fill height: Fit padding: 30 align: Align{x: 0.5} photo_large := DetailAvatar {width: 220 height: 220}}
                DetailSection {
                    change_photo := DetailAction {title.text: #(crate::i18n::tr("Change Photo")) title.i18n_text: "Change Photo"}
                    DetailDivider {}
                    remove_photo := DetailAction {title +: {text: #(crate::i18n::tr("Remove Photo")) i18n_text: "Remove Photo" draw_text.color: #xfa5151}}
                }
            }
            account := ScrollYView {
                width: Fill height: Fill flow: Down
                DetailSection {
                    personal_row := DetailRow {title.text: #(crate::i18n::tr("Personal Information")) title.i18n_text: "Personal Information"}
                    DetailDivider {}
                    account_id := DetailRow {title.text: #(crate::i18n::tr("Matrix ID")) title.i18n_text: "Matrix ID"}
                    DetailDivider {}
                    server_row := DetailRow {title.text: #(crate::i18n::tr("Homeserver")) title.i18n_text: "Homeserver" chevron.visible: false}
                }
                DetailGap {}
                DetailSection {
                    verify_row := DetailRow {title.text: #(crate::i18n::tr("Device Verification")) title.i18n_text: "Device Verification"}
                    DetailDivider {}
                    device_row := DetailRow {title.text: #(crate::i18n::tr("Device ID")) title.i18n_text: "Device ID" chevron.visible: false}
                    DetailDivider {}
                    manage_row := DetailRow {title.text: #(crate::i18n::tr("Manage Account")) title.i18n_text: "Manage Account"}
                }
                DetailNote {text: #(crate::i18n::tr("Account and password options open on your homeserver's website.")) i18n_text: "Account and password options open on your homeserver's website."}
            }
            general := ScrollYView {
                width: Fill height: Fill flow: Down
                hagency_section := DetailSection {
                    visible: #(cfg!(feature = "agent_chat"))
                    hagency_row := DetailRow {title.text: #(crate::i18n::tr("Hagency")) title.i18n_text: "Hagency"}
                }
                DetailGap {}
                DetailSection {language_row := DetailRow {title.text: #(crate::i18n::tr("Language")) title.i18n_text: "Language"}}
                DetailGap {}
                DetailSection {
                    enter_row := SwitchRow {title.text: #(crate::i18n::tr("Send with Enter")) title.i18n_text: "Send with Enter"}
                }
                DetailNote {text: #(crate::i18n::tr("Applies to a hardware keyboard. Shift + Enter inserts a new line.")) i18n_text: "Applies to a hardware keyboard. Shift + Enter inserts a new line."}
                DetailSection {
                    zoom_row := DetailRow {title.text: #(crate::i18n::tr("Display Size")) title.i18n_text: "Display Size"}
                    DetailDivider {}
                    images_row := DetailRow {title.text: #(crate::i18n::tr("Chat Image Size")) title.i18n_text: "Chat Image Size"}
                }
            }
            language := ScrollYView {
                width: Fill height: Fill flow: Down
                DetailSection {
                    language_en := DetailRow {title.text: "English"}
                    DetailDivider {}
                    language_zh := DetailRow {title.text: "简体中文"}
                }
                DetailNote {text: #(crate::i18n::tr("Interface language changes immediately. Messages and names stay unchanged.")) i18n_text: "Interface language changes immediately. Messages and names stay unchanged."}
            }
            hagency := ScrollYView {
                width: Fill height: Fill flow: Down padding: 16
                mod.widgets.AgentChatPreferences {}
            }
            zoom := ScrollYView {
                width: Fill height: Fill flow: Down
                DetailSection {
                    zoom_small := DetailRow {title.text: #(crate::i18n::tr("Smaller")) title.i18n_text: "Smaller" value.text: "90%"}
                    DetailDivider {}
                    zoom_default := DetailRow {title.text: #(crate::i18n::tr("Standard")) title.i18n_text: "Standard" value.text: "100%"}
                    DetailDivider {}
                    zoom_large := DetailRow {title.text: #(crate::i18n::tr("Larger")) title.i18n_text: "Larger" value.text: "110%"}
                }
                DetailNote {text: #(crate::i18n::tr("Changes the size of text and controls throughout Robrix.")) i18n_text: "Changes the size of text and controls throughout Robrix."}
            }
            images := ScrollYView {
                width: Fill height: Fill flow: Down
                DetailSection {
                    images_small := DetailRow {title.text: #(crate::i18n::tr("Small")) title.i18n_text: "Small"}
                    DetailDivider {}
                    images_medium := DetailRow {title.text: #(crate::i18n::tr("Medium")) title.i18n_text: "Medium"}
                    DetailDivider {}
                    images_large := DetailRow {title.text: #(crate::i18n::tr("Large")) title.i18n_text: "Large"}
                }
            }
            privacy := ScrollYView {
                width: Fill height: Fill flow: Down
                DetailSection {
                    public_receipts := SwitchRow {title.text: #(crate::i18n::tr("Share Read Receipts")) title.i18n_text: "Share Read Receipts"}
                    DetailDivider {}
                    automatic_read := SwitchRow {title.text: #(crate::i18n::tr("Mark Messages as Read")) title.i18n_text: "Mark Messages as Read"}
                    DetailDivider {}
                    show_receipts := SwitchRow {title.text: #(crate::i18n::tr("Show Read Receipts")) title.i18n_text: "Show Read Receipts"}
                }
                DetailNote {text: #(crate::i18n::tr("When sharing is off, read markers sync only to your own devices. Turn off automatic marking to mark chats as read manually.")) i18n_text: "When sharing is off, read markers sync only to your own devices. Turn off automatic marking to mark chats as read manually."}
                DetailSection {blocked_row := DetailRow {title.text: #(crate::i18n::tr("Blocked Users")) title.i18n_text: "Blocked Users"}}
            }
            blocked := View {
                width: Fill height: Fill flow: Down
                blocked_empty := DetailNote {text: #(crate::i18n::tr("You haven't blocked anyone.")) i18n_text: "You haven't blocked anyone."}
                blocked_list := PortalList {
                    width: Fill height: Fill
                    User := DetailSection {
                        unblock := DetailRow {title +: {width: Fill text_overflow: Ellipsis} value +: {width: Fit text: #(crate::i18n::tr("Unblock")) i18n_text: "Unblock"}}
                        DetailDivider {}
                    }
                }
            }
            about := ScrollYView {
                width: Fill height: Fill flow: Down
                View {
                    width: Fill height: Fit padding: Inset{top: 48 bottom: 40} flow: Down spacing: 16 align: Align{x: 0.5}
                    Image {width: 72 height: 72 fit: ImageFit.Smallest src: crate_resource("self://resources/robrix_logo_alpha.png")}
                    DetailLabel {text: "Robrix" draw_text.text_style: theme.font_bold {font_size: 20}}
                    version := DetailLabel {draw_text.color: #x888888}
                }
                DetailSection {
                    website_row := DetailRow {title.text: #(crate::i18n::tr("Website")) title.i18n_text: "Website"}
                    DetailDivider {}
                    policy_row := DetailRow {title.text: #(crate::i18n::tr("Privacy Policy")) title.i18n_text: "Privacy Policy"}
                    DetailDivider {}
                    source_row := DetailRow {title.text: #(crate::i18n::tr("Source Code")) title.i18n_text: "Source Code"}
                }
            }
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Page { #[default] Settings, Personal, Name, Photo, Account, General, Language, Hagency, Zoom, Images, Privacy, Blocked, About }
impl Page {
    fn id(self) -> LiveId { match self {
        Self::Settings => id!(settings), Self::Personal => id!(personal), Self::Name => id!(name),
        Self::Photo => id!(photo), Self::Account => id!(account), Self::General => id!(general),
        Self::Zoom => id!(zoom), Self::Images => id!(images), Self::Privacy => id!(privacy),
        Self::Blocked => id!(blocked), Self::About => id!(about), Self::Language => id!(language),
        Self::Hagency => id!(hagency),
    }}
    fn title(self) -> &'static str { crate::i18n::tr(match self {
        Self::Settings => crate::i18n::tr("Settings"), Self::Personal => crate::i18n::tr("Personal Information"), Self::Name => crate::i18n::tr("Name"),
        Self::Photo => crate::i18n::tr("Profile Photo"), Self::Account => crate::i18n::tr("Account and Security"), Self::General => crate::i18n::tr("General"),
        Self::Zoom => crate::i18n::tr("Display Size"), Self::Images => crate::i18n::tr("Chat Image Size"), Self::Privacy => crate::i18n::tr("Privacy"),
        Self::Blocked => crate::i18n::tr("Blocked Users"), Self::About => crate::i18n::tr("About Robrix"), Self::Language => crate::i18n::tr("Language"),
        Self::Hagency => "Hagency",
    })}
}

#[derive(Script, ScriptHook, Widget)]
pub struct MobileSettings {
    #[source] source: ScriptObjectRef,
    #[deref] view: View,
    #[rust] page: Page,
    #[rust] history: Vec<Page>,
    #[rust] profile: Option<UserProfile>,
    #[rust] saving_name: bool,
    #[rust] saving_photo: bool,
    #[rust] account_url: AccountManagementUrl,
    #[rust] blocked_users: Vec<OwnedUserId>,
}
impl MobileSettings {
    fn open(&mut self, cx: &mut Cx, page: Page) {
        if self.page == page { return; }
        self.history.push(self.page);
        self.page = page;
        if page == Page::Name {
            self.view.text_input(cx, ids!(name_input)).set_text(cx, self.profile.as_ref().and_then(|p| p.username.as_deref()).unwrap_or(""));
        }
        cx.set_key_focus(self.view.area());
        self.redraw(cx);
    }
    fn back(&mut self, cx: &mut Cx) {
        if self.saving_name && self.page == Page::Name { return; }
        if let Some(page) = self.history.pop() { self.page = page; self.redraw(cx); }
        else { cx.action(NavigationBarAction::CloseSettings); }
    }
    fn copy_id(&self, cx: &mut Cx) {
        if let Some(profile) = &self.profile {
            cx.copy_to_clipboard(profile.user_id.as_str());
            enqueue_popup_notification(crate::i18n::tr("Matrix ID copied."), PopupKind::Success, Some(2.0));
        }
    }
    fn show_avatar(&self, cx: &mut Cx, path: &[LiveId]) {
        let Some(profile) = &self.profile else { return; };
        let avatar = self.view.avatar(cx, path);
        let mut state = profile.avatar_state.clone();
        state.update_from_cache(cx)
            .and_then(|image| avatar.show_image(cx, None, |cx, img| utils::load_avatar_image(&img, cx, image)).ok())
            .unwrap_or_else(|| avatar.show_text(cx, None, None, profile.displayable_name()));
    }
    fn open_account_url(&self) {
        match &self.account_url {
            AccountManagementUrl::Known(url) => utils::open_url(url.as_str()),
            _ => enqueue_popup_notification(crate::i18n::tr("Your homeserver has no account management page available."), PopupKind::Warning, Some(5.0)),
        }
    }
}
impl Widget for MobileSettings {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if scope.data.get::<AppState>().is_some_and(|app| app.selected_tab == crate::home::navigation_tab_bar::SelectedTab::Settings) && event.back_pressed() {
            self.back(cx); return;
        }
        if let Hit::KeyUp(key) = event.hits(cx, self.view.area()) {
            if key.key_code == KeyCode::Escape { self.back(cx); return; }
        }
        if matches!(event, Event::Signal) {
            crate::avatar_cache::process_avatar_updates(cx);
            self.redraw(cx);
        }
        let Event::Actions(actions) = event else { return; };
        for action in actions {
            if matches!(action.downcast_ref::<LogoutAction>(), Some(LogoutAction::LogoutSuccess) | Some(LogoutAction::ClearAppState {..})) {
                self.profile = None; self.history.clear(); self.page = Page::Settings;
                self.saving_name = false; self.saving_photo = false; self.account_url = AccountManagementUrl::Unknown;
            }
            if self.profile.as_ref().map(|p| &p.user_id) != current_user_id().as_ref() { continue; }
            match action.downcast_ref::<AccountDataAction>() {
                Some(AccountDataAction::DisplayNameChanged(name)) => {
                    if let Some(profile) = &mut self.profile { profile.username = name.clone(); }
                    if self.saving_name { self.saving_name = false; if self.page == Page::Name { self.back(cx); } }
                }
                Some(AccountDataAction::DisplayNameChangeFailed(_)) => self.saving_name = false,
                Some(AccountDataAction::AvatarChanged(url)) => {
                    if let Some(profile) = &mut self.profile { profile.avatar_state = AvatarState::Known(url.clone()); }
                    if self.saving_photo && !crate::home::home_screen::effective_is_desktop(cx) {
                        enqueue_popup_notification(crate::i18n::tr("Profile photo updated."), PopupKind::Success, Some(3.0));
                    }
                    self.saving_photo = false;
                }
                Some(AccountDataAction::AvatarChangeFailed(error)) => {
                    if !crate::home::home_screen::effective_is_desktop(cx) {
                        enqueue_popup_notification(error.clone(), PopupKind::Error, Some(5.0));
                    }
                    self.saving_photo = false;
                }
                Some(AccountDataAction::AccountManagementUrlFetched(url)) => {
                    let waiting = matches!(self.account_url, AccountManagementUrl::OpenWhenKnown);
                    self.account_url = url.clone();
                    if waiting { self.open_account_url(); }
                }
                _ => {}
            }
            if action.downcast_ref::<AccountSettingsAction>().is_some() { self.saving_photo = true; }
        }
        if self.view.button(cx, ids!(header.back)).clicked(actions) { self.back(cx); }
        for (path, page) in [
            (ids!(account_row), Page::Account), (ids!(general_row), Page::General), (ids!(privacy_row), Page::Privacy),
            (ids!(about_row), Page::About), (ids!(personal_row), Page::Personal), (ids!(photo_row), Page::Photo),
            (ids!(name_row), Page::Name), (ids!(more_row), Page::Account), (ids!(zoom_row), Page::Zoom),
            (ids!(images_row), Page::Images), (ids!(blocked_row), Page::Blocked), (ids!(language_row), Page::Language),
            (ids!(hagency_row), Page::Hagency),
        ] { if self.view.navigation_bar_button(cx, path).clicked(actions) { self.open(cx, page); } }
        for (path, language) in [(ids!(language_en), crate::i18n::Language::English), (ids!(language_zh), crate::i18n::Language::Chinese)] {
            if self.view.navigation_bar_button(cx, path).clicked(actions) {
                if let Err(error) = crate::i18n::set_language(cx, language) {
                    enqueue_popup_notification(crate::i18n::format("Could not save language: {error}", &[("error", error.to_string())]), PopupKind::Error, Some(5.0));
                }
            }
        }
        for path in [ids!(id_row), ids!(account_id)] {
            if self.view.navigation_bar_button(cx, path).clicked(actions) { self.copy_id(cx); }
        }
        if self.view.button(cx, ids!(header.save)).clicked(actions) && !self.saving_name {
            let value = self.view.text_input(cx, ids!(name_input)).text().trim().to_owned();
            let new_display_name = if value.is_empty() { None } else { Some(value) };
            if self.profile.as_ref().is_some_and(|p| p.username != new_display_name) {
                self.saving_name = true;
                submit_async_request(MatrixRequest::SetDisplayName {new_display_name});
            } else { self.back(cx); }
        }
        if self.view.navigation_bar_button(cx, ids!(change_photo)).clicked(actions) && !self.saving_photo { pick_profile_photo(); }
        if self.view.navigation_bar_button(cx, ids!(remove_photo)).clicked(actions) && !self.saving_photo {
            if self.profile.as_ref().is_some_and(|p| p.avatar_state.has_avatar()) { confirm_remove_profile_photo(cx); }
        }
        if self.view.navigation_bar_button(cx, ids!(verify_row)).clicked(actions) {
            submit_async_request(MatrixRequest::RequestSelfVerification);
        }
        if self.view.navigation_bar_button(cx, ids!(manage_row)).clicked(actions) {
            match self.account_url {
                AccountManagementUrl::Unknown => { self.account_url = AccountManagementUrl::OpenWhenKnown; submit_async_request(MatrixRequest::GetAccountManagementUrl); }
                AccountManagementUrl::OpenWhenKnown => {}
                _ => self.open_account_url(),
            }
        }
        if self.view.navigation_bar_button(cx, ids!(logout_row)).clicked(actions) { cx.action(LogoutConfirmModalAction::Open); }
        for (path, url) in [
            (ids!(help_row), "https://github.com/project-robius/robrix/issues/new"),
            (ids!(website_row), "https://robrix.app"), (ids!(policy_row), "https://robrix.app/privacy/"),
            (ids!(source_row), "https://github.com/project-robius/robrix"),
        ] { if self.view.navigation_bar_button(cx, path).clicked(actions) { utils::open_url(url); } }
        if let Some(app) = scope.data.get_mut::<AppState>() {
            let prefs = &mut app.app_prefs;
            if let Some(active) = self.view.check_box(cx, ids!(enter_row.toggle)).changed(actions) {
                prefs.send_on_enter = active; prefs.on_send_on_enter_changed(cx);
            }
            if let Some(active) = self.view.check_box(cx, ids!(public_receipts.toggle)).changed(actions) {
                prefs.read_receipts_privacy = if active { ReadReceiptsPrivacy::Everyone } else { ReadReceiptsPrivacy::OnlyMyDevices };
                prefs.on_read_receipts_privacy_changed(cx);
            }
            if let Some(active) = self.view.check_box(cx, ids!(automatic_read.toggle)).changed(actions) {
                prefs.mark_as_read_behavior = if active { MarkAsReadBehavior::WhenViewingMessages } else { MarkAsReadBehavior::Manual };
                prefs.on_mark_as_read_behavior_changed(cx);
            }
            if let Some(active) = self.view.check_box(cx, ids!(show_receipts.toggle)).changed(actions) {
                prefs.show_read_receipts = active; prefs.on_show_read_receipts_changed(cx);
            }
            for (path, value) in [(ids!(zoom_small), 0.9), (ids!(zoom_default), 1.0), (ids!(zoom_large), 1.1)] {
                if self.view.navigation_bar_button(cx, path).clicked(actions) {
                    prefs.ui_zoom = UiZoom::new(value); prefs.on_ui_zoom_changed(cx); self.back(cx);
                }
            }
            for (path, value) in [(ids!(images_small), ThumbnailMaxHeight::Small), (ids!(images_medium), ThumbnailMaxHeight::Medium), (ids!(images_large), ThumbnailMaxHeight::Large)] {
                if self.view.navigation_bar_button(cx, path).clicked(actions) {
                    prefs.thumbnail_max_height = value; prefs.on_thumbnail_max_height_changed(cx); self.back(cx);
                }
            }
        }
        let list = self.view.portal_list(cx, ids!(blocked_list));
        for (index, item) in list.items_with_actions(actions) {
            if item.navigation_bar_button(cx, ids!(unblock)).clicked(actions) {
                if let Some(user_id) = self.blocked_users.get(index).cloned() {
                    cx.action(BlockUserModalAction::Open(BlockUserRequest {user_id, display_name: None, block: false, reject_invite_to: None}));
                }
            }
        }
        self.redraw(cx);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.page_flip(cx, ids!(pages)).set_active_page(cx, self.page.id());
        self.view.label(cx, ids!(header.title)).set_text(cx, self.page.title());
        let language = crate::i18n::language();
        self.view.label(cx, ids!(language_row.value)).set_text(cx, language.name());
        self.view.label(cx, ids!(language_en.value)).set_text(cx, if language == crate::i18n::Language::English { "✓" } else { "" });
        self.view.label(cx, ids!(language_zh.value)).set_text(cx, if language == crate::i18n::Language::Chinese { "✓" } else { "" });
        let mut save = self.view.button(cx, ids!(header.save));
        save.set_visible(cx, self.page == Page::Name);
        let enabled = !self.saving_name;
        script_apply_eval!(cx, save, {enabled: #(enabled)});
        save.set_text(cx, if self.saving_name { crate::i18n::tr("Saving…") } else { crate::i18n::tr("Save") });
        self.view.text_input(cx, ids!(name_input)).set_is_read_only(cx, self.saving_name);
        if let Some(profile) = &self.profile {
            self.view.label(cx, ids!(name_row.value)).set_text(cx, profile.displayable_name());
            for path in [ids!(id_row.value), ids!(account_id.value)] { self.view.label(cx, path).set_text(cx, profile.user_id.as_str()); }
            self.view.label(cx, ids!(server_row.value)).set_text(cx, profile.user_id.server_name().as_str());
        }
        self.show_avatar(cx, ids!(photo_thumbnail)); self.show_avatar(cx, ids!(photo_large));
        self.view.widget(cx, ids!(remove_photo)).set_visible(cx, self.profile.as_ref().is_some_and(|p| p.avatar_state.has_avatar()));
        self.view.label(cx, ids!(change_photo.title)).set_text(cx, if self.saving_photo { crate::i18n::tr("Updating…") } else { crate::i18n::tr("Change Photo") });
        if let Some(client) = get_client() {
            self.view.label(cx, ids!(device_row.value)).set_text(cx, client.device_id().map(|id| id.as_str()).unwrap_or(""));
            self.view.label(cx, ids!(verify_row.value)).set_text(cx, if client.encryption().verification_state().get() == VerificationState::Verified { crate::i18n::tr("Verified") } else { crate::i18n::tr("Verify") });
        }
        let prefs = cx.global::<AppPreferencesGlobal>().0.clone();
        self.view.label(cx, ids!(zoom_row.value)).set_text(cx, &prefs.ui_zoom.format_percent());
        self.view.label(cx, ids!(images_row.value)).set_text(cx, match prefs.thumbnail_max_height {ThumbnailMaxHeight::Small => crate::i18n::tr("Small"), ThumbnailMaxHeight::Medium => crate::i18n::tr("Medium"), ThumbnailMaxHeight::Large => crate::i18n::tr("Large"), _ => crate::i18n::tr("Custom")});
        for (path, active) in [
            (ids!(enter_row.toggle), prefs.send_on_enter),
            (ids!(public_receipts.toggle), prefs.read_receipts_privacy == ReadReceiptsPrivacy::Everyone),
            (ids!(automatic_read.toggle), prefs.mark_as_read_behavior == MarkAsReadBehavior::WhenViewingMessages),
            (ids!(show_receipts.toggle), prefs.show_read_receipts),
        ] { self.view.check_box(cx, path).set_active(cx, active, Animate::No); }
        self.view.label(cx, ids!(version)).set_text(cx, concat!("Version ", env!("CARGO_PKG_VERSION")));
        self.blocked_users = get_blocked_users();
        let blocked = &self.blocked_users;
        self.view.label(cx, ids!(blocked_row.value)).set_text(cx, &blocked.len().to_string());
        self.view.widget(cx, ids!(blocked_empty)).set_visible(cx, blocked.is_empty());
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.borrow_mut::<PortalList>() {
                list.set_item_range(cx, 0, blocked.len());
                while let Some(index) = list.next_visible_item(cx) {
                    if let Some(user) = blocked.get(index) {
                        let item = list.item(cx, index, id!(User));
                        item.label(cx, ids!(title)).set_text(cx, user.as_str());
                        item.draw_all(cx, scope);
                    }
                }
            }
        }
        DrawStep::done()
    }
}
impl MobileSettingsRef {
    pub fn populate(&self, cx: &mut Cx, profile: Option<UserProfile>) {
        if let Some(mut inner) = self.borrow_mut() {
            let profile = profile.or_else(|| get_own_profile(cx));
            if inner.profile.as_ref().map(|p| &p.user_id) != profile.as_ref().map(|p| &p.user_id) {
                inner.account_url = AccountManagementUrl::Unknown;
                inner.saving_name = false; inner.saving_photo = false;
            }
            inner.profile = profile; inner.history.clear(); inner.page = Page::Settings; inner.redraw(cx);
        }
    }
    pub fn open_personal_info(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() { inner.page = Page::Personal; inner.history.clear(); inner.redraw(cx); }
    }
}
