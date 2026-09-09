//! Attach to an explicitly selected local Studio build. This transport does not
//! own that build and never stops, replaces or clears it on disconnect.

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use makepad_micro_serde::{DeBin, DeJson, SerBin, SerJson};
use makepad_studio_protocol::{StudioToApp, StudioToAppVec, hub_protocol::{ClientId, QueryId, ClientToHub, ClientToHubEnvelope, HubToClient}};
use serde::Deserialize;
use tungstenite::{Message, WebSocket};
use crate::{evidence::{Diagnostics, validate_capture, write_capture}, locator::{Selector, unique_match, input_center}, proto::{Key, Modifiers, Msg, WidgetSnapshot, strip_trailing_commas}};

pub struct StudioDriver {
    socket: WebSocket<TcpStream>,
    client_id: ClientId,
    counter: u64,
    build_id: QueryId,
    diagnostics: Diagnostics,
    last_snapshot: Vec<WidgetSnapshot>,
    windowed: bool,
}

fn snapshot_evidence(widgets: &[WidgetSnapshot]) -> Result<Vec<u8>, String> {
    let mut widgets = widgets.to_vec();
    for widget in &mut widgets {
        if widget.id.to_ascii_lowercase().contains("password") && widget.widget_type.contains("Input") {
            widget.text = Some("[redacted]".into());
            widget.value = Some("[redacted]".into());
        }
    }
    serde_json::to_vec_pretty(&widgets).map_err(|e| e.to_string())
}

