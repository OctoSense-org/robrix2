//! Attach to an explicitly selected local Studio build. This transport does not
//! own that build and never stops, replaces or clears it on disconnect.

use std::net::{SocketAddr, TcpStream};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use makepad_micro_serde::{DeBin, DeJson, SerBin, SerJson};
use makepad_studio_protocol::{StudioToApp, StudioToAppVec, hub_protocol::{ClientId, QueryId, ClientToHub, ClientToHubEnvelope, HubToClient}};
use serde::Deserialize;
use tungstenite::{Message, WebSocket, HandshakeError};
use crate::{evidence::{Diagnostics, validate_capture, write_capture}, locator::{Selector, input_center, parse_path_query_rects, unique_match, unique_path_match}, proto::{Key, Modifiers, Msg, WidgetSnapshot, strip_trailing_commas}};

pub struct StudioDriver {
    socket: WebSocket<StudioStream>,
    client_id: ClientId,
    counter: u64,
    build_id: QueryId,
    diagnostics: Diagnostics,
    last_snapshot: Vec<WidgetSnapshot>,
    windowed: bool,
}

fn transport_wait(deadline: Instant, now: Instant) -> Option<Duration> {
    let remaining = deadline.saturating_duration_since(now);
    if remaining.is_zero() { None } else { Some(remaining.min(Duration::from_millis(50))) }
}

// tungstenite can perform several underlying reads in one handshake/read call.
// Apply the same absolute deadline to each one, including continuous fragments.
#[derive(Debug)]
struct StudioStream {
    stream: TcpStream,
    deadline: Instant,
}

impl StudioStream {
    fn wait(&self) -> io::Result<Duration> {
        transport_wait(self.deadline, Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::WouldBlock, "Studio phase deadline expired"))
    }

    fn normalize_wait<T>(result: io::Result<T>) -> io::Result<T> {
        result.map_err(|error| match error.kind() {
            // Platforms differ in the error used for a socket timeout. The
            // handshake keeps its buffered state only for WouldBlock.
            io::ErrorKind::TimedOut | io::ErrorKind::Interrupted => io::Error::new(io::ErrorKind::WouldBlock, error),
            _ => error,
        })
    }
}

impl Read for StudioStream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.wait()?))?;
        Self::normalize_wait(self.stream.read(bytes))
    }
}

