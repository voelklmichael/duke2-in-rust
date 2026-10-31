use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use dn2_engine::{ReadFile, RgbaImage};

/// Game files from a folder, the engine's own files from the user data dir,
/// higher-detail pictures from another folder.
pub struct FsPlatform {
    /// Upper-cased file name to path: DOS names ignore case, Linux doesn't.
    files: HashMap<String, PathBuf>,
    store_dir: Option<PathBuf>,
    hd_dir: Option<PathBuf>,
}

impl FsPlatform {
    pub fn new(data_dir: &Path, hd_dir: Option<PathBuf>) -> Self {
        let mut files = HashMap::new();
        match fs::read_dir(data_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        files.insert(name.to_ascii_uppercase(), entry.path());
                    }
                }
            }
            Err(e) => log::warn!("cannot list {}: {e}", data_dir.display()),
        }
        let store_dir = ProjectDirs::from("", "", "dn2_rust").map(|d| d.data_dir().to_path_buf());
        Self {
            files,
            store_dir,
            hd_dir,
        }
    }
}

/// A PNG file as 0xAARRGGBB pixels.
fn read_png(path: &Path) -> Result<RgbaImage, Box<dyn std::error::Error>> {
    let mut decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(path)?));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("too large")?];
    let info = reader.next_frame(&mut buffer)?;
    let bytes = &buffer[..info.buffer_size()];
    let argb = |r: u8, g: u8, b: u8, a: u8| u32::from_be_bytes([a, r, g, b]);
    let pixels = match info.color_type {
        png::ColorType::Rgba => bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&[r, g, b, a]| argb(r, g, b, a))
            .collect(),
        png::ColorType::Rgb => bytes
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[r, g, b]| argb(r, g, b, 255))
            .collect(),
        png::ColorType::GrayscaleAlpha => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[v, a]| argb(v, v, v, a))
            .collect(),
        png::ColorType::Grayscale => bytes.iter().map(|&v| argb(v, v, v, 255)).collect(),
        png::ColorType::Indexed => return Err("indexed colours left after expanding".into()),
    };
    Ok(RgbaImage {
        width: info.width,
        height: info.height,
        pixels,
    })
}

impl ReadFile for FsPlatform {
    fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        fs::read(self.files.get(&name.to_ascii_uppercase())?).ok()
    }

    fn read_image(&self, name: &str) -> Option<RgbaImage> {
        let path = self.hd_dir.as_ref()?.join(format!("{name}.png"));
        if !path.exists() {
            return None;
        }
        read_png(&path)
            .inspect_err(|e| log::warn!("{}: {e}", path.display()))
            .ok()
    }
}

impl dn2_engine::Store for FsPlatform {
    fn load_store(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(self.store_dir.as_ref()?.join(key)).ok()
    }

    fn save_store(&mut self, key: &str, bytes: &[u8]) {
        let Some(dir) = &self.store_dir else {
            log::error!("no user data directory, cannot save {key}");
            return;
        };
        let result = fs::create_dir_all(dir).and_then(|()| fs::write(dir.join(key), bytes));
        if let Err(e) = result {
            log::error!("cannot save {key} in {}: {e}", dir.display());
        }
    }
}
