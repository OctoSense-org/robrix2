//! Failure evidence and ownership rules shared by UI transports.

use std::collections::VecDeque;
use std::io::Cursor;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::Child;

pub struct OwnedChild(pub Child);

impl Deref for OwnedChild {
    type Target = Child;
    fn deref(&self) -> &Child { &self.0 }
}

impl DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Child { &mut self.0 }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

pub struct Diagnostics {
    lines: VecDeque<String>,
    bytes: usize,
    capacity: usize,
}

impl Diagnostics {
    pub fn new(capacity: usize) -> Self {
        Self { lines: VecDeque::new(), bytes: 0, capacity }
    }

    pub fn push(&mut self, mut line: String) {
        if self.capacity == 0 { return; }
        if line.len() >= self.capacity {
            let mut start = line.len() - self.capacity + 1;
            while !line.is_char_boundary(start) { start += 1; }
            line = line[start..].to_string();
        }
        let size = line.len() + 1;
        while self.bytes + size > self.capacity {
            let Some(old) = self.lines.pop_front() else { break };
            self.bytes -= old.len() + 1;
        }
        self.bytes += size;
        self.lines.push_back(line);
    }

    pub fn len_bytes(&self) -> usize { self.bytes }
    pub fn text(&self) -> String { self.lines.iter().cloned().collect::<Vec<_>>().join("\n") }
}

pub fn validate_capture(request_id: u64, request_ids: &[u64], width: u32, height: u32, bytes: &[u8]) -> Result<(), String> {
    if !request_ids.contains(&request_id) {
        return Err(format!("stale screenshot: expected request {request_id}, received {request_ids:?}"));
    }
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|e| format!("invalid capture PNG: {e}"))?;
    if width == 0 || height == 0 || reader.info().width != width || reader.info().height != height {
        return Err("capture dimensions do not match the response".to_string());
    }
    let size = reader.output_buffer_size().filter(|size| *size <= 128 * 1024 * 1024)
        .ok_or("capture exceeds the 128 MiB decoded image limit")?;
    reader.next_frame(&mut vec![0; size]).map_err(|e| format!("incomplete capture PNG: {e}"))?;
    Ok(())
}

pub fn write_capture(directory: &Path, label: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    if label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err("capture label must contain only letters, digits, underscores or hyphens".into());
    }
    std::fs::create_dir_all(directory).map_err(|e| format!("create capture directory: {e}"))?;
    let path = directory.join(format!("scene_{label}.png"));
    let temporary = directory.join(format!(".scene_{label}.png.tmp"));
    std::fs::write(&temporary, bytes).map_err(|e| format!("write capture: {e}"))?;
    if let Err(e) = std::fs::rename(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("publish capture: {e}"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0, 0, 0, 255]).unwrap();
        }
        bytes
    }

    #[test]
    fn stale_capture_is_error() {
        assert!(validate_capture(18, &[17], 1, 1, &png()).is_err());
        assert!(validate_capture(18, &[18], 1, 1, &png()).is_ok());
        assert!(validate_capture(18, &[18], 2, 1, &png()).is_err());
    }

    proptest::proptest! {
        #[test]
        fn prop_capture_requires_matching_request(request in 0u64..1000, received in 0u64..1000) {
            proptest::prop_assert_eq!(validate_capture(request, &[received], 1, 1, &png()).is_ok(), request == received);
        }

        #[test]
        fn prop_diagnostics_storage_never_exceeds_capacity(capacity in 0usize..1000, lines in proptest::collection::vec(".{0,100}", 0..40)) {
            let mut diagnostics = Diagnostics::new(capacity);
            for line in lines {
                diagnostics.push(line);
                proptest::prop_assert!(diagnostics.len_bytes() <= capacity);
                proptest::prop_assert!(diagnostics.text().len() <= capacity);
            }
        }
    }

    #[test]
    fn capture_write_failure_is_error() {
        let path = std::env::temp_dir().join(format!("ux-capture-test-{}", std::process::id()));
        std::fs::write(&path, b"a file cannot be an output directory").unwrap();
        let result = write_capture(&path, "frame", &png());
        std::fs::remove_file(path).unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn stderr_diagnostics_are_bounded_and_retained() {
        let mut log = Diagnostics::new(128);
        for i in 0..100 { log.push(format!("stderr line {i}: {}", "测试".repeat(40))); }
        log.push("stderr: final failure".into());
        assert!(log.text().contains("stderr: final failure"));
        assert!(log.len_bytes() <= 128);
    }

    #[cfg(unix)]
    #[test]
    fn owned_child_is_reaped_on_early_return() {
        let child = std::process::Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        let owned = OwnedChild(child);
        drop(owned);
        let status = std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()]).stderr(std::process::Stdio::null()).status().unwrap();
        assert!(!status.success(), "owned child {pid} survived driver scope");
    }
}
