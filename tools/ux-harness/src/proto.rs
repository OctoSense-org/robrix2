//! The makepad studio wire protocol, as much of it as the harness needs.
//!
//! Makepad's headless backend (`MAKEPAD=headless`, run with `--stdin-loop`)
//! speaks newline-delimited JSON: it reads `StudioToApp` on stdin and writes
//! `AppToStudio` on stdout. That is the *internal event channel* this harness
//! drives — no OS-level synthetic input, no window server, no screen recording
//! permission.
//!
//! The encoding is `makepad_micro_serde`'s `SerJson`, which shapes enums as:
//!   - tuple variant  → `{"MouseDown":[{..}]}`
//!   - struct variant → `{"WindowGeomChange":{..}}`
//!   - unit variant   → `{"Tick":[]}`
//!
//! We hand-write the subset instead of depending on `makepad_studio_protocol`
//! so the harness stays a standalone binary that builds in seconds.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Mouse button bits as makepad's `MouseButton::from_bits_retain` expects
/// them. PRIMARY is bit 0 (`1 << 0`); sending 0 means "no button", which
/// silently misses every `is_primary_hit()` widget (plain Buttons still work
/// because they only test `is_over`, which is how this went unnoticed).
pub const MOUSE_PRIMARY: u32 = 1;

/// `KeyCode` crosses the wire as an index into makepad's `KEYCODE_VARIANTS`
/// table (it hand-rolls integer encoding to keep LLVM IR small). Only the keys
/// a UX scenario actually needs are mapped here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    A,
    Escape,
    Tab,
    Return,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Backspace,
}

impl Key {
    pub fn wire_index(self) -> u32 {
        match self {
            Key::A => 30,
            Key::Escape => 0,
            Key::Backspace => 15,
            Key::Tab => 16,
            Key::Return => 29,
            Key::Space => 56,
            Key::ArrowUp => 97,
            Key::ArrowDown => 98,
            Key::ArrowLeft => 99,
            Key::ArrowRight => 100,
        }
    }

