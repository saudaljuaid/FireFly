pub mod css;
pub mod dom;
pub mod html;
pub mod layout;
pub mod network;
pub mod paint;
pub mod resource;
pub mod style;
pub mod text;
pub mod url;

use std::fmt;
use std::fs;
use std::path::Path;

use dom::{Document, NodeKind};
use layout::ImageSource;
use network::Client;
use url::Url;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    InvalidInput(String),
    Network(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidInput(message) => write!(f, "{message}"),
            Self::Network(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn render(source: &str, viewport_width: f32) -> Result<String, Error> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidInput("HTML input exceeds 16 MiB".into()));
    }
    validate_width(viewport_width)?;
    let document = html::parse(source)?;
    let sheet = css::parse(&document.stylesheets());
    Ok(render_document(
        &document,
        &sheet,
        &vec![None; document.nodes.len()],
        viewport_width,
    ))
}

/// Render an HTML byte stream. `transport_content_type` is the final HTTP
/// response's Content-Type; pass `None` for a local file.
pub fn render_bytes(
    source: &[u8],
    transport_content_type: Option<&str>,
    viewport_width: f32,
) -> Result<String, Error> {
    validate_width(viewport_width)?;
    let document = html::parse_bytes(source, transport_content_type)?;
    let sheet = css::parse(&document.stylesheets());
    Ok(render_document(
        &document,
        &sheet,
        &vec![None; document.nodes.len()],
        viewport_width,
    ))
}

fn validate_width(viewport_width: f32) -> Result<(), Error> {
    if !viewport_width.is_finite() || viewport_width < 1.0 || viewport_width > 16_384.0 {
        return Err(Error::InvalidInput(
            "viewport width must be between 1 and 16384 pixels".into(),
        ));
    }
    Ok(())
}

fn render_document(
    document: &Document,
    sheet: &css::Stylesheet,
    images: &[Option<ImageSource>],
    viewport_width: f32,
) -> String {
    let styles = style::compute(document, sheet);
    let scene = layout::layout_with_images(document, &styles, images, viewport_width);
    paint::to_svg(&scene)
}

fn load_images(
    document: &Document,
    mut load: impl FnMut(&str) -> Result<ImageSource, Error>,
    warnings: &mut Vec<String>,
) -> Vec<Option<ImageSource>> {
    let mut images = vec![None; document.nodes.len()];
    let mut requests = 0;
    for id in document.preorder() {
        let node = &document.nodes[id];
        if in_template_content(document, id) {
            continue;
        }
        let NodeKind::Element(element) = &node.kind else {
            continue;
        };
        if element.tag != "img" {
            continue;
        }
        let Some(src) = element.attribute("src").filter(|src| !src.is_empty()) else {
            continue;
        };
        if requests >= 16 {
            warnings.push("image limit reached; later images were skipped".into());
            break;
        }
        requests += 1;
        match load(src) {
            Ok(image) => images[id] = Some(image),
            Err(error) => warnings.push(format!("image {src}: {error}")),
        }
    }
    images
}

