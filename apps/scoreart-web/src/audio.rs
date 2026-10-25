//! Web Audio output: the synth renders short chunks that are scheduled back to back on an
//! `AudioContext`, a little ahead of the audio clock.

use scoreart_synth::Synth;
use wasm_bindgen::JsValue;

/// Rendered ahead of the clock (s).
const LOOKAHEAD: f64 = 0.6;
/// One scheduled buffer (s).
const CHUNK: f64 = 0.2;

#[derive(Default)]
pub struct WebAudio {
    ctx: Option<web_sys::AudioContext>,
    synth: Option<Synth>,
    /// Context time at which the synth's position 0 sounds.
    t0: f64,
    scheduled_until: f64,
    sources: Vec<web_sys::AudioBufferSourceNode>,
}

impl WebAudio {
    fn context(&mut self) -> Result<web_sys::AudioContext, JsValue> {
        if let Some(c) = &self.ctx {
            return Ok(c.clone());
        }
        let c = web_sys::AudioContext::new()?;
        self.ctx = Some(c.clone());
        Ok(c)
    }

    pub fn rate(&mut self) -> u32 {
        self.context().map(|c| c.sample_rate() as u32).unwrap_or(44_100)
    }

    pub fn start(&mut self, synth: Synth) -> Result<(), String> {
        self.stop();
        let ctx = self
            .context()
            .map_err(|_| "Web Audio is not available".to_string())?;
        // Browsers suspend audio until the page was interacted with; key presses and clicks count.
        let _ = ctx.resume();
        self.t0 = ctx.current_time() + 0.06;
        self.scheduled_until = self.t0;
        self.synth = Some(synth);
        self.pump();
        Ok(())
    }

    pub fn stop(&mut self) {
        for s in self.sources.drain(..) {
            #[allow(deprecated)]
            let _ = s.stop();
        }
        self.synth = None;
    }

    pub fn active(&self) -> bool {
        self.synth.is_some()
    }

    /// Schedules more audio when the queue runs short; ends when the synth finished playing.
    pub fn pump(&mut self) {
        let (Some(ctx), Some(synth)) = (self.ctx.clone(), self.synth.as_mut()) else {
            return;
        };
        let now = ctx.current_time();
        if self.scheduled_until < now {
            // We fell behind (hidden tab): skip ahead instead of piling up late buffers.
            self.scheduled_until = now + 0.05;
        }
        let rate = synth.rate();
        let frames = (f64::from(rate) * CHUNK) as usize;
        let mut guard = 0;
        while self.scheduled_until < now + LOOKAHEAD && !synth.finished() && guard < 16 {
            guard += 1;
            let mut inter = vec![0.0f32; frames * 2];
            synth.render(&mut inter);
            let left: Vec<f32> = inter.iter().step_by(2).copied().collect();
            let right: Vec<f32> = inter.iter().skip(1).step_by(2).copied().collect();
            let Ok(buf) = ctx.create_buffer(2, frames as u32, rate as f32) else {
                break;
            };
            let _ = buf.copy_to_channel(&left, 0);
            let _ = buf.copy_to_channel(&right, 1);
            let Ok(src) = ctx.create_buffer_source() else {
                break;
            };
            src.set_buffer(Some(&buf));
            let _ = src.connect_with_audio_node(&ctx.destination());
            let _ = src.start_with_when(self.scheduled_until);
            self.scheduled_until += frames as f64 / f64::from(rate);
            self.sources.push(src);
        }
        // Keep only buffers that may still be playing.
        let keep = ((LOOKAHEAD / CHUNK) as usize) + 4;
        if self.sources.len() > keep {
            let drop = self.sources.len() - keep;
            self.sources.drain(..drop);
        }
        if synth.finished() && now >= self.scheduled_until {
            self.synth = None;
        }
    }

    /// Seconds heard since the start, from the audio clock.
    pub fn position(&mut self) -> Option<f64> {
        let ctx = self.ctx.as_ref()?;
        self.synth.as_ref()?;
        Some((ctx.current_time() - self.t0).max(0.0))
    }
}
