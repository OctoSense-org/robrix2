use article_blitz::{
    render_html, RenderError, RenderOptions, ResourceMap, MAX_HTML_BYTES, MAX_OUTPUT_PIXELS,
};
fn render(html: &str) -> article_blitz::RenderedArticle {
    render_html(
        html,
        RenderOptions {
            width_css: 200,
            viewport_height_css: 200,
            max_height_css: 1024,
            ..Default::default()
        },
        &ResourceMap::default(),
    )
    .unwrap()
}
fn pixel(image: &article_blitz::RenderedArticle, x: usize, y: usize) -> &[u8] {
    &image.rgba[(y * image.width as usize + x) * 4..][..4]
}
#[test]
fn paints_css_cascade_and_converts_makepad_bgra() {
    let image = render(
        "<!doctype html><style>html,body{margin:0}section{width:100px;height:100px;background:#ff0000}.selected{background:#00ff00}</style><section class='selected' style='background:#123456'></section>",
    );
    assert_eq!(pixel(&image, 20, 20), &[0x12, 0x34, 0x56, 255]);
    assert_eq!(
        image.to_bgra_u32()[20 * image.width as usize + 20],
        0xff123456
    );
    assert_eq!(
        image.rgba.len(),
        image.width as usize * image.height as usize * 4
    );
}
#[test]
fn styles_inherit_variables_and_media_queries() {
    let image = render(
        "<style>:root{--paint:#ff0000}body{margin:0}section{background:var(--paint);height:100px}@media(min-width:180px){:root{--paint:#00ff00}}</style><section></section>",
    );
    assert_eq!(pixel(&image, 20, 20), &[0, 255, 0, 255]);
}
#[test]
fn refuses_remote_file_and_data_resources_without_waiting_forever() {
    let image = render(
        "<link rel='stylesheet' href='https://ungranted.invalid/style.css'><img src='https://ungranted.invalid/image.png'><img src='file:///etc/passwd'><img src='data:image/png;base64,AAAA'><style>body{background-image:url(https://ungranted.invalid/bg.png)}</style><p>offline</p>",
    );
    assert_eq!(image.resources.served, 0);
    for prefix in [
        "https://ungranted.invalid/style.css",
        "https://ungranted.invalid/image.png",
        "file:///etc/passwd",
        "data:",
        "https://ungranted.invalid/bg.png",
    ] {
        assert!(
            image.resources.denied.iter().any(|u| u.starts_with(prefix)),
            "missing denial: {prefix}; {:?}",
            image.resources
        );
    }
}
#[test]
fn grants_only_exact_host_bytes_and_follows_no_redirects() {
    let mut resources = ResourceMap::default();
    resources
        .insert_css("approved.css", "body{margin:0;background:#abcdef}")
        .unwrap();
    let image=render_html("<link rel='stylesheet' href='approved.css'><link rel='stylesheet' href='approved.css?elsewhere'><p>Hello</p>",RenderOptions::default(),&resources).unwrap();
    assert_eq!(image.resources.served, 1);
    assert!(
        image
            .resources
            .denied
            .iter()
            .any(|s| s.ends_with("?elsewhere"))
    );
    assert_eq!(pixel(&image, 1, 1), &[0xab, 0xcd, 0xef, 255]);
}
#[test]
fn approved_css_cannot_import_unapproved_resources() {
    let mut resources = ResourceMap::default();
    resources
        .insert_css(
            "approved.css",
            "@import url('https://ungranted.invalid/nested.css'); body{background:#fff}",
        )
        .unwrap();
    let image = render_html(
        "<link rel='stylesheet' href='approved.css'><p>Hello</p>",
        RenderOptions::default(),
        &resources,
    )
    .unwrap();
    assert_eq!(image.resources.served, 1);
    assert!(
        image
            .resources
            .denied
            .iter()
            .any(|s| s.ends_with("/nested.css"))
    );
}
#[test]
fn approved_image_is_decoded_and_painted() {
    let mut resources = ResourceMap::default();
    resources
        .insert_image("cover.png", include_bytes!("fixtures/cover.png").to_vec())
        .unwrap();
    let image = render_html(
        "<style>body{margin:0}img{width:200px}</style><img src='cover.png'>",
        RenderOptions::default(),
        &resources,
    )
    .unwrap();
    assert_eq!(image.resources.served, 1);
    assert!(image.resources.denied.is_empty());
    assert_ne!(pixel(&image, 20, 20), &[255, 255, 255, 255]);
}
#[test]
fn rejects_bad_image_and_resource_identifiers() {
    let mut resources = ResourceMap::default();
    assert_eq!(
        resources.insert_image("x", b"<svg/>".to_vec()),
        Err(RenderError::InvalidImage)
    );
    for id in ["..", "../foo", "file:///a", "a?b", "a/b", ""] {
        assert_eq!(
            resources.insert_css(id, "p{}"),
            Err(RenderError::InvalidResourceId)
        );
    }
}
#[test]
fn rejects_large_html_and_deep_trees_before_layout() {
    assert_eq!(
        render_html(
            &"x".repeat(MAX_HTML_BYTES + 1),
            RenderOptions::default(),
            &ResourceMap::default()
        )
        .unwrap_err(),
        RenderError::InputTooLarge
    );
    assert_eq!(
        render_html(
            &format!("{}hello{}", "<section>".repeat(70), "</section>".repeat(70)),
            RenderOptions::default(),
            &ResourceMap::default()
        )
        .unwrap_err(),
        RenderError::DocumentTooComplex
    );
}
#[test]
fn rejects_active_and_subdocument_elements() {
    for tag in ["script", "iframe", "form", "object", "svg", "base"] {
        assert!(
            matches!(
                render_html(
                    &format!("<{tag}></{tag}>"),
                    RenderOptions::default(),
                    &ResourceMap::default()
                ),
                Err(RenderError::UnsupportedElement(_))
            ),
            "{tag}"
        );
    }
}
#[test]
fn rejects_nonfinite_scale_and_invalid_dimensions() {
    for options in [
        RenderOptions {
            width_css: 0,
            ..Default::default()
        },
        RenderOptions {
            scale: f32::NAN,
            ..Default::default()
        },
        RenderOptions {
            scale: f32::INFINITY,
            ..Default::default()
        },
        RenderOptions {
            max_height_css: u32::MAX,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            render_html("<p>x</p>", options, &ResourceMap::default()),
            Err(RenderError::InvalidViewport(_))
        ));
    }
}
#[test]
fn clips_long_content_and_bounds_output_allocation() {
    let image = render_html(
        "<style>body{margin:0}section{height:50000px;background:#fc0}</style><section></section>",
        RenderOptions {
            max_height_css: 300,
            ..Default::default()
        },
        &ResourceMap::default(),
    )
    .unwrap();
    assert_eq!(image.height, 300);
    assert!(image.clipped);
    assert!(image.css_content_height >= 50000.0);
    assert!(u64::from(image.width) * u64::from(image.height) <= MAX_OUTPUT_PIXELS);
}
#[test]
fn caps_resource_requests() {
    let html = (0..150)
        .map(|i| format!("<img src='https://ungranted.invalid/{i}.png'>"))
        .collect::<String>();
    let image = render(&html);
    assert!(image.resources.budget_exhausted);
    assert!(image.resources.denied.len() <= 128);
}
#[test]
fn renders_chinese_article_in_light_dark_and_responsive_widths() {
    let mut resources = ResourceMap::default();
    resources
        .insert_css("article.css", include_str!("fixtures/article.css"))
        .unwrap();
    resources
        .insert_image("cover.png", include_bytes!("fixtures/cover.png").to_vec())
        .unwrap();
    let html = include_str!("fixtures/article.html");
    let mobile = render_html(html, RenderOptions::default(), &resources).unwrap();
    let desktop = render_html(
        html,
        RenderOptions {
            width_css: 760,
            ..Default::default()
        },
        &resources,
    )
    .unwrap();
    let dark = render_html(
        html,
        RenderOptions {
            dark_mode: true,
            ..Default::default()
        },
        &resources,
    )
    .unwrap();
    for image in [&mobile, &desktop, &dark] {
        assert!(!image.clipped);
        assert_eq!(image.resources.served, 2);
        assert!(image.resources.denied.is_empty());
        assert!(image.height > 600);
    }
    assert_ne!(mobile.css_content_height, desktop.css_content_height);
    assert_ne!(pixel(&mobile, 1, 1), pixel(&dark, 1, 1));
}
