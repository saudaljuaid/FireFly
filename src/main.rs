use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use phos::{Error, Viewport, render_file_with_viewport, render_url_with_viewport};

struct Options {
    input: String,
    output: PathBuf,
    width: f32,
    height: Option<f32>,
}

fn usage() -> &'static str {
    "Usage: scarlite <input.html|http(s)://url> --output <output.svg> [--width <pixels>] [--height <pixels>]\n\
     Render a local HTML file or web page with the Phos engine."
}

fn options() -> Result<Options, Error> {
    let mut arguments = env::args().skip(1);
    let mut input = None;
    let mut output = None;
    let mut width = 900.0;
    let mut height = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--output" | "-o" => {
                output = Some(PathBuf::from(arguments.next().ok_or_else(|| {
                    Error::InvalidInput("missing value for --output".into())
                })?));
            }
            "--width" => {
                width = arguments
                    .next()
                    .ok_or_else(|| Error::InvalidInput("missing value for --width".into()))?
                    .parse()
                    .map_err(|_| Error::InvalidInput("invalid viewport width".into()))?;
            }
            "--height" => {
                height = Some(
                    arguments
                        .next()
                        .ok_or_else(|| Error::InvalidInput("missing value for --height".into()))?
                        .parse()
                        .map_err(|_| Error::InvalidInput("invalid viewport height".into()))?,
                );
            }
            "--help" | "-h" => return Err(Error::InvalidInput(usage().into())),
            _ if argument.starts_with('-') => {
                return Err(Error::InvalidInput(format!("unknown option: {argument}")));
            }
            _ if input.is_none() => input = Some(argument),
            _ => {
                return Err(Error::InvalidInput(
                    "provide exactly one HTML input file".into(),
                ));
            }
        }
    }
    Ok(Options {
        input: input.ok_or_else(|| Error::InvalidInput(usage().into()))?,
        output: output.ok_or_else(|| Error::InvalidInput(usage().into()))?,
        width,
        height,
    })
}

fn run() -> Result<(), Error> {
    let options = options()?;
    let viewport = Viewport {
        width: options.width,
        height: options.height,
    };
    let remote = options
        .input
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || options
            .input
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
    let svg = if remote {
        let page = render_url_with_viewport(&options.input, viewport)?;
        println!(
            "Loaded {} ({} stylesheets)",
            page.url.as_string(),
            page.stylesheets
        );
        for warning in &page.warnings {
            eprintln!("scarlite: {warning}");
        }
        page.svg
    } else {
        let input = PathBuf::from(&options.input);
        let metadata = fs::metadata(&input)?;
        if metadata.len() > 16 * 1024 * 1024 {
            return Err(Error::InvalidInput("HTML input exceeds 16 MiB".into()));
        }
        let (svg, warnings) = render_file_with_viewport(&input, viewport)?;
        for warning in warnings {
            eprintln!("scarlite: {warning}");
        }
        svg
    };
    if svg.starts_with("<svg data-phos-truncated=\"true\"") {
        eprintln!(
            "scarlite: rendering or stylesheet budget reached; output contains a bounded prefix"
        );
    }
    fs::write(&options.output, svg)?;
    println!("Rendered {}", options.output.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("scarlite: {error}");
            ExitCode::FAILURE
        }
    }
}