impl StudioDriver {
    pub fn connect(address: &str, build_id: u64) -> Result<Self, String> {
        let address: SocketAddr = address.parse().map_err(|e| format!("Studio address must be IP:port: {e}"))?;
        if !address.ip().is_loopback() { return Err("Studio attachment requires a loopback address".into()); }
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).map_err(|e| format!("connect Studio: {e}"))?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
        let (socket, _) = tungstenite::client(format!("ws://{address}/ui"), stream).map_err(|e| format!("Studio handshake: {e}"))?;
        let mut driver = Self { socket, client_id: ClientId(0), counter: 1, build_id: QueryId(build_id), diagnostics: Diagnostics::new(64 * 1024), last_snapshot: Vec::new(), windowed: false };
        match driver.receive(Instant::now() + Duration::from_secs(5))? {
            HubToClient::Hello { client_id } => driver.client_id = client_id,
            _ => return Err("Studio did not start with Hello".into()),
        }
        Ok(driver)
    }

    fn receive(&mut self, deadline: Instant) -> Result<HubToClient, String> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { return Err("Studio response deadline expired".into()); }
            self.socket.get_mut().set_read_timeout(Some(remaining)).map_err(|e| e.to_string())?;
            let message = match self.socket.read().map_err(|e| format!("Studio read: {e}"))? {
                Message::Binary(bytes) => HubToClient::deserialize_bin(&bytes).map_err(|e| format!("invalid Studio binary response: {e:?}"))?,
                Message::Text(text) => HubToClient::deserialize_json(&text).map_err(|e| format!("invalid Studio JSON response: {e:?}"))?,
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => return Err("Studio connection closed".into()),
                _ => return Err("unsupported Studio websocket frame".into()),
            };
            if let HubToClient::Error { message } = message { return Err(format!("Studio: {message}")); }
            return Ok(message);
        }
    }

    fn send(&mut self, msg: ClientToHub) -> Result<QueryId, String> {
        let query_id = QueryId::new(self.client_id, self.counter);
        self.counter += 1;
        let envelope = ClientToHubEnvelope { query_id, msg };
        self.socket.send(Message::Binary(envelope.serialize_bin().into())).map_err(|e| format!("Studio write: {e}"))?;
        Ok(query_id)
    }

    fn record_unrelated(&mut self, message: HubToClient) {
        // A shared hub can send another build's widgets, files or frames.
        // Keep correlation metadata, never that build's content in our log.
        let line = match message {
            HubToClient::WidgetSnapshot { query_id, build_id, widgets } =>
                format!("ignored snapshot query={} build={} widgets={}", query_id.0, build_id.0, widgets.len()),
            HubToClient::Screenshot { query_id, build_id, width, height, .. } =>
                format!("ignored screenshot query={} build={} size={width}x{height}", query_id.0, build_id.0),
            HubToClient::BuildStopped { build_id, exit_code } =>
                format!("build {} stopped: {exit_code:?}", build_id.0),
            _ => "ignored unrelated Studio event".into(),
        };
        self.diagnostics.push(line);
    }

    fn snapshot_until(&mut self, deadline: Instant) -> Result<Vec<WidgetSnapshot>, String> {
        let query = self.send(ClientToHub::WidgetSnapshot { build_id: self.build_id })?;
        loop {
            match self.receive(deadline)? {
                HubToClient::WidgetSnapshot { query_id, build_id, widgets } if query_id == query && build_id == self.build_id => {
                    let json = strip_trailing_commas(&widgets.serialize_json());
                    let widgets: Vec<WidgetSnapshot> = serde_json::from_str(&json).map_err(|e| format!("invalid widget snapshot: {e}"))?;
                    self.last_snapshot = widgets.clone();
                    return Ok(widgets);
                }
                HubToClient::BuildStopped { build_id, exit_code } if build_id == self.build_id => return Err(format!("selected build stopped: {exit_code:?}")),
                other => self.record_unrelated(other),
            }
        }
    }

    fn input(&mut self, window: usize, msg: Msg) -> Result<(), String> {
        // This fork's hub ignores RunViewInput.window_id. Until it routes that
        // field, reject non-primary windows rather than misdirecting events.
        if window != 0 { return Err("this Studio revision cannot route input to a non-primary window".into()); }
        let msg = StudioToApp::deserialize_json(&msg.to_line()).map_err(|e| format!("incompatible input protocol: {e:?}"))?;
        self.send(ClientToHub::RunViewInput { build_id: self.build_id, window_id: window, msg_bin: StudioToAppVec(vec![msg]).serialize_bin() })?;
        Ok(())
    }

    fn locate(&mut self, selector: &Selector, timeout_ms: u64) -> Result<WidgetSnapshot, String> {
        if !(1..=60_000).contains(&timeout_ms) { return Err("locator timeout must be 1..60000 ms".into()); }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let widgets = self.snapshot_until(deadline)?;
            match unique_match(&widgets, selector) {
                Ok(widget) => return Ok(widget.clone()),
                Err(error) => {
                    if !error.starts_with("no visible match") || Instant::now() >= deadline { return Err(error); }
                    self.diagnostics.push(error);
                }
            }
            std::thread::sleep(Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())));
        }
    }

    fn click(&mut self, selector: &Selector) -> Result<(), String> {
        let widget = self.locate(selector, 10_000)?;
        if !widget.enabled { return Err(format!("widget {} is disabled", widget.id)); }
        let (x, y) = input_center(&self.last_snapshot, &widget, self.windowed)?;
        let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs_f64();
        self.input(widget.window_index, Msg::MouseMove { x, y, time, modifiers: Modifiers::default() })?;
        self.input(widget.window_index, Msg::MouseDown { x, y, time, modifiers: Modifiers::default() })?;
        self.input(widget.window_index, Msg::MouseUp { x, y, time, modifiers: Modifiers::default() })
    }

    fn scroll(&mut self, selector: &Selector, sx: f64, sy: f64) -> Result<(), String> {
        if !sx.is_finite() || !sy.is_finite() { return Err("scroll deltas must be finite".into()); }
        let widget = self.locate(selector, 10_000)?;
        if !widget.enabled { return Err(format!("widget {} is disabled", widget.id)); }
        let (x, y) = input_center(&self.last_snapshot, &widget, self.windowed)?;
        let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs_f64();
        self.input(widget.window_index, Msg::Scroll { x, y, sx, sy, time })
    }

    fn capture(&mut self, out: &Path, label: &str) -> Result<PathBuf, String> {
        let query = self.send(ClientToHub::Screenshot { build_id: self.build_id, kind_id: Some(0) })?;
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            match self.receive(deadline)? {
                HubToClient::Screenshot { query_id, build_id, path, width, height, .. } if query_id == query && build_id == self.build_id => {
                    let bytes = std::fs::read(&path).map_err(|e| format!("read correlated Studio capture: {e}"))?;
                    validate_capture(query.0, &[query_id.0], width, height, &bytes)?;
                    return write_capture(out, label, &bytes);
                }
                other => self.record_unrelated(other),
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Step {
    Snapshot,
    Click { selector: Selector, expect: Selector, #[serde(default = "default_step_timeout")] timeout_ms: u64 },
    Scroll { selector: Selector, sx: f64, sy: f64, expect: Selector, #[serde(default = "default_step_timeout")] timeout_ms: u64 },
    Type { text: String, expect: Selector, #[serde(default = "default_step_timeout")] timeout_ms: u64 },
    Key { key: String, #[serde(default)] logo: bool, #[serde(default)] shift: bool, expect: Selector, #[serde(default = "default_step_timeout")] timeout_ms: u64 },
    Wait { selector: Selector, timeout_ms: u64 },
    Capture { label: String },
}

fn default_step_timeout() -> u64 { 10_000 }

impl Step {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Click { expect, timeout_ms, .. } | Self::Scroll { expect, timeout_ms, .. }
            | Self::Type { expect, timeout_ms, .. } | Self::Key { expect, timeout_ms, .. }
            | Self::Wait { selector: expect, timeout_ms } => {
                expect.validate()?;
                if !(1..=60_000).contains(timeout_ms) { return Err("timeout must be 1..60000 ms".into()); }
            }
            _ => {}
        }
        match self {
            Self::Click { selector, .. } => selector.validate()?,
            Self::Scroll { selector, sx, sy, .. } => {
                selector.validate()?;
                if !sx.is_finite() || !sy.is_finite() { return Err("scroll deltas must be finite".into()); }
            }
            Self::Key { key, .. } => {
                if Key::parse(key).is_none() { return Err(format!("unsupported key: {key}")); }
            }
            Self::Capture { label } => {
                if label.is_empty() || !label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_') {
                    return Err("capture label must contain only letters, digits, '-' or '_'".into());
                }
            }
            _ => {}
        }
        Ok(())
    }
}

pub fn run_cli(args: &[String]) -> Result<(), String> {
    let mut address = "127.0.0.1:18001".to_string();
    let mut build = None;
    let mut steps = None;
    let mut windowed = None;
    let mut out = PathBuf::from("target/ux-studio");
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).ok_or_else(|| format!("missing value for {}", args[i]))?;
        match args[i].as_str() {
            "--address" => address = value.clone(),
            "--app-mode" => windowed = Some(match value.as_str() { "windowed" => true, "embedded" => false, _ => return Err("--app-mode must be windowed or embedded".into()) }),
            "--build-id" => build = Some(value.parse().map_err(|_| "invalid build ID")?),
            "--steps" => steps = Some(PathBuf::from(value)),
            "--out" => out = PathBuf::from(value),
            other => return Err(format!("unknown Studio argument: {other}")),
        }
        i += 2;
    }
    let build = build.ok_or("Studio requires explicit --build-id")?;
    let windowed = windowed.ok_or("Studio requires --app-mode windowed|embedded for explicit coordinate semantics")?;
    let steps = steps.ok_or("Studio requires --steps <JSON file>")?;
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(steps).map_err(|e| e.to_string())?).map_err(|e| format!("invalid Studio steps: {e}"))?;
    if steps.is_empty() { return Err("Studio steps must not be empty".into()); }
    for (index, step) in steps.iter().enumerate() {
        step.validate().map_err(|e| format!("invalid Studio step {index}: {e}"))?;
    }
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir(&out).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            "Studio output directory already exists; choose a new path for this run".to_string()
        } else { format!("create Studio output directory: {e}") }
    })?;
    let mut driver = match StudioDriver::connect(&address, build) {
        Ok(driver) => driver,
        Err(error) => {
            std::fs::write(out.join("failure.txt"), &error)
                .map_err(|e| format!("{error}; evidence write failed: {e}"))?;
            return Err(error);
        }
    };
    driver.windowed = windowed;
    let result = (|| {
        for (index, step) in steps.into_iter().enumerate() {
            let start = Instant::now();
            match step {
                Step::Snapshot => { driver.snapshot_until(Instant::now() + Duration::from_secs(10))?; }
                Step::Click { selector, expect, timeout_ms } => {
                    driver.click(&selector)?;
                    driver.locate(&expect, timeout_ms)?;
                }
                Step::Scroll { selector, sx, sy, expect, timeout_ms } => {
                    driver.scroll(&selector, sx, sy)?;
                    driver.locate(&expect, timeout_ms)?;
                }
                Step::Type { text, expect, timeout_ms } => {
                    driver.input(0, Msg::TextInput { input: text })?;
                    driver.locate(&expect, timeout_ms)?;
                }
                Step::Key { key, logo, shift, expect, timeout_ms } => {
                    let key = Key::parse(&key).ok_or_else(|| format!("unsupported key: {key}"))?;
                    let modifiers = Modifiers { logo, shift, ..Default::default() };
                    driver.input(0, Msg::KeyDown { key, time: 0.0, modifiers })?;
                    driver.input(0, Msg::KeyUp { key, time: 0.0, modifiers })?;
                    driver.locate(&expect, timeout_ms)?;
                }
                Step::Wait { selector, timeout_ms } => { driver.locate(&selector, timeout_ms)?; }
                Step::Capture { label } => {
                    driver.capture(&out, &label)?;
                    driver.snapshot_until(Instant::now() + Duration::from_secs(10))?;
                }
            }
            // Keep the exact correlated snapshot that satisfied the expected
            // state. A later frame could already have moved beyond that state.
            let widgets = &driver.last_snapshot;
            let path = out.join(format!("step-{index:03}.json"));
            std::fs::write(path, snapshot_evidence(widgets)?).map_err(|e| e.to_string())?;
            eprintln!("Studio step {index}: observed {} widgets in {} ms", widgets.len(), start.elapsed().as_millis());
        }
        Ok(())
    })();
    std::fs::write(out.join("studio.log"), driver.diagnostics.text()).map_err(|e| e.to_string())?;
    if let Err(error) = &result {
        std::fs::write(out.join("failure.txt"), error).map_err(|e| format!("{error}; evidence write failed: {e}"))?;
        std::fs::write(out.join("last-snapshot.json"), snapshot_evidence(&driver.last_snapshot)?).map_err(|e| e.to_string())?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn studio_mutations_require_explicit_postconditions() {
        for json in [
            r#"{"action":"click","selector":{"id":"send"}}"#,
            r#"{"action":"scroll","selector":{"id":"threads_list"},"sx":0,"sy":420}"#,
            r#"{"action":"type","text":"hello"}"#,
            r#"{"action":"key","key":"Return"}"#,
        ] {
            assert!(serde_json::from_str::<Step>(json).is_err(),
                "a mutation without an observable postcondition must be rejected: {json}");
        }
    }

    #[test]
    fn studio_validates_the_complete_plan_before_any_input() {
        let directory = std::env::temp_dir().join(format!("ux-studio-invalid-plan-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        for invalid in [
            r#"{"action":"type","text":"hello","expect":{"id":"composer"},"timeout_ms":0}"#,
            r#"{"action":"click","selector":{"id":"send"},"expect":{"id":"sent"},"timeout_ms":60001}"#,
            r#"{"action":"type","text":"hello","expect":{}}"#,
            r#"{"action":"click","selector":{"id":"send","within":{}},"expect":{"id":"sent"}}"#,
            r#"{"action":"key","key":"unsupported-key","expect":{"id":"sent"}}"#,
        ] {
            let steps = directory.join("steps.json");
            std::fs::write(&steps, format!(r#"[{{"action":"type","text":"first","expect":{{"id":"composer","value":"first"}}}},{invalid}]"#)).unwrap();
            let error = run_cli(&["--address".into(), "invalid-address".into(), "--build-id".into(), "42".into(),
                "--app-mode".into(), "windowed".into(), "--steps".into(), steps.display().to_string(),
                "--out".into(), directory.join("output").display().to_string()]).unwrap_err();
            assert!(error.starts_with("invalid Studio step 1:"), "validate later invalid step before connecting or sending the first input: {error}");
            assert!(!directory.join("output").exists());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn studio_rejects_existing_output_without_touching_its_evidence() {
        let directory = std::env::temp_dir().join(format!("ux-studio-existing-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let steps = directory.join("steps.json");
        std::fs::write(&steps, r#"[{"action":"snapshot"}]"#).unwrap();
        let old_failure = directory.join("failure.txt");
        std::fs::write(&old_failure, "previous run failure").unwrap();
        let result = run_cli(&["--address".into(), "invalid-address".into(), "--build-id".into(), "42".into(),
            "--app-mode".into(), "windowed".into(), "--steps".into(), steps.display().to_string(),
            "--out".into(), directory.display().to_string()]);
        let error = result.unwrap_err();
        assert!(error.contains("output directory already exists"), "must reject output before connecting: {error}");
        assert_eq!(std::fs::read_to_string(&old_failure).unwrap(), "previous run failure");
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn postcondition_probe(delayed: bool) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let directory = std::env::temp_dir().join(format!("ux-studio-postcondition-{}-{delayed}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let steps = directory.join("steps.json");
        std::fs::write(&steps, r#"[{"action":"type","text":"new value","expect":{"id":"composer","value":"new value"},"timeout_ms":1000}]"#).unwrap();
        let server = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                    Err(error) => panic!("no Studio client: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
            let (mut inputs, mut snapshots) = (0, 0);
            while let Ok(Message::Binary(bytes)) = ws.read() {
                let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
                match envelope.msg {
                    ClientToHub::RunViewInput { build_id, msg_bin, .. } => {
                        assert_eq!(build_id, QueryId(42));
                        let events = StudioToAppVec::deserialize_bin(&msg_bin).unwrap();
                        assert!(matches!(events.0.as_slice(), [StudioToApp::TextInput(event)] if event.input == "new value"));
                        inputs += 1;
                    }
                    ClientToHub::WidgetSnapshot { build_id } => {
                        assert_eq!(build_id, QueryId(42));
                        snapshots += 1;
                        let value = if delayed && snapshots >= 3 { "new value" } else { "old value" };
                        let widgets = vec![makepad_studio_protocol::WidgetSnapshot {
                            id: "composer".into(), widget_type: "TextInput".into(), value: Some(value.into()),
                            visible: true, enabled: true, width: 200, height: 40, ..Default::default()
                        }];
                        if ws.send(Message::Binary(HubToClient::WidgetSnapshot { query_id: envelope.query_id, build_id, widgets }.serialize_bin().into())).is_err() { break; }
                    }
                    _ => panic!("unexpected Studio request"),
                }
            }
            (inputs, snapshots)
        });
        let started = Instant::now();
        let result = run_cli(&["--address".into(), address, "--build-id".into(), "42".into(),
            "--app-mode".into(), "windowed".into(), "--steps".into(), steps.display().to_string(),
            "--out".into(), directory.join("output").display().to_string()]);
        let (inputs, snapshots) = server.join().unwrap();
        assert_eq!(inputs, 1, "polling a postcondition must never replay input");
        if delayed {
            assert!(result.is_ok(), "delayed expected state must pass: {result:?}");
            let widgets: Vec<WidgetSnapshot> = serde_json::from_slice(&std::fs::read(directory.join("output/step-000.json")).unwrap()).unwrap();
            assert_eq!(widgets[0].value.as_deref(), Some("new value"));
            assert!(snapshots >= 3);
        } else {
            assert!(result.is_err(), "ignored input must not pass on an unchanged snapshot");
            assert!(started.elapsed() < Duration::from_secs(5), "postcondition must have a bounded deadline");
            assert!(directory.join("output/failure.txt").is_file());
            assert!(directory.join("output/last-snapshot.json").is_file());
            assert!(!directory.join("output/step-000.json").exists());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn studio_ignored_input_cannot_pass_its_postcondition() { postcondition_probe(false); }

    #[test]
    fn studio_waits_for_delayed_postcondition_without_replaying_input() { postcondition_probe(true); }

    #[test]
    fn studio_scroll_targets_observed_container_and_records_new_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let directory = std::env::temp_dir().join(format!("ux-studio-scroll-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let steps = directory.join("steps.json");
        std::fs::write(&steps, r#"[{"action":"scroll","selector":{"id":"threads_list"},"sx":0,"sy":420,"expect":{"id":"threads_list","text":"new thread"}}]"#).unwrap();
        let server = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                    Err(error) => panic!("no Studio client: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
            for scrolled in [false, true] {
                let Message::Binary(bytes) = ws.read().unwrap() else { panic!("expected snapshot request"); };
                let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
                assert!(matches!(envelope.msg, ClientToHub::WidgetSnapshot { build_id: QueryId(42) }));
                let widgets = vec![
                    makepad_studio_protocol::WidgetSnapshot { widget_type: "Window".into(), visible: true, x: 100, y: 200, width: 800, height: 600, ..Default::default() },
                    makepad_studio_protocol::WidgetSnapshot { id: "threads_list".into(), visible: true, enabled: true, x: 600, y: 250, width: 200, height: 400, text: Some(if scrolled { "new thread" } else { "old thread" }.into()), ..Default::default() },
                ];
                ws.send(Message::Binary(HubToClient::WidgetSnapshot { query_id: envelope.query_id, build_id: QueryId(42), widgets }.serialize_bin().into())).unwrap();
                if !scrolled {
                    let Message::Binary(bytes) = ws.read().unwrap() else { panic!("expected scroll input"); };
                    let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
                    let ClientToHub::RunViewInput { build_id, window_id, msg_bin } = envelope.msg else { panic!("expected RunViewInput"); };
                    assert_eq!(build_id, QueryId(42));
                    assert_eq!(window_id, 0);
                    let events = StudioToAppVec::deserialize_bin(&msg_bin).unwrap();
                    assert!(matches!(events.0.as_slice(), [StudioToApp::Scroll(event)] if event.x == 600.0 && event.y == 250.0 && event.sx == 0.0 && event.sy == 420.0));
                }
            }
        });
        let result = run_cli(&["--address".into(), address, "--build-id".into(), "42".into(),
            "--app-mode".into(), "windowed".into(), "--steps".into(), steps.display().to_string(),
            "--out".into(), directory.join("output").display().to_string()]);
        assert!(result.is_ok(), "scroll must be supported: {result:?}");
        server.join().unwrap();
        let snapshot = std::fs::read_to_string(directory.join("output/step-000.json")).unwrap();
        assert!(snapshot.contains("new thread"));
        assert!(!snapshot.contains("old thread"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn studio_connection_failure_preserves_cli_evidence() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let directory = std::env::temp_dir().join(format!("ux-studio-connect-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let steps = directory.join("steps.json");
        std::fs::write(&steps, r#"[{"action":"snapshot"}]"#).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.close(None).unwrap();
        });
        let args = vec!["--address".into(), address, "--build-id".into(), "42".into(),
            "--app-mode".into(), "windowed".into(), "--steps".into(), steps.display().to_string(),
            "--out".into(), directory.join("output").display().to_string()];
        let error = run_cli(&args).unwrap_err();
        server.join().unwrap();
        assert_eq!(std::fs::read_to_string(directory.join("output/failure.txt")).ok().as_deref(), Some(error.as_str()),
            "connection failures must remain reviewable in the output directory");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn studio_snapshot_requires_matching_build_and_query() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
            let Message::Binary(bytes) = ws.read().unwrap() else { panic!("expected binary request"); };
            let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
            assert_eq!(envelope.query_id.client_id(), ClientId(3));
            assert!(matches!(envelope.msg, ClientToHub::WidgetSnapshot { build_id: QueryId(42) }));
            for (query_id, build_id, label) in [
                (QueryId(envelope.query_id.0 + 16), QueryId(42), "stale request"),
                (envelope.query_id, QueryId(99), "wrong build"),
                (envelope.query_id, QueryId(42), "项目二"),
            ] {
                let widgets = vec![makepad_studio_protocol::WidgetSnapshot { text: Some(label.into()), ..Default::default() }];
                let reply = HubToClient::WidgetSnapshot { query_id, build_id, widgets };
                ws.send(Message::Binary(reply.serialize_bin().into())).unwrap();
            }
        });
        let mut driver = StudioDriver::connect(&address, 42).unwrap();
        let snapshot = driver.snapshot_until(Instant::now() + Duration::from_secs(5)).unwrap();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].text.as_deref(), Some("项目二"));
        assert!(!driver.diagnostics.text().contains("wrong build"), "foreign widget text must not leak into diagnostics");
        server.join().unwrap();
    }

    #[test]
    fn studio_disconnect_is_error_not_empty_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
            ws.read().unwrap();
            ws.close(None).unwrap();
        });
        let mut driver = StudioDriver::connect(&address, 42).unwrap();
        assert!(driver.snapshot_until(Instant::now() + Duration::from_secs(5)).is_err());
        server.join().unwrap();
    }

    #[test]
    fn studio_input_uses_the_apps_vector_envelope() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
            let Message::Binary(bytes) = ws.read().unwrap() else { panic!("expected binary input"); };
            let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
            let ClientToHub::RunViewInput { build_id, window_id, msg_bin } = envelope.msg else { panic!("expected RunViewInput"); };
            assert_eq!(build_id, QueryId(42));
            assert_eq!(window_id, 0);
            let events = StudioToAppVec::deserialize_bin(&msg_bin).expect("application decodes event vectors");
            assert!(matches!(events.0.as_slice(), [StudioToApp::TextInput(event)] if event.input == "项目二"));
        });
        let mut driver = StudioDriver::connect(&address, 42).unwrap();
        driver.input(0, Msg::TextInput { input: "项目二".into() }).unwrap();
        server.join().unwrap();
    }
}
