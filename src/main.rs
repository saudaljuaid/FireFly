use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use phos::{Error, render};

struct Options {
    input: PathBuf,
    output: PathBuf,
    width: f32,
}

fn usage() -> &'static str {
    "Usage: firefly <input.html> --output <output.svg> [--width <pixels>]\n\
     Render a local HTML file with FireFly's early engine core."
}

fn options() -> Result<Options, Error> {
    let mut arguments = env::args().skip(1);
    let mut input = None;
    let mut output = None;
    let mut width = 900.0;
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
            "--help" | "-h" => return Err(Error::InvalidInput(usage().into())),
            _ if argument.starts_with('-') => {
                return Err(Error::InvalidInput(format!("unknown option: {argument}")));
            }
            _ if input.is_none() => input = Some(PathBuf::from(argument)),
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
    })
}

fn run() -> Result<(), Error> {
    let options = options()?;
    let metadata = fs::metadata(&options.input)?;
    if metadata.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidInput(
            "input exceeds the 16 MiB limit for this early renderer".into(),
        ));
    }
    let html = fs::read_to_string(&options.input)?;
    let svg = render(&html, options.width)?;
    fs::write(&options.output, svg)?;
    println!("Rendered {}", options.output.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("firefly: {error}");
            ExitCode::FAILURE
        }
    }
}
