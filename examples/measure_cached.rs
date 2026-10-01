//! Reproducible release timings on unchanged original fixtures; emits CSV.
use std::path::Path;

use phos::{LoadedDocument, Viewport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("fixture,width,iteration,stylesheet_ms,layout_ms,svg_ms,svg_bytes");
    let mut paths = [
        "article",
        "layout-landing",
        "layout-dashboard",
        "nested",
        "start_page",
    ]
    .map(|fixture| format!("tests/render/{fixture}.html"))
    .to_vec();
    // Optional additional paths allow the unchanged stress generator's sources
    // to remain outside this repository.
    paths.extend(std::env::args().skip(1));
    for path in paths {
        let fixture = Path::new(&path).file_stem().unwrap().to_string_lossy();
        let loaded = LoadedDocument::from_file(Path::new(&path))?;
        for width in [320.0, 900.0] {
            let viewport = Viewport {
                width,
                height: Some(600.0),
            };
            loaded.render(viewport)?; // Warm font and allocator caches.
            for iteration in 0..10 {
                let (svg, elapsed) = loaded.render_with_timings(viewport)?;
                println!(
                    "{fixture},{width},{iteration},{:.3},{:.3},{:.3},{}",
                    elapsed.stylesheet.as_secs_f64() * 1000.0,
                    elapsed.layout.as_secs_f64() * 1000.0,
                    elapsed.svg.as_secs_f64() * 1000.0,
                    svg.len()
                );
            }
        }
    }
    Ok(())
}
