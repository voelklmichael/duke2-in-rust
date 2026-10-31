use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dn2_engine::{Engine, EngineEvent, FrameBuffer, Key, TICK_HZ};
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::PhysicalKey;
use winit::window::{Icon, Window, WindowId};

use crate::audio::AudioOut;
use crate::keys;

/// Longest stretch of real time one update catches up on, in ticks.
const MAX_TICKS: u32 = TICK_HZ;
/// The rate the music runs at without a sound device: it still times the
/// movies. 50 samples a tick.
const SILENT_RATE: u32 = 50 * TICK_HZ;
/// How often the loop wakes up to run ticks and draw.
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 70);

type WindowSurface = Surface<Rc<Window>, Rc<Window>>;

pub struct App {
    engine: Engine,
    audio: Option<AudioOut>,
    frame: FrameBuffer,
    window: Option<Rc<Window>>,
    surface: Option<WindowSurface>,
    /// Keys passed to the engine as down and not yet up.
    pressed: Vec<Key>,
    last_update: Instant,
    /// Ticks owed, fractional.
    ticks_owed: f64,
    /// Source column per destination column of the picture, reused per frame.
    columns: Vec<u32>,
    /// The window opens at 16:9 instead of 4:3.
    wide: bool,
}

impl App {
    pub fn new(engine: Engine, audio: Option<AudioOut>, wide: bool) -> Self {
        let (width, height) = engine.frame_size();
        Self {
            engine,
            audio,
            frame: FrameBuffer::new(width, height),
            window: None,
            surface: None,
            pressed: Vec::new(),
            last_update: Instant::now(),
            ticks_owed: 0.0,
            columns: Vec::new(),
            wide,
        }
    }

    fn release_all(&mut self) {
        for key in self.pressed.drain(..) {
            self.engine.key_up(key);
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        self.engine.render(&mut self.frame);
        if let Err(e) = surface.resize(w, h) {
            log::error!("cannot resize the surface: {e}");
            return;
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(e) => {
                log::error!("cannot get the surface buffer: {e}");
                return;
            }
        };
        blit(
            &self.frame,
            &mut buffer,
            (size.width, size.height),
            &mut self.columns,
        );
        if let Err(e) = buffer.present() {
            log::error!("cannot present: {e}");
        }
    }
}

/// Draws `frame` into `dest` (row by row, `size` pixels), nearest-neighbour,
/// as large as fits with the original's tall pixels (320x200 fills 4:3),
/// centred, with black borders.
fn blit(frame: &FrameBuffer, dest: &mut [u32], size: (u32, u32), columns: &mut Vec<u32>) {
    let (dw, dh) = size;
    dest.fill(0);
    if frame.width == 0 || frame.height == 0 {
        return;
    }
    // The picture's shape: width by 1.2 times its height.
    let (aw, ah) = (u64::from(frame.width) * 5, u64::from(frame.height) * 6);
    let (w, h) = if u64::from(dw) * ah >= u64::from(dh) * aw {
        ((u64::from(dh) * aw / ah) as u32, dh)
    } else {
        (dw, (u64::from(dw) * ah / aw) as u32)
    };
    if w == 0 || h == 0 {
        return;
    }
    let (x0, y0) = ((dw - w) / 2, (dh - h) / 2);
    columns.clear();
    columns.extend((0..w).map(|x| x * frame.width / w));
    for y in 0..h {
        let sy = y * frame.height / h;
        let src = &frame.pixels[(sy * frame.width) as usize..][..frame.width as usize];
        let start = ((y0 + y) * dw + x0) as usize;
        for (d, &sx) in dest[start..start + w as usize]
            .iter_mut()
            .zip(columns.iter())
        {
            *d = src[sx as usize];
        }
    }
}

/// The web page's icon, for the title bar and task bar. Wayland and macOS
/// do not take one from the window.
fn window_icon() -> Option<Icon> {
    const PNG: &[u8] = include_bytes!("../../dn2_web/www/icon-192.png");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(PNG));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(info.buffer_size());
    if info.color_type != png::ColorType::Rgba {
        log::warn!("window icon is {:?}, not RGBA", info.color_type);
        return None;
    }
    Icon::from_rgba(rgba, info.width, info.height).ok()
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Duke Nukem II")
            .with_window_icon(window_icon())
            .with_inner_size(if self.wide {
                LogicalSize::new(1280, 720)
            } else {
                LogicalSize::new(1280, 960)
            });
        let window = Rc::new(
            event_loop
                .create_window(attributes)
                .expect("cannot create the window"),
        );
        let context = Context::new(window.clone()).expect("cannot create the graphics context");
        let surface = Surface::new(&context, window.clone()).expect("cannot create the surface");
        self.window = Some(window);
        self.surface = Some(surface);
        self.last_update = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Focused(false) => self.release_all(),
            WindowEvent::KeyboardInput { event, .. } if !event.repeat => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let Some(key) = keys::translate(code) else {
                    log::debug!("key {code:?} not used");
                    return;
                };
                match event.state {
                    ElementState::Pressed => {
                        if !self.pressed.contains(&key) {
                            self.pressed.push(key);
                        }
                        self.engine.key_down(key);
                    }
                    ElementState::Released => {
                        self.pressed.retain(|&k| k != key);
                        self.engine.key_up(key);
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = &self.window else {
            return;
        };
        let now = Instant::now();
        let owed =
            self.ticks_owed + now.duration_since(self.last_update).as_secs_f64() * TICK_HZ as f64;
        self.last_update = now;
        let whole = owed.floor().min(MAX_TICKS as f64);
        // After a stall the rest is dropped, not caught up on later.
        self.ticks_owed = if owed > MAX_TICKS as f64 {
            0.0
        } else {
            owed - whole
        };
        for event in self.engine.update(whole as u32) {
            // Sounds and music are already in the engine's mix.
            if event == EngineEvent::Quit {
                event_loop.exit();
            }
        }
        match &mut self.audio {
            Some(audio) => audio.fill(&mut self.engine),
            None => mix_silently(&mut self.engine, whole as u32),
        }
        window.request_redraw();
        event_loop.set_control_flow(ControlFlow::WaitUntil(now + FRAME_INTERVAL));
    }
}

/// Mixes the time that passed and drops it, when there is no sound.
fn mix_silently(engine: &mut Engine, ticks: u32) {
    let mut samples = vec![0.0; (ticks * SILENT_RATE / TICK_HZ) as usize];
    engine.mix_audio(&mut samples, SILENT_RATE);
}

#[cfg(test)]
mod tests {
    #[test]
    fn window_icon_decodes() {
        assert!(super::window_icon().is_some());
    }
}
