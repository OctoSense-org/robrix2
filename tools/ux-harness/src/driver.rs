//! Spawns the headless app and drives it through makepad's stdin/stdout event
//! channel.
//!
//! The headless backend is *pull*-driven: it only advances timers, next-frames
//! and repaints when it receives a `Tick`. It answers with
//! `RequestAnimationFrame` whenever it still has work pending. So the driver's
//! job is to keep ticking until the app goes quiet, then take measurements —
//! that is what makes a run deterministic instead of a race against wall-clock.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::proto::{decode, Incoming, Key, Modifiers, Msg, WidgetSnapshot};
use crate::evidence::{Diagnostics, OwnedChild, validate_capture, write_capture};

pub struct Driver {
    child: OwnedChild,
    stdin: ChildStdin,
    rx: Receiver<Incoming>,
    pending: VecDeque<Incoming>,
    next_request_id: u64,
    /// Virtual clock handed to the app. Real elapsed time would make runs
    /// unreproducible; a fixed step makes every animation land identically.
    clock: f64,
    frames_dir: PathBuf,
    pub last_focus_rect: Option<(f64, f64, f64, f64)>,
    diagnostics: Arc<Mutex<Diagnostics>>,
    /// Set UX_HARNESS_TRACE=1 to mirror the whole protocol exchange to stderr.
    trace: bool,
}

const TICK_STEP: f64 = 1.0 / 60.0;
/// How long a Tick may go unanswered before the UI counts as idle. Generous
/// because the headless backend rasterises on the CPU.
const IDLE_WINDOW: Duration = Duration::from_millis(2500);
/// Wall-clock ceiling for any single settle, so a permanently-animating UI
/// (blinking caret) cannot stall a run.
const SETTLE_BUDGET: Duration = Duration::from_secs(8);

