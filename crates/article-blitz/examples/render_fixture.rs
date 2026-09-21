use article_blitz::{render_html, RenderOptions, ResourceMap, BLITZ_REVISION};
use std::{fs, path::PathBuf, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "article-blitz-output".into()),
    );
    fs::create_dir_all(&output)?;
    let mut resources = ResourceMap::default();
    resources.insert_css("article.css", include_str!("../tests/fixtures/article.css"))?;
    resources.insert_image(
        "cover.png",
        include_bytes!("../tests/fixtures/cover.png").to_vec(),
    )?;
    let mut results = Vec::new();
    for (name, width_css, scale, dark_mode) in [
        ("mobile-light", 390, 2.0, false),
        ("desktop-light", 760, 1.0, false),
        ("mobile-dark", 390, 2.0, true),
    ] {
        let start = Instant::now();
        let rendered = render_html(
            include_str!("../tests/fixtures/article.html"),
            RenderOptions {
                width_css,
                scale,
                dark_mode,
                ..Default::default()
            },
            &resources,
        )?;
        let elapsed_ms = start.elapsed().as_millis();
        let file = fs::File::create(output.join(format!("{name}.png")))?;
        let mut encoder = png::Encoder::new(file, rendered.width, rendered.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&rendered.rgba)?;
        results.push(serde_json::json!({"name":name,"width_css":width_css,"scale":scale,"width":rendered.width,"height":rendered.height,"css_content_height":rendered.css_content_height,"clipped":rendered.clipped,"requested":rendered.resources.requested,"served":rendered.resources.served,"denied":rendered.resources.denied,"elapsed_ms":elapsed_ms}));
    }
    let report = serde_json::json!({"blitz_revision":BLITZ_REVISION,"platform":std::env::consts::OS,"results":results,"qualification":"Synthetic article fixtures; not a WeChat visual similarity score or full HTML editor."});
    fs::write(
        output.join("render-results.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