    pub fn parse(name: &str) -> Option<Key> {
        Some(match name.to_ascii_lowercase().as_str() {
            "a" => Key::A,
            "escape" | "esc" => Key::Escape,
            "tab" => Key::Tab,
            "return" | "enter" => Key::Return,
            "space" => Key::Space,
            "up" => Key::ArrowUp,
            "down" => Key::ArrowDown,
            "left" => Key::ArrowLeft,
            "right" => Key::ArrowRight,
            "backspace" => Key::Backspace,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub logo: bool,
}

impl Modifiers {
    fn to_json(self) -> Value {
        json!({
            "shift": self.shift,
            "control": self.control,
            "alt": self.alt,
            "logo": self.logo,
        })
    }
}

/// Outbound messages (harness → app).
pub enum Msg {
    Tick,
    MouseDown { x: f64, y: f64, time: f64, modifiers: Modifiers },
    MouseUp { x: f64, y: f64, time: f64, modifiers: Modifiers },
    MouseMove { x: f64, y: f64, time: f64, modifiers: Modifiers },
    Scroll { x: f64, y: f64, sx: f64, sy: f64, time: f64 },
    KeyDown { key: Key, time: f64, modifiers: Modifiers },
    KeyUp { key: Key, time: f64, modifiers: Modifiers },
    TextInput { input: String },
    Screenshot { request_id: u64 },
    WidgetSnapshot { request_id: u64 },
    WidgetTreeDump { request_id: u64 },
    WindowGeomChange { window_id: usize, dpi_factor: f64, left: f64, top: f64, width: f64, height: f64 },
    Kill,
}

impl Msg {
    pub fn to_line(&self) -> String {
        let v = match self {
            Msg::Tick => json!({ "Tick": [] }),
            Msg::MouseDown { x, y, time, modifiers } => json!({
                "MouseDown": [{
                    "button_raw_bits": MOUSE_PRIMARY,
                    "x": x, "y": y, "time": time,
                    "modifiers": modifiers.to_json(),
                }]
            }),
            Msg::MouseUp { x, y, time, modifiers } => json!({
                "MouseUp": [{
                    "button_raw_bits": MOUSE_PRIMARY,
                    "x": x, "y": y, "time": time,
                    "modifiers": modifiers.to_json(),
                }]
            }),
            Msg::MouseMove { x, y, time, modifiers } => json!({
                "MouseMove": [{
                    "time": time, "x": x, "y": y,
                    "modifiers": modifiers.to_json(),
                }]
            }),
            Msg::Scroll { x, y, sx, sy, time } => json!({
                "Scroll": [{
                    "time": time, "sx": sx, "sy": sy, "x": x, "y": y,
                    "is_mouse": true,
                    "modifiers": Modifiers::default().to_json(),
                }]
            }),
            Msg::KeyDown { key, time, modifiers } => json!({
                "KeyDown": [{
                    "key_code": key.wire_index(),
                    "is_repeat": false,
                    "modifiers": modifiers.to_json(),
                    "time": time,
                }]
            }),
            Msg::KeyUp { key, time, modifiers } => json!({
                "KeyUp": [{
                    "key_code": key.wire_index(),
                    "is_repeat": false,
                    "modifiers": modifiers.to_json(),
                    "time": time,
                }]
            }),
            Msg::TextInput { input } => json!({
                "TextInput": [{
                    "input": input,
                    "replace_last": false,
                    "was_paste": false,
                }]
            }),
            Msg::Screenshot { request_id } => json!({
                "Screenshot": [{ "request_id": request_id, "kind_id": 0 }]
            }),
            Msg::WidgetSnapshot { request_id } => json!({
                "WidgetSnapshot": [{ "request_id": request_id }]
            }),
            Msg::WidgetTreeDump { request_id } => json!({
                "WidgetTreeDump": [{ "request_id": request_id }]
            }),
            Msg::WindowGeomChange { window_id, dpi_factor, left, top, width, height } => json!({
                "WindowGeomChange": {
                    "dpi_factor": dpi_factor,
                    "window_id": window_id,
                    "left": left, "top": top,
                    "width": width, "height": height,
                }
            }),
            Msg::Kill => json!({ "Kill": [] }),
        };
        let mut s = v.to_string();
        s.push('\n');
        s
    }
}

/// `makepad_micro_serde` emits a trailing comma whenever the last field of a
/// struct is a `None` `Option` — its own `DeJson` accepts that, strict JSON
/// parsers do not. `WidgetSnapshot` ends in four optional fields, so nearly
/// every snapshot line needs this. Quoted strings are skipped so a comma
/// inside a label is never touched.
pub(crate) fn strip_trailing_commas(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            // Look ahead past whitespace: a comma right before a closer is the
            // artifact we are removing.
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// One widget as reported by makepad's `WidgetSnapshot`. This is the
/// accessibility-tree-equivalent the harness reasons over: real post-layout
/// geometry plus the semantics each widget chooses to expose.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct WidgetSnapshot {
    pub id: String,
    pub widget_type: String,
    pub window_id: String,
    #[serde(default)]
    pub window_index: usize,
    pub visible: bool,
    pub enabled: bool,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub checked: Option<bool>,
    #[serde(default)]
    pub selected: Option<String>,
}

impl WidgetSnapshot {
    pub fn right(&self) -> i64 {
        self.x + self.width
    }
    pub fn bottom(&self) -> i64 {
        self.y + self.height
    }
    /// Text the user can actually read, trimmed and normalised.
    pub fn label(&self) -> String {
        self.text.clone().unwrap_or_default().trim().to_string()
    }
}

/// Inbound messages (app → harness), decoded to just what we consume.
#[derive(Debug)]
pub enum Incoming {
    BeforeStartup,
    AfterStartup,
    RequestAnimationFrame,
    WidgetSnapshot { request_id: u64, widgets: Vec<WidgetSnapshot> },
    WidgetTreeDump { request_id: u64, dump: String },
    KeyFocusRect { x: Option<f64>, y: Option<f64>, width: Option<f64>, height: Option<f64> },
    Screenshot { request_ids: Vec<u64>, width: u32, height: u32, png: Vec<u8> },
    Log(String),
    ProtocolError(String),
    Other(String),
}

#[derive(Deserialize)]
struct SnapshotResponse {
    request_id: u64,
    widgets: Vec<WidgetSnapshot>,
}

#[derive(Deserialize)]
struct ScreenshotResponse {
    request_ids: Vec<u64>,
    width: u32,
    height: u32,
    png: Vec<u8>,
}

/// Decode one stdout line. Unknown variants collapse to `Other` — makepad emits
/// a lot of chatter (logs, cursor changes, flips) the harness does not care
/// about, and new variants must not break an audit run.
pub fn decode(line: &str) -> Option<Incoming> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let v: Value = match serde_json::from_str(&strip_trailing_commas(line)) {
        Ok(value) => value,
        Err(error) if line.starts_with('{') => return Some(Incoming::ProtocolError(format!("invalid Studio JSON: {error}"))),
        Err(_) => return Some(Incoming::Log(line.to_string())),
    };
    let obj = v.as_object()?;
    let (tag, body) = obj.iter().next()?;
    Some(match tag.as_str() {
        "BeforeStartup" => Incoming::BeforeStartup,
        "AfterStartup" => Incoming::AfterStartup,
        "RequestAnimationFrame" => Incoming::RequestAnimationFrame,
        "WidgetSnapshot" => {
            match serde_json::from_value::<SnapshotResponse>(body.get(0).cloned().unwrap_or(Value::Null)) {
                Ok(response) => Incoming::WidgetSnapshot { request_id: response.request_id, widgets: response.widgets },
                Err(error) => Incoming::ProtocolError(format!("invalid WidgetSnapshot: {error}")),
            }
        }
        "WidgetTreeDump" => {
            let inner = body.get(0)?;
            Incoming::WidgetTreeDump {
                request_id: inner.get("request_id").and_then(|v| v.as_u64()).unwrap_or(0),
                dump: inner.get("dump").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            }
        }
        "RunViewKeyFocusRect" => {
            let inner = body.get(0)?;
            let f = |k: &str| inner.get(k).and_then(|v| v.as_f64());
            Incoming::KeyFocusRect {
                x: f("x"),
                y: f("y"),
                width: f("width"),
                height: f("height"),
            }
        }
        "Screenshot" => {
            match serde_json::from_value::<ScreenshotResponse>(body.get(0).cloned().unwrap_or(Value::Null)) {
                Ok(response) => Incoming::Screenshot { request_ids: response.request_ids, width: response.width, height: response.height, png: response.png },
                Err(error) => Incoming::ProtocolError(format!("invalid Screenshot: {error}")),
            }
        }
        "LogItem" => Incoming::Log(body.to_string()),
        other => Incoming::Other(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_comma_cleanup_preserves_chinese() {
        let input = "{\"text\":\"项目二，选择 agent\",}";
        assert_eq!(strip_trailing_commas(input), "{\"text\":\"项目二，选择 agent\"}");
    }

    proptest::proptest! {
        #[test]
        fn prop_protocol_cleanup_preserves_text(text in ".{0,100}") {
            let encoded = serde_json::json!({"text":text}).to_string();
            proptest::prop_assert_eq!(strip_trailing_commas(&encoded), encoded);
        }
    }

    #[test]
    fn malformed_widget_snapshot_is_error() {
        let message = decode(r#"{"WidgetSnapshot":[{"request_id":17,"widgets":{"invalid":true}}]}"#);
        assert!(!matches!(message, Some(Incoming::WidgetSnapshot { .. })), "malformed data must not be accepted as an empty tree");
        assert!(matches!(message, Some(Incoming::ProtocolError(_))));
    }

    #[test]
    fn missing_snapshot_request_id_is_error() {
        let message = decode(r#"{"WidgetSnapshot":[{"widgets":[]}]}"#);
        assert!(!matches!(message, Some(Incoming::WidgetSnapshot { .. })), "missing request identity must not become zero");
        assert!(matches!(message, Some(Incoming::ProtocolError(_))));
    }

    #[test]
    fn valid_empty_widget_snapshot_is_preserved() {
        let message = decode(r#"{"WidgetSnapshot":[{"request_id":17,"widgets":[]}]}"#);
        assert!(matches!(message, Some(Incoming::WidgetSnapshot { request_id: 17, widgets }) if widgets.is_empty()));
    }
}
