use std::collections::BTreeMap;
use makepad_widgets::*;
use ruma::{OwnedEventId, OwnedRoomId, OwnedUserId};
use crate::{
    home::rooms_list::RoomsListRef,
    shared::navigation_bar_button::NavigationBarButtonWidgetRefExt,
    sliding_sync::{current_user_id, get_client, spawn_async_task},
    utils::RoomNameId,
};
use super::{
    document::*,
    model::{Grant, ArticlePackage},
    storage::{self, Library, Publication, Operation, OperationKind},
    backend::{self, ArticleContent},
    rich_input::ArticleRichInputWidgetRefExt,
};

#[derive(Clone, Debug)]
pub enum ArticleAction {
    Open,
    Close,
    Read {
        room: OwnedRoomId,
        event: OwnedEventId,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Page {
    #[default]
    Details,
    Consent,
    Library,
    Edit,
    Source,
    Images,
    ImageSettings,
    Link,
    Theme,
    Cover,
    Preview,
    Review,
    Rooms,
    Confirm,
    Publication,
    Withdraw,
    Reader,
}
#[derive(Clone, Debug)]
enum ResultAction {
    Image {
        instance: String,
        document: String,
        cover: bool,
        result: Result<storage::Asset, String>,
    },
    Published {
        instance: String,
        result: Result<Publication, String>,
    },
    Shared {
        instance: String,
        result: Result<(), String>,
    },
    Read {
        instance: String,
        result: Result<ArticleContent, String>,
    },
    Media {
        instance: String,
        id: String,
        result: Result<Vec<u8>, String>,
    },
}
const TOP: f64 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };
fn tr(s: &str) -> &str {
    crate::i18n::tr(s)
}
fn color(hex: u32) -> Vec4f {
    vec4(
        ((hex >> 16) & 255) as f32 / 255.0,
        ((hex >> 8) & 255) as f32 / 255.0,
        (hex & 255) as f32 / 255.0,
        1.0,
    )
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.ArticleLabel = Label {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text +: {color: #x191919 text_style: theme.font_regular{font_size: 13}}}
    mod.widgets.ArticleButton = RobrixNeutralIconButton {grab_key_focus: false height: 42 padding: Inset{left: 12 right: 12} draw_text +: {color: #x333333 text_style: theme.font_regular{font_size: 12}} draw_bg +: {color: #xffffff color_hover: #xf2f5f3 color_down: #xe7f2e9 border_radius: 6}}
    mod.widgets.ArticlePrimary = mod.widgets.ArticleButton {width: Fill height: 46 align: Align{x: 0.5 y: 0.5} draw_bg +: {color: #x07c160 color_hover: #x06ad56 color_down: #x05984b} draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff}}
    mod.widgets.ArticleInput = TextInput {width: Fill height: 44 draw_bg +: {color: #xffffff color_hover: #xffffff color_focus: #xffffff color_empty: #xffffff border_color: #xe5e5e5 border_color_focus: #x07c160} draw_text +: {color: #x191919 color_focus: #x191919 color_hover: #x191919 text_style: theme.font_regular{font_size: 14}}}
    mod.widgets.ArticleHtml = Html {width: Fill height: Fit padding: 0 font_size: 14 font_color: #x191919 draw_text.color: #x191919 text_style_normal: theme.font_regular{font_size: 14} text_style_bold: theme.font_bold{font_size: 14} text_style_italic: theme.font_italic{font_size: 14} text_style_bold_italic: theme.font_bold_italic{font_size: 14}}
    mod.widgets.ArticleRich = ArticleRichInput {width: Fill height: Fit is_multiline: true flow: Flow.Right{wrap: true} padding: Inset{top: 4 bottom: 12 left: 0 right: 0} empty_text: #(crate::i18n::tr("Write your article…")) draw_bg +: {color: #x00000000 color_hover: #x00000000 color_focus: #x00000000 color_down: #x00000000 color_empty: #x00000000 border_size: 0} draw_text +: {color: #x191919 color_hover: #x191919 color_focus: #x191919 text_style: theme.font_regular{font_size: 14}} draw_bold.text_style: theme.font_bold{font_size: 14} draw_italic.text_style: theme.font_italic{font_size: 14} draw_bold_italic.text_style: theme.font_bold_italic{font_size: 14}}
    mod.widgets.ArticleThemeTile = NavigationBarButton {width: Fill height: 210 flow: Down align: Align{x: 0.0 y: 0.0} padding: 12 spacing: 8
        draw_bg +: {color: instance(#xffffff) border_size: 1.0 border_color: #xe0e0e0 get_color: fn() {return self.color}}
        name := mod.widgets.ArticleLabel {padding: 0 max_lines: 1 draw_text.text_style: theme.font_bold{font_size: 11}}
        title := mod.widgets.ArticleLabel {padding: 0 max_lines: 2 draw_text.text_style: theme.font_bold{font_size: 11}}
        sample := mod.widgets.ArticleLabel {padding: 0 max_lines: 3 draw_text.text_style: theme.font_regular{font_size: 9}}
        picture := Image {width: Fill height: 65 fit: ImageFit.Biggest}
    }
    mod.widgets.ArticlePanel = #(ArticlePanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down draw_bg.color: #xffffff
        padding: Inset{top: mod.widgets.SAFE_INSET_PAD_TOP + #(TOP) bottom: mod.widgets.SAFE_INSET_PAD_BOTTOM}
        header := SolidView {width: Fill height: 52 flow: Right align: Align{y: 0.5} padding: Inset{left: 8 right: 12} spacing: 8 draw_bg.color: #xededed
            article_back := RobrixNeutralIconButton {width: 36 height: 44 padding: 12 spacing: 0 draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000 border_size: 0} draw_icon +: {svg: ICON_CHEVRON_LEFT color: #x191919} icon_walk: Walk{width: 8 height: 14}}
            article_heading := mod.widgets.ArticleLabel {width: Fill max_lines: 1 draw_text.text_style: theme.font_bold{font_size: 16}}
            article_save := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Save")) i18n_text: "Save" visible: false}
            article_done := mod.widgets.ArticleButton {visible: false text: #(crate::i18n::tr("Done")) i18n_text: "Done" draw_text.color: #x07a858}
            article_close := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Close")) i18n_text: "Close"}
        }
        details := View {width: Fill height: Fill flow: Down padding: 28 spacing: 24
            View {width: Fill height: 35}
            Label {text: "Aa" draw_text +: {color: #x07c160 text_style: theme.font_bold{font_size: 56}}}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Article studio")) i18n_text: "Article studio" draw_text.text_style: theme.font_bold{font_size: 24}}
            mod.widgets.ArticleLabel {text: "OctoSense · 2.0" draw_text.color: #x777777}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Write visually. Add images, choose a theme, review and publish.")) i18n_text: "Write visually. Add images, choose a theme, review and publish."}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Sharing this app shares neither your drafts nor your account permissions.")) i18n_text: "Sharing this app shares neither your drafts nor your account permissions." draw_text.color: #x777777}
            View {width: Fill height: Fill}
            article_continue := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Continue with Robrix")) i18n_text: "Continue with Robrix"}
        }
        consent := View {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 24
            consent_account := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 16}}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("This app requests permission to:")) i18n_text: "This app requests permission to:"}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("• Save your drafts and selected images on this device\n\n• Upload images and publish only after confirmation\n\n• Update or withdraw your own articles after confirmation")) i18n_text: "• Save your drafts and selected images on this device\n\n• Upload images and publish only after confirmation\n\n• Update or withdraw your own articles after confirmation"}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Your password and session token stay with Robrix. Permission expires when you close this app or after one hour.")) i18n_text: "Your password and session token stay with Robrix. Permission expires when you close this app or after one hour." draw_text.color: #x777777}
            View {width: Fill height: Fill}
            article_allow := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Allow and open")) i18n_text: "Allow and open"}
            article_cancel := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Cancel")) i18n_text: "Cancel"}
        }
        library := View {visible: false width: Fill height: Fill flow: Down padding: 16 spacing: 12
            article_search := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("Search articles")) i18n_empty_text: "Search articles"}
            View {width: Fill height: 42 flow: Right spacing: 6
                drafts_tab := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Drafts")) i18n_text: "Drafts"}
                published_tab := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Published")) i18n_text: "Published"}
                withdrawn_tab := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Withdrawn")) i18n_text: "Withdrawn"}
            }
            library_empty := mod.widgets.ArticleLabel {text: #(crate::i18n::tr("No articles here yet. Create your first article.")) i18n_text: "No articles here yet. Create your first article." draw_text.color: #x777777}
            article_library := PortalList {width: Fill height: Fill
                Entry := NavigationBarButton {width: Fill height: 112 flow: Right spacing: 12 padding: 12 draw_bg +: {color_hover: #xf2f5f3 color_active: #xe7f5ec}
                    thumbnail := Image {width: 78 height: 78 fit: ImageFit.Biggest}
                    View {width: Fill height: Fit flow: Down spacing: 8
                        title := mod.widgets.ArticleLabel {max_lines: 2 draw_text.text_style: theme.font_bold{font_size: 14}}
                        summary := mod.widgets.ArticleLabel {max_lines: 2 draw_text +: {color: #x777777 text_style: theme.font_regular{font_size: 11}}}
                    }
                }
            }
            article_new := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("New article")) i18n_text: "New article"}
            article_share := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Share app")) i18n_text: "Share app"}
        }
        editor := View {visible: false width: Fill height: Fill flow: Right align: Align{x: 0.5}
            editor_sidebar := View {visible: false width: 230 height: Fill flow: Down padding: 18 spacing: 18
                sidebar_new := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("New article")) i18n_text: "New article"}
                sidebar_library := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("My articles")) i18n_text: "My articles"}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Article outline")) i18n_text: "Article outline" draw_text.text_style: theme.font_bold{font_size: 14}}
                outline := mod.widgets.ArticleLabel {draw_text.color: #x777777}
                View {width: Fill height: Fill}
                sidebar_share := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Share app")) i18n_text: "Share app"}
            }
            editor_paper := SolidView {width: Fill height: Fill flow: Down padding: Inset{left: 20 right: 20 top: 16 bottom: 12} spacing: 8 draw_bg.color: #xffffff
                article_title := mod.widgets.ArticleInput {height: 52 empty_text: #(crate::i18n::tr("Article title")) i18n_empty_text: "Article title" draw_text.text_style: theme.font_bold{font_size: 18} draw_bg +: {border_size: 0 color: #x00000000 color_empty: #x00000000 color_hover: #x00000000 color_focus: #x00000000 color_down: #x00000000}}
                article_author := mod.widgets.ArticleInput {height: 34 empty_text: #(crate::i18n::tr("Author")) i18n_empty_text: "Author" draw_bg +: {border_size: 0 color: #x00000000 color_empty: #x00000000 color_hover: #x00000000 color_focus: #x00000000 color_down: #x00000000} draw_text +: {color: #x777777 text_style: theme.font_regular{font_size: 12}}}
                format_bar := ScrollXView {width: Fill height: 42 flow: Right spacing: 4
                    article_bold := mod.widgets.ArticleButton {text: "B" width: 38 draw_text.text_style: theme.font_bold{font_size: 14}}
                    article_italic := mod.widgets.ArticleButton {text: "I" width: 38 draw_text.text_style: theme.font_italic{font_size: 14}}
                    article_link := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Link")) i18n_text: "Link"}
                    article_h2 := mod.widgets.ArticleButton {text: "H2" width: 42}
                    article_quote := mod.widgets.ArticleButton {text: "❝" width: 38}
                    article_list := mod.widgets.ArticleButton {text: "•" width: 38}
                    article_undo := mod.widgets.ArticleButton {text: "↶" width: 38}
                    article_redo := mod.widgets.ArticleButton {text: "↷" width: 38}
                    block_up := mod.widgets.ArticleButton {text: "↑" width: 38}
                    block_down := mod.widgets.ArticleButton {text: "↓" width: 38}
                    block_remove := mod.widgets.ArticleButton {text: "−" width: 38}
                }
                article_blocks := PortalList {width: Fill height: Fill
                    TextBlock := View {width: Fill height: Fit flow: Right spacing: 8
                        prefix := Label {visible: false width: 16 height: Fit padding: Inset{top: 5} draw_text +: {color: #x07a858 text_style: theme.font_regular{font_size: 14}}}
                        rich := mod.widgets.ArticleRich {}
                    }
                    Picture := View {width: Fill height: Fit flow: Down spacing: 6 padding: Inset{top: 8 bottom: 12}
                        picture := Image {width: Fill height: 210 fit: ImageFit.Smallest}
                        caption := mod.widgets.ArticleLabel {draw_text +: {color: #x777777 text_style: theme.font_regular{font_size: 11}}}
                        image_settings := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Image settings")) i18n_text: "Image settings"}
                    }
                    Rule := View {width: Fill height: 30 align: Align{y: 0.5} SolidView {width: Fill height: 1 draw_bg.color: #xdddddd}}
                }
                article_stats := mod.widgets.ArticleLabel {draw_text +: {color: #x888888 text_style: theme.font_regular{font_size: 10}}}
                ScrollXView {width: Fill height: 44 flow: Right spacing: 6
                    article_add_text := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Text")) i18n_text: "Text"}
                    article_images := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Images")) i18n_text: "Images"}
                    article_theme := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Style")) i18n_text: "Style"}
                    article_cover := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Cover")) i18n_text: "Cover"}
                    article_source := mod.widgets.ArticleButton {text: "Markdown"}
                }
                article_preview := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Full preview")) i18n_text: "Full preview"}
            }
            editor_inspector := View {visible: false width: 260 height: Fill flow: Down padding: 18 spacing: 20
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Article style")) i18n_text: "Article style" draw_text +: {color: #x07a858 text_style: theme.font_bold{font_size: 16}}}
                inspector_theme := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Choose theme")) i18n_text: "Choose theme"}
                inspector_html := mod.widgets.ArticleHtml {}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Cover preview")) i18n_text: "Cover preview"}
                inspector_cover := Image {width: Fill height: 120 fit: ImageFit.Biggest}
                inspector_cover_button := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Cover and summary")) i18n_text: "Cover and summary"}
                View {width: Fill height: Fill}
                inspector_preview := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Full preview")) i18n_text: "Full preview"}
            }
        }
        source := View {visible: false width: Fill height: Fill flow: Down padding: 18 spacing: 12
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Markdown source · your theme and cover are preserved")) i18n_text: "Markdown source · your theme and cover are preserved"}
            article_markdown := mod.widgets.ArticleInput {height: Fill is_multiline: true flow: Flow.Right{wrap: true}}
            source_apply := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Apply and return to visual editing")) i18n_text: "Apply and return to visual editing"}
        }
        images := View {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 14
            image_pick := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Choose from device")) i18n_text: "Choose from device"}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Selected images stay on this device until you confirm publication.")) i18n_text: "Selected images stay on this device until you confirm publication." draw_text.color: #x777777}
            image_library := PortalList {width: Fill height: Fill
                Asset := NavigationBarButton {width: Fill height: 130 flow: Right spacing: 14 padding: 10
                    picture := Image {width: 140 height: 108 fit: ImageFit.Smallest}
                    name := mod.widgets.ArticleLabel {width: Fill max_lines: 3}
                }
            }
        }
        link_page := View {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 20
            article_link_url := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("HTTPS link")) i18n_empty_text: "HTTPS link"}
            link_apply := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Apply link")) i18n_text: "Apply link"}
            link_remove := mod.widgets.ArticleButton {text: #(crate::i18n::tr("Remove link")) i18n_text: "Remove link"}
        }
        image_settings := View {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 12
            selected_image := Image {width: Fill height: 190 fit: ImageFit.Smallest}
            image_caption := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("Caption")) i18n_empty_text: "Caption"}
            image_alt := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("Alternative text")) i18n_empty_text: "Alternative text"}
            View {width: Fill height: 44 flow: Right spacing: 8
                image_full := mod.widgets.ArticleButton {width: Fill text: "100%"}
                image_medium := mod.widgets.ArticleButton {width: Fill text: "75%"}
                image_small := mod.widgets.ArticleButton {width: Fill text: "50%"}
            }
            image_replace := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Replace image")) i18n_text: "Replace image"}
            image_remove := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Remove image")) i18n_text: "Remove image" draw_text.color: #xe34d4d}
            View {width: Fill height: Fill}
            image_done := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Done")) i18n_text: "Done"}
        }
        themes := View {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 12
            theme_list := PortalList {width: Fill height: Fill
                Theme := View {width: Fill height: 222 flow: Right spacing: 10 padding: Inset{bottom: 12}
                    left := mod.widgets.ArticleThemeTile {}
                    right := mod.widgets.ArticleThemeTile {}
                }
            }
            View {width: Fill height: 44 flow: Right spacing: 8
                size_normal := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Standard type")) i18n_text: "Standard type"}
                size_large := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Large type")) i18n_text: "Large type"}
            }
            View {width: Fill height: 44 flow: Right spacing: 8
                spacing_comfortable := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Comfortable")) i18n_text: "Comfortable"}
                spacing_compact := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Compact")) i18n_text: "Compact"}
            }
            theme_done := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Done")) i18n_text: "Done"}
        }
        cover := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 12
            cover_wide := Image {width: Fill height: 160 fit: ImageFit.Biggest}
            cover_square := Image {width: 100 height: 100 fit: ImageFit.Biggest}
            cover_pick := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Choose cover image")) i18n_text: "Choose cover image"}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Cover focal point · horizontal / vertical")) i18n_text: "Cover focal point · horizontal / vertical"}
            cover_x := Slider {width: Fill height: 30 min: 0 max: 1000 step: 1}
            cover_y := Slider {width: Fill height: 30 min: 0 max: 1000 step: 1}
            cover_summary := mod.widgets.ArticleInput {height: 85 is_multiline: true flow: Flow.Right{wrap: true} empty_text: #(crate::i18n::tr("Summary")) i18n_empty_text: "Summary"}
            cover_show := CheckBox {text: #(crate::i18n::tr("Show cover at the top of the article"))}
            cover_remove := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Remove cover")) i18n_text: "Remove cover"}
            cover_done := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Done")) i18n_text: "Done"}
        }
        preview := SolidView {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 12 draw_bg.color: #xffffff
            preview_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 18}}
            preview_author := mod.widgets.ArticleLabel {draw_text.color: #x777777}
            article_reader := PortalList {width: Fill height: Fill
                Text := View {width: Fill height: Fit flow: Down padding: Inset{bottom: 12} body := mod.widgets.ArticleHtml {}}
                Image := View {width: Fill height: Fit flow: Down spacing: 6 padding: Inset{bottom: 16}
                    picture := Image {width: Fill height: 220 fit: ImageFit.Smallest}
                    caption := mod.widgets.ArticleLabel {draw_text +: {color: #x777777 text_style: theme.font_regular{font_size: 11}}}
                }
            }
            preview_stats := mod.widgets.ArticleLabel {draw_text +: {color: #x888888 text_style: theme.font_regular{font_size: 11}}}
            preview_check := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Publication review")) i18n_text: "Publication review"}
        }
        review := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 20
            review_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 22}}
            review_checks := mod.widgets.ArticleLabel {}
            review_destination := mod.widgets.ArticleLabel {draw_text.color: #x576b95}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Images are uploaded only after your final confirmation.")) i18n_text: "Images are uploaded only after your final confirmation." draw_text.color: #x777777}
            View {width: Fill height: 12}
            review_continue := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Continue")) i18n_text: "Continue"}
        }
        rooms := View {visible: false width: Fill height: Fill flow: Down padding: 18 spacing: 12
            article_chat_search := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("Find a chat")) i18n_empty_text: "Find a chat"}
            article_rooms := PortalList {width: Fill height: Fill
                Chat := NavigationBarButton {width: Fill height: 64 padding: 12 name := mod.widgets.ArticleLabel {max_lines: 2}}
            }
        }
        confirm := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 20
            confirm_cover := Image {width: Fill height: 170 fit: ImageFit.Biggest}
            confirm_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 18}}
            confirm_summary := mod.widgets.ArticleLabel {draw_text.color: #x777777}
            publish_account := mod.widgets.ArticleLabel {}
            publish_room := mod.widgets.ArticleLabel {draw_text +: {color: #x576b95 text_style: theme.font_bold{font_size: 15}}}
            confirm_details := mod.widgets.ArticleLabel {}
            View {width: Fill height: 12}
            article_confirm := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Confirm publish")) i18n_text: "Confirm publish"}
            article_change := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Back to editing")) i18n_text: "Back to editing"}
        }
        publication := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 18
            publication_state := mod.widgets.ArticleLabel {draw_text +: {color: #x07a858 text_style: theme.font_bold{font_size: 24}}}
            publication_cover := Image {width: Fill height: 170 fit: ImageFit.Biggest}
            publication_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 18}}
            publication_info := mod.widgets.ArticleLabel {}
            publication_read := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Read full article")) i18n_text: "Read full article"}
            publication_edit := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Edit article")) i18n_text: "Edit article"}
            publication_withdraw := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Withdraw article")) i18n_text: "Withdraw article" draw_text.color: #xe34d4d}
            View {width: Fill height: 12}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Your local draft is preserved.")) i18n_text: "Your local draft is preserved." draw_text.color: #x777777}
            publication_library := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("My articles")) i18n_text: "My articles"}
        }
        withdraw := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 24
            withdraw_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: 22}}
            withdraw_room := mod.widgets.ArticleLabel {}
            View {width: Fill height: 12}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Withdraw this article?")) i18n_text: "Withdraw this article?" draw_text.text_style: theme.font_bold{font_size: 20}}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Your local draft will remain. Copies already downloaded or forwarded cannot be recalled.")) i18n_text: "Your local draft will remain. Copies already downloaded or forwarded cannot be recalled."}
            View {width: Fill height: 12}
            withdraw_confirm := mod.widgets.ArticlePrimary {text: #(crate::i18n::tr("Confirm withdrawal")) i18n_text: "Confirm withdrawal" draw_bg +: {color: #xe34d4d color_hover: #xd54444 color_down: #xc33b3b}}
            withdraw_cancel := mod.widgets.ArticleButton {width: Fill text: #(crate::i18n::tr("Cancel")) i18n_text: "Cancel"}
        }
        article_status := mod.widgets.ArticleLabel {padding: Inset{left: 18 right: 18 top: 4 bottom: 8} draw_text +: {color: #x777777 text_style: theme.font_regular{font_size: 11}}}
    }
}

type ReaderAssets = BTreeMap<String, Vec<u8>>;
type ImageBindings = std::cell::RefCell<BTreeMap<WidgetUid, String>>;

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
    page: Page,
    #[rust]
    doc: Document,
    #[rust]
    library: Library,
    #[rust]
    library_tab: usize,
    #[rust]
    entries: Vec<String>,
    #[rust]
    active_block: usize,
    #[rust]
    undo: Vec<Document>,
    #[rust]
    redo: Vec<Document>,
    #[rust]
    pending: bool,
    #[rust]
    dirty: bool,
    #[rust]
    save_timer: Timer,
    #[rust]
    rooms: Vec<RoomNameId>,
    #[rust]
    sharing: bool,
    #[rust]
    share_room: Option<RoomNameId>,
    #[rust]
    share_transaction: String,
    #[rust]
    selected_publication: Option<Publication>,
    #[rust]
    operation: Option<Operation>,
    #[rust]
    selecting_cover: bool,
    #[rust]
    replacing_image: bool,
    #[rust]
    reader_only: bool,
    #[rust]
    reader_assets: ReaderAssets,
    #[rust]
    image_bindings: ImageBindings,
    #[rust]
    remote_article: Option<ArticleContent>,
}
impl ArticlePanel {
    fn status(&self, cx: &mut Cx, s: &str) {
        self.label(cx, ids!(article_status)).set_text(cx, tr(s));
    }
    fn allowed(&self) -> bool {
        self.grant
            .as_ref()
            .is_some_and(|g| g.valid(current_user_id().as_deref()))
            && !crate::logout::logout_state_machine::is_logout_in_progress()
    }
    fn editable(&self) -> bool {
        self.allowed() && !self.reader_only
    }
    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    fn changed(&mut self, cx: &mut Cx) {
        self.doc.modified = now();
        self.dirty = true;
        self.operation = None;
        cx.stop_timer(self.save_timer);
        self.save_timer = cx.start_timeout(0.7);
        self.status(cx, "Unsaved changes");
        self.refresh_document(cx);
    }
    fn save(&mut self, cx: &mut Cx) -> bool {
        if !self.editable() {
            return false;
        }
        let Some(g) = &self.grant else { return false };
        match storage::save_document(crate::app_data_dir(), g, &self.doc) {
            Ok(()) => {
                self.dirty = false;
                self.status(cx, "Draft saved on this device");
                true
            }
            Err(e) => {
                self.status(cx, &e);
                false
            }
        }
    }
    fn load_library(&mut self, cx: &mut Cx) {
        if let Some(g) = &self.grant {
            match storage::load(crate::app_data_dir(), g) {
                Ok(lib) => self.library = lib,
                Err(e) => self.status(cx, &e),
            }
        }
        self.filter_library(cx);
    }
    fn filter_library(&mut self, cx: &mut Cx) {
        let query = self
            .text_input(cx, ids!(article_search))
            .text()
            .to_lowercase();
        self.entries = if self.library_tab == 0 {
            self.library
                .documents
                .iter()
                .filter(|d| {
                    d.title.to_lowercase().contains(&query)
                        || d.summary.to_lowercase().contains(&query)
                })
                .map(|d| d.id.clone())
                .collect()
        } else {
            self.library
                .publications
                .iter()
                .filter(|p| {
                    p.withdrawn == (self.library_tab == 2)
                        && p.document.title.to_lowercase().contains(&query)
                })
                .map(|p| p.id.clone())
                .collect()
        };
        self.label(cx, ids!(library_empty))
            .set_visible(cx, self.entries.is_empty());
        for (id, tab) in [
            (id!(drafts_tab), 0),
            (id!(published_tab), 1),
            (id!(withdrawn_tab), 2),
        ] {
            let mut button = self.button(cx, &[id]);
            let ink = color(if tab == self.library_tab {
                0x07a858
            } else {
                0x777777
            });
            script_apply_eval!(cx, button, {draw_text +: {color: #(ink) color_hover: #(ink) color_down: #(ink)}});
        }
        self.portal_list(cx, ids!(article_library))
            .set_first_id_and_scroll(0, 0.0);
        self.view.redraw(cx);
    }
    fn show(&mut self, cx: &mut Cx, page: Page) {
        self.page = page;
        self.button(cx, ids!(article_done)).set_visible(
            cx,
            matches!(page, Page::Theme | Page::Cover | Page::ImageSettings),
        );
        for (id, p) in [
            (id!(details), Page::Details),
            (id!(consent), Page::Consent),
            (id!(library), Page::Library),
            (id!(editor), Page::Edit),
            (id!(source), Page::Source),
            (id!(images), Page::Images),
            (id!(image_settings), Page::ImageSettings),
            (id!(link_page), Page::Link),
            (id!(themes), Page::Theme),
            (id!(cover), Page::Cover),
            (id!(preview), Page::Preview),
            (id!(review), Page::Review),
            (id!(rooms), Page::Rooms),
            (id!(confirm), Page::Confirm),
            (id!(publication), Page::Publication),
            (id!(withdraw), Page::Withdraw),
        ] {
            self.view(cx, &[id]).set_visible(
                cx,
                p == page || (p == Page::Preview && page == Page::Reader),
            );
        }
        let heading = match page {
            Page::Details => "App details",
            Page::Consent => "Authorize app",
            Page::Library => "Article studio",
            Page::Edit => "Edit article",
            Page::Source => "Markdown source",
            Page::Images => "Insert images",
            Page::ImageSettings => "Image settings",
            Page::Link => "Link",
            Page::Theme => "Article style",
            Page::Cover => "Cover and summary",
            Page::Preview => "Full preview",
            Page::Review => "Publication review",
            Page::Rooms => "Choose chat",
            Page::Confirm => {
                if self.sharing {
                    "Confirm share"
                } else if self
                    .operation
                    .as_ref()
                    .is_some_and(|o| o.kind == OperationKind::Update)
                {
                    "Confirm update"
                } else {
                    "Confirm publish"
                }
            }
            Page::Publication => "Publication record",
            Page::Withdraw => "Withdraw article",
            Page::Reader => "Read full article",
        };
        self.label(cx, ids!(article_heading))
            .set_text(cx, tr(heading));
        self.button(cx, ids!(article_save))
            .set_visible(cx, page == Page::Edit);
        self.button(cx, ids!(preview_check))
            .set_visible(cx, page == Page::Preview && !self.reader_only);
        self.status(cx, "");
        self.refresh_document(cx);
        self.view.redraw(cx);
    }
    fn refresh_document(&mut self, cx: &mut Cx) {
        let (chars, minutes, images) = self.doc.stats();
        let stats = format!(
            "{} · {} {} · {} {}",
            chars,
            minutes,
            tr("min read"),
            images,
            tr("images")
        );
        self.label(cx, ids!(article_stats)).set_text(cx, &stats);
        self.label(cx, ids!(preview_stats)).set_text(cx, &stats);
        self.label(cx, ids!(preview_title))
            .set_text(cx, &self.doc.title);
        self.label(cx, ids!(preview_author))
            .set_text(cx, &self.doc.author);
        let outline = self
            .doc
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Heading2 | BlockKind::Heading3))
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        self.label(cx, ids!(outline)).set_text(cx, &outline);
        self.html(cx, ids!(inspector_html)).set_text(
            cx,
            &format!(
                "<h2>{}</h2><p>{}</p><blockquote>{}</blockquote>",
                escape(tr(self.doc.theme.name())),
                escape(tr("Your article, your style.")),
                escape(tr("Only style changes; content stays the same."))
            ),
        );
        let (paper, ink, _) = self.doc.theme.colors();
        let paper = color(paper);
        let ink = color(ink);
        let mut editor = self.view(cx, ids!(editor_paper));
        script_apply_eval!(cx,editor,{draw_bg +: {color: #(paper)}});
        let mut preview = self.view(cx, ids!(preview));
        script_apply_eval!(cx,preview,{draw_bg +: {color: #(paper)}});
        for id in [id!(preview_title), id!(preview_author)] {
            let mut label = self.label(cx, &[id]);
            script_apply_eval!(cx,label,{draw_text +: {color: #(ink)}});
        }
        self.load_cover(cx, ids!(inspector_cover), false);
        self.view.redraw(cx);
    }
    fn bind(&mut self, cx: &mut Cx) {
        let state = super::model::Draft {
            title: self.doc.title.clone(),
            markdown: self.doc.markdown(),
        };
        match super::model::realize_editor(
            &state,
            crate::i18n::language() == crate::i18n::Language::Chinese,
        ) {
            Ok(fields) => {
                for (field, id) in fields
                    .iter()
                    .zip([id!(article_title), id!(article_markdown)])
                {
                    let input = self.text_input(cx, &[id]);
                    input.set_text(cx, &field.text);
                    input.set_empty_text(cx, field.placeholder.clone());
                }
            }
            Err(error) => self.status(cx, &error),
        }
        self.text_input(cx, ids!(article_author))
            .set_text(cx, &self.doc.author);
        self.active_block = 0;
        self.portal_list(cx, ids!(article_blocks))
            .set_first_id_and_scroll(0, 0.0);
        self.refresh_document(cx);
    }
    fn new_document(&mut self, cx: &mut Cx) {
        if self.dirty && !self.save(cx) {
            return;
        }
        self.doc = Document::default();
        self.selected_publication = None;
        self.operation = None;
        self.undo.clear();
        self.redo.clear();
        self.bind(cx);
        self.save(cx);
        self.show(cx, Page::Edit);
    }
    fn load_asset(&self, cx: &mut Cx, widget: ImageRef, id: Option<&str>) {
        widget.set_visible(cx, id.is_some());
        let Some(id) = id else { return };
        if self
            .image_bindings
            .borrow()
            .get(&widget.widget_uid())
            .is_some_and(|key| key == id)
        {
            return;
        }
        let result = if let Some(bytes) = self.reader_assets.get(id) {
            widget.load_image_from_data(cx, bytes)
        } else if let Some(path) = self
            .grant
            .as_ref()
            .and_then(|g| storage::asset_path(crate::app_data_dir(), g, id).ok())
        {
            widget.load_image_file_by_path(cx, &path)
        } else {
            return;
        };
        if result.is_ok() {
            self.image_bindings
                .borrow_mut()
                .insert(widget.widget_uid(), id.into());
        }
    }
    fn cover_image(&self, cx: &mut Cx, widget: ImageRef, cover: &Cover, square: bool) {
        widget.set_visible(cx, true);
        let key = format!(
            "{}:{}:{}:{}",
            cover.asset, cover.focal_x, cover.focal_y, square
        );
        if self.image_bindings.borrow().get(&widget.widget_uid()) == Some(&key) {
            return;
        }
        let bytes = self.reader_assets.get(&cover.asset).cloned().or_else(|| {
            self.grant
                .as_ref()
                .and_then(|g| storage::asset_bytes(crate::app_data_dir(), g, &cover.asset).ok())
        });
        if let Some(bytes) = bytes.and_then(|bytes| crop_cover(&bytes, cover, square).ok()) {
            if widget.load_image_from_data(cx, &bytes).is_ok() {
                self.image_bindings
                    .borrow_mut()
                    .insert(widget.widget_uid(), key);
            }
        }
    }
    fn load_cover(&self, cx: &mut Cx, path: &[LiveId], square: bool) {
        let widget = self.image(cx, path);
        if let Some(cover) = &self.doc.cover {
            self.cover_image(cx, widget, cover, square);
        } else {
            widget.set_visible(cx, false);
        }
    }
    fn open_cover(&mut self, cx: &mut Cx) {
        self.text_input(cx, ids!(cover_summary))
            .set_text(cx, &self.doc.summary);
        if let Some(c) = &self.doc.cover {
            self.slider(cx, ids!(cover_x))
                .set_value(cx, c.focal_x as f64);
            self.slider(cx, ids!(cover_y))
                .set_value(cx, c.focal_y as f64);
            self.check_box(cx, ids!(cover_show))
                .set_active(cx, c.show_in_article, Animate::No);
        }
        self.show(cx, Page::Cover);
        self.load_cover(cx, ids!(cover_wide), false);
        self.load_cover(cx, ids!(cover_square), true);
    }
    fn choose_images(&mut self, cx: &mut Cx, cover: bool, replace: bool) {
        self.selecting_cover = cover;
        self.replacing_image = replace;
        self.load_library(cx);
        self.show(cx, Page::Images);
    }
    fn use_image(&mut self, cx: &mut Cx, asset: storage::Asset) {
        self.checkpoint();
        if self.selecting_cover {
            self.doc.cover = Some(Cover {
                asset: asset.id,
                focal_x: 500,
                focal_y: 500,
                show_in_article: true,
            });
            self.changed(cx);
            self.open_cover(cx);
        } else {
            if self.replacing_image {
                if let Some(b) = self.doc.blocks.get_mut(self.active_block) {
                    b.asset = Some(asset.id);
                }
            } else {
                let mut block = Block::new(BlockKind::Image, "");
                block.asset = Some(asset.id);
                block.alt = asset.name;
                let index = (self.active_block + 1).min(self.doc.blocks.len());
                self.doc.blocks.insert(index, block);
                self.active_block = index;
            }
            self.changed(cx);
            self.show(cx, Page::Edit);
        }
    }
    fn pick(&mut self, cx: &mut Cx) {
        let Some(grant) = self.grant.clone() else {
            return;
        };
        let document = self.doc.id.clone();
        let cover = self.selecting_cover;
        let result = robius_file_picker::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_image(move |result| {
                let result = match result {
                    Ok(None) => return,
                    Ok(Some(file)) => {
                        file.into_local_file()
                            .map_err(|e| e.to_string())
                            .and_then(|file| {
                                storage::import_image(
                                    crate::app_data_dir(),
                                    &grant,
                                    file.path(),
                                    file.display_name().unwrap_or("image"),
                                )
                            })
                    }
                    Err(e) => Err(e.to_string()),
                };
                Cx::post_action(ResultAction::Image {
                    instance: grant.instance,
                    document,
                    cover,
                    result,
                });
            });
        if let Err(e) = result {
            self.status(cx, &e.to_string())
        }
    }
    fn preview(&mut self, cx: &mut Cx) {
        if let Err(e) = self.doc.ready() {
            self.status(cx, &e);
            return;
        }
        if !self.save(cx) {
            return;
        }
        self.portal_list(cx, ids!(article_reader))
            .set_first_id_and_scroll(0, 0.0);
        self.show(cx, Page::Preview);
    }
    fn review(&mut self, cx: &mut Cx) {
        let result = (|| {
            self.doc.ready()?;
            let g = self.grant.as_ref().ok_or("Authorization expired")?;
            for id in self.doc.asset_ids() {
                storage::asset_bytes(crate::app_data_dir(), g, &id)?;
            }
            Ok::<_, String>(())
        })();
        if let Err(e) = result {
            self.status(cx, &e);
            return;
        }
        self.show(cx, Page::Review);
        self.label(cx, ids!(review_title))
            .set_text(cx, &self.doc.title);
        self.label(cx, ids!(review_checks)).set_text(
            cx,
            &format!(
                "✓ {}\n\n✓ {}\n\n{}",
                tr("Title and body are ready"),
                tr("Image files are available"),
                if self.doc.cover.is_some() {
                    tr("Cover and summary reviewed")
                } else {
                    tr("This article will be published without a cover.")
                }
            ),
        );
        self.label(cx, ids!(review_destination)).set_text(
            cx,
            &self
                .selected_publication
                .as_ref()
                .filter(|p| !p.withdrawn)
                .map(|p| p.room_name.clone())
                .unwrap_or_else(|| tr("Choose a chat in the next step.").into()),
        );
    }
    fn choose_room(&mut self, cx: &mut Cx, sharing: bool) {
        self.sharing = sharing;
        self.operation = None;
        self.share_room = None;
        self.share_transaction = new_id();
        self.rooms = cx.get_global::<RoomsListRef>().mini_app_share_rooms();
        self.text_input(cx, ids!(article_chat_search))
            .set_text(cx, "");
        self.show(cx, Page::Rooms);
    }
    fn prepare(&mut self, cx: &mut Cx, room: OwnedRoomId, name: String) {
        let p = self
            .selected_publication
            .as_ref()
            .filter(|p| !p.withdrawn && p.room == room);
        self.operation = Some(Operation {
            id: new_id(),
            kind: if p.is_some() {
                OperationKind::Update
            } else {
                OperationKind::Publish
            },
            document: self.doc.clone(),
            room,
            room_name: name,
            publication: p.map(|p| p.id.clone()),
            root: p.map(|p| p.root.clone()),
            version: p.map(|p| p.version + 1).unwrap_or(1),
            uploaded: BTreeMap::new(),
            redacted: vec![],
            confirmed: None,
            finished: false,
        });
        self.confirm(cx);
    }
    fn confirm(&mut self, cx: &mut Cx) {
        self.show(cx, Page::Confirm);
        let saved = self.operation.as_ref().is_some_and(|op| {
            self.library
                .outbox
                .iter()
                .any(|saved| saved.id == op.id && !saved.finished)
        });
        self.button(cx, ids!(article_change)).set_text(
            cx,
            tr(if saved {
                "Keep editing a copy"
            } else {
                "Back to editing"
            }),
        );
        let label = if self.sharing {
            "Confirm share"
        } else if self
            .operation
            .as_ref()
            .is_some_and(|o| o.kind == OperationKind::Update)
        {
            "Confirm update"
        } else {
            "Confirm publish"
        };
        self.button(cx, ids!(article_confirm))
            .set_text(cx, tr(label));
        self.button(cx, ids!(article_confirm)).set_enabled(cx, true);
        self.label(cx, ids!(confirm_title)).set_text(
            cx,
            if self.sharing {
                tr("Article studio")
            } else {
                &self.doc.title
            },
        );
        self.label(cx, ids!(confirm_summary)).set_text(
            cx,
            if self.sharing {
                tr("Sharing this app shares neither your drafts nor your account permissions.")
            } else {
                &self.doc.summary
            },
        );
        self.label(cx, ids!(publish_account))
            .set_text(cx, self.owner.as_ref().map(|u| u.as_str()).unwrap_or(""));
        let room = if self.sharing {
            self.share_room.as_ref().map(|r| r.display().into_owned())
        } else {
            self.operation.as_ref().map(|o| o.room_name.clone())
        };
        self.label(cx, ids!(publish_room))
            .set_text(cx, &room.unwrap_or_default());
        self.label(cx, ids!(confirm_details)).set_text(
            cx,
            &self
                .operation
                .as_ref()
                .map(|o| {
                    format!(
                        "{} {} · {} {}",
                        tr("Version"),
                        o.version,
                        o.document.asset_ids().len(),
                        tr("images")
                    )
                })
                .unwrap_or_else(|| {
                    tr("Each recipient opens this app with their own account.").into()
                }),
        );
        if self.sharing {
            self.image(cx, ids!(confirm_cover)).set_visible(cx, false);
        } else {
            self.load_cover(cx, ids!(confirm_cover), false);
        }
    }
    fn send(&mut self, cx: &mut Cx) {
        if self.pending || !self.editable() {
            return;
        }
        let (Some(client), Some(grant)) = (get_client(), self.grant.clone()) else {
            return;
        };
        self.pending = true;
        self.button(cx, ids!(article_confirm))
            .set_enabled(cx, false);
        self.status(cx, "Uploading and publishing…");
        if self.sharing {
            let Some(room) = self.share_room.clone() else {
                self.pending = false;
                return;
            };
            let transaction = self.share_transaction.clone();
            spawn_async_task(async move {
                let result = async {
                    if !grant.valid(client.user_id()) {
                        return Err("Authorization expired".into());
                    }
                    let room = backend::writable(&client, &grant, room.room_id()).await?;
                    if room.state() != matrix_sdk::RoomState::Joined {
                        return Err("This chat is no longer joined.".into());
                    }
                    room.send(ArticlePackage::builtin().message())
                        .with_transaction_id(ruma::OwnedTransactionId::from(transaction))
                        .with_request_config(
                            matrix_sdk::config::RequestConfig::new().retry_limit(0),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(())
                }
                .await;
                Cx::post_action(ResultAction::Shared {
                    instance: grant.instance,
                    result,
                });
            });
        } else if let Some(op) = self.operation.clone() {
            spawn_async_task(async move {
                let instance = grant.instance.clone();
                let result = backend::execute(client, grant, op).await;
                Cx::post_action(ResultAction::Published { instance, result });
            });
        } else {
            self.pending = false;
        }
    }
    fn publication(&mut self, cx: &mut Cx, p: Publication) {
        self.doc = p.document.clone();
        self.selected_publication = Some(p.clone());
        self.show(cx, Page::Publication);
        self.label(cx, ids!(publication_state)).set_text(
            cx,
            tr(if p.withdrawn {
                "Withdrawn"
            } else {
                "Published"
            }),
        );
        self.label(cx, ids!(publication_title))
            .set_text(cx, &p.document.title);
        self.label(cx, ids!(publication_info)).set_text(
            cx,
            &format!("{}\n\n{} {}", p.room_name, tr("Version"), p.version),
        );
        self.button(cx, ids!(publication_withdraw))
            .set_visible(cx, !p.withdrawn);
        self.load_cover(cx, ids!(publication_cover), false);
    }
    fn back(&mut self, cx: &mut Cx) {
        if self.pending {
            return;
        }
        match self.page {
            Page::Details => cx.action(ArticleAction::Close),
            Page::Consent => self.show(cx, Page::Details),
            Page::Library | Page::Reader => cx.action(ArticleAction::Close),
            Page::Edit => {
                if self.save(cx) {
                    self.load_library(cx);
                    self.show(cx, Page::Library)
                }
            }
            Page::Source => self.show(cx, Page::Edit),
            Page::Confirm => {
                self.load_library(cx);
                self.show(cx, Page::Library);
            }
            Page::Publication => {
                self.load_library(cx);
                self.show(cx, Page::Library)
            }
            Page::Withdraw => {
                if let Some(p) = self.selected_publication.clone() {
                    self.publication(cx, p)
                }
            }
            _ => self.show(cx, Page::Edit),
        }
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
        if self.save_timer.is_event(event).is_some() && self.dirty && !self.pending {
            self.save(cx);
        }
        if let Event::Actions(actions) = event {
            for action in actions {
                if matches!(self.page, Page::Preview | Page::Reader) {
                    if let HtmlLinkAction::Clicked { url, .. } = action.as_widget_action().cast() {
                        if validate_link(&url).is_ok() {
                            crate::utils::open_url(&url);
                        }
                    }
                }
                let Some(result) = action.downcast_ref::<ResultAction>() else {
                    continue;
                };
                let instance = match result {
                    ResultAction::Image { instance, .. }
                    | ResultAction::Published { instance, .. }
                    | ResultAction::Shared { instance, .. }
                    | ResultAction::Read { instance, .. }
                    | ResultAction::Media { instance, .. } => instance,
                };
                if !self.grant.as_ref().is_some_and(|g| &g.instance == instance) {
                    continue;
                }
                match result {
                    ResultAction::Image {
                        document,
                        cover,
                        result,
                        ..
                    } if document == &self.doc.id => match result {
                        Ok(asset) => {
                            self.selecting_cover = *cover;
                            self.use_image(cx, asset.clone());
                        }
                        Err(e) => self.status(cx, e),
                    },
                    ResultAction::Published { result, .. } => {
                        self.pending = false;
                        self.button(cx, ids!(article_confirm)).set_enabled(cx, true);
                        self.button(cx, ids!(withdraw_confirm))
                            .set_enabled(cx, true);
                        match result {
                            Ok(p) => {
                                self.load_library(cx);
                                self.publication(cx, p.clone());
                            }
                            Err(e) => {
                                self.load_library(cx);
                                self.button(cx, ids!(article_change))
                                    .set_text(cx, tr("Keep editing a copy"));
                                self.status(cx, e);
                            }
                        }
                    }
                    ResultAction::Shared { result, .. } => {
                        self.pending = false;
                        self.button(cx, ids!(article_confirm)).set_enabled(cx, true);
                        match result {
                            Ok(()) => {
                                self.load_library(cx);
                                self.show(cx, Page::Library);
                                self.status(cx, "Mini app sent");
                            }
                            Err(e) => self.status(cx, e),
                        }
                    }
                    ResultAction::Read { result, .. } => {
                        self.pending = false;
                        match result {
                            Ok(article) => {
                                self.doc = article.document.clone();
                                self.remote_article = Some(article.clone());
                                self.show(cx, Page::Reader);
                                if let (Some(client), Some(grant)) =
                                    (get_client(), self.grant.clone())
                                {
                                    for (id, asset) in &article.assets {
                                        let id = id.clone();
                                        let asset = asset.clone();
                                        let client = client.clone();
                                        let instance = grant.instance.clone();
                                        let grant = grant.clone();
                                        spawn_async_task(async move {
                                            let result =
                                                backend::download_image(client, grant, asset).await;
                                            Cx::post_action(ResultAction::Media {
                                                instance,
                                                id,
                                                result,
                                            });
                                        });
                                    }
                                }
                            }
                            Err(e) => self.status(cx, e),
                        }
                    }
                    ResultAction::Media { id, result, .. } => match result {
                        Ok(bytes) => {
                            self.reader_assets.insert(id.clone(), bytes.clone());
                            self.view.redraw(cx);
                        }
                        Err(e) => self.status(cx, e),
                    },
                    _ => (),
                }
            }
            if self.button(cx, ids!(article_close)).clicked(actions)
                || self.button(cx, ids!(article_cancel)).clicked(actions)
            {
                if !self.dirty || self.save(cx) {
                    cx.action(ArticleAction::Close)
                }
                return;
            }
            if self.button(cx, ids!(article_back)).clicked(actions) {
                self.back(cx);
                return;
            }
            if self.pending {
                return;
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
                    self.status(cx, "Sign in to Robrix to authorize this app.");
                }
            }
            if self.page == Page::Consent && self.button(cx, ids!(article_allow)).clicked(actions) {
                if let Some(owner) = current_user_id().filter(|u| Some(u) == self.owner.as_ref()) {
                    self.grant = Some(Grant::new(owner));
                    self.reader_only = false;
                    self.load_library(cx);
                    self.show(cx, Page::Library);
                    if self.library.legacy_source.is_some() {
                        self.status(cx,"Your earlier Markdown draft is preserved. Open Markdown source to review unsupported formatting.");
                    }
                }
            }
            if !self.editable() {
                return;
            }
            if self.button(cx, ids!(article_done)).clicked(actions) {
                self.show(cx, Page::Edit);
                return;
            }
            if self.button(cx, ids!(article_save)).clicked(actions) {
                self.save(cx);
            }
            if self.page == Page::Library {
                for (id, tab) in [
                    (id!(drafts_tab), 0),
                    (id!(published_tab), 1),
                    (id!(withdrawn_tab), 2),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.library_tab = tab;
                        self.filter_library(cx);
                    }
                }
                if self
                    .text_input(cx, ids!(article_search))
                    .changed(actions)
                    .is_some()
                {
                    self.filter_library(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(article_library))
                    .items_with_actions(actions)
                {
                    if item.as_navigation_bar_button().clicked(actions) {
                        if let Some(id) = self.entries.get(index).cloned() {
                            if self.library_tab == 0 {
                                if let Some(doc) =
                                    self.library.documents.iter().find(|d| d.id == id).cloned()
                                {
                                    self.doc = doc;
                                    self.selected_publication = self
                                        .library
                                        .publications
                                        .iter()
                                        .rev()
                                        .find(|p| p.document_id == id && !p.withdrawn)
                                        .cloned();
                                    self.operation = self
                                        .library
                                        .outbox
                                        .iter()
                                        .find(|o| !o.finished && o.document.id == id)
                                        .cloned();
                                    self.undo.clear();
                                    self.redo.clear();
                                    self.bind(cx);
                                    if let Some(operation) = self.operation.clone() {
                                        self.doc = operation.document.clone();
                                        self.sharing = false;
                                        if operation.kind == OperationKind::Withdraw {
                                            self.show(cx, Page::Withdraw);
                                            self.label(cx, ids!(withdraw_title))
                                                .set_text(cx, &self.doc.title);
                                            self.label(cx, ids!(withdraw_room))
                                                .set_text(cx, &operation.room_name);
                                        } else {
                                            self.confirm(cx);
                                        }
                                        self.status(cx,"A publication is pending. Retry uses the saved transaction.");
                                    } else {
                                        self.show(cx, Page::Edit);
                                    }
                                }
                            } else if let Some(p) = self
                                .library
                                .publications
                                .iter()
                                .find(|p| p.id == id)
                                .cloned()
                            {
                                self.publication(cx, p);
                            }
                        }
                        break;
                    }
                }
                if self.button(cx, ids!(article_new)).clicked(actions) {
                    self.new_document(cx);
                }
                if self.button(cx, ids!(article_share)).clicked(actions) {
                    self.choose_room(cx, true);
                }
            }
            if self.page == Page::Edit {
                if let Some(title) = self.text_input(cx, ids!(article_title)).changed(actions) {
                    let mut state = super::model::Draft {
                        title: self.doc.title.clone(),
                        markdown: String::new(),
                    };
                    match super::model::apply_input(&mut state, "title_changed", &title) {
                        Ok(()) => {
                            self.checkpoint();
                            self.doc.title = state.title;
                            self.changed(cx)
                        }
                        Err(e) => {
                            self.text_input(cx, ids!(article_title))
                                .set_text(cx, &self.doc.title);
                            self.status(cx, &e);
                        }
                    }
                }
                if let Some(author) = self.text_input(cx, ids!(article_author)).changed(actions) {
                    self.checkpoint();
                    self.doc.author = author;
                    self.changed(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(article_blocks))
                    .items_with_actions(actions)
                {
                    if index >= self.doc.blocks.len() {
                        continue;
                    }
                    self.active_block = index;
                    let input = item.article_rich_input(cx, ids!(rich));
                    if input.changed(actions).is_some() {
                        if let Some((text, marks)) = input.content() {
                            self.checkpoint();
                            self.doc.blocks[index].text = text;
                            self.doc.blocks[index].marks = marks;
                            self.changed(cx);
                        }
                    }
                    if item.button(cx, ids!(image_settings)).clicked(actions) {
                        self.text_input(cx, ids!(image_caption))
                            .set_text(cx, &self.doc.blocks[index].caption);
                        self.text_input(cx, ids!(image_alt))
                            .set_text(cx, &self.doc.blocks[index].alt);
                        let picture = self.image(cx, ids!(selected_image));
                        self.load_asset(cx, picture, self.doc.blocks[index].asset.as_deref());
                        self.show(cx, Page::ImageSettings);
                    }
                }
                for (id, bold) in [(id!(article_bold), true), (id!(article_italic), false)] {
                    if self.button(cx, &[id]).clicked(actions) {
                        let item = self.portal_list(cx, ids!(article_blocks)).item(
                            cx,
                            self.active_block,
                            id!(TextBlock),
                        );
                        let input = item.article_rich_input(cx, ids!(rich));
                        self.checkpoint();
                        if input.toggle_format(cx, bold) {
                            if let Some((text, marks)) = input.content() {
                                if let Some(b) = self.doc.blocks.get_mut(self.active_block) {
                                    b.text = text;
                                    b.marks = marks;
                                }
                                self.changed(cx);
                            }
                        } else {
                            self.status(cx, "Select some text to format.");
                        }
                    }
                }
                for (id, kind) in [
                    (id!(article_h2), BlockKind::Heading2),
                    (id!(article_quote), BlockKind::Quote),
                    (id!(article_list), BlockKind::Bullet),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.checkpoint();
                        if let Some(block) = self.doc.blocks.get_mut(self.active_block) {
                            if block.kind != BlockKind::Image {
                                block.kind = if block.kind == kind {
                                    BlockKind::Paragraph
                                } else {
                                    kind
                                };
                            }
                        }
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(article_add_text)).clicked(actions) {
                    self.checkpoint();
                    let pos = (self.active_block + 1).min(self.doc.blocks.len());
                    self.doc
                        .blocks
                        .insert(pos, Block::new(BlockKind::Paragraph, ""));
                    self.active_block = pos;
                    self.changed(cx);
                }
                if self.button(cx, ids!(block_up)).clicked(actions) && self.active_block > 0 {
                    self.checkpoint();
                    self.doc
                        .blocks
                        .swap(self.active_block, self.active_block - 1);
                    self.active_block -= 1;
                    self.changed(cx);
                }
                if self.button(cx, ids!(block_down)).clicked(actions)
                    && self.active_block + 1 < self.doc.blocks.len()
                {
                    self.checkpoint();
                    self.doc
                        .blocks
                        .swap(self.active_block, self.active_block + 1);
                    self.active_block += 1;
                    self.changed(cx);
                }
                if self.button(cx, ids!(block_remove)).clicked(actions)
                    && self.active_block < self.doc.blocks.len()
                {
                    self.checkpoint();
                    self.doc.blocks.remove(self.active_block);
                    if self.doc.blocks.is_empty() {
                        self.doc.blocks.push(Block::new(BlockKind::Paragraph, ""));
                    }
                    self.active_block = self.active_block.min(self.doc.blocks.len() - 1);
                    self.changed(cx);
                }
                if self.button(cx, ids!(article_undo)).clicked(actions) {
                    if let Some(previous) = self.undo.pop() {
                        self.redo.push(self.doc.clone());
                        self.doc = previous;
                        self.bind(cx);
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(article_redo)).clicked(actions) {
                    if let Some(next) = self.redo.pop() {
                        self.undo.push(self.doc.clone());
                        self.doc = next;
                        self.bind(cx);
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(article_link)).clicked(actions) {
                    let item = self.portal_list(cx, ids!(article_blocks)).item(
                        cx,
                        self.active_block,
                        id!(TextBlock),
                    );
                    let input = item.article_rich_input(cx, ids!(rich));
                    if input.has_selection() {
                        self.text_input(cx, ids!(article_link_url))
                            .set_text(cx, "https://");
                        self.show(cx, Page::Link);
                    } else {
                        self.status(cx, "Select some text to format.");
                    }
                }
                if self.button(cx, ids!(article_images)).clicked(actions) {
                    self.choose_images(cx, false, false);
                }
                if self.button(cx, ids!(article_theme)).clicked(actions)
                    || self.button(cx, ids!(inspector_theme)).clicked(actions)
                {
                    self.show(cx, Page::Theme);
                }
                if self.button(cx, ids!(article_cover)).clicked(actions)
                    || self
                        .button(cx, ids!(inspector_cover_button))
                        .clicked(actions)
                {
                    self.open_cover(cx);
                }
                if self.button(cx, ids!(article_source)).clicked(actions) {
                    let source = self
                        .library
                        .legacy_source
                        .clone()
                        .unwrap_or_else(|| self.doc.markdown());
                    self.text_input(cx, ids!(article_markdown))
                        .set_text(cx, &source);
                    self.show(cx, Page::Source);
                }
                if self.button(cx, ids!(article_preview)).clicked(actions)
                    || self.button(cx, ids!(inspector_preview)).clicked(actions)
                {
                    self.preview(cx);
                }
                if self.button(cx, ids!(sidebar_new)).clicked(actions) {
                    self.new_document(cx);
                }
                if self.button(cx, ids!(sidebar_library)).clicked(actions) {
                    self.back(cx);
                }
                if self.button(cx, ids!(sidebar_share)).clicked(actions) {
                    if self.save(cx) {
                        self.choose_room(cx, true);
                    }
                }
            }
            if self.page == Page::Link {
                let apply = self.button(cx, ids!(link_apply)).clicked(actions);
                let remove = self.button(cx, ids!(link_remove)).clicked(actions);
                if apply || remove {
                    let url = if remove {
                        None
                    } else {
                        Some(self.text_input(cx, ids!(article_link_url)).text())
                    };
                    let item = self.portal_list(cx, ids!(article_blocks)).item(
                        cx,
                        self.active_block,
                        id!(TextBlock),
                    );
                    let input = item.article_rich_input(cx, ids!(rich));
                    match input.apply_link(cx, url) {
                        Ok(()) => {
                            self.checkpoint();
                            if let Some((text, marks)) = input.content() {
                                self.doc.blocks[self.active_block].text = text;
                                self.doc.blocks[self.active_block].marks = marks;
                            }
                            self.changed(cx);
                            self.show(cx, Page::Edit);
                        }
                        Err(error) => self.status(cx, &error),
                    }
                }
            }
            if self.page == Page::Source && self.button(cx, ids!(source_apply)).clicked(actions) {
                let text = self.text_input(cx, ids!(article_markdown)).text();
                let mut state = super::model::Draft {
                    title: self.doc.title.clone(),
                    markdown: String::new(),
                };
                let result = super::model::apply_input(&mut state, "markdown_changed", &text)
                    .and_then(|()| Document::from_markdown(&state.title, &state.markdown));
                match result {
                    Ok(mut doc) => {
                        let mut matched_images = std::collections::HashSet::new();
                        for block in &mut doc.blocks {
                            if block.kind == BlockKind::Image {
                                if let Some((index, old)) =
                                    self.doc.blocks.iter().enumerate().find(|(index, old)| {
                                        !matched_images.contains(index)
                                            && old.kind == BlockKind::Image
                                            && old.asset == block.asset
                                    })
                                {
                                    matched_images.insert(index);
                                    block.id = old.id.clone();
                                    block.caption = old.caption.clone();
                                    block.width = old.width;
                                }
                            }
                        }
                        self.checkpoint();
                        self.doc.blocks = doc.blocks;
                        self.library.legacy_source = None;
                        if let Some(grant) = &self.grant {
                            let _ = storage::update(crate::app_data_dir(), grant, |lib| {
                                lib.legacy_source = None;
                                Ok(())
                            });
                        }
                        self.bind(cx);
                        self.changed(cx);
                        self.show(cx, Page::Edit);
                    }
                    Err(e) => self.status(cx, &e),
                }
            }
            if self.page == Page::Images {
                if self.button(cx, ids!(image_pick)).clicked(actions) {
                    self.pick(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(image_library))
                    .items_with_actions(actions)
                {
                    if item.as_navigation_bar_button().clicked(actions) {
                        if let Some(asset) = self.library.assets.values().nth(index).cloned() {
                            self.use_image(cx, asset)
                        }
                        break;
                    }
                }
            }
            if self.page == Page::ImageSettings {
                for (id, caption) in [(id!(image_caption), true), (id!(image_alt), false)] {
                    if let Some(text) = self.text_input(cx, &[id]).changed(actions) {
                        self.checkpoint();
                        if let Some(b) = self.doc.blocks.get_mut(self.active_block) {
                            if caption {
                                b.caption = text
                            } else {
                                b.alt = text
                            }
                        }
                        self.changed(cx);
                    }
                }
                for (id, width) in [
                    (id!(image_full), 100),
                    (id!(image_medium), 75),
                    (id!(image_small), 50),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.checkpoint();
                        if let Some(b) = self.doc.blocks.get_mut(self.active_block) {
                            b.width = width;
                        }
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(image_replace)).clicked(actions) {
                    self.choose_images(cx, false, true);
                }
                if self.button(cx, ids!(image_remove)).clicked(actions) {
                    self.checkpoint();
                    if self.active_block < self.doc.blocks.len() {
                        self.doc.blocks.remove(self.active_block);
                    }
                    if self.doc.blocks.is_empty() {
                        self.doc.blocks.push(Block::new(BlockKind::Paragraph, ""));
                    }
                    self.active_block = 0;
                    self.changed(cx);
                    self.show(cx, Page::Edit);
                }
                if self.button(cx, ids!(image_done)).clicked(actions) {
                    self.show(cx, Page::Edit);
                }
            }
            if self.page == Page::Theme {
                for (index, item) in self
                    .portal_list(cx, ids!(theme_list))
                    .items_with_actions(actions)
                {
                    for (offset, id) in [id!(left), id!(right)].into_iter().enumerate() {
                        if item.navigation_bar_button(cx, &[id]).clicked(actions) {
                            if let Some(t) = Theme::ALL.get(index * 2 + offset) {
                                self.checkpoint();
                                self.doc.theme = *t;
                                self.changed(cx);
                            }
                        }
                    }
                }
                for (id, large) in [(id!(size_normal), false), (id!(size_large), true)] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.checkpoint();
                        self.doc.large_type = large;
                        self.changed(cx);
                    }
                }
                for (id, compact) in [
                    (id!(spacing_comfortable), false),
                    (id!(spacing_compact), true),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.checkpoint();
                        self.doc.compact = compact;
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(theme_done)).clicked(actions) {
                    self.show(cx, Page::Edit);
                }
            }
            if self.page == Page::Cover {
                if self.button(cx, ids!(cover_pick)).clicked(actions) {
                    self.choose_images(cx, true, false);
                }
                if let Some(summary) = self.text_input(cx, ids!(cover_summary)).changed(actions) {
                    self.checkpoint();
                    self.doc.summary = summary;
                    self.changed(cx);
                }
                let x = self.slider(cx, ids!(cover_x)).slided(actions);
                let y = self.slider(cx, ids!(cover_y)).slided(actions);
                if x.is_some() || y.is_some() {
                    if let Some(c) = &mut self.doc.cover {
                        if let Some(x) = x {
                            c.focal_x = x.clamp(0.0, 1000.0) as u16
                        }
                        if let Some(y) = y {
                            c.focal_y = y.clamp(0.0, 1000.0) as u16
                        }
                    }
                    self.changed(cx);
                    self.load_cover(cx, ids!(cover_wide), false);
                    self.load_cover(cx, ids!(cover_square), true);
                }
                if let Some(value) = self.check_box(cx, ids!(cover_show)).changed(actions) {
                    self.checkpoint();
                    if let Some(c) = &mut self.doc.cover {
                        c.show_in_article = value;
                    }
                    self.changed(cx);
                }
                if self.button(cx, ids!(cover_remove)).clicked(actions) {
                    self.checkpoint();
                    self.doc.cover = None;
                    self.changed(cx);
                    self.open_cover(cx);
                }
                if self.button(cx, ids!(cover_done)).clicked(actions) {
                    self.show(cx, Page::Edit);
                }
            }
            if self.page == Page::Preview && self.button(cx, ids!(preview_check)).clicked(actions) {
                self.review(cx);
            }
            if self.page == Page::Review && self.button(cx, ids!(review_continue)).clicked(actions)
            {
                self.sharing = false;
                if let Some(p) = self.selected_publication.clone().filter(|p| !p.withdrawn) {
                    self.prepare(cx, p.room, p.room_name)
                } else {
                    self.choose_room(cx, false)
                }
            }
            if self.page == Page::Rooms {
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
                    self.view.redraw(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(article_rooms))
                    .items_with_actions(actions)
                {
                    if item.as_navigation_bar_button().clicked(actions) {
                        if let Some(room) = self.rooms.get(index).cloned() {
                            if self.sharing {
                                self.share_room = Some(room);
                                self.confirm(cx)
                            } else {
                                self.prepare(
                                    cx,
                                    room.room_id().to_owned(),
                                    room.display().into_owned(),
                                )
                            }
                        }
                        break;
                    }
                }
            }
            if self.page == Page::Confirm {
                if self.button(cx, ids!(article_confirm)).clicked(actions) {
                    self.send(cx);
                }
                if self.button(cx, ids!(article_change)).clicked(actions) {
                    self.load_library(cx);
                    if self.operation.as_ref().is_some_and(|op| {
                        self.library
                            .outbox
                            .iter()
                            .any(|saved| saved.id == op.id && !saved.finished)
                    }) {
                        self.doc.id = new_id();
                        self.selected_publication = None;
                    }
                    self.operation = None;
                    self.bind(cx);
                    self.show(cx, Page::Edit);
                }
            }
            if self.page == Page::Publication {
                if self.button(cx, ids!(publication_library)).clicked(actions) {
                    self.load_library(cx);
                    self.show(cx, Page::Library);
                }
                if self.button(cx, ids!(publication_edit)).clicked(actions) {
                    if self
                        .selected_publication
                        .as_ref()
                        .is_some_and(|p| p.withdrawn)
                    {
                        self.doc.id = new_id();
                        self.selected_publication = None;
                    }
                    self.bind(cx);
                    self.show(cx, Page::Edit);
                }
                if self.button(cx, ids!(publication_read)).clicked(actions) {
                    self.show(cx, Page::Preview);
                }
                if self.button(cx, ids!(publication_withdraw)).clicked(actions) {
                    if let Some(p) = self.selected_publication.clone() {
                        self.show(cx, Page::Withdraw);
                        self.label(cx, ids!(withdraw_title))
                            .set_text(cx, &p.document.title);
                        self.label(cx, ids!(withdraw_room))
                            .set_text(cx, &p.room_name);
                    }
                }
            }
            if self.page == Page::Withdraw {
                if self.button(cx, ids!(withdraw_cancel)).clicked(actions) {
                    self.back(cx);
                }
                if self.button(cx, ids!(withdraw_confirm)).clicked(actions) {
                    if let Some(p) = self.selected_publication.clone() {
                        self.sharing = false;
                        self.operation = self
                            .library
                            .outbox
                            .iter()
                            .find(|o| {
                                !o.finished
                                    && o.kind == OperationKind::Withdraw
                                    && o.publication.as_ref() == Some(&p.id)
                            })
                            .cloned()
                            .or_else(|| {
                                Some(Operation {
                                    id: new_id(),
                                    kind: OperationKind::Withdraw,
                                    document: p.document.clone(),
                                    room: p.room.clone(),
                                    room_name: p.room_name.clone(),
                                    publication: Some(p.id),
                                    root: Some(p.root),
                                    version: p.version,
                                    uploaded: p.assets,
                                    redacted: vec![],
                                    confirmed: None,
                                    finished: false,
                                })
                            });
                        self.button(cx, ids!(withdraw_confirm))
                            .set_enabled(cx, false);
                        self.send(cx);
                        self.status(cx, "Withdrawing article and its revisions…");
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
            self.back(cx);
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let available = cx.turtle().inner_rect().size.x;
        let wide = available >= 960.0;
        let paper_width = if wide {
            Size::Fixed((available - 490.0).min(760.0))
        } else {
            Size::fill()
        };
        let mut paper = self.view(cx, ids!(editor_paper));
        script_apply_eval!(cx, paper, {width: #(paper_width)});
        self.view(cx, ids!(editor_sidebar)).set_visible(cx, wide);
        self.view(cx, ids!(editor_inspector)).set_visible(cx, wide);
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            let uid = item.widget_uid();
            let library = uid == self.portal_list(cx, ids!(article_library)).widget_uid();
            let blocks = uid == self.portal_list(cx, ids!(article_blocks)).widget_uid();
            let reader = uid == self.portal_list(cx, ids!(article_reader)).widget_uid();
            let themes = uid == self.portal_list(cx, ids!(theme_list)).widget_uid();
            let images = uid == self.portal_list(cx, ids!(image_library)).widget_uid();
            if let Some(mut list) = item.borrow_mut::<PortalList>() {
                let cover = self.doc.cover.as_ref().filter(|c| c.show_in_article);
                let count = if library {
                    self.entries.len()
                } else if blocks {
                    self.doc.blocks.len()
                } else if reader {
                    self.doc.blocks.len() + usize::from(cover.is_some())
                } else if themes {
                    2
                } else if images {
                    self.library.assets.len()
                } else {
                    self.rooms.len()
                };
                list.set_item_range(cx, 0, count);
                while let Some(index) = list.next_visible_item(cx) {
                    if index >= count {
                        continue;
                    }
                    if library {
                        let Some(id) = self.entries.get(index) else {
                            continue;
                        };
                        let (doc, status) = if self.library_tab == 0 {
                            let Some(d) = self.library.documents.iter().find(|d| &d.id == id)
                            else {
                                continue;
                            };
                            (d, tr("Draft").to_owned())
                        } else {
                            let Some(p) = self.library.publications.iter().find(|p| &p.id == id)
                            else {
                                continue;
                            };
                            (
                                &p.document,
                                format!("{} · {} {}", p.room_name, tr("Version"), p.version),
                            )
                        };
                        let row = list.item(cx, index, id!(Entry));
                        row.label(cx, ids!(title)).set_text(
                            cx,
                            if doc.title.is_empty() {
                                tr("Untitled article")
                            } else {
                                &doc.title
                            },
                        );
                        row.label(cx, ids!(summary))
                            .set_text(cx, &format!("{}\n{}", doc.summary, status));
                        let thumbnail = row.image(cx, ids!(thumbnail));
                        self.load_asset(
                            cx,
                            thumbnail,
                            doc.cover.as_ref().map(|c| c.asset.as_str()),
                        );
                        row.draw_all(cx, &mut Scope::empty());
                    } else if blocks {
                        let Some(block) = self.doc.blocks.get(index) else {
                            continue;
                        };
                        let template = match block.kind {
                            BlockKind::Image => id!(Picture),
                            BlockKind::Divider => id!(Rule),
                            _ => id!(TextBlock),
                        };
                        let row = list.item(cx, index, template);
                        if block.kind == BlockKind::Image {
                            let picture = row.image(cx, ids!(picture));
                            self.load_asset(cx, picture, block.asset.as_deref());
                            row.label(cx, ids!(caption)).set_text(cx, &block.caption);
                            let width = (cx.turtle().inner_rect().size.x - 12.0)
                                * (block.width as f64 / 100.0);
                            let mut image = row.image(cx, ids!(picture));
                            script_apply_eval!(cx,image,{width: #(width)});
                        } else if block.kind != BlockKind::Divider {
                            let prefix = row.label(cx, ids!(prefix));
                            prefix.set_text(
                                cx,
                                match block.kind {
                                    BlockKind::Quote => "❝",
                                    BlockKind::Bullet => "•",
                                    BlockKind::Numbered => "1.",
                                    _ => "",
                                },
                            );
                            prefix.set_visible(
                                cx,
                                matches!(
                                    block.kind,
                                    BlockKind::Quote | BlockKind::Bullet | BlockKind::Numbered
                                ),
                            );
                            let size = match block.kind {
                                BlockKind::Heading2 => 20.0,
                                BlockKind::Heading3 => 17.0,
                                _ => {
                                    if self.doc.large_type {
                                        16.0
                                    } else {
                                        14.0
                                    }
                                }
                            };
                            let mut input = row.article_rich_input(cx, ids!(rich));
                            let (_, ink, accent) = self.doc.theme.colors();
                            let ink = color(if block.kind == BlockKind::Quote {
                                accent
                            } else {
                                ink
                            });
                            let spacing = if self.doc.compact { 1.0 } else { 1.25 };
                            script_apply_eval!(cx,input,{draw_text +: {color: #(ink) color_focus: #(ink) color_hover: #(ink) text_style +: {line_spacing: #(spacing)}}});
                            input.set_block(cx, block, size);
                        }
                        row.draw_all(cx, &mut Scope::empty());
                    } else if reader {
                        let block = if cover.is_some() {
                            if index == 0 {
                                None
                            } else {
                                self.doc.blocks.get(index - 1)
                            }
                        } else {
                            self.doc.blocks.get(index)
                        };
                        let picture =
                            block.is_none() || block.is_some_and(|b| b.kind == BlockKind::Image);
                        let row =
                            list.item(cx, index, if picture { id!(Image) } else { id!(Text) });
                        if picture {
                            let asset = block
                                .and_then(|b| b.asset.as_deref())
                                .or_else(|| cover.map(|c| c.asset.as_str()));
                            let mut image = row.image(cx, ids!(picture));
                            if let Some(cover) = cover.filter(|_| block.is_none()) {
                                self.cover_image(cx, image.clone(), cover, false);
                            } else {
                                self.load_asset(cx, image.clone(), asset);
                            }
                            let full_width = cx.turtle().inner_rect().size.x;
                            let width =
                                full_width * block.map(|b| b.width as f64 / 100.0).unwrap_or(1.0);
                            let dimensions = asset.and_then(|id| {
                                self.library
                                    .assets
                                    .get(id)
                                    .map(|a| (a.width, a.height))
                                    .or_else(|| {
                                        self.remote_article.as_ref().and_then(|a| {
                                            a.assets
                                                .get(id)
                                                .map(|a| (a.asset.width, a.asset.height))
                                        })
                                    })
                            });
                            let ratio = if block.is_none() {
                                2.35
                            } else {
                                dimensions
                                    .map(|(w, h)| w as f64 / h.max(1) as f64)
                                    .unwrap_or(1.8)
                            };
                            let height = (width / ratio).min(600.0);
                            script_apply_eval!(cx, image, {width: #(width) height: #(height)});
                            row.label(cx, ids!(caption))
                                .set_text(cx, block.map(|b| b.caption.as_str()).unwrap_or(""));
                        } else if let Some(block) = block {
                            let mut html = row.html(cx, ids!(body));
                            let (_, ink, _) = self.doc.theme.colors();
                            let ink = color(ink);
                            let size = if self.doc.large_type { 16.0 } else { 14.0 };
                            let spacing = if self.doc.compact { 1.0 } else { 1.25 };
                            script_apply_eval!(cx,html,{
                                font_size: #(size) font_color: #(ink)
                                draw_text +: {color: #(ink)}
                                text_style_normal +: {line_spacing: #(spacing)}
                                text_style_bold +: {line_spacing: #(spacing)}
                                text_style_italic +: {line_spacing: #(spacing)}
                                text_style_bold_italic +: {line_spacing: #(spacing)}
                            });
                            html.set_text(cx, &block.html());
                        }
                        row.draw_all(cx, &mut Scope::empty());
                    } else if themes {
                        let row = list.item(cx, index, id!(Theme));
                        for (offset, id) in [id!(left), id!(right)].into_iter().enumerate() {
                            let theme = Theme::ALL[index * 2 + offset];
                            let mut tile = row.widget(cx, &[id]);
                            tile.label(cx, ids!(name)).set_text(
                                cx,
                                &format!(
                                    "{}{}",
                                    if theme == self.doc.theme { "✓ " } else { "" },
                                    tr(theme.name())
                                ),
                            );
                            tile.label(cx, ids!(title)).set_text(
                                cx,
                                if self.doc.title.is_empty() {
                                    tr("Your article, your style.")
                                } else {
                                    &self.doc.title
                                },
                            );
                            tile.label(cx, ids!(sample))
                                .set_text(cx, tr("A thoughtful layout makes reading a pleasure."));
                            let image = tile.image(cx, ids!(picture));
                            let asset =
                                self.doc
                                    .cover
                                    .as_ref()
                                    .map(|c| c.asset.as_str())
                                    .or_else(|| {
                                        self.doc.blocks.iter().find_map(|b| b.asset.as_deref())
                                    });
                            self.load_asset(cx, image, asset);
                            let (bg, ink, accent) = theme.colors();
                            let bg = color(bg);
                            let ink = color(ink);
                            let border = color(if theme == self.doc.theme {
                                accent
                            } else {
                                0xe0e0e0
                            });
                            script_apply_eval!(cx, tile, {draw_bg +: {color: #(bg) border_color: #(border)} name +: {draw_text +: {color: #(ink)}} title +: {draw_text +: {color: #(ink)}} sample +: {draw_text +: {color: #(ink)}}});
                        }
                        row.draw_all(cx, &mut Scope::empty());
                    } else if images {
                        if let Some(asset) = self.library.assets.values().nth(index) {
                            let row = list.item(cx, index, id!(Asset));
                            let picture = row.image(cx, ids!(picture));
                            self.load_asset(cx, picture, Some(&asset.id));
                            row.label(cx, ids!(name)).set_text(cx, &asset.name);
                            row.draw_all(cx, &mut Scope::empty());
                        }
                    } else if let Some(room) = self.rooms.get(index) {
                        let row = list.item(cx, index, id!(Chat));
                        row.label(cx, ids!(name)).set_text(cx, &room.display());
                        row.draw_all(cx, &mut Scope::empty());
                    }
                }
            }
        }
        DrawStep::done()
    }
}
fn crop_cover(bytes: &[u8], cover: &Cover, square: bool) -> Result<Vec<u8>, String> {
    let image = ::image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let ratio = if square { 1.0 } else { 2.35 };
    let (w, h) = (image.width(), image.height());
    let (cw, ch) = if w as f64 / h as f64 > ratio {
        ((h as f64 * ratio) as u32, h)
    } else {
        (w, (w as f64 / ratio) as u32)
    };
    let x = ((w - cw) as f64 * cover.focal_x as f64 / 1000.0) as u32;
    let y = ((h - ch) as f64 * cover.focal_y as f64 / 1000.0) as u32;
    let cropped = image.crop_imm(x, y, cw.max(1), ch.max(1));
    let mut output = std::io::Cursor::new(Vec::new());
    cropped
        .write_to(&mut output, ::image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(output.into_inner())
}
impl ArticlePanelRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &ArticleAction) {
        let Some(mut panel) = self.borrow_mut() else {
            return;
        };
        match action {
            ArticleAction::Open | ArticleAction::Read { .. } => {
                if let Some(g) = panel.grant.take() {
                    g.revoke()
                }
                panel.active = true;
                panel.owner = current_user_id();
                panel.reader_only = matches!(action, ArticleAction::Read { .. });
                panel.doc = Document::default();
                panel.library = Library::default();
                panel.pending = false;
                panel.dirty = false;
                panel.operation = None;
                panel.selected_publication = None;
                panel.reader_assets.clear();
                panel.image_bindings.borrow_mut().clear();
                panel.remote_article = None;
                panel.sharing = false;
                panel.show(cx, Page::Details);
                modal.open(cx);
                if let ArticleAction::Read { room, event } = action {
                    if let (Some(owner), Some(client)) = (current_user_id(), get_client()) {
                        let grant = Grant::new(owner);
                        let instance = grant.instance.clone();
                        panel.grant = Some(grant.clone());
                        panel.pending = true;
                        panel.show(cx, Page::Reader);
                        panel.status(cx, "Loading article…");
                        let room = room.clone();
                        let event = event.clone();
                        spawn_async_task(async move {
                            let result = backend::read_article(client, grant, room, event).await;
                            Cx::post_action(ResultAction::Read { instance, result });
                        });
                    }
                }
            }
            ArticleAction::Close => {
                if panel.dirty && panel.editable() && !panel.save(cx) {
                    return;
                }
                if let Some(g) = panel.grant.take() {
                    g.revoke()
                }
                panel.active = false;
                panel.pending = false;
                panel.owner = None;
                panel.doc = Document::default();
                panel.library = Library::default();
                panel.reader_assets.clear();
                panel.image_bindings.borrow_mut().clear();
                panel.remote_article = None;
                panel.operation = None;
                panel.selected_publication = None;
                cx.stop_timer(panel.save_timer);
                modal.close(cx);
            }
        }
    }
}
