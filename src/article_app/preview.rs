//! Optional preview of validated structured articles. No raw public HTML import.
use article_blitz::{RenderedArticle, RenderOptions, ResourceMap};
use article_core::{assets::crop_cover, document::*, host::Capability};
use super::{model::Grant, storage};
use std::sync::{Arc, Mutex, TryLockError};

static RENDERING: Mutex<()> = Mutex::new(());

fn bundle(
    document: &Document,
    mut read_asset: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<(String, ResourceMap), String> {
    document.ready()?;
    let mut resources = ResourceMap::default();
    let mut urls = std::collections::BTreeMap::new();
    for id in document.asset_ids() {
        let url = resources
            .insert_image(&id, read_asset(&id)?)
            .map_err(|e| e.to_string())?;
        urls.insert(id, url);
    }
    let (paper, ink, accent) = document.theme.colors();
    let font = if document.large_type { 18 } else { 16 };
    let spacing = if document.compact { "1.55" } else { "1.85" };
    let mut html = format!(
        r#"<html><head><meta charset="utf-8"><style>
html,body{{margin:0;background:#{paper:06x};color:#{ink:06x};font-family:"PingFang SC",system-ui,sans-serif}}
article{{box-sizing:border-box;max-width:760px;margin:0 auto;padding:24px;font-size:{font}px;line-height:{spacing};overflow-wrap:break-word}}
h1{{font-size:28px;line-height:1.4}} h2{{font-size:22px;color:#{accent:06x};margin-top:28px}}
h3{{font-size:19px}} p{{margin:16px 0}} .author,figcaption{{font-size:13px;opacity:.7}}
blockquote{{margin:20px 0;padding:12px 18px;border-left:4px solid #{accent:06x};background:#f0f2ef}}
figure{{margin:20px auto;text-align:center}} figure img{{width:100%;max-width:100%;height:auto}} a{{color:#{accent:06x}}} hr{{border:0;border-top:1px solid #{accent:06x};margin:24px 0}}
</style></head><body><article><h1>{}</h1><p class="author">{}</p>"#,
        escape(&document.title),
        escape(&document.author)
    );
    if let Some(cover) = &document.cover {
        if cover.show_in_article {
            let bytes = crop_cover(&read_asset(&cover.asset)?, cover, false)?;
            let url = resources
                .insert_image("article-cover.png", bytes)
                .map_err(|e| e.to_string())?;
            html.push_str(&format!(
                "<figure><img src=\"{url}\" alt=\"{}\"></figure>",
                escape(&document.title)
            ));
        }
    }
    for block in &document.blocks {
        if block.kind == BlockKind::Image {
            let url = urls
                .get(block.asset.as_deref().unwrap_or_default())
                .ok_or("Article image is missing")?;
            html.push_str(&format!("<figure style=\"width:{}%\"><img src=\"{url}\" alt=\"{}\"><figcaption>{}</figcaption></figure>", block.width, escape(&block.alt), escape(&block.caption)));
        } else {
            html.push_str(&block.html());
        }
    }
    html.push_str("</article></body></html>");
    Ok((html, resources))
}

pub fn render(
    document: Document,
    grant: Grant,
    options: RenderOptions,
) -> Result<Arc<RenderedArticle>, String> {
    // At most one expensive render runs in this process. Closing/reopening the
    // app cannot create an unbounded queue of CPU render jobs.
    let _lock = match RENDERING.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Err("Another article preview is still rendering.".into());
        }
        // Only a concurrency permit is protected. A failed worker has no
        // reusable document state and must not disable future previews.
        Err(TryLockError::Poisoned(error)) => error.into_inner(),
    };
    grant.authorize(Capability::ReadDrafts)?;
    let (html, resources) = bundle(&document, |id| {
        storage::asset_bytes(crate::app_data_dir(), &grant, id)
    })?;
    grant.authorize(Capability::ReadDrafts)?;
    let result =
        article_blitz::render_html(&html, options, &resources).map_err(|e| e.to_string())?;
    grant.authorize(Capability::ReadDrafts)?;
    Ok(Arc::new(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_only_generates_escaped_html_from_validated_document() {
        let mut doc = Document::from_markdown("<script>title</script>", "你好 **世界**").unwrap();
        doc.author = "<img src=file:///private>".into();
        let (html, resources) = bundle(&doc, |_| panic!("No assets expected")).unwrap();
        assert!(html.contains("&lt;script&gt;title&lt;/script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img src=file:"));
        let bitmap =
            article_blitz::render_html(&html, RenderOptions::default(), &resources).unwrap();
        assert!(bitmap.resources.denied.is_empty());
        assert!(!bitmap.clipped);
    }

    #[test]
    fn small_images_honor_selected_percentage_instead_of_intrinsic_width() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(8, 8, image::Rgba([220, 10, 10, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let bytes = bytes.into_inner();
        let id = blake3::hash(&bytes).to_hex().to_string();
        let mut doc = Document::from_markdown("Image width", "Body").unwrap();
        let mut block = Block::new(BlockKind::Image, "");
        block.asset = Some(id);
        block.width = 50;
        doc.blocks.push(block);
        let options = RenderOptions {
            width_css: 400,
            scale: 1.0,
            ..Default::default()
        };
        let (html, resources) = bundle(&doc, |_| Ok(bytes.clone())).unwrap();
        let half = article_blitz::render_html(&html, options, &resources).unwrap();
        doc.blocks.last_mut().unwrap().width = 100;
        let (html, resources) = bundle(&doc, |_| Ok(bytes.clone())).unwrap();
        let full = article_blitz::render_html(&html, options, &resources).unwrap();
        assert_eq!(full.resources.denied.len(), 0);
        assert!(full.css_content_height > half.css_content_height + 100.0);
    }
}
