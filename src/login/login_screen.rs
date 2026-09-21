use makepad_widgets::*;

use crate::sliding_sync::{submit_async_request, LoginByPassword, LoginRequest, MatrixRequest};

use super::homeserver::{login_server, password_identifier, LoginMethods};
use super::login_status_modal::{LoginStatusModalAction, LoginStatusModalWidgetExt};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.IMG_APP_LOGO = crate_resource("self://resources/robrix_logo_alpha.png")
    mod.widgets.ICON_EYE_OPEN   = crate_resource("self://resources/icons/eye_open.svg")
    mod.widgets.ICON_EYE_CLOSED = crate_resource("self://resources/icons/eye_closed.svg")

    let ProviderButton = RobrixNeutralIconButton {
        width: Fill, height: Fill
        margin: 0
        padding: Inset{left: 42, right: 10, top: 10, bottom: 10}
        align: Align{x: 0.5, y: 0.5}
        draw_bg +: {
            color: #fff
            color_hover: #xf5f5f5
            color_down: #xe5e5e5
            border_size: 1
            border_color: #xc8c8c8
            border_color_hover: #x999999
            border_color_down: #x777777
        }
    }

    mod.widgets.LoginScreen = set_type_default() do #(LoginScreen::register_widget(vm)) {
        ..mod.widgets.SolidView

        width: Fill, height: Fill,
        align: Align{x: 0.5, y: 0.5}
        show_bg: true,
        draw_bg +: {
            color: COLOR_SECONDARY
        }

        ScrollYView {
            width: Fill, height: Fill,
            flow: Down, // Required for vertical scrolling to work.
            align: Align{x: 0.5, y: 0.5}
            show_bg: true,
            draw_bg.color: (COLOR_SECONDARY)

            // allow the view to be scrollable but hide the actual scroll bar
            scroll_bars: {
                show_scroll_x: false, show_scroll_y: true,
                scroll_bar_y: {
                    bar_size: 0.0
                    min_handle_size: 0.0
                    drag_scrolling: true
                }
            }

            RoundedView {
                margin: Inset{top: 50, bottom: 50}
                width: Fill
                height: Fit
                align: Align{x: 0.5, y: 0.5}
                flow: Overlay,

                show_bg: true,
                draw_bg +: {
                    color: (COLOR_SECONDARY)
                    border_radius: 6.0
                }

                View {
                    width: Fill
                    height: Fit
                    flow: Down
                    align: Align{x: 0.5, y: 0.5}
                    spacing: 15.0

                    logo_image := Image {
                        fit: ImageFit.Smallest,
                        width: 80
                        src: (mod.widgets.IMG_APP_LOGO),
                    }

                    title := Label {
                        width: Fit, height: Fit
                        margin: Inset{ bottom: 5 }
                        padding: 0,
                        draw_text +: {
                            color: (COLOR_TEXT)
                            text_style: TITLE_TEXT {font_size: 16.0}
                        }
                        text: #(crate::i18n::tr("Sign in to Robrix")) i18n_text: "Sign in to Robrix"
                    }

                    View {
                        width: Fit height: Fit flow: Right spacing: 12
                        login_language_en := ButtonFlat {text: "English"}
                        login_language_zh := ButtonFlat {text: "简体中文"}
                    }

                    user_id_input := RobrixTextInput {
                        width: 275, height: Fit
                        flow: Flow.Right { wrap: false },
                        padding: 10,
                        empty_text: #(crate::i18n::tr("@name:matrix.org or username")) i18n_empty_text: "@name:matrix.org or username"
                        autocapitalize: None,
                        autocorrect: Disabled,
                        content_type: Username,
                    }

                    View {
                        width: 275, height: Fit
                        flow: Overlay
                        align: Align{x: 1.0, y: 0.5}

                        password_input := RobrixTextInput {
                            width: Fill, height: Fit
                            flow: Flow.Right { wrap: false },
                            padding: Inset{top: 10, bottom: 10, left: 10, right: 38}
                            empty_text: #(crate::i18n::tr("Password")) i18n_empty_text: "Password"
                            is_password: true,
                            autocapitalize: None,
                            autocorrect: Disabled,
                            content_type: Password,
                        }

                        View {
                            width: 38, height: Fill
                            align: Align{x: 0.5, y: 0.5}

                            show_password_button := RobrixNeutralIconButton {
                                width: Fit, height: Fit,
                                align: Align{x: 0.5, y: 0.5}
                                padding: 5
                                spacing: 0
                                margin: 0
                                draw_bg +: {
                                    color: (COLOR_SECONDARY * 1.05)
                                }
                                draw_icon +: {
                                    svg: (mod.widgets.ICON_EYE_CLOSED),
                                    color: #8C8C8C,
                                }
                                icon_walk: Walk{width: 18, height: 18, margin: 0}
                                text: ""
                            }

                            hide_password_button := RobrixNeutralIconButton {
                                visible: false,
                                align: Align{x: 0.5, y: 0.5}
                                width: Fit, height: Fit,
                                padding: 5
                                spacing: 0
                                margin: 0
                                draw_bg +: {
                                    color: (COLOR_SECONDARY * 1.05)
                                }
                                draw_icon +: {
                                    svg: (mod.widgets.ICON_EYE_OPEN),
                                    color: #8C8C8C,
                                }
                                icon_walk: Walk{width: 18, height: 18, margin: 0}
                                text: ""
                            }
                        }
                    }

                    View {
                        width: 275, height: Fit,
                        flow: Down,

                        homeserver_input := RobrixTextInput {
                            width: 275, height: Fit,
                            flow: Flow.Right { wrap: false },
                            padding: Inset{top: 5, bottom: 5, left: 10, right: 10}
                            empty_text: #(crate::i18n::tr("Auto from Matrix ID (matrix.org)")) i18n_empty_text: "Auto from Matrix ID (matrix.org)"
                            autocapitalize: None,
                            autocorrect: Disabled,
                            content_type: Url,
                            input_mode: Url,
                            draw_text +: {
                                text_style: TITLE_TEXT {font_size: 10.0}
                            }
                        }

                        View {
                            width: 275,
                            height: Fit,
                            flow: Right,
                            padding: Inset{top: 3, left: 2, right: 2}
                            spacing: 0.0,
                            align: Align{x: 0.5, y: 0.5} // center horizontally and vertically

                            LineH { draw_bg.color: #C8C8C8 }

                            Label {
                                width: Fit, height: Fit
                                padding: 0
                                draw_text +: {
                                    color: #8C8C8C
                                    text_style: REGULAR_TEXT {font_size: 9}
                                }
                                text: #(crate::i18n::tr("Your homeserver (name or URL)")) i18n_text: "Your homeserver (name or URL)"
                            }

                            LineH { draw_bg.color: #C8C8C8 }
                        }
                    }
                    

                    login_button := RobrixIconButton {
                        width: 275,
                        height: 40
                        padding: 10
                        margin: Inset{top: 5, bottom: 10}
                        align: Align{x: 0.5, y: 0.5}
                        text: #(crate::i18n::tr("Sign in with password")) i18n_text: "Sign in with password"
                    }

                    social_login_buttons := View {
                        width: 275, height: 44
                        flow: Right
                        spacing: 11
                        View {
                            width: 132, height: Fill
                            flow: Overlay
                            align: Align{y: 0.5}
                            google_login_button := ProviderButton {text: "Google"}
                            google_icon := Image {
                                width: 24, height: 24
                                margin: Inset{left: 14}
                                fit: ImageFit.Smallest
                                src: crate_resource("self://resources/img/google.png")
                            }
                        }
                        View {
                            width: 132, height: Fill
                            flow: Overlay
                            align: Align{y: 0.5}
                            github_login_button := ProviderButton {text: "GitHub"}
                            github_icon := Image {
                                width: 24, height: 24
                                margin: Inset{left: 14}
                                fit: ImageFit.Smallest
                                src: crate_resource("self://resources/img/github.png")
                            }
                        }
                    }

                    browser_login_button := RobrixIconButton {
                        width: 275, height: 40
                        padding: 10
                        align: Align{x: 0.5, y: 0.5}
                        text: #(crate::i18n::tr("Continue in browser")) i18n_text: "Continue in browser"
                    }

                    check_server_button := RobrixIconButton {
                        width: 275, height: 35
                        padding: 8
                        align: Align{x: 0.5, y: 0.5}
                        text: #(crate::i18n::tr("Check server")) i18n_text: "Check server"
                    }
                    server_status := Label {
                        width: 275, height: Fit
                        flow: Flow.Right{wrap: true}
                        draw_text +: {
                            color: COLOR_TEXT
                            text_style: REGULAR_TEXT {font_size: 10}
                        }
                        text: #(crate::i18n::tr("Choose Google, GitHub or another provider in your browser. Leave the server blank for matrix.org or discovery from your Matrix ID.")) i18n_text: "Choose Google, GitHub or another provider in your browser. Leave the server blank for matrix.org or discovery from your Matrix ID."
                    }

                    View {
                        width: 275,
                        height: Fit,
                        flow: Right,
                        // padding: 3,
                        spacing: 0.0,
                        align: Align{x: 0.5, y: 0.5} // center horizontally and vertically

                        LineH { draw_bg.color: #C8C8C8 }

                        Label {
                            width: Fit, height: Fit
                            padding: Inset{left: 1, right: 1, top: 0, bottom: 0}
                            draw_text +: {
                                color: #x6c6c6c
                                text_style: REGULAR_TEXT {}
                            }
                            text: #(crate::i18n::tr("Don't have an account?")) i18n_text: "Don't have an account?"
                        }

                        LineH { draw_bg.color: #C8C8C8 }
                    }
                    
                    signup_button := RobrixIconButton {
                        width: Fit, height: Fit
                        padding: Inset{left: 15, right: 15, top: 10, bottom: 10}
                        margin: Inset{bottom: 5}
                        align: Align{x: 0.5, y: 0.5}
                        text: #(crate::i18n::tr("Sign up here")) i18n_text: "Sign up here"
                    }
                }

                // The modal that pops up to display login status messages,
                // such as when the user is logging in or when there is an error.
                login_status_modal := Modal {
                    can_dismiss: false,
                    content := mod.widgets.LoginStatusModal {}
                }
            }
        }
    }
}

static MATRIX_SIGN_UP_URL: &str = "https://matrix.org/docs/chat_basics/matrix-for-im/#creating-a-matrix-account";

#[derive(Script, ScriptHook, Widget)]
pub struct LoginScreen {
    #[source] source: ScriptObjectRef,
    #[deref] view: View,
    /// Whether the password field is currently showing plaintext.
    #[rust] password_visible: bool,
    #[rust] sso_pending: bool,
    #[rust] login_pending: bool,
    #[rust] discovery_generation: u64,
    #[rust] discovery_pending: bool,

}


impl Widget for LoginScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        self.match_event(cx, event);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl LoginScreen {
    fn show_status(&mut self, cx: &mut Cx, title: &str, status: &str, button: &str, enabled: bool) {
        let content = self.view.login_status_modal(cx, ids!(login_status_modal.content));
        content.set_title(cx, title);
        content.set_status(cx, status);
        content.button_ref(cx).set_text(cx, button);
        content.button_ref(cx).set_enabled(cx, enabled);
        self.view.modal(cx, ids!(login_status_modal)).open(cx);
        self.redraw(cx);
    }

    fn check_server(&mut self, cx: &mut Cx, user: String, server: String) {
        self.discovery_generation += 1;
        let generation = self.discovery_generation;
        self.discovery_pending = true;
        self.view.label(cx, ids!(server_status)).set_text(cx, crate::i18n::tr("Checking homeserver and sign-in methods…"));
        crate::sliding_sync::spawn_async_task(async move {
            let result = super::homeserver::discover(&user, &server).await.map_err(|e| e.to_string());
            Cx::post_action(LoginAction::ServerDiscovered { generation, result });
        });
    }
}

impl MatchEvent for LoginScreen {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        for (path, language) in [(ids!(login_language_en), crate::i18n::Language::English), (ids!(login_language_zh), crate::i18n::Language::Chinese)] {
            if self.view.button(cx, path).clicked(actions) {
                if let Err(error) = crate::i18n::set_language(cx, language) {
                    crate::shared::popup_list::enqueue_popup_notification(crate::i18n::format("Could not save language: {error}", &[("error", error.to_string())]), crate::shared::popup_list::PopupKind::Error, Some(5.0));
                }
            }
        }
        let user_input = self.view.text_input(cx, ids!(user_id_input));
        let password_input = self.view.text_input(cx, ids!(password_input));
        let server_input = self.view.text_input(cx, ids!(homeserver_input));
        let modal = self.view.modal(cx, ids!(login_status_modal));
        let user = user_input.text().trim().to_owned();
        let server = server_input.text().trim().to_owned();

        let show = self.view.button(cx, ids!(show_password_button));
        let hide = self.view.button(cx, ids!(hide_password_button));
        if show.clicked(actions) || hide.clicked(actions) {
            self.password_visible = !self.password_visible;
            password_input.toggle_is_password(cx);
            show.set_visible(cx, !self.password_visible);
            hide.set_visible(cx, self.password_visible);
            password_input.set_key_focus(cx);
        }
        if user_input.changed(actions).is_some() || server_input.changed(actions).is_some() {
            self.discovery_generation += 1;
            self.discovery_pending = false;
            self.view.label(cx, ids!(server_status)).set_text(cx, crate::i18n::tr("Check server to see its available sign-in methods."));
        }
        if self.view.button(cx, ids!(signup_button)).clicked(actions) {
            let _ = robius_open::Uri::new(MATRIX_SIGN_UP_URL).open();
        }
        if !self.login_pending && !self.discovery_pending && (
            self.view.button(cx, ids!(check_server_button)).clicked(actions)
            || server_input.returned(actions).is_some()
        ) {
            self.check_server(cx, user.clone(), server.clone());
        }
        if !self.login_pending && (
            self.view.button(cx, ids!(login_button)).clicked(actions)
            || user_input.returned(actions).is_some()
            || password_input.returned(actions).is_some()
        ) {
            let destination = login_server(&user, Some(&server)).and_then(|server| {
                password_identifier(&user)?;
                if password_input.text().is_empty() { anyhow::bail!(crate::i18n::tr("Enter your password, or choose Continue in browser.")); }
                Ok(server)
            });
            match destination {
                Ok(destination) => {
                    self.login_pending = true;
                    self.show_status(cx, crate::i18n::tr("Signing in"), &crate::i18n::format("Connecting to {destination}…", &[("destination", (destination).to_string())]), crate::i18n::tr("Please wait…"), false);
                    submit_async_request(MatrixRequest::Login(LoginRequest::LoginByPassword(LoginByPassword {
                        user_id: user.clone(), password: password_input.text(), homeserver: Some(destination),
                    })));
                }
                Err(error) => self.show_status(cx, crate::i18n::tr("Check sign-in details"), &error.to_string(), crate::i18n::tr("Okay"), true),
            }
        }
        // Social shortcuts use the selected server's provider chooser. Provider
        // IDs are server-defined, and matrix.org now delegates this page to MAS.
        let browser_login_clicked = [ids!(browser_login_button), ids!(google_login_button), ids!(github_login_button)]
            .into_iter().any(|id| self.view.button(cx, id).clicked(actions));
        if !self.login_pending && browser_login_clicked {
            match login_server(&user, Some(&server)) {
                Ok(destination) => {
                    self.login_pending = true;
                    self.sso_pending = true;
                    self.show_status(cx, crate::i18n::tr("Connecting to your server"), &crate::i18n::format("Checking browser sign-in for {destination}…", &[("destination", (destination).to_string())]), crate::i18n::tr("Cancel"), true);
                    submit_async_request(MatrixRequest::SpawnSSOServer { homeserver_url: destination });
                }
                Err(error) => self.show_status(cx, crate::i18n::tr("Check your homeserver"), &error.to_string(), crate::i18n::tr("Okay"), true),
            }
        }

        for action in actions {
            if let LoginStatusModalAction::Close = action.as_widget_action().cast() {
                if self.sso_pending {
                    submit_async_request(MatrixRequest::CancelSsoLogin);
                    self.show_status(cx, crate::i18n::tr("Cancelling sign-in"), crate::i18n::tr("Closing the sign-in connection…"), crate::i18n::tr("Please wait…"), false);
                } else if !self.login_pending {
                    modal.close(cx);
                }
            }
            match action.downcast_ref() {
                Some(LoginAction::ServerDiscovered { generation, result }) if *generation == self.discovery_generation => {
                    self.discovery_pending = false;
                    let text = match result {
                        Ok(methods) => {
                            let mut available = Vec::new();
                            if methods.password { available.push(crate::i18n::tr("Password")); }
                            if methods.sso { available.push(crate::i18n::tr("Browser SSO")); }
                            let methods_text = if available.is_empty() {
                                crate::i18n::tr("No supported sign-in method. OAuth-only and QR sign-in are not supported yet.").to_owned()
                            } else { available.join(" · ") };
                            let providers = if methods.providers.is_empty() { String::new() } else { format!("\n{}", methods.providers.join(", ")) };
                            format!("{}\n{methods_text}{providers}", methods.homeserver)
                        }
                        Err(error) => crate::i18n::format("Could not check this server: {error}", &[("error", (error).to_string())]),
                    };
                    self.view.label(cx, ids!(server_status)).set_text(cx, &text);
                }
                Some(LoginAction::CliAutoLogin { user_id, homeserver }) => {
                    self.login_pending = true;
                    user_input.set_text(cx, user_id);
                    password_input.set_text(cx, "");
                    server_input.set_text(cx, homeserver.as_deref().unwrap_or_default());
                    self.show_status(cx, crate::i18n::tr("Signing in"), crate::i18n::tr("Connecting to your account…"), crate::i18n::tr("Please wait…"), false);
                }
                Some(LoginAction::Status { title, status }) => {
                    self.login_pending = true;
                    self.show_status(cx, title, status, if self.sso_pending { crate::i18n::tr("Cancel") } else { crate::i18n::tr("Please wait…") }, self.sso_pending);
                }
                Some(LoginAction::LoginSuccess) => {
                    self.login_pending = false;
                    self.sso_pending = false;
                    user_input.set_text(cx, "");
                    password_input.set_text(cx, "");
                    server_input.set_text(cx, "");
                    if self.password_visible {
                        self.password_visible = false;
                        password_input.toggle_is_password(cx);
                        show.set_visible(cx, true);
                        hide.set_visible(cx, false);
                    }
                    modal.close(cx);
                }
                Some(LoginAction::LoginFailure(error)) => {
                    self.login_pending = false;
                    self.sso_pending = false;
                    self.show_status(cx, crate::i18n::tr("Sign-in failed"), error, crate::i18n::tr("Okay"), true);
                }
                Some(LoginAction::SsoPending(pending)) => self.sso_pending = *pending,
                Some(LoginAction::Cancelled) => {
                    self.login_pending = false;
                    self.sso_pending = false;
                    modal.close(cx);
                }
                _ => {}
            }
        }
        self.view.button(cx, ids!(login_button)).set_enabled(cx, !self.login_pending);
        self.view.button(cx, ids!(browser_login_button)).set_enabled(cx, !self.login_pending);
        self.view.button(cx, ids!(google_login_button)).set_enabled(cx, !self.login_pending);
        self.view.button(cx, ids!(github_login_button)).set_enabled(cx, !self.login_pending);
        self.view.button(cx, ids!(check_server_button)).set_enabled(cx, !self.login_pending && !self.discovery_pending);
        self.redraw(cx);
    }
}

/// Actions sent to or from the login screen. Never include a password or token.
#[derive(Clone, Default, Debug)]
pub enum LoginAction {
    LoginSuccess,
    LoginFailure(String),
    Status { title: String, status: String },
    CliAutoLogin { user_id: String, homeserver: Option<String> },
    SsoPending(bool),
    Cancelled,
    ServerDiscovered { generation: u64, result: Result<LoginMethods, String> },
    #[default]
    None,
}
