//! Sample decoding for SFZ instruments: WAV (PCM 8/16/24/32 bit, 32/64-bit float, extensible)
//! with loop points from the `smpl` chunk, and FLAC through `claxon`.

use std::io::Cursor;

/// Longest sample accepted (frames), about 9 minutes at 48 kHz.
const MAX_FRAMES: usize = 26_000_000;

/// A decoded sample: interleaved `f32` frames.
#[derive(Clone, Debug, Default)]
pub struct Sample {
    pub data: Vec<f32>,
    /// 1 or 2.
    pub channels: usize,
    pub rate: u32,
    /// Loop start and end (frames, end inclusive) stored in the file.
    pub loop_points: Option<(usize, usize)>,
}

impl Sample {
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels.max(1)
    }

    /// Left and right value at an integer frame (silence outside).
    #[inline]
    pub fn frame(&self, i: usize) -> (f32, f32) {
        if self.channels == 2 {
            match (self.data.get(2 * i), self.data.get(2 * i + 1)) {
                (Some(l), Some(r)) => (*l, *r),
                _ => (0.0, 0.0),
            }
        } else {
            let v = self.data.get(i).copied().unwrap_or(0.0);
            (v, v)
        }
    }
}

/// Decodes a WAV or FLAC file by its content.
pub fn decode(bytes: &[u8]) -> Result<Sample, String> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        wav(bytes)
    } else if bytes.starts_with(b"fLaC") {
        flac(bytes)
    } else {
        Err("not a WAV or FLAC file".into())
    }
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn wav(b: &[u8]) -> Result<Sample, String> {
    let mut fmt: Option<(u16, usize, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut loop_points = None;
    let mut i = 12usize;
    while i + 8 <= b.len() {
        let id = b.get(i..i + 4).unwrap_or(&[]);
        let len = u32_at(b, i + 4).unwrap_or(0) as usize;
        let body = b
            .get(i + 8..(i + 8).saturating_add(len).min(b.len()))
            .unwrap_or(&[]);
        match id {
            b"fmt " => {
                let mut tag = u16_at(body, 0).unwrap_or(0);
                let ch = usize::from(u16_at(body, 2).unwrap_or(0));
                let rate = u32_at(body, 4).unwrap_or(0);
                let bits = u16_at(body, 14).unwrap_or(0);
                if tag == 0xFFFE {
                    // WAVE_FORMAT_EXTENSIBLE: the real format is the first two bytes of the GUID.
                    tag = u16_at(body, 24).unwrap_or(0);
                }
                fmt = Some((tag, ch, rate, bits));
            }
            b"data" => data = Some(body),
            b"smpl" if u32_at(body, 28).unwrap_or(0) > 0 => {
                let start = u32_at(body, 36 + 8).unwrap_or(0) as usize;
                let end = u32_at(body, 36 + 12).unwrap_or(0) as usize;
                if end > start {
                    loop_points = Some((start, end));
                }
            }
            _ => {}
        }
        i = i.saturating_add(8).saturating_add(len).saturating_add(len & 1);
    }
    let (tag, ch, rate, bits) = fmt.ok_or("WAV without format")?;
    let data = data.ok_or("WAV without data")?;
    if ch == 0 || !(1000..=384_000).contains(&rate) {
        return Err("unsupported WAV channels or rate".into());
    }
    let bytes_per = usize::from(bits / 8);
    if bytes_per == 0 {
        return Err("unsupported WAV bit depth".into());
    }
    let frames = (data.len() / (bytes_per * ch)).min(MAX_FRAMES);
    let read = |k: usize| -> f32 {
        let s = data.get(k * bytes_per..(k + 1) * bytes_per).unwrap_or(&[]);
        match (tag, bits) {
            (1, 8) => s.first().map_or(0.0, |v| (f32::from(*v) - 128.0) / 128.0),
            (1, 16) => s
                .try_into()
                .map_or(0.0, |a| f32::from(i16::from_le_bytes(a)) / 32768.0),
            (1, 24) => match s {
                [a, b, c] => (i32::from_le_bytes([0, *a, *b, *c]) >> 8) as f32 / 8_388_608.0,
                _ => 0.0,
            },
            (1, 32) => s
                .try_into()
                .map_or(0.0, |a| i32::from_le_bytes(a) as f32 / 2_147_483_648.0),
            (3, 32) => s.try_into().map_or(0.0, f32::from_le_bytes),
            (3, 64) => s.try_into().map_or(0.0, |a| f64::from_le_bytes(a) as f32),
            _ => 0.0,
        }
    };
    if !matches!((tag, bits), (1, 8 | 16 | 24 | 32) | (3, 32 | 64)) {
        return Err(format!("unsupported WAV encoding {tag}/{bits}"));
    }
    let out_ch = ch.min(2);
    let mut out = Vec::with_capacity(frames * out_ch);
    for f in 0..frames {
        for c in 0..out_ch {
            let v = read(f * ch + c);
            out.push(if v.is_finite() { v } else { 0.0 });
        }
    }
    Ok(Sample {
        data: out,
        channels: out_ch,
        rate,
        loop_points: loop_points.filter(|(_, e)| *e < frames),
    })
}

fn flac(b: &[u8]) -> Result<Sample, String> {
    let mut reader = claxon::FlacReader::new(Cursor::new(b)).map_err(|e| e.to_string())?;
    let info = reader.streaminfo();
    let ch = info.channels as usize;
    let rate = info.sample_rate;
    let bits = info.bits_per_sample;
    if ch == 0 || !(1000..=384_000).contains(&rate) || !(4..=32).contains(&bits) {
        return Err("unsupported FLAC stream".into());
    }
    let scale = 1.0 / (1u64 << (bits - 1)) as f32;
    let out_ch = ch.min(2);
    let mut out = Vec::new();
    for (k, s) in reader.samples().enumerate() {
        let s = s.map_err(|e| e.to_string())?;
        if k % ch < out_ch {
            out.push(s as f32 * scale);
        }
        if out.len() >= MAX_FRAMES * out_ch {
            break;
        }
    }
    out.truncate(out.len() / out_ch * out_ch);
    Ok(Sample {
        data: out,
        channels: out_ch,
        rate,
        loop_points: None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A tiny PCM WAV written by hand, with a loop.
    pub fn wav16(samples: &[i16], channels: u16, rate: u32, loop_pts: Option<(u32, u32)>) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&channels.to_le_bytes());
        fmt.extend_from_slice(&rate.to_le_bytes());
        fmt.extend_from_slice(&(rate * 2 * u32::from(channels)).to_le_bytes());
        fmt.extend_from_slice(&(2 * channels).to_le_bytes());
        fmt.extend_from_slice(&16u16.to_le_bytes());
        let mut data = Vec::new();
        for s in samples {
            data.extend_from_slice(&s.to_le_bytes());
        }
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        for (id, chunk) in [(b"fmt ", fmt), (b"data", data)] {
            body.extend_from_slice(id);
            body.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
            body.extend_from_slice(&chunk);
        }
        if let Some((s, e)) = loop_pts {
            let mut smpl = vec![0u8; 36];
            smpl[28..32].copy_from_slice(&1u32.to_le_bytes());
            smpl.extend_from_slice(&[0u8; 8]);
            smpl.extend_from_slice(&s.to_le_bytes());
            smpl.extend_from_slice(&e.to_le_bytes());
            smpl.extend_from_slice(&[0u8; 8]);
            body.extend_from_slice(b"smpl");
            body.extend_from_slice(&(smpl.len() as u32).to_le_bytes());
            body.extend_from_slice(&smpl);
        }
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn decodes_pcm_wav_with_loop() {
        let w = wav16(&[0, 16384, -16384, 32767], 1, 22_050, Some((1, 3)));
        let s = decode(&w).unwrap();
        assert_eq!((s.channels, s.rate, s.frames()), (1, 22_050, 4));
        assert!((s.data[1] - 0.5).abs() < 1e-4);
        assert_eq!(s.loop_points, Some((1, 3)));
        let st = decode(&wav16(&[1, 2, 3, 4], 2, 44_100, None)).unwrap();
        assert_eq!((st.channels, st.frames()), (2, 2));
    }

    #[test]
    fn hostile_audio_does_not_panic() {
        for junk in [
            &b""[..],
            b"RIFF",
            b"RIFF\0\0\0\0WAVE",
            b"RIFF\xff\xff\xff\xffWAVEfmt \xff\xff\xff\xff",
            b"fLaC\0\0\0",
        ] {
            assert!(decode(junk).is_err());
        }
        let mut w = wav16(&[1, 2, 3], 1, 44_100, None);
        w.truncate(w.len() - 3);
        let _ = decode(&w);
    }
}
