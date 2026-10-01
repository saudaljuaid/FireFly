//! Resource loading separated from a viewport-dependent render.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::dom::{Document, NodeKind};
use crate::layout::ImageSource;
use crate::network::Client;
use crate::url::Url;
use crate::{Error, MAX_COMBINED_CSS_BYTES, Viewport, css, html, layout, paint, resource, style};

const MAX_STYLESHEET_BYTES: usize = 2 * 1024 * 1024;
const MAX_TITLE_BYTES: usize = 65_536;

#[derive(Debug)]
struct CachedStylesheet {
    source: String,
    media: Option<css::MediaQueryList>,
}

/// A bounded parsed document and its loaded resources. Rendering performs no
/// I/O, so an embedder can resize without repeating requests. All linked
/// stylesheets (including presently inactive media) share the existing 16-link
/// and 4 MiB combined cache limits; images retain the existing decode limits.
#[derive(Debug)]
pub struct LoadedDocument {
    document: Document,
    sheets: Vec<CachedStylesheet>,
    images: Vec<Option<ImageSource>>,
    css_truncated: bool,
    /// Final HTTP redirect destination; local and in-memory sources have none.
    pub final_url: Option<Url>,
    pub title: Option<String>,
    pub warnings: Vec<String>,
    /// Successfully retained linked stylesheets, excluding inline styles.
    pub stylesheets: usize,
}

/// Measurements of the actual cached render path, excluding resource I/O.
#[derive(Debug, Clone, Copy)]
pub struct RenderTimings {
    /// Active stylesheet selection, concatenation and bounded CSS parsing.
    pub stylesheet: Duration,
    /// Computed styles and layout together.
    pub layout: Duration,
    /// Serialization of the complete bounded scene to SVG.
    pub svg: Duration,
}

impl LoadedDocument {
    /// Parse in-memory HTML; like `render`, this does not load external resources.
    pub fn from_html(source: &str) -> Result<Self, Error> {
        if source.len() > 16 * 1024 * 1024 {
            return Err(Error::InvalidInput("HTML input exceeds 16 MiB".into()));
        }
        Self::from_document(html::parse(source)?, None, None, None)
    }

    /// Decode HTML using the same encoding metadata and limits as `render_bytes`.
    pub fn from_bytes(source: &[u8], transport_content_type: Option<&str>) -> Result<Self, Error> {
        Self::from_document(
            html::parse_bytes(source, transport_content_type)?,
            None,
            None,
            None,
        )
    }

    /// Load a local document once, resolving resources against its directory.
    pub fn from_file(path: &Path) -> Result<Self, Error> {
        if fs::metadata(path)?.len() > 16 * 1024 * 1024 {
            return Err(Error::InvalidInput("HTML input exceeds 16 MiB".into()));
        }
        let document = html::parse_bytes(&fs::read(path)?, None)?;
        Self::from_document(document, None, Some(path), None)
    }

    /// Load an HTTP(S) document once with the existing redirect, MIME, TLS,
    /// encoding and resource safeguards. The final response determines the base.
    pub fn from_url(input: &str) -> Result<Self, Error> {
        let response = Client::new().fetch(&Url::parse(input)?)?;
        if let Some(content_type) = response.header("content-type") {
            let mime = content_type.split(';').next().unwrap_or("").trim();
            if !mime.eq_ignore_ascii_case("text/html")
                && !mime.eq_ignore_ascii_case("application/xhtml+xml")
            {
                return Err(Error::Network(format!(
                    "document is not HTML: {content_type}"
                )));
            }
        }
        let document = html::parse_bytes(&response.body, response.header("content-type"))?;
        let base = document
            .preorder()
            .into_iter()
            .filter(|&id| !crate::in_template_content(&document, id))
            .filter_map(|id| {
                document
                    .element(id)
                    .filter(|element| element.tag == "base")
                    .and_then(|element| element.attribute("href"))
            })
            .find_map(|href| response.final_url.join(href).ok())
            .unwrap_or_else(|| response.final_url.clone());
        Self::from_document(document, Some(response.final_url), None, Some(base))
    }

