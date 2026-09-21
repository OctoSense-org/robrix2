//! Offline HTML/CSS rendering, independent of Robrix, OctoSense and Makepad.
//!
//! Documents receive only resources already placed in [`ResourceMap`] by the host.
//! There is no HTTP client, file loader, navigation provider, JavaScript runtime,
//! clipboard, account, or credential API. Optional OS font discovery is separate
//! from document resource loading. This is not a process/CPU sandbox.

use anyrender::{PaintScene as _, render_to_buffer};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{DocumentConfig, util::Color};
use blitz_html::HtmlDocument;
use blitz_paint::paint_scene;
use blitz_traits::{
    net::{Body, Bytes, Method, NetHandler, NetProvider, Request},
    shell::{ColorScheme, Viewport},
};
use html5ever::tokenizer::{
    BufferQueue, Token, TokenSink, TokenSinkResult, Tokenizer, TagKind, states::RawKind,
};
use peniko::{Fill, kurbo::Rect};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::Cursor,
    sync::{Arc, Mutex},
};

#[cfg(feature = "makepad")]
pub mod makepad;

pub const BLITZ_REVISION: &str = "e99fbdbd1d03b9f0aa1622c3f810d95daac92042";
pub const ASSET_BASE_URL: &str = "https://article.invalid/assets/";
pub const MAX_HTML_BYTES: usize = 256 * 1024;
pub const MAX_RESOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_TOTAL_RESOURCE_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_RESOURCES: usize = 48;
pub const MAX_RESOURCE_REQUESTS: usize = 128;
pub const MAX_OUTPUT_PIXELS: u64 = 16 * 1024 * 1024;
pub const MAX_OUTPUT_DIMENSION: u32 = 16_384;
const MAX_TOKENS: usize = 16_384;
const MAX_DEPTH: usize = 64;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RenderError {
    #[error("article input exceeds {MAX_HTML_BYTES} bytes")]
    InputTooLarge,
    #[error("article exceeds token or nesting limits")]
    DocumentTooComplex,
    #[error("unsupported active/document element: {0}")]
    UnsupportedElement(String),
    #[error("invalid viewport: {0}")]
    InvalidViewport(&'static str),
    #[error("invalid resource identifier")]
    InvalidResourceId,
    #[error("resource count or byte budget exceeded")]
    ResourceBudget,
    #[error("invalid or oversized PNG/JPEG image")]
    InvalidImage,
    #[error("stylesheet must be UTF-8 and at most 64 KiB")]
    InvalidStylesheet,
    #[error("resource resolution did not settle within eight passes")]
    UnsettledResources,
    #[error("renderer returned invalid dimensions")]
    InvalidLayout,
}

/// Explicit grants are immutable byte snapshots, not URLs to be downloaded.
#[derive(Clone, Default)]
pub struct ResourceMap {
    entries: BTreeMap<String, Bytes>,
    bytes: usize,
}

impl ResourceMap {
    /// Grant a CSS stylesheet under a synthetic URL. Nested imports are subject
    /// to the same map and request budget, including when a sheet imports itself.
    pub fn insert_css(&mut self, id: &str, css: &str) -> Result<String, RenderError> {
        if css.len() > 64 * 1024 {
            return Err(RenderError::InvalidStylesheet);
        }
        self.insert(id, css.as_bytes().to_vec())
    }

    /// Grant a host-selected PNG/JPEG. Check dimensions and bounded decode before
    /// handing the data to Blitz's image decoder. No SVG or animated formats.
    pub fn insert_image(&mut self, id: &str, bytes: Vec<u8>) -> Result<String, RenderError> {
        if bytes.len() > MAX_RESOURCE_BYTES {
            return Err(RenderError::ResourceBudget);
        }
        let mut reader = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| RenderError::InvalidImage)?;
        if !matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
        ) {
            return Err(RenderError::InvalidImage);
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(MAX_IMAGE_PIXELS * 4);
        reader.limits(limits);
        reader.decode().map_err(|_| RenderError::InvalidImage)?;
        self.insert(id, bytes)
    }

    fn insert(&mut self, id: &str, bytes: Vec<u8>) -> Result<String, RenderError> {
        if id.is_empty()
            || id.len() > 96
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
            || id == "."
            || id == ".."
        {
            return Err(RenderError::InvalidResourceId);
        }
        let url = format!("{ASSET_BASE_URL}{id}");
        let old_len = self.entries.get(&url).map_or(0, Bytes::len);
        let total = self.bytes - old_len + bytes.len();
        if bytes.len() > MAX_RESOURCE_BYTES
            || total > MAX_TOTAL_RESOURCE_BYTES
            || (!self.entries.contains_key(&url) && self.entries.len() >= MAX_RESOURCES)
        {
            return Err(RenderError::ResourceBudget);
        }
        self.bytes = total;
        self.entries.insert(url.clone(), Bytes::from(bytes));
        Ok(url)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
    /// CSS pixel width, inclusive range 64..=2048.
    pub width_css: u32,
    /// Initial layout viewport height; controls vh units, not the article crop.
    pub viewport_height_css: u32,
    /// Output maximum height in CSS pixels. Longer articles report `clipped`.
    pub max_height_css: u32,
    /// Device pixels per CSS pixel, finite and in 0.5..=3.
    pub scale: f32,
    pub dark_mode: bool,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width_css: 390,
            viewport_height_css: 844,
            max_height_css: 8192,
            scale: 1.0,
            dark_mode: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceReport {
    pub requested: usize,
    pub served: usize,
    /// At most 128 URLs; each is truncated to 256 characters. Do not display
    /// document-provided URLs as trusted UI or include them in telemetry.
    pub denied: Vec<String>,
    pub budget_exhausted: bool,
}

/// An opaque, already-composited RGBA8 bitmap. No executable document is retained.
#[derive(Debug)]
pub struct RenderedArticle {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub css_content_height: f32,
    pub clipped: bool,
    pub resources: ResourceReport,
}
impl RenderedArticle {
    /// Makepad `TextureFormat::VecBGRAu8_32` pixels, numeric AARRGGBB.
    pub fn to_bgra_u32(&self) -> Vec<u32> {
        self.rgba
            .chunks_exact(4)
            .map(|p| {
                (u32::from(p[3]) << 24)
                    | (u32::from(p[0]) << 16)
                    | (u32::from(p[1]) << 8)
                    | u32::from(p[2])
            })
            .collect()
    }
}

struct MemoryProvider {
    grants: ResourceMap,
    report: Mutex<ResourceReport>,
}
impl NetProvider for MemoryProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.as_str().to_owned();
        let bytes = {
            let mut report = self.report.lock().unwrap();
            report.requested += 1;
            let in_budget = report.requested <= MAX_RESOURCE_REQUESTS;
            let granted = in_budget
                && request.method == Method::GET
                && matches!(request.body, Body::Empty)
                && request.headers.is_empty()
                && !request.signal.as_ref().is_some_and(|s| s.aborted());
            let bytes = granted
                .then(|| self.grants.entries.get(&url).cloned())
                .flatten();
            if bytes.is_some() {
                report.served += 1;
            } else if in_budget {
                report.denied.push(url.chars().take(256).collect());
            }
            report.budget_exhausted |= !in_budget;
            bytes.unwrap_or_default()
        };
        // Always complete the request, even for denial; otherwise a denied CSS
        // resource leaves Blitz permanently waiting for a critical stylesheet.
        // Never hold the report mutex here: CSS @import invokes fetch recursively.
        handler.bytes(url, bytes);
    }
}

