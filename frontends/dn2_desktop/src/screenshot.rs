use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::Context;
use dn2_engine::{Engine, FrameBuffer};

/// Renders the engine's first frame into a PNG file.
pub fn save(engine: &mut Engine, path: &Path) -> anyhow::Result<()> {
    let (width, height) = engine.frame_size();
    let mut frame = FrameBuffer::new(width, height);
    engine.render(&mut frame);
    let (width, height) = (frame.width, frame.height);
    let rgb: Vec<u8> = frame
        .pixels
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();

    let file = File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()?
        .write_image_data(&rgb)
        .context("cannot write the PNG")?;
    log::info!("wrote {}", path.display());
    Ok(())
}