impl Write for StudioStream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.stream.set_write_timeout(Some(self.wait()?))?;
            match Self::normalize_wait(self.stream.write(bytes)) {
                // WouldBlock reports no written bytes. Retry this TCP write,
                // retaining tungstenite's frame/cursor and the fixed deadline.
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.set_write_timeout(Some(self.wait()?))?;
        Self::normalize_wait(self.stream.flush())
    }
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
        let deadline = Instant::now() + Duration::from_secs(5);
        let stream = StudioStream { stream, deadline };
        let mut handshake = tungstenite::client(format!("ws://{address}/ui"), stream);
        let socket = loop {
            if transport_wait(deadline, Instant::now()).is_none() { return Err("Studio upgrade deadline expired".into()); }
            match handshake {
                Ok((socket, _)) => break socket,
                Err(HandshakeError::Interrupted(partial)) => handshake = partial.handshake(),
                Err(HandshakeError::Failure(error)) => return Err(format!("Studio upgrade: {error}")),
            }
        };
        let mut driver = Self { socket, client_id: ClientId(0), counter: 1, build_id: QueryId(build_id), diagnostics: Diagnostics::new(64 * 1024), last_snapshot: Vec::new(), windowed: false };
        match driver.receive(Instant::now() + Duration::from_secs(5), "Hello")? {
            HubToClient::Hello { client_id } => driver.client_id = client_id,
            _ => return Err("Studio did not start with Hello".into()),
        }
        Ok(driver)
    }

    fn receive(&mut self, deadline: Instant, phase: &str) -> Result<HubToClient, String> {
        self.socket.get_mut().deadline = deadline;
        loop {
            if transport_wait(deadline, Instant::now()).is_none() { return Err(format!("Studio {phase} deadline expired")); }
            let response = match self.socket.read() {
                Ok(response) => response,
                Err(tungstenite::Error::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error) => return Err(format!("Studio {phase} read: {error}")),
            };
            let message = match response {
                Message::Binary(bytes) => HubToClient::deserialize_bin(&bytes).map_err(|e| format!("Studio {phase}: invalid binary response: {e:?}"))?,
                Message::Text(text) => HubToClient::deserialize_json(&text).map_err(|e| format!("Studio {phase}: invalid JSON response: {e:?}"))?,
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => return Err(format!("Studio {phase}: connection closed")),
                _ => return Err(format!("Studio {phase}: unsupported websocket frame")),
            };
            if transport_wait(deadline, Instant::now()).is_none() { return Err(format!("Studio {phase} deadline expired")); }
            if let HubToClient::Error { message } = message { return Err(format!("Studio {phase}: {message}")); }
            return Ok(message);
        }
    }

    fn send(&mut self, msg: ClientToHub, deadline: Instant, phase: &str) -> Result<QueryId, String> {
        if transport_wait(deadline, Instant::now()).is_none() { return Err(format!("Studio {phase} deadline expired")); }
        self.socket.get_mut().deadline = deadline;
        let query_id = QueryId::new(self.client_id, self.counter);
        self.counter += 1;
        let envelope = ClientToHubEnvelope { query_id, msg };
        // A failed write may already have sent bytes. Report uncertainty and
        // never recreate or resubmit this command on another connection.
        self.socket.send(Message::Binary(envelope.serialize_bin().into())).map_err(|e| format!("Studio {phase} write: {e}"))?;
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
        let query = self.send(ClientToHub::WidgetSnapshot { build_id: self.build_id }, deadline, "command")?;
        loop {
            match self.receive(deadline, "command")? {
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

    fn path_query_until(&mut self, path: &[String], deadline: Instant) -> Result<Vec<String>, String> {
        let query_text = format!("path:{}", path.join("/"));
        let query_id = self.send(ClientToHub::WidgetQuery { build_id: self.build_id, query: query_text.clone() }, deadline, "path query")?;
        loop {
            match self.receive(deadline, "path query")? {
                HubToClient::WidgetQuery { query_id: response_id, build_id, query, rects }
                    if response_id == query_id && build_id == self.build_id && query == query_text => return Ok(rects),
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
        self.send(ClientToHub::RunViewInput { build_id: self.build_id, window_id: window, msg_bin: StudioToAppVec(vec![msg]).serialize_bin() }, Instant::now() + Duration::from_secs(5), "command")?;
        Ok(())
    }

    fn locate(&mut self, selector: &Selector, timeout_ms: u64) -> Result<WidgetSnapshot, String> {
        if !(1..=60_000).contains(&timeout_ms) { return Err("locator timeout must be 1..60000 ms".into()); }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let path_rows = if let Some(path) = &selector.path {
                Some(parse_path_query_rects(&self.path_query_until(path, deadline)?)?)
            } else {
                None
            };
            let widgets = self.snapshot_until(deadline)?;
            let result = if let Some(rows) = &path_rows {
                unique_path_match(&widgets, selector, rows, self.windowed)
            } else {
                unique_match(&widgets, selector)
            };
            match result {
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
        let deadline = Instant::now() + Duration::from_secs(25);
        let query = self.send(ClientToHub::Screenshot { build_id: self.build_id, kind_id: Some(0) }, deadline, "capture")?;
        loop {
            match self.receive(deadline, "capture")? {
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
    fn studio_transport_wait_boundary() {
        let start = Instant::now();
        let deadline = start + Duration::from_millis(80);
        assert_eq!(transport_wait(deadline, start), Some(Duration::from_millis(50)));
        assert_eq!(transport_wait(deadline, start + Duration::from_millis(79)), Some(Duration::from_millis(1)));
        assert_eq!(transport_wait(deadline, deadline), None);
        assert_eq!(transport_wait(deadline, deadline + Duration::from_millis(1)), None);
    }

    proptest::proptest! {
        #[test]
        fn prop_studio_transport_wait_never_extends_deadline(budget_ms in 0u64..60_001, elapsed_ms in 0u64..60_001) {
            let start = Instant::now();
            let result = transport_wait(start + Duration::from_millis(budget_ms), start + Duration::from_millis(elapsed_ms));
            if elapsed_ms >= budget_ms {
                proptest::prop_assert_eq!(result, None);
            } else {
                let wait = result.expect("a future deadline has a positive wait");
                proptest::prop_assert!(!wait.is_zero());
                proptest::prop_assert!(wait <= Duration::from_millis(50));
                proptest::prop_assert!(wait <= Duration::from_millis(budget_ms - elapsed_ms));
            }
        }
    }

    #[test]
    fn studio_transport_slow_command_write_keeps_one_frame() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let stream = transport_test_accept(&listener);
            let mut socket = tungstenite::accept(stream).unwrap();
            socket.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
            assert!(socket.get_mut().peek(&mut [0]).unwrap() > 0);
            std::thread::sleep(Duration::from_millis(250));
            let mut received = Vec::new();
            while let Ok(Message::Binary(bytes)) = socket.read() {
                let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
                let ClientToHub::RunViewInput { msg_bin, .. } = envelope.msg else { panic!("expected input"); };
                let input = StudioToAppVec::deserialize_bin(&msg_bin).unwrap();
                let [StudioToApp::TextInput(input)] = input.0.as_slice() else { panic!("expected one text input"); };
                assert!(input.input.bytes().all(|byte| byte == b'x'));
                received.push(input.input.len());
            }
            received
        });
        let mut driver = StudioDriver::connect(&address, 42).unwrap();
        let length = 8 * 1024 * 1024;
        let result = driver.input(0, Msg::TextInput { input: "x".repeat(length) });
        drop(driver);
        let received = server.join().unwrap();
        assert!(result.is_ok(), "a temporary write wait within the command budget must preserve the frame: {result:?}");
        assert_eq!(received, [length], "a partial write must neither truncate nor replay input");
    }

    fn transport_test_accept(listener: &TcpListener) -> TcpStream {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
                    stream.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
                    return stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("test Studio did not receive a connection: {error}"),
            }
        }
    }

    // Split the real server's HTTP response without reimplementing its
    // protocol or accept-key calculation. Only the first three writes delay.
    #[derive(Debug)]
    struct FragmentedUpgrade {
        stream: TcpStream,
        interval: Duration,
        started: Instant,
        writes: u32,
    }

    impl std::io::Read for FragmentedUpgrade {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            std::io::Read::read(&mut self.stream, bytes)
        }
    }

    impl std::io::Write for FragmentedUpgrade {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.writes < 3 {
                let due = self.started + self.interval * self.writes;
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
                self.writes += 1;
                return std::io::Write::write(&mut self.stream, &bytes[..bytes.len().min(50)]);
            }
            std::io::Write::write(&mut self.stream, bytes)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            std::io::Write::flush(&mut self.stream)
        }
    }

    fn fragmented_upgrade_probe(interval: Duration) -> (Result<(), String>, Duration) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let stream = transport_test_accept(&listener);
            let stream = FragmentedUpgrade { stream, interval, started: Instant::now(), writes: 0 };
            if let Ok(mut socket) = tungstenite::accept(stream) {
                let _hello = socket.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into()));
            }
            assert!(matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
                "resuming an upgrade must not open a replacement connection");
        });
        let started = Instant::now();
        let result = StudioDriver::connect(&address, 42).map(|_| ());
        let elapsed = started.elapsed();
        server.join().unwrap();
        (result, elapsed)
    }

    #[test]
    fn studio_transport_upgrade_progress_cannot_extend_deadline() {
        let (result, elapsed) = fragmented_upgrade_probe(Duration::from_secs(3));
        assert!(result.is_err(), "fragment progress must not extend the five-second upgrade budget; accepted after {elapsed:?}");
        let error = result.unwrap_err();
        assert!(error.contains("Studio upgrade deadline expired"), "wrong failing phase: {error}");
        assert!(elapsed < Duration::from_millis(5800), "upgrade exceeded its deadline tolerance: {elapsed:?}");
    }

    #[test]
    fn studio_transport_fragmented_upgrade_succeeds_on_original_connection() {
        let (result, elapsed) = fragmented_upgrade_probe(Duration::from_millis(150));
        assert!(result.is_ok(), "fragments inside the budget must retain handshake state: {result:?}");
        assert!(elapsed >= Duration::from_millis(250), "server must exercise delayed fragments");
        assert!(elapsed < Duration::from_secs(5));
    }

    #[test]
    fn studio_transport_hello_timeout_names_phase() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let stream = transport_test_accept(&listener);
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().is_err(), "a client waiting for Hello must not submit commands");
        });
        let started = Instant::now();
        let result = StudioDriver::connect(&address, 42).map(|_| ());
        let elapsed = started.elapsed();
        server.join().unwrap();
        let error = result.unwrap_err();
        assert!(error.contains("Studio Hello deadline expired"), "wrong failing phase: {error}");
        assert!(elapsed >= Duration::from_millis(4900) && elapsed < Duration::from_secs(6), "unexpected Hello budget: {elapsed:?}");
    }

    fn silent_response_server(listener: TcpListener, capture: bool) -> usize {
        let stream = transport_test_accept(&listener);
        let mut socket = tungstenite::accept(stream).unwrap();
        socket.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
        let mut requests = 0;
        while let Ok(Message::Binary(bytes)) = socket.read() {
            let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
            if capture {
                assert!(matches!(envelope.msg, ClientToHub::Screenshot { build_id: QueryId(42), kind_id: Some(0) }));
            } else {
                assert!(matches!(envelope.msg, ClientToHub::WidgetSnapshot { build_id: QueryId(42) }));
            }
            requests += 1;
        }
        requests
    }

    #[test]
    fn studio_transport_command_timeout_names_phase_without_replay() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || silent_response_server(listener, false));
        let mut driver = StudioDriver::connect(&address, 42).unwrap();
        let started = Instant::now();
        let error = driver.snapshot_until(started + Duration::from_millis(150)).unwrap_err();
        let elapsed = started.elapsed();
        drop(driver);
        let requests = server.join().unwrap();
        assert_eq!(requests, 1, "a response timeout must never resubmit the snapshot");
        assert!(error.contains("Studio command deadline expired"), "wrong failing phase: {error}");
        assert!(elapsed >= Duration::from_millis(140) && elapsed < Duration::from_secs(1), "unexpected command budget: {elapsed:?}");
    }

    #[test]
    fn studio_path_query_requires_exact_correlation_before_snapshot_join() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let stream = transport_test_accept(&listener);
            let mut socket = tungstenite::accept(stream).unwrap();
            socket.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
            let Message::Binary(bytes) = socket.read().unwrap() else { panic!("expected path query"); };
            let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
            let ClientToHub::WidgetQuery { build_id, query } = &envelope.msg else { panic!("expected native widget query"); };
            assert_eq!(*build_id, QueryId(42));
            assert_eq!(query, "path:info_button/inner_button");
            socket.send(Message::Binary(HubToClient::WidgetQuery {
                query_id: QueryId(envelope.query_id.0 + 1), build_id: QueryId(42), query: query.clone(), rects: vec!["wrong".into()],
            }.serialize_bin().into())).unwrap();
            socket.send(Message::Binary(HubToClient::WidgetQuery {
                query_id: envelope.query_id, build_id: QueryId(7), query: query.clone(), rects: vec!["wrong".into()],
            }.serialize_bin().into())).unwrap();
            socket.send(Message::Binary(HubToClient::WidgetQuery {
                query_id: envelope.query_id, build_id: QueryId(42), query: query.clone(), rects: vec!["220 inner_button Button 148 72 40 40".into()],
            }.serialize_bin().into())).unwrap();
        });
        let mut driver = StudioDriver::connect(&address, 42).unwrap();
        let rows = driver.path_query_until(&["info_button".into(), "inner_button".into()], Instant::now() + Duration::from_secs(1)).unwrap();
        assert_eq!(rows, ["220 inner_button Button 148 72 40 40"]);
        drop(driver);
        server.join().unwrap();
    }

    #[test]
    fn studio_transport_command_fragments_cannot_extend_deadline() {
        #[derive(Debug)]
        struct FragmentedResponse { stream: TcpStream, remaining: usize }
        impl std::io::Read for FragmentedResponse {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                std::io::Read::read(&mut self.stream, bytes)
            }
        }
        impl std::io::Write for FragmentedResponse {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.remaining > 0 {
                    self.remaining -= 1;
                    std::thread::sleep(Duration::from_millis(20));
                    return std::io::Write::write(&mut self.stream, &bytes[..1]);
                }
                std::io::Write::write(&mut self.stream, bytes)
            }
            fn flush(&mut self) -> std::io::Result<()> { std::io::Write::flush(&mut self.stream) }
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let stream = transport_test_accept(&listener);
            let mut socket = tungstenite::accept(FragmentedResponse { stream, remaining: 0 }).unwrap();
            socket.send(Message::Binary(HubToClient::Hello { client_id: ClientId(3) }.serialize_bin().into())).unwrap();
            let Message::Binary(bytes) = socket.read().unwrap() else { panic!("expected snapshot request"); };
            let envelope = ClientToHubEnvelope::deserialize_bin(&bytes).unwrap();
            assert!(matches!(envelope.msg, ClientToHub::WidgetSnapshot { build_id: QueryId(42) }));
            socket.get_mut().remaining = 20;
            let reply = HubToClient::WidgetSnapshot { query_id: envelope.query_id, build_id: QueryId(42), widgets: vec![] };
            let _late_reply = socket.send(Message::Binary(reply.serialize_bin().into()));
        });
        let mut driver = StudioDriver::connect(&address, 42).unwrap();
        let started = Instant::now();
        let result = driver.snapshot_until(started + Duration::from_millis(150));
        let elapsed = started.elapsed();
        drop(driver);
        server.join().unwrap();
        assert!(result.is_err(), "continuous frame progress must not make a late snapshot successful: {elapsed:?}");
        assert!(elapsed < Duration::from_millis(300), "an outer receive-loop deadline is insufficient: {elapsed:?}");
    }

    #[test]
    fn studio_transport_capture_timeout_names_phase_without_replay() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let directory = std::env::temp_dir().join(format!("ux-studio-transport-capture-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let server = std::thread::spawn(move || silent_response_server(listener, true));
        let mut driver = StudioDriver::connect(&address, 42).unwrap();
        let started = Instant::now();
        let result = driver.capture(&directory, "absent");
        let elapsed = started.elapsed();
        drop(driver);
        let requests = server.join().unwrap();
        let entries = std::fs::read_dir(&directory).unwrap().count();
        std::fs::remove_dir(&directory).unwrap();
        assert_eq!(requests, 1, "a capture timeout must never submit another screenshot");
        assert_eq!(entries, 0, "a missing frame must not create success evidence");
        let error = result.unwrap_err();
        assert!(error.contains("Studio capture deadline expired"), "wrong failing phase: {error}");
        assert!(elapsed >= Duration::from_millis(24900) && elapsed < Duration::from_secs(27), "unexpected capture budget: {elapsed:?}");
    }

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