/// Render a local HTML file, resolving image paths against its directory.
pub fn render_file(path: &Path, viewport_width: f32) -> Result<(String, Vec<String>), Error> {
    validate_width(viewport_width)?;
    if fs::metadata(path)?.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidInput("HTML input exceeds 16 MiB".into()));
    }
    let document = html::parse_bytes(&fs::read(path)?, None)?;
    let client = Client::new();
    let mut warnings = Vec::new();
    let mut css_source = String::new();
    let mut links = 0;
    for id in document.preorder() {
        let node = &document.nodes[id];
        if in_template_content(&document, id) {
            continue;
        }
        let NodeKind::Element(element) = &node.kind else {
            continue;
        };
        if element.tag == "style" {
            for &child in &node.children {
                if let NodeKind::Text(text) = &document.nodes[child].kind
                    && css_source.len() + text.len() <= 4 * 1024 * 1024
                {
                    css_source.push_str(text);
                    css_source.push('\n');
                }
            }
        } else if element.tag == "link"
            && element.attribute("rel").is_some_and(|rel| {
                rel.split_ascii_whitespace()
                    .any(|token| token.eq_ignore_ascii_case("stylesheet"))
            })
        {
            if links >= 16 {
                warnings.push("stylesheet limit reached; later links were skipped".into());
                break;
            }
            links += 1;
            let Some(href) = element.attribute("href") else {
                continue;
            };
            let loaded = if href.starts_with("http://") || href.starts_with("https://") {
                Url::parse(href)
                    .and_then(|url| client.fetch_limited(&url, 2 * 1024 * 1024))
                    .and_then(|response| response.text())
            } else {
                let target = path.parent().unwrap_or_else(|| Path::new(".")).join(href);
                fs::metadata(&target)
                    .map_err(Error::from)
                    .and_then(|metadata| {
                        if metadata.len() > 2 * 1024 * 1024 {
                            return Err(Error::InvalidInput("stylesheet exceeds 2 MiB".into()));
                        }
                        fs::read_to_string(target).map_err(Error::from)
                    })
            };
            match loaded {
                Ok(source) if css_source.len() + source.len() <= 4 * 1024 * 1024 => {
                    css_source.push_str(&source);
                    css_source.push('\n');
                }
                Ok(_) => warnings.push("combined stylesheet limit reached".into()),
                Err(error) => warnings.push(format!("stylesheet {href}: {error}")),
            }
        }
    }
    let sheet = css::parse(&css_source);
    let images = load_images(
        &document,
        |src| {
            if src.starts_with("http://") || src.starts_with("https://") {
                let response =
                    client.fetch_limited(&Url::parse(src)?, resource::MAX_IMAGE_BYTES)?;
                resource::decode_image(&response.body, response.header("content-type"))
            } else {
                let target = path.parent().unwrap_or_else(|| Path::new(".")).join(src);
                if fs::metadata(&target)?.len() > resource::MAX_IMAGE_BYTES as u64 {
                    return Err(Error::InvalidInput(
                        "image exceeds 4 MiB compressed-byte limit".into(),
                    ));
                }
                resource::decode_image(&fs::read(target)?, None)
            }
        },
        &mut warnings,
    );
    Ok((
        render_document(&document, &sheet, &images, viewport_width),
        warnings,
    ))
}

fn in_template_content(document: &Document, mut id: usize) -> bool {
    while let Some(parent) = document.nodes[id].parent {
        if matches!(document.nodes[parent].kind, NodeKind::TemplateContent) {
            return true;
        }
        id = parent;
    }
    false
}

pub struct LoadedPage {
    pub url: Url,
    pub svg: String,
    pub warnings: Vec<String>,
    pub stylesheets: usize,
}

