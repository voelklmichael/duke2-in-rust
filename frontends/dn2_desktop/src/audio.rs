//! Sound output through the default device (cpal).
//!
//! The engine mixes on the main thread; the device callback takes samples
//! from a shared queue. Each update tops the queue up to a target length,
//! so the music follows the device's clock even when frames are late.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample, Stream};
use dn2_engine::Engine;

/// Audio kept queued, in milliseconds: a game frame is 57 ms, so this
/// covers a late frame; effects start up to this much after their event.
const TARGET_MS: usize = 90;

type Queue = Arc<Mutex<VecDeque<f32>>>;

pub struct AudioOut {
    /// Kept alive: dropping the stream stops the sound.
    _stream: Stream,
    queue: Queue,
    rate: u32,
    /// Reused for each top-up.
    buffer: Vec<f32>,
}

impl AudioOut {
    /// Opens the default output device, or says why that failed.
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("no audio output format: {e}"))?;
        let config = supported.config();
        let rate = config.sample_rate;
        let channels = usize::from(config.channels);
        let queue = Queue::default();
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, channels, &queue),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, channels, &queue),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, channels, &queue),
            other => return Err(format!("unsupported sample format {other:?}")),
        }?;
        stream
            .play()
            .map_err(|e| format!("cannot start audio: {e}"))?;
        Ok(Self {
            _stream: stream,
            queue,
            rate,
            buffer: Vec::new(),
        })
    }

    /// Mixes what the queue lacks of its target length.
    pub fn fill(&mut self, engine: &mut Engine) {
        let target = self.rate as usize * TARGET_MS / 1000;
        let queued = self.queue.lock().map_or(target, |q| q.len());
        self.buffer.resize(target.saturating_sub(queued), 0.0);
        if self.buffer.is_empty() {
            return;
        }
        engine.mix_audio(&mut self.buffer, self.rate);
        if let Ok(mut q) = self.queue.lock() {
            q.extend(&self.buffer);
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    queue: &Queue,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let queue = Arc::clone(queue);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
                for frame in data.chunks_mut(channels) {
                    frame.fill(T::from_sample(q.pop_front().unwrap_or(0.0)));
                }
            },
            |e| log::error!("audio: {e}"),
            None,
        )
        .map_err(|e| format!("cannot open audio stream: {e}"))
}