impl Driver {
    /// Launch `app_bin` in headless stdin-loop mode with frames landing in
    /// `frames_dir`.
    pub fn launch(app_bin: &Path, frames_dir: &Path, extra_env: &[(String, String)]) -> Result<Driver, String> {
        std::fs::create_dir_all(frames_dir)
            .map_err(|e| format!("cannot create frames dir {}: {e}", frames_dir.display()))?;

        // The headless rasteriser defaults to at most 4 threads to stay polite
        // on a developer's machine. An audit is a batch job — every frame is
        // something the harness is blocked on — so give it the box.
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).max(1))
            .unwrap_or(4);

        let mut cmd = Command::new(app_bin);
        cmd.arg("--stdin-loop")
            .env("MAKEPAD_HEADLESS_OUT_DIR", frames_dir)
            .env("MAKEPAD_STDIN_LOOP", "1")
            .env("MAKEPAD_HEADLESS_THREADS", threads.to_string())
            // Keep the audit off the developer's real Matrix session/state.
            .env("RUST_BACKTRACE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in extra_env {
            cmd.env(k, v);
        }

        let mut child = OwnedChild(cmd
            .spawn()
            .map_err(|e| format!("cannot spawn {}: {e}", app_bin.display()))?);

        let stdin = child.stdin.take().ok_or("no stdin on child")?;
        let stdout = child.stdout.take().ok_or("no stdout on child")?;
        let stderr = child.stderr.take().ok_or("no stderr on child")?;

        let trace = std::env::var("UX_HARNESS_TRACE").is_ok();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if trace {
                    eprintln!("← {}", line.chars().take(220).collect::<String>());
                }
                if let Some(msg) = decode(&line) {
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            }
        });
        // Drain stderr into a bounded tail, including errors outside the protocol.
        let diagnostics = Arc::new(Mutex::new(Diagnostics::new(64 * 1024)));
        let stderr_diagnostics = diagnostics.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if trace {
                    eprintln!("!! {line}");
                }
                if let Ok(mut log) = stderr_diagnostics.lock() {
                    log.push(format!("stderr: {line}"));
                }
            }
        });

        Ok(Driver {
            child,
            stdin,
            rx,
            pending: VecDeque::new(),
            next_request_id: 1,
            clock: 0.0,
            frames_dir: frames_dir.to_path_buf(),
            last_focus_rect: None,
            diagnostics,
            trace: std::env::var("UX_HARNESS_TRACE").is_ok(),
        })
    }

    fn record_log(&self, line: String) {
        if let Ok(mut log) = self.diagnostics.lock() { log.push(line); }
    }

    pub fn diagnostic_text(&self) -> String {
        self.diagnostics.lock().map(|log| log.text()).unwrap_or_else(|_| "diagnostics lock poisoned".into())
    }

    fn send(&mut self, msg: Msg) -> Result<(), String> {
        let line = msg.to_line();
        if self.trace {
            if matches!(msg, Msg::TextInput { .. }) { eprintln!("→ TextInput [redacted]"); }
            else { eprintln!("→ {}", line.trim()); }
        }
        self.stdin
            .write_all(line.as_bytes())
            .map_err(|e| format!("write to app failed (did it crash?): {e}"))?;
        self.stdin.flush().map_err(|e| format!("flush failed: {e}"))
    }

    fn recv_until(&mut self, deadline: Instant) -> Result<Incoming, String> {
        if Instant::now() >= deadline { return Err("app response deadline expired".into()); }
        if let Some(msg) = self.pending.pop_front() {
            if let Incoming::ProtocolError(error) = msg { return Err(error); }
            return Ok(msg);
        }
        match self.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(msg) => {
                if let Incoming::ProtocolError(error) = msg { return Err(error); }
                if let Incoming::KeyFocusRect { x, y, width, height } = &msg {
                    self.last_focus_rect = match (x, y, width, height) {
                        (Some(x), Some(y), Some(w), Some(h)) => Some((*x, *y, *w, *h)),
                        _ => None,
                    };
                }
                Ok(msg)
            }
            Err(RecvTimeoutError::Timeout) => Err("app response deadline expired".to_string()),
            Err(RecvTimeoutError::Disconnected) => Err("app exited".to_string()),
        }
    }

    /// Block until the app finishes its startup draw.
    pub fn wait_for_startup(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if Instant::now() > deadline {
                return Err("timed out waiting for AfterStartup".to_string());
            }
            match self.recv_until(deadline)? {
                Incoming::AfterStartup => return Ok(()),
                Incoming::Log(l) => self.record_log(l),
                _ => {}
            }
        }
    }

    /// Tick until the app stops asking for animation frames, or `max_ticks` is
    /// reached. This is the "let the UI settle" primitive every step uses.
    ///
    /// Returns the number of ticks actually spent, which doubles as a cheap
    /// *responsiveness* measurement: a control that settles in 2 ticks feels
    /// instant, one that churns for 200 does not.
    pub fn settle(&mut self, max_ticks: usize) -> Result<usize, String> {
        let mut ticks = 0;
        let mut quiet_rounds = 0;
        // A UI with a focused text field never truly idles — the caret blink
        // keeps asking for frames forever. Tick budgets alone would then let a
        // single settle run for minutes on the CPU rasteriser, so cap the wall
        // clock too and treat "still animating" as a normal outcome.
        let budget = Instant::now() + SETTLE_BUDGET;
        while ticks < max_ticks && Instant::now() < budget {
            self.clock += TICK_STEP;
            self.send(Msg::Tick)?;
            ticks += 1;

            // One Tick == at most one RequestAnimationFrame, and the app only
            // sends it once the frame is fully rasterised. Rendering a full
            // window on the CPU takes real time, so wait for that answer
            // instead of pipelining more Ticks — queueing them just makes the
            // app render frames nobody asked for.
            let mut wants_more = false;
            let deadline = (Instant::now() + IDLE_WINDOW).min(budget);
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match self.rx.recv_timeout(remaining) {
                    Ok(Incoming::RequestAnimationFrame) => {
                        wants_more = true;
                        break;
                    }
                    Ok(Incoming::Log(l)) => self.record_log(l),
                    Ok(Incoming::ProtocolError(error)) => return Err(error),
                    Ok(Incoming::KeyFocusRect { x, y, width, height }) => {
                        self.last_focus_rect = match (x, y, width, height) {
                            (Some(x), Some(y), Some(w), Some(h)) => Some((x, y, w, h)),
                            _ => None,
                        };
                    }
                    Ok(other) => self.pending.push_back(other),
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return Err("app exited".to_string()),
                }
            }
            if wants_more {
                quiet_rounds = 0;
            } else {
                quiet_rounds += 1;
                if quiet_rounds >= 2 {
                    break;
                }
            }
        }
        Ok(ticks)
    }

    pub fn set_viewport(&mut self, width: f64, height: f64, dpi: f64) -> Result<(), String> {
        self.send(Msg::WindowGeomChange {
            window_id: 0,
            dpi_factor: dpi,
            left: 0.0,
            top: 0.0,
            width,
            height,
        })?;
        self.settle(60)?;
        Ok(())
    }

    pub fn mouse_move(&mut self, x: f64, y: f64) -> Result<(), String> {
        let time = self.clock;
        self.send(Msg::MouseMove { x, y, time, modifiers: Modifiers::default() })?;
        self.settle(60)?;
        Ok(())
    }

    /// A full press/release at a point, returning how many ticks the UI needed
    /// to settle afterwards.
    pub fn click(&mut self, x: f64, y: f64) -> Result<usize, String> {
        let time = self.clock;
        self.send(Msg::MouseMove { x, y, time, modifiers: Modifiers::default() })?;
        self.send(Msg::MouseDown { x, y, time, modifiers: Modifiers::default() })?;
        self.send(Msg::MouseUp { x, y, time, modifiers: Modifiers::default() })?;
        self.settle(24)
    }

    pub fn key(&mut self, key: Key, modifiers: Modifiers) -> Result<usize, String> {
        let time = self.clock;
        self.send(Msg::KeyDown { key, time, modifiers })?;
        self.send(Msg::KeyUp { key, time, modifiers })?;
        self.settle(120)
    }

    pub fn type_text(&mut self, text: &str) -> Result<(), String> {
        self.send(Msg::TextInput { input: text.to_string() })?;
        self.settle(120)?;
        Ok(())
    }

    pub fn scroll(&mut self, x: f64, y: f64, sx: f64, sy: f64) -> Result<usize, String> {
        let time = self.clock;
        self.send(Msg::Scroll { x, y, sx, sy, time })?;
        self.settle(60)
    }

    /// Ask the app for its post-layout widget tree.
    pub fn widget_snapshot(&mut self) -> Result<Vec<WidgetSnapshot>, String> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send(Msg::WidgetSnapshot { request_id: id })?;

        // The app answers this straight out of its message loop — no Tick
        // needed — so just wait for it.
        let mut found = None;
        let mut keep = VecDeque::new();
        while let Some(msg) = self.pending.pop_front() {
            match msg {
                Incoming::WidgetSnapshot { request_id, widgets } if request_id == id => {
                    found = Some(widgets);
                }
                other => keep.push_back(other),
            }
        }
        self.pending = keep;
        if let Some(widgets) = found {
            return Ok(widgets);
        }

        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("no WidgetSnapshot response within 180s".to_string());
            }
            match self.rx.recv_timeout(remaining) {
                Ok(Incoming::WidgetSnapshot { request_id, widgets }) if request_id == id => {
                    return Ok(widgets)
                }
                Ok(Incoming::Log(l)) => self.record_log(l),
                Ok(Incoming::ProtocolError(error)) => return Err(error),
                Ok(other) => self.pending.push_back(other),
                Err(RecvTimeoutError::Timeout) => {
                    return Err("no WidgetSnapshot response within 180s".to_string())
                }
                Err(RecvTimeoutError::Disconnected) => return Err("app exited".to_string()),
            }
        }
    }

    /// Save only the PNG returned for this request; old on-disk frames are not evidence.
    pub fn capture(&mut self, label: &str) -> Result<Option<PathBuf>, String> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        let deadline = Instant::now() + Duration::from_secs(25);
        self.send(Msg::Screenshot { request_id: id })?;
        self.settle(120)?;
        while Instant::now() < deadline {
            match self.recv_until(deadline)? {
                Incoming::Screenshot { request_ids, width, height, png } if request_ids.contains(&id) => {
                    validate_capture(id, &request_ids, width, height, &png)?;
                    return write_capture(&self.frames_dir, label, &png).map(Some);
                }
                Incoming::Log(line) => self.record_log(line),
                _ => {}
            }
        }
        Err(format!("no screenshot response for request {id} before deadline"))
    }

    pub fn shutdown(mut self) {
        let _ = self.send(Msg::Kill);
        let _ = self.stdin.flush();
        thread::sleep(Duration::from_millis(200));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn test_driver(directory: &Path) -> (Driver, mpsc::Sender<Incoming>) {
        let mut child = OwnedChild(Command::new("/bin/cat")
            .stdin(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap());
        let stdin = child.stdin.take().unwrap();
        let (tx, rx) = mpsc::channel();
        (Driver { child, stdin, rx, pending: VecDeque::new(), next_request_id: 18,
            clock: 0.0, frames_dir: directory.into(), last_focus_rect: None,
            diagnostics: Arc::new(Mutex::new(Diagnostics::new(1024))), trace: false }, tx)
    }

    #[test]
    fn expired_response_deadline_rejects_queued_frame() {
        let (mut driver, _tx) = test_driver(Path::new("."));
        driver.pending.push_back(Incoming::Screenshot { request_ids: vec![18], width: 1, height: 1, png: vec![] });
        let result = driver.recv_until(Instant::now() - Duration::from_millis(1));
        assert!(result.is_err(), "an already queued frame must not defeat an expired deadline");
    }

    #[test]
    fn capture_deadline_never_reuses_old_file_or_response() {
        let directory = std::env::temp_dir().join(format!("ux-stale-capture-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let old_frame = directory.join("window_0_frame_1.png");
        std::fs::write(&old_frame, b"old frame must never become evidence").unwrap();
        let (mut driver, tx) = test_driver(&directory);
        tx.send(Incoming::Screenshot { request_ids: vec![17], width: 1, height: 1, png: vec![] }).unwrap();
        let result = driver.capture("deadline");
        assert!(result.is_err());
        assert!(!directory.join("scene_deadline.png").exists());
        assert_eq!(std::fs::read(&old_frame).unwrap(), b"old frame must never become evidence");
        drop(driver);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
