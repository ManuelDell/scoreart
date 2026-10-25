//! Desktop audio output: one cpal stream that pulls stereo frames from the current synth.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use scoreart_synth::Synth;

type Shared = Arc<Mutex<Option<Synth>>>;

#[derive(Default)]
pub struct Audio {
    stream: Option<cpal::Stream>,
    shared: Shared,
    rate: Option<u32>,
    failed: Option<String>,
}

fn lock(s: &Shared) -> std::sync::MutexGuard<'_, Option<Synth>> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

/// Fills an interleaved buffer of `channels` channels from the synth's stereo output.
fn fill<T: cpal::SizedSample + cpal::FromSample<f32>>(
    shared: &Shared,
    out: &mut [T],
    channels: usize,
    scratch: &mut Vec<f32>,
) {
    let frames = out.len() / channels.max(1);
    scratch.clear();
    scratch.resize(frames * 2, 0.0);
    if let Some(s) = lock(shared).as_mut() {
        s.render(scratch);
    }
    for (i, frame) in out.chunks_mut(channels.max(1)).enumerate() {
        let l = scratch.get(i * 2).copied().unwrap_or(0.0);
        let r = scratch.get(i * 2 + 1).copied().unwrap_or(0.0);
        for (c, sample) in frame.iter_mut().enumerate() {
            let v = match (channels, c) {
                (1, _) => (l + r) * 0.5,
                (_, 0) => l,
                (_, 1) => r,
                _ => 0.0,
            };
            *sample = T::from_sample(v);
        }
    }
}

impl Audio {
    /// Opens the default output device once.
    fn ensure(&mut self) -> Result<u32, String> {
        if let Some(r) = self.rate {
            return Ok(r);
        }
        if let Some(e) = &self.failed {
            return Err(e.clone());
        }
        let result = (|| {
            let host = cpal::default_host();
            let device = host.default_output_device().ok_or("no audio output device")?;
            let config = device.default_output_config().map_err(|e| e.to_string())?;
            let rate = config.sample_rate().0;
            let channels = usize::from(config.channels());
            let shared = self.shared.clone();
            let err = |e: cpal::StreamError| eprintln!("scoreart: audio stream error: {e}");
            let cfg: cpal::StreamConfig = config.clone().into();
            let mut scratch = Vec::new();
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_output_stream(
                    &cfg,
                    move |o: &mut [f32], _| fill(&shared, o, channels, &mut scratch),
                    err,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_output_stream(
                    &cfg,
                    move |o: &mut [i16], _| fill(&shared, o, channels, &mut scratch),
                    err,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_output_stream(
                    &cfg,
                    move |o: &mut [u16], _| fill(&shared, o, channels, &mut scratch),
                    err,
                    None,
                ),
                other => return Err(format!("unsupported sample format {other}")),
            }
            .map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            Ok::<_, String>((stream, rate))
        })();
        match result {
            Ok((stream, rate)) => {
                self.stream = Some(stream);
                self.rate = Some(rate);
                Ok(rate)
            }
            Err(e) => {
                self.failed = Some(e.clone());
                Err(e)
            }
        }
    }

    pub fn rate(&mut self) -> u32 {
        self.ensure().unwrap_or(44_100)
    }

    pub fn start(&mut self, synth: Synth) -> Result<(), String> {
        self.ensure()?;
        *lock(&self.shared) = Some(synth);
        Ok(())
    }

    pub fn stop(&mut self) {
        *lock(&self.shared) = None;
    }

    pub fn position(&mut self) -> Option<f64> {
        lock(&self.shared)
            .as_ref()
            .filter(|s| !s.finished())
            .map(Synth::position)
    }
}
