//! A room: the classic arrangement of eight parallel feedback comb filters with damping,
//! followed by four series allpass filters, per channel (Schroeder/Moorer; the tuning is the
//! well-known public-domain one). Linear, so the reverb of a sum is the sum of the reverbs:
//! stems rendered one by one still add up to the mix.

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
/// Extra delay of the right channel.
const SPREAD: usize = 23;
const FEEDBACK: f32 = 0.84;
const DAMP: f32 = 0.3;
const INPUT: f32 = 0.015;

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(n: usize) -> Comb {
        Comb {
            buf: vec![0.0; n.max(1)],
            i: 0,
            store: 0.0,
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let Some(y) = self.buf.get(self.i).copied() else {
            return 0.0;
        };
        self.store = y * (1.0 - DAMP) + self.store * DAMP;
        if let Some(v) = self.buf.get_mut(self.i) {
            *v = x + self.store * FEEDBACK;
        }
        self.i = (self.i + 1) % self.buf.len();
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(n: usize) -> Allpass {
        Allpass {
            buf: vec![0.0; n.max(1)],
            i: 0,
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let Some(b) = self.buf.get(self.i).copied() else {
            return x;
        };
        if let Some(v) = self.buf.get_mut(self.i) {
            *v = x + b * 0.5;
        }
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}

struct Channel {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
}

impl Channel {
    fn new(rate: f32, extra: usize) -> Channel {
        let k = rate / 44_100.0;
        let len = |n: usize| (((n + extra) as f32) * k) as usize;
        Channel {
            combs: COMBS.iter().map(|n| Comb::new(len(*n))).collect(),
            allpasses: ALLPASSES.iter().map(|n| Allpass::new(len(*n))).collect(),
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let x = x * INPUT;
        let mut y: f32 = self.combs.iter_mut().map(|c| c.tick(x)).sum();
        for a in &mut self.allpasses {
            y = a.tick(y);
        }
        y
    }
}

/// A stereo room.
pub struct Reverb {
    l: Channel,
    r: Channel,
    /// 0 (dry) … 1.
    pub mix: f32,
}

impl Reverb {
    pub fn new(rate: f32, mix: f32) -> Reverb {
        Reverb {
            l: Channel::new(rate, 0),
            r: Channel::new(rate, SPREAD),
            mix: mix.clamp(0.0, 1.0),
        }
    }

    /// Adds the room to an interleaved stereo buffer.
    pub fn process(&mut self, buf: &mut [f32]) {
        if self.mix <= 0.0 {
            return;
        }
        let wet = self.mix * 3.0;
        for [a, b] in buf.as_chunks_mut::<2>().0 {
            // Each side its own room, so a part placed left is heard left.
            let (yl, yr) = (self.l.tick(*a), self.r.tick(*b));
            *a += yl * wet;
            *b += yr * wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_on_and_dies_away() {
        let mut r = Reverb::new(44_100.0, 0.3);
        let mut buf = vec![0.0f32; 2 * 44_100 * 4];
        buf[0] = 1.0;
        buf[1] = 1.0;
        r.process(&mut buf);
        let energy = |a: usize, b: usize| buf[a * 2..b * 2].iter().map(|v| v * v).sum::<f32>();
        assert!(energy(4410, 22_050) > 1e-6, "a tail");
        assert!(
            energy(132_300, 176_400) < energy(4410, 22_050) / 100.0,
            "dies away"
        );
        assert!(buf.iter().all(|v| v.is_finite() && v.abs() < 2.0));
        // Dry: untouched.
        let mut dry = vec![0.5f32; 64];
        Reverb::new(44_100.0, 0.0).process(&mut dry);
        assert!(dry.iter().all(|v| *v == 0.5));
    }
}
