use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use octoscript_ui_l0::{InstanceStore, NodeValue, RealizeLimits, UiNode};
use ruma::{
    OwnedUserId,
    events::room::message::{MessageType, RoomMessageEventContent},
};
use serde::{Deserialize, Serialize};

pub const MSGTYPE: &str = "rs.robius.robrix.article_app";
const APP_ID: &str = "org.octosense.article-editor";
pub const SOURCE: &str = include_str!("../../resources/mini_apps/article-editor/app.card");
const MAX_MARKDOWN: usize = 24_000;
static SESSION: AtomicU64 = AtomicU64::new(1);
pub fn invalidate_sessions() {
    SESSION.fetch_add(1, Ordering::SeqCst);
}

/// A reference to a built-in, reviewed package. The build is the trust root;
/// this is deliberately NOT a remotely asserted publisher signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArticlePackage {
    pub app_id: String,
    pub version: u32,
    pub source_hash: String,
}
impl ArticlePackage {
    pub fn builtin() -> Self {
        Self {
            app_id: APP_ID.into(),
            version: 1,
            source_hash: blake3::hash(SOURCE.as_bytes()).to_hex().to_string(),
        }
    }
    pub fn from_message(message: &MessageType) -> Result<Self, String> {
        if message.msgtype() != MSGTYPE {
            return Err("Not an article app".into());
        }
        let package: Self = serde_json::from_value(
            message
                .data()
                .get("app")
                .cloned()
                .ok_or("Missing article app")?,
        )
        .map_err(|_| "Invalid article app")?;
        if package != Self::builtin() {
            return Err("This article app needs a compatible Robrix update.".into());
        }
        Ok(package)
    }
    pub fn message(&self) -> RoomMessageEventContent {
        let mut data = serde_json::Map::new();
        data.insert("app".into(), serde_json::to_value(self).unwrap());
        RoomMessageEventContent::new(
            MessageType::new(
                MSGTYPE,
                "[Mini app] Article editor · Markdown / native preview".into(),
                data,
            )
            .unwrap(),
        )
    }
}