#[derive(Default)]
struct PreflightState {
    tokens: usize,
    stack: Vec<String>,
    error: Option<RenderError>,
}
#[derive(Default)]
struct Preflight(RefCell<PreflightState>);
impl TokenSink for Preflight {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut state = self.0.borrow_mut();
        if state.error.is_some() {
            return TokenSinkResult::Continue;
        }
        state.tokens += 1;
        if state.tokens > MAX_TOKENS {
            state.error = Some(RenderError::DocumentTooComplex);
            return TokenSinkResult::Continue;
        }
        if let Token::TagToken(tag) = token {
            let name = tag.name.as_ref();
            if tag.kind == TagKind::StartTag {
                if matches!(
                    name,
                    "script"
                        | "iframe"
                        | "frame"
                        | "frameset"
                        | "object"
                        | "embed"
                        | "form"
                        | "input"
                        | "button"
                        | "textarea"
                        | "select"
                        | "svg"
                        | "math"
                        | "base"
                        | "xml"
                ) {
                    state.error = Some(RenderError::UnsupportedElement(name.into()));
                    return TokenSinkResult::Continue;
                }
                if !matches!(
                    name,
                    "area"
                        | "base"
                        | "br"
                        | "col"
                        | "embed"
                        | "hr"
                        | "img"
                        | "input"
                        | "link"
                        | "meta"
                        | "param"
                        | "source"
                        | "track"
                        | "wbr"
                ) {
                    // HTML ignores the self-closing flag on ordinary elements.
                    state.stack.push(name.to_owned());
                    if state.stack.len() > MAX_DEPTH {
                        state.error = Some(RenderError::DocumentTooComplex);
                    }
                }
                if matches!(name, "style" | "xmp" | "noembed" | "noframes") {
                    return TokenSinkResult::RawData(RawKind::Rawtext);
                }
                if name == "title" {
                    return TokenSinkResult::RawData(RawKind::Rcdata);
                }
            } else if let Some(index) = state.stack.iter().rposition(|n| n == name) {
                state.stack.truncate(index);
            }
        }
        TokenSinkResult::Continue
    }
}

