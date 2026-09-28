pub mod css;
pub mod dom;
pub mod html;
pub mod layout;
pub mod paint;
pub mod style;

use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    InvalidInput(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidInput(message) => write!(f, "{message}"),
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
    if !viewport_width.is_finite() || viewport_width < 1.0 || viewport_width > 16_384.0 {
        return Err(Error::InvalidInput(
            "viewport width must be between 1 and 16384 pixels".into(),
        ));
    }
    let document = html::parse(source)?;
    let sheet = css::parse(&document.stylesheets());
    let styles = style::compute(&document, &sheet);
    let scene = layout::layout(&document, &styles, viewport_width);
    Ok(paint::to_svg(&scene))
}