/// Per-open consent, never serialized or inherited through a shared card.
#[derive(Clone, Debug)]
pub struct Grant {
    pub owner: OwnedUserId,
    pub instance: String,
    generation: u64,
    expires: Instant,
    revoked: Arc<AtomicBool>,
}
impl Grant {
    pub fn new(owner: OwnedUserId) -> Self {
        Self {
            owner,
            instance: ruma::TransactionId::new().to_string(),
            generation: SESSION.load(Ordering::SeqCst),
            expires: Instant::now() + Duration::from_secs(3600),
            revoked: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn valid(&self, owner: Option<&ruma::UserId>) -> bool {
        owner == Some(self.owner.as_ref())
            && self.generation == SESSION.load(Ordering::SeqCst)
            && Instant::now() < self.expires
            && !self.revoked.load(Ordering::SeqCst)
    }
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::SeqCst);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub title: String,
    pub markdown: String,
}
impl Draft {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.chars().count() > 120
            || self.title.chars().any(char::is_control)
            || self.markdown.len() > MAX_MARKDOWN
        {
            return Err(crate::i18n::tr(
                "Use a title up to 120 characters and Markdown up to 24 KB.",
            )
            .into());
        }
        Ok(())
    }
    pub fn message(&self) -> Result<RoomMessageEventContent, String> {
        self.validate()?;
        if self.title.trim().is_empty() || self.markdown.trim().is_empty() {
            return Err(crate::i18n::tr("Add a title and some text first.").into());
        }
        let raw = RoomMessageEventContent::text_markdown(&self.markdown);
        let MessageType::Text(text) = raw.msgtype else {
            unreachable!()
        };
        let body = text
            .formatted
            .map(|f| f.body)
            .unwrap_or_else(|| escape(&self.markdown).replace('\n', "<br>"));
        let html =
            ruma::html::Html::parse(&format!("<h1>{}</h1>{body}", escape(self.title.trim())));
        html.sanitize_with(&ruma::html::SanitizerConfig::strict()
            .remove_elements(["script", "style", "img", "iframe", "object", "embed", "form", "input", "video", "audio", "svg", "math"])
            // No active links or mentions in this first native article renderer.
            .ignore_elements(["a"]));
        let html = html.to_string();
        let plain = crate::shared::slash_commands::html_to_plaintext(&html);
        let message = RoomMessageEventContent::text_html(plain, html);
        if serde_json::to_vec(&message)
            .map_err(|e| e.to_string())?
            .len()
            > 55_000
        {
            return Err(crate::i18n::tr(
                "This article is too large to send. Shorten it and try again.",
            )
            .into());
        }
        Ok(message)
    }
    pub fn html(&self) -> Result<String, String> {
        let MessageType::Text(text) = self.message()?.msgtype else {
            unreachable!()
        };
        Ok(text.formatted.unwrap().body)
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn draft_path(root: &Path, owner: &ruma::UserId) -> PathBuf {
    root.join("mini-apps")
        .join(blake3::hash(owner.as_bytes()).to_hex().as_str())
        .join(APP_ID)
        .join("draft.json")
}
pub fn read_draft(
    root: &Path,
    grant: &Grant,
    owner: Option<&ruma::UserId>,
) -> Result<Draft, String> {
    if !grant.valid(owner) {
        return Err("Authorization expired".into());
    }
    let path = draft_path(root, &grant.owner);
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Draft::default()),
        Err(e) => return Err(e.to_string()),
    };
    use std::io::Read;
    let mut data = Vec::new();
    file.take(160_001).read_to_end(&mut data).map_err(|e|e.to_string())?;
    if data.len() > 160_000 {
        return Err("Draft exceeds storage limit".into());
    }
    let draft: Draft = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    draft.validate()?;
    Ok(draft)
}
pub fn save_draft(
    root: &Path,
    grant: &Grant,
    owner: Option<&ruma::UserId>,
    draft: &Draft,
) -> Result<(), String> {
    if !grant.valid(owner) {
        return Err("Authorization expired".into());
    }
    draft.validate()?;
    let path = draft_path(root, &grant.owner);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", grant.instance));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let result = (|| {
        let mut file = options.open(&temp).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(draft).unwrap())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        if !grant.valid(owner) {
            return Err("Authorization expired".into());
        }
        std::fs::rename(&temp, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

/// A deliberately small native kit adapter. Every Field and event comes from
/// the admitted L0 tree. Authority, preview and send controls belong to Robrix.
pub struct EditorField {
    pub text: String,
    pub placeholder: String,
    pub event: String,
}
pub fn realize_editor(draft: &Draft, chinese: bool) -> Result<Vec<EditorField>, String> {
    let data = serde_json::json!({"title":draft.title,"markdown":draft.markdown,"env":{"locale":{"lang":if chinese {"zh"} else {"en"}}}});
    let report = octoscript_ui_l0::realize(
        SOURCE,
        &data,
        RealizeLimits {
            max_nodes: 16,
            max_depth: 8,
            max_collection: 2,
            max_work: 64,
        },
    );
    let root = report.complete_root()?;
    fn fields(node: &UiNode, out: &mut Vec<EditorField>) -> Result<(), String> {
        match node.kind.as_str() {
            "Surface" | "Col" => (),
            "Field" => {
                let arg = |name: &str| node.args.iter().find(|(k, _)| k == name).map(|(_, v)| v);
                let (
                    Some(NodeValue::Text(text)),
                    Some(NodeValue::Text(placeholder)),
                    Some(NodeValue::Event(event)),
                ) = (arg("text"), arg("placeholder"), arg("on_change"))
                else {
                    return Err("Invalid article field binding".into());
                };
                out.push(EditorField {
                    text: text.clone(),
                    placeholder: placeholder.clone(),
                    event: event.clone(),
                });
            }
            _ => return Err("Unsupported article kit role".into()),
        }
        for child in &node.children {
            fields(child, out)?;
        }
        Ok(())
    }
    let mut out = Vec::new();
    fields(root, &mut out)?;
    if out.len() != 2 || out[0].event != "title_changed" || out[1].event != "markdown_changed" {
        return Err("Unsupported article field mapping".into());
    }
    Ok(out)
}
pub fn apply_input(draft: &mut Draft, event: &str, value: &str) -> Result<(), String> {
    let mut store = InstanceStore::default();
    store.set_cell(
        octoscript_ui_l0::CARD_STATE_KEY,
        "title",
        serde_json::json!(draft.title),
    );
    store.set_cell(
        octoscript_ui_l0::CARD_STATE_KEY,
        "markdown",
        serde_json::json!(draft.markdown),
    );
    if matches!(event, "title_changed") && value == draft.title
        || matches!(event, "markdown_changed") && value == draft.markdown
    {
        return Ok(());
    }
    // Top-level state dispatch uses the root instance; only these declared
    // setter events exist. User text is a JSON string, never script source.
    if !octoscript_ui_l0::dispatch_with(
        SOURCE,
        &mut store,
        "root",
        event,
        Some(&serde_json::Value::String(value.into())),
    ) {
        return Err("Unknown editor event".into());
    }
    let field = match event {
        "title_changed" => "title",
        "markdown_changed" => "markdown",
        _ => return Err("Unknown editor event".into()),
    };
    let text = store
        .get(octoscript_ui_l0::CARD_STATE_KEY, field)
        .and_then(|v| v.as_str())
        .ok_or("Missing editor state")?;
    let mut next = draft.clone();
    if field == "title" {
        next.title = text.into()
    } else {
        next.markdown = text.into()
    };
    next.validate()?;
    *draft = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_or_previous_login_grant_cannot_read_or_save() {
        let owner = ruma::user_id!("@alice:example.org");
        let mut grant = Grant::new(owner.to_owned());
        grant.expires = Instant::now() - Duration::from_secs(1);
        assert!(!grant.valid(Some(owner)));
        let root = std::env::temp_dir();
        assert!(read_draft(&root, &grant, Some(owner)).is_err());
        assert!(save_draft(&root, &grant, Some(owner), &Draft::default()).is_err());
        let mut old = Grant::new(owner.to_owned());
        old.generation = 0;
        assert!(!old.valid(Some(owner))); // even the same user on a later login
        assert!(!Grant::new(owner.to_owned()).valid(None));
    }
    #[test]
    fn input_limits_and_clear_preserve_state_contract() {
        let mut draft=Draft {title:"A title".into(), markdown:"Hello".into()};
        apply_input(&mut draft,"title_changed","").unwrap();
        apply_input(&mut draft,"markdown_changed","").unwrap();
        assert_eq!(draft,Draft::default());
        assert!(draft.message().is_err());
        assert!(apply_input(&mut draft,"title_changed",&"长".repeat(121)).is_err());
        assert!(apply_input(&mut draft,"markdown_changed",&"x".repeat(MAX_MARKDOWN+1)).is_err());
        assert!(apply_input(&mut draft,"title_changed","line\nbreak").is_err());
        assert_eq!(draft,Draft::default());
    }
    #[test]
    fn package_never_transports_identity_or_grants() {
        let p = ArticlePackage::builtin();
        let m = p.message();
        assert_eq!(ArticlePackage::from_message(&m.msgtype).unwrap(), p);
        let wire = serde_json::to_string(&m).unwrap();
        for key in ["access_token", "owner", "grant", "draft", "password"] {
            assert!(!wire.contains(key));
        }
        let mut bad = p;
        bad.source_hash = "forged".into();
        assert!(ArticlePackage::from_message(&bad.message().msgtype).is_err());
    }
    #[test]
    fn native_l0_input_and_bilingual_fields() {
        let report = octoscript_ui_l0::check_ui_l0(SOURCE);
        assert!(report.valid, "{:?}", report.diagnostics);
        let mut draft = Draft::default();
        apply_input(&mut draft, "title_changed", "中文 Title").unwrap();
        apply_input(&mut draft, "markdown_changed", "## hello").unwrap();
        for chinese in [false, true] {
            let fields = realize_editor(&draft, chinese).unwrap();
            assert_eq!(fields[0].text, draft.title);
            assert_eq!(fields[1].text, draft.markdown);
            assert_eq!(
                fields[0].placeholder,
                if chinese {
                    "文章标题"
                } else {
                    "Article title"
                }
            );
        }
        assert!(apply_input(&mut draft, "publish", "arbitrary").is_err());
    }
    #[test]
    fn hostile_html_is_inert_and_title_escaped() {
        let draft=Draft{title:"<script>title</script>".into(),markdown:"## hello\n\n**bold**\n\n<script>bad()</script><img src=\"https://bad.invalid/pixel\"><iframe>secret</iframe><a href=\"javascript:alert(1)\">link</a>".into()};
        let html = draft.html().unwrap();
        for token in ["<script", "<img", "<iframe", "href=", "bad()", "secret"] {
            assert!(!html.contains(token), "{html}");
        }
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("&lt;script&gt;title"));
    }
    #[test]
    fn grants_drafts_and_revocation_are_account_local() {
        let a = ruma::user_id!("@alice:example.org");
        let b = ruma::user_id!("@bob:example.org");
        let grant = Grant::new(a.to_owned());
        let root = std::env::temp_dir().join(format!("robrix-article-test-{}", grant.instance));
        let draft = Draft {
            title: "private".into(),
            markdown: "my own draft".into(),
        };
        assert!(save_draft(&root, &grant, Some(b), &draft).is_err());
        save_draft(&root, &grant, Some(a), &draft).unwrap();
        assert_eq!(read_draft(&root, &grant, Some(a)).unwrap(), draft);
        let bob = Grant::new(b.to_owned());
        assert_eq!(read_draft(&root, &bob, Some(b)).unwrap(), Draft::default());
        let clone = grant.clone();
        grant.revoke();
        assert!(!clone.valid(Some(a)));
        assert!(read_draft(&root, &grant, Some(a)).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