fn validate(html: &str, options: RenderOptions) -> Result<(u32, u32), RenderError> {
    if html.len() > MAX_HTML_BYTES {
        return Err(RenderError::InputTooLarge);
    }
    if !(64..=2048).contains(&options.width_css)
        || !(64..=8192).contains(&options.viewport_height_css)
        || !(64..=32768).contains(&options.max_height_css)
    {
        return Err(RenderError::InvalidViewport(
            "CSS dimensions outside supported range",
        ));
    }
    if !options.scale.is_finite() || !(0.5..=3.0).contains(&options.scale) {
        return Err(RenderError::InvalidViewport(
            "scale outside supported range",
        ));
    }
    let width = (options.width_css as f32 * options.scale).ceil() as u32;
    let max_height = ((options.max_height_css as f32 * options.scale).ceil() as u32)
        .min((MAX_OUTPUT_PIXELS / u64::from(width)) as u32)
        .min(MAX_OUTPUT_DIMENSION);
    let input = BufferQueue::default();
    input.push_back(html.into());
    let tokenizer = Tokenizer::new(Preflight::default(), Default::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    if let Some(error) = tokenizer.sink.0.into_inner().error {
        return Err(error);
    }
    Ok((width, max_height))
}

/// Render static HTML/CSS with preapproved in-memory resources. Run on a host
/// worker thread, not the Makepad event loop. Inputs and output allocation are
/// bounded, but upstream layout/rasterization has no cancellable CPU deadline.
/// This entry point does not provide WYSIWYG editing, hit testing or selection.
pub fn render_html(
    html: &str,
    options: RenderOptions,
    resources: &ResourceMap,
) -> Result<RenderedArticle, RenderError> {
    let (width, max_height) = validate(html, options)?;
    let provider = Arc::new(MemoryProvider {
        grants: resources.clone(),
        report: Mutex::new(ResourceReport::default()),
    });
    // Force standards-mode HTML parsing; avoid upstream content sniffing into XHTML.
    let html = format!("<!doctype html>\n{html}");
    let mut document = HtmlDocument::from_html(
        &html,
        DocumentConfig {
            base_url: Some(ASSET_BASE_URL.into()),
            net_provider: Some(provider.clone()),
            viewport: Some(Viewport::new(
                width,
                (options.viewport_height_css as f32 * options.scale).ceil() as u32,
                options.scale,
                if options.dark_mode {
                    ColorScheme::Dark
                } else {
                    ColorScheme::Light
                },
            )),
            ..Default::default()
        },
    );
    let mut settled = false;
    for _ in 0..8 {
        let before = provider.report.lock().unwrap().requested;
        document.resolve(0.0);
        let after = provider.report.lock().unwrap().requested;
        if before == after && !document.has_pending_critical_resources() {
            settled = true;
            break;
        }
    }
    if !settled {
        return Err(RenderError::UnsettledResources);
    }
    let css_content_height = document.root_element().final_layout().size.height;
    if !css_content_height.is_finite() || css_content_height < 0.0 {
        return Err(RenderError::InvalidLayout);
    }
    let natural_height = (css_content_height * options.scale).ceil().max(1.0);
    let height = (natural_height.min(max_height as f32)) as u32;
    let clipped = natural_height > max_height as f32;
    let background = if options.dark_mode {
        Color::BLACK
    } else {
        Color::WHITE
    };
    let rgba = render_to_buffer::<VelloCpuImageRenderer, _>(
        |scene| {
            scene.fill(
                Fill::NonZero,
                Default::default(),
                background,
                Default::default(),
                &Rect::new(0.0, 0.0, width as f64, height as f64),
            );
            paint_scene(
                scene,
                &mut document,
                options.scale as f64,
                width,
                height,
                0,
                0,
            );
        },
        width,
        height,
    );
    let resources = provider.report.lock().unwrap().clone();
    Ok(RenderedArticle {
        width,
        height,
        rgba,
        css_content_height,
        clipped,
        resources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narrow_tall_output_stays_within_texture_dimensions() {
        let (width, height) = validate(
            "<p>x</p>",
            RenderOptions {
                width_css: 64,
                max_height_css: 32768,
                scale: 3.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(height <= MAX_OUTPUT_DIMENSION);
        assert!(u64::from(width) * u64::from(height) <= MAX_OUTPUT_PIXELS);
    }
}
