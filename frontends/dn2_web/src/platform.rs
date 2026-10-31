use std::cell::RefCell;
use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use dn2_engine::{ReadFile, RgbaImage};
use web_sys::Storage;

/// Prefix of the engine's own files in local storage.
const STORE_PREFIX: &str = "dn2_rust/";

/// Game files and higher-detail pictures the page passed in, the engine's
/// own files in local storage.
pub struct WebPlatform {
    /// Upper-cased file name to contents: DOS names ignore case.
    files: HashMap<String, Vec<u8>>,
    /// Upper-cased picture name (`CZONE1.MNI/SOLID`) to PNG bytes; the
    /// engine reads each once, at start, so they are let go when read.
    pictures: RefCell<HashMap<String, Vec<u8>>>,
    storage: Option<Storage>,
}

impl WebPlatform {
    pub fn new(files: HashMap<String, Vec<u8>>, pictures: HashMap<String, Vec<u8>>) -> Self {
        let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
        if storage.is_none() {
            log::warn!("no local storage: settings and saved games are not kept");
        }
        Self {
            files,
            pictures: RefCell::new(pictures),
            storage,
        }
    }
}

impl ReadFile for WebPlatform {
    fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        self.files.get(&name.to_ascii_uppercase()).cloned()
    }

    fn read_image(&self, name: &str) -> Option<RgbaImage> {
        let bytes = self
            .pictures
            .borrow_mut()
            .remove(&name.to_ascii_uppercase())?;
        decode_png(&bytes)
            .inspect_err(|e| log::warn!("{name}.png: {e}"))
            .ok()
    }
}

/// A PNG file as 0xAARRGGBB pixels.
fn decode_png(bytes: &[u8]) -> Result<RgbaImage, Box<dyn std::error::Error>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
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

impl dn2_engine::Store for WebPlatform {
    fn load_store(&self, key: &str) -> Option<Vec<u8>> {
        let text = self
            .storage
            .as_ref()?
            .get_item(&format!("{STORE_PREFIX}{key}"))
            .ok()??;
        STANDARD.decode(text).ok()
    }

    fn save_store(&mut self, key: &str, bytes: &[u8]) {
        let Some(storage) = &self.storage else {
            return;
        };
        if let Err(e) = storage.set_item(&format!("{STORE_PREFIX}{key}"), &STANDARD.encode(bytes)) {
            log::error!("cannot save {key}: {e:?}");
        }
    }
}