    fn from_document(
        document: Document,
        final_url: Option<Url>,
        path: Option<&Path>,
        base: Option<Url>,
    ) -> Result<Self, Error> {
        let client = Client::new();
        let mut sheets = Vec::new();
        let mut warnings = Vec::new();
        let mut cached_bytes = 0usize;
        let mut stylesheet_requests = 0;
        let mut stylesheets = 0;
        let mut css_truncated = false;
        let mut limit_reported = false;
        for id in document.preorder() {
            if crate::in_template_content(&document, id) {
                continue;
            }
            let Some(element) = document.element(id) else {
                continue;
            };
            let media = element.attribute("media").map(css::parse_media_query_list);
            let sources = if element.tag == "style" {
                document.nodes[id]
                    .children
                    .iter()
                    .filter_map(|&child| match &document.nodes[child].kind {
                        NodeKind::Text(text) => Some((text.clone(), false)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            } else if element.tag == "link"
                && element.attribute("rel").is_some_and(|rel| {
                    rel.split_ascii_whitespace()
                        .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                })
            {
                let Some(href) = element.attribute("href") else {
                    continue;
                };
                if path.is_none() && base.is_none() {
                    continue;
                }
                if stylesheet_requests >= 16 {
                    warnings.push("stylesheet limit reached; later links were skipped".into());
                    css_truncated = true;
                    break;
                }
                stylesheet_requests += 1;
                match load_stylesheet(&client, href, path, base.as_ref()) {
                    Ok(source) => vec![(source, true)],
                    Err(error) => {
                        warnings.push(format!("stylesheet {href}: {error}"));
                        continue;
                    }
                }
            } else {
                continue;
            };
            for (source, linked) in sources {
                let Some(next_bytes) = cached_bytes
                    .checked_add(source.len())
                    .and_then(|bytes| bytes.checked_add(1))
                    .filter(|&bytes| bytes <= MAX_COMBINED_CSS_BYTES)
                else {
                    css_truncated = true;
                    if !limit_reported {
                        warnings.push("combined stylesheet limit reached".into());
                        limit_reported = true;
                    }
                    continue;
                };
                cached_bytes = next_bytes;
                sheets.push(CachedStylesheet {
                    source,
                    media: media.clone(),
                });
                stylesheets += usize::from(linked);
            }
        }
        let images = if path.is_some() || base.is_some() {
            crate::load_images(
                &document,
                |src| {
                    if let Some(base) = &base {
                        let response =
                            client.fetch_limited(&base.join(src)?, resource::MAX_IMAGE_BYTES)?;
                        resource::decode_image(&response.body, response.header("content-type"))
                    } else if src.starts_with("http://") || src.starts_with("https://") {
                        let response =
                            client.fetch_limited(&Url::parse(src)?, resource::MAX_IMAGE_BYTES)?;
                        resource::decode_image(&response.body, response.header("content-type"))
                    } else {
                        let target = path
                            .unwrap()
                            .parent()
                            .unwrap_or_else(|| Path::new("."))
                            .join(src);
                        if fs::metadata(&target)?.len() > resource::MAX_IMAGE_BYTES as u64 {
                            return Err(Error::InvalidInput(
                                "image exceeds 4 MiB compressed-byte limit".into(),
                            ));
                        }
                        resource::decode_image(&fs::read(target)?, None)
                    }
                },
                &mut warnings,
            )
        } else {
            vec![None; document.nodes.len()]
        };
        Ok(Self {
            title: document_title(&document),
            document,
            sheets,
            images,
            css_truncated,
            final_url,
            warnings,
            stylesheets,
        })
    }

    /// Recompute styles and layout for CSS pixels in an explicit screen
    /// environment. `height` is never inferred from the document extent.
    pub fn render(&self, viewport: Viewport) -> Result<String, Error> {
        self.render_with_timings(viewport).map(|(svg, _)| svg)
    }

    pub fn render_with_timings(
        &self,
        viewport: Viewport,
    ) -> Result<(String, RenderTimings), Error> {
        crate::validate_viewport(viewport)?;
        let start = Instant::now();
        let mut source = String::new();
        for sheet in &self.sheets {
            if sheet
                .media
                .as_ref()
                .is_none_or(|media| media.matches(viewport.width))
            {
                // The entire cache including separators already fits this bound.
                let appended = crate::append_stylesheet(&mut source, &sheet.source);
                debug_assert!(appended);
            }
        }
        let mut sheet = css::parse(&source);
        sheet.truncated |= self.css_truncated;
        let stylesheet = start.elapsed();
        let start = Instant::now();
        let computed = style::compute_with_status(&self.document, &sheet, viewport);
        let mut scene = layout::layout_with_images_and_viewport(
            &self.document,
            &computed.styles,
            &self.images,
            viewport,
        );
        scene.truncated |= computed.truncated;
        let layout = start.elapsed();
        let start = Instant::now();
        let svg = paint::to_svg(&scene);
        Ok((
            svg,
            RenderTimings {
                stylesheet,
                layout,
                svg: start.elapsed(),
            },
        ))
    }
}

fn load_stylesheet(
    client: &Client,
    href: &str,
    path: Option<&Path>,
    base: Option<&Url>,
) -> Result<String, Error> {
    if base.is_some() || href.starts_with("http://") || href.starts_with("https://") {
        let url = base.map_or_else(|| Url::parse(href), |base| base.join(href))?;
        let sheet = client.fetch_limited(&url, MAX_STYLESHEET_BYTES)?;
        if let Some(content_type) = sheet.header("content-type") {
            let mime = content_type.split(';').next().unwrap_or("").trim();
            if !mime.eq_ignore_ascii_case("text/css") {
                return Err(Error::Network(format!(
                    "stylesheet is not CSS: {content_type}"
                )));
            }
        }
        sheet.text()
    } else {
        let target = path
            .unwrap()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(href);
        if fs::metadata(&target)?.len() > MAX_STYLESHEET_BYTES as u64 {
            return Err(Error::InvalidInput("stylesheet exceeds 2 MiB".into()));
        }
        fs::read_to_string(target).map_err(Error::from)
    }
}

fn document_title(document: &Document) -> Option<String> {
    let id = document.preorder().into_iter().find(|&id| {
        !crate::in_template_content(document, id)
            && document
                .element(id)
                .is_some_and(|element| element.tag == "title")
    })?;
    let mut pending = vec![id];
    let mut text = String::new();
    while let Some(id) = pending.pop() {
        if let NodeKind::Text(value) = &document.nodes[id].kind {
            let mut end = value.len().min(MAX_TITLE_BYTES - text.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            text.push_str(&value[..end]);
            if text.len() == MAX_TITLE_BYTES {
                break;
            }
        }
        pending.extend(document.nodes[id].children.iter().rev().copied());
    }
    let title = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;

    use super::*;

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "phos-loaded-{}-{:?}",
                std::process::id(),
                thread::current().id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn viewport(width: f32) -> Viewport {
        Viewport {
            width,
            height: Some(240.0),
        }
    }

    #[test]
    fn cached_local_media_and_images_resize_after_sources_are_deleted() {
        let directory = TempDirectory::new();
        let path = directory.0.join("page.html");
        fs::write(&path, "<title>Cached &amp; responsive</title><style>p{color:#112233}</style><link rel=stylesheet media='(min-width:600px)' href=wide.css><style media='(max-width:599px)'>p{color:#abcdef}</style><p>Ink</p><img src=image.png><template><link rel=stylesheet href=missing.css></template>").unwrap();
        fs::write(directory.0.join("wide.css"), "p{color:#654321}").unwrap();
        fs::write(
            directory.0.join("image.png"),
            include_bytes!("../tests/render/two-pixels.png"),
        )
        .unwrap();
        let cached = LoadedDocument::from_file(&path).unwrap();
        assert_eq!(cached.title.as_deref(), Some("Cached & responsive"));
        assert_eq!(cached.stylesheets, 1);
        assert!(cached.final_url.is_none());
        assert!(cached.warnings.is_empty());
        fs::remove_dir_all(&directory.0).unwrap();
        for _ in 0..2 {
            let narrow = cached.render(viewport(320.0)).unwrap();
            let wide = cached.render(viewport(900.0)).unwrap();
            assert!(narrow.contains("#abcdef"));
            assert!(!narrow.contains("#654321"));
            assert!(wide.contains("#654321"));
            assert!(!wide.contains("#abcdef"));
            assert!(narrow.contains("<image "));
            assert!(wide.contains("<image "));
        }
    }

    #[test]
    fn cached_redirect_metadata_media_and_mime_use_existing_network_safeguards() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            for _ in 0..6 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let path = request.split(|byte| *byte == b' ').nth(1).unwrap();
                let (headers, body): (&[u8], &[u8]) = match path {
                    b"/start" => (b"HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\n\r\n", b""),
                    b"/final" => (b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=windows-1252\r\nConnection: close\r\n\r\n", b"<title>Price \x80</title><base href=/assets/><link rel=stylesheet media='(max-width:599px)' href=narrow.css><link rel=stylesheet media='(min-width:600px)' href=wide.css><link rel=stylesheet href=wrong.css><p>Ink</p><img src=pixel.png><template><link rel=stylesheet href=never.css></template>"),
                    b"/assets/narrow.css" => (b"HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nConnection: close\r\n\r\n", b"p{color:#123456}"),
                    b"/assets/wide.css" => (b"HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nConnection: close\r\n\r\n", b"p{color:#654321}"),
                    b"/assets/wrong.css" => (b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n", b"p{color:#ff0000}"),
                    b"/assets/pixel.png" => (b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n", include_bytes!("../tests/render/two-pixels.png")),
                    _ => panic!("unexpected request: {}", String::from_utf8_lossy(path)),
                };
                stream.write_all(headers).unwrap();
                stream.write_all(body).unwrap();
            }
        });
        let cached = LoadedDocument::from_url(&format!("http://127.0.0.1:{port}/start")).unwrap();
        server.join().unwrap();
        assert_eq!(cached.final_url.as_ref().unwrap().path_and_query, "/final");
        assert_eq!(cached.title.as_deref(), Some("Price €"));
        assert_eq!(cached.stylesheets, 2);
        assert_eq!(cached.warnings.len(), 1);
        assert!(cached.warnings[0].contains("stylesheet is not CSS"));
        // Listener has closed: these responsive renders cannot make requests.
        let narrow = cached.render(viewport(320.0)).unwrap();
        let wide = cached.render(viewport(900.0)).unwrap();
        assert!(narrow.contains("#123456"));
        assert!(wide.contains("#654321"));
        assert!(!wide.contains("#ff0000"));
        assert!(wide.contains("<image "));
    }

    #[test]
    fn every_cached_stylesheet_including_inactive_media_shares_the_byte_limit() {
        let accepted = " ".repeat(MAX_COMBINED_CSS_BYTES - 2);
        let cached = LoadedDocument::from_html(&format!(
            "<style media='(min-width:600px)'>{accepted}</style><style>x</style><style>p{{color:#123456}}</style><p>Alive</p>"
        ))
        .unwrap();
        assert_eq!(cached.sheets.len(), 1);
        assert!(cached.css_truncated);
        assert_eq!(cached.warnings, ["combined stylesheet limit reached"]);
        assert!(cached.render(viewport(320.0)).unwrap().contains("Alive"));
        assert_eq!(
            cached
                .sheets
                .iter()
                .map(|sheet| sheet.source.len() + 1)
                .sum::<usize>(),
            MAX_COMBINED_CSS_BYTES - 1
        );
        let oversized = LoadedDocument::from_html(&format!(
            "<style>{}</style><style>p{{color:#123456}}</style><p>Alive</p>",
            " ".repeat(MAX_COMBINED_CSS_BYTES)
        ))
        .unwrap();
        assert_eq!(oversized.sheets.len(), 1);
        assert!(
            oversized
                .render(viewport(320.0))
                .unwrap()
                .contains("#123456")
        );
    }

    #[test]
    fn linked_sheet_count_and_image_byte_bounds_stay_active() {
        let directory = TempDirectory::new();
        let path = directory.0.join("page.html");
        fs::write(directory.0.join("a.css"), "p{color:#123456}").unwrap();
        fs::write(
            directory.0.join("large.png"),
            vec![0; resource::MAX_IMAGE_BYTES + 1],
        )
        .unwrap();
        fs::write(
            &path,
            format!(
                "{}<p>Alive</p><img src=large.png>",
                "<link rel=stylesheet media='(min-width:600px)' href=a.css>".repeat(17)
            ),
        )
        .unwrap();
        let cached = LoadedDocument::from_file(&path).unwrap();
        assert_eq!(cached.stylesheets, 16);
        assert_eq!(cached.warnings.len(), 2);
        assert!(cached.warnings[0].contains("stylesheet limit reached"));
        assert!(cached.warnings[1].contains("4 MiB compressed-byte limit"));
        assert!(cached.images.iter().all(Option::is_none));
        assert!(!cached.render(viewport(320.0)).unwrap().contains("#123456"));
        assert!(cached.render(viewport(900.0)).unwrap().contains("#123456"));
    }

    #[test]
    fn cached_and_original_render_paths_agree_on_original_fixtures() {
        for path in [
            "tests/render/article.html",
            "tests/render/layout-landing.html",
            "tests/render/layout-dashboard.html",
            "tests/render/nested.html",
            "tests/render/start_page.html",
        ] {
            let path = Path::new(path);
            let cached = LoadedDocument::from_file(path).unwrap();
            for width in [320.0, 900.0] {
                let viewport = viewport(width);
                assert_eq!(
                    cached.render(viewport).unwrap(),
                    crate::render_file_with_viewport(path, viewport).unwrap().0,
                    "fixture {path:?}, width {width}"
                );
            }
        }
    }

    #[test]
    fn cached_viewport_validation_and_explicit_height_match_original_contract() {
        let source =
            "<style>html,body{margin:0}p{height:50vh;background:#123456}</style><p>Ink</p>";
        let cached = LoadedDocument::from_html(source).unwrap();
        for viewport in [
            viewport(320.0),
            Viewport {
                width: 320.0,
                height: None,
            },
        ] {
            assert_eq!(
                cached.render(viewport).unwrap(),
                crate::render_with_viewport(source, viewport).unwrap()
            );
        }
        for invalid in [
            Viewport {
                width: 0.0,
                height: None,
            },
            Viewport {
                width: f32::NAN,
                height: None,
            },
            Viewport {
                width: 320.0,
                height: Some(0.0),
            },
            Viewport {
                width: 320.0,
                height: Some(16_385.0),
            },
        ] {
            assert!(cached.render(invalid).is_err());
        }
        assert!(LoadedDocument::from_html(&" ".repeat(16 * 1024 * 1024 + 1)).is_err());
        assert!(LoadedDocument::from_bytes(&vec![b' '; 16 * 1024 * 1024 + 1], None).is_err());
    }
}