pub fn render_url(input: &str, viewport_width: f32) -> Result<LoadedPage, Error> {
    validate_width(viewport_width)?;
    let client = Client::new();
    let response = client.fetch(&Url::parse(input)?)?;
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
    let base_url = document
        .preorder()
        .into_iter()
        .filter_map(|id| match &document.nodes[id].kind {
            NodeKind::Element(element)
                if element.tag == "base" && !in_template_content(&document, id) =>
            {
                element.attribute("href")
            }
            _ => None,
        })
        .find_map(|href| response.final_url.join(href).ok())
        .unwrap_or_else(|| response.final_url.clone());
    let mut css_source = String::new();
    let mut warnings = Vec::new();
    let mut stylesheets = 0;
    let mut stylesheet_requests = 0;
    for id in document.preorder() {
        let node = &document.nodes[id];
        if in_template_content(&document, id) {
            continue;
        }
        let NodeKind::Element(element) = &node.kind else {
            continue;
        };
        match element.tag.as_str() {
            "style" => {
                for &child in &node.children {
                    if let NodeKind::Text(text) = &document.nodes[child].kind {
                        css_source.push_str(text);
                        css_source.push('\n');
                    }
                }
            }
            "link"
                if element.attribute("rel").is_some_and(|rel| {
                    rel.split_ascii_whitespace()
                        .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                }) =>
            {
                if element.attribute("media").is_some_and(|media| {
                    !matches!(media.to_ascii_lowercase().as_str(), "all" | "screen")
                }) {
                    continue;
                }
                let Some(href) = element.attribute("href") else {
                    continue;
                };
                if stylesheet_requests >= 16 {
                    warnings.push("stylesheet limit reached; later links were skipped".into());
                    break;
                }
                stylesheet_requests += 1;
                let load = base_url
                    .join(href)
                    .and_then(|url| client.fetch_limited(&url, 2 * 1024 * 1024))
                    .and_then(|sheet| {
                        if sheet.body.len() > 2 * 1024 * 1024 {
                            return Err(Error::Network("stylesheet exceeds 2 MiB".into()));
                        }
                        if let Some(content_type) = sheet.header("content-type") {
                            let mime = content_type.split(';').next().unwrap_or("").trim();
                            if !mime.eq_ignore_ascii_case("text/css") {
                                return Err(Error::Network(format!(
                                    "stylesheet is not CSS: {content_type}"
                                )));
                            }
                        }
                        sheet.text()
                    });
                match load {
                    Ok(source) => {
                        if css_source.len() + source.len() > 4 * 1024 * 1024 {
                            warnings.push("combined stylesheet limit reached".into());
                            break;
                        }
                        css_source.push_str(&source);
                        css_source.push('\n');
                        stylesheets += 1;
                    }
                    Err(error) => warnings.push(format!("stylesheet {href}: {error}")),
                }
            }
            _ => {}
        }
    }
    let sheet = css::parse(&css_source);
    let images = load_images(
        &document,
        |src| {
            let url = base_url.join(src)?;
            let response = client.fetch_limited(&url, resource::MAX_IMAGE_BYTES)?;
            resource::decode_image(&response.body, response.header("content-type"))
        },
        &mut warnings,
    );
    Ok(LoadedPage {
        url: response.final_url,
        svg: render_document(&document, &sheet, &images, viewport_width),
        warnings,
        stylesheets,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    #[test]
    fn loads_redirected_document_and_linked_stylesheet() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let request = String::from_utf8_lossy(&request);
                let response = if request.starts_with("GET /start ") {
                    "HTTP/1.1 302 Found\r\nLocation: /page\r\nContent-Length: 0\r\n\r\n"
                } else if request.starts_with("GET /page ") {
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<base href='/assets/'><link rel='stylesheet' href='site.css'><h1>Hello</h1>"
                } else {
                    assert!(request.starts_with("GET /assets/site.css "));
                    "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nConnection: close\r\n\r\nh1 { color: #123456 }"
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let page = render_url(&format!("http://127.0.0.1:{port}/start"), 500.0).unwrap();
        server.join().unwrap();
        assert!(page.svg.contains("#123456"));
        assert!(page.svg.contains(">Hello</text>"));
        assert_eq!(page.stylesheets, 1);
        assert!(page.warnings.is_empty());
        assert_eq!(page.url.path_and_query, "/page");
    }

    #[test]
    fn final_redirect_charset_decodes_html_and_keeps_template_styles_inert() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let response: &[u8] = if request.starts_with(b"GET /start ") {
                    b"HTTP/1.1 302 Found\r\nLocation: /page\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 0\r\n\r\n"
                } else if request.starts_with(b"GET /page ") {
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=ISO-8859-1\r\nConnection: close\r\n\r\n<meta charset=utf-8><style>h1 {color:#111111}</style><template><style>h1 {color:#ff0000}</style></template><link rel=stylesheet href=/site.css><h1>Price \x80</h1>"
                } else {
                    assert!(request.starts_with(b"GET /site.css "));
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nConnection: close\r\n\r\nh1 {color:#123456}"
                };
                stream.write_all(response).unwrap();
            }
        });
        let page = render_url(&format!("http://127.0.0.1:{port}/start"), 500.0).unwrap();
        server.join().unwrap();
        assert!(page.svg.contains("Price</text>"));
        assert!(page.svg.contains(">€</text>"));
        assert!(page.svg.contains("#123456"));
        assert!(!page.svg.contains("#ff0000"));
        assert_eq!(page.stylesheets, 1);
    }

    #[test]
    fn redirect_to_late_meta_restarts_and_loads_live_styles() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let response = if request.starts_with(b"GET /start ") {
                    b"HTTP/1.1 302 Found\r\nLocation: /page\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 0\r\n\r\n".to_vec()
                } else if request.starts_with(b"GET /page ") {
                    let mut response =
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n"
                            .to_vec();
                    response.extend(vec![b' '; 1030]);
                    response.extend_from_slice(b"<meta charset=latin1><style>h1 {color:#123456}</style><template><style>h1 {color:#ff0000}</style><link rel=stylesheet href=/inert.css></template><link rel=stylesheet href=/site.css><h1>Price \x80</h1><p>Linked</p>");
                    response
                } else {
                    assert!(request.starts_with(b"GET /site.css "));
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nConnection: close\r\n\r\np {color:#654321}".to_vec()
                };
                stream.write_all(&response).unwrap();
            }
        });
        let page = render_url(&format!("http://127.0.0.1:{port}/start"), 500.0).unwrap();
        server.join().unwrap();
        assert!(page.svg.contains(">€</text>"));
        assert!(page.svg.contains("#123456"));
        assert!(page.svg.contains("#654321"));
        assert!(!page.svg.contains("#ff0000"));
        assert_eq!(page.stylesheets, 1);
        assert_eq!(page.url.path_and_query, "/page");
    }
}
