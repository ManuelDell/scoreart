//! Chord symbols as sounding notes: the root, the chord's tones above it and the bass note,
//! read from the symbol's text ("C", "F#m7", "Bbmaj7/D", "Asus4", "G7(b9)", "Hm" …).

/// The notes a chord symbol stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChordTones {
    /// Pitch class of the root (0 = C … 11 = B).
    pub root: u8,
    /// Semitones above the root, ascending, the root (0) first.
    pub intervals: Vec<u8>,
    /// Pitch class of the bass note (the root unless a slash names another).
    pub bass: u8,
}

/// A note name at the start of `s`: its pitch class and how many characters it takes.
/// H is B (German names); a B followed by "b" is B flat.
fn note_name(s: &str) -> Option<(u8, usize)> {
    let mut it = s.char_indices();
    let (_, c) = it.next()?;
    let base: i32 = match c.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' | 'H' => 11,
        _ => return None,
    };
    let mut pc = base;
    let mut used = c.len_utf8();
    for (i, a) in s[used..].char_indices() {
        let step = match a {
            '#' | '♯' => 1,
            'b' | '♭' => -1,
            _ => break,
        };
        // "Bb" is B flat, but "b" also starts nothing else here, so every b after a name is a flat.
        pc += step;
        used = c.len_utf8() + i + a.len_utf8();
    }
    Some((pc.rem_euclid(12) as u8, used))
}

fn take(n: usize, b: &str) -> &str {
    b.get(n..).unwrap_or("")
}

/// The tones of a chord symbol; None when it does not start with a note name.
pub fn chord_tones(symbol: &str) -> Option<ChordTones> {
    let s = symbol.trim();
    let (root, used) = note_name(s)?;
    let mut rest = &s[used..];
    // A bass note after the last slash.
    let mut bass = root;
    if let Some(i) = rest.rfind('/')
        && let Some((b, n)) = note_name(&rest[i + 1..])
        && n == rest.len() - i - 1
    {
        bass = b;
        rest = &rest[..i];
    }
    let q: String = rest
        .chars()
        .filter(|c| !matches!(c, '(' | ')' | ' ' | ','))
        .collect();
    let low = q.to_lowercase();

    let mut third: Option<u8> = Some(4);
    let mut fifth: Option<u8> = Some(7);
    let mut seventh: Option<u8> = None;
    let mut extra: Vec<u8> = Vec::new();

    let starts = |p: &str| low.starts_with(p);
    let major_word = q.starts_with("maj")
        || q.starts_with("Maj")
        || q.starts_with('M')
        || q.starts_with('Δ')
        || q.starts_with('^');
    let mut body: &str = &q;
    if q.starts_with('ø') || low.starts_with("m7b5") || low.starts_with("m7♭5") {
        third = Some(3);
        fifth = Some(6);
        seventh = Some(10);
        body = "";
    } else if starts("dim") || q.starts_with('°') || q.starts_with('o') {
        third = Some(3);
        fifth = Some(6);
        body = q
            .trim_start_matches("dim")
            .trim_start_matches('°')
            .trim_start_matches('o');
        if body.starts_with('7') {
            seventh = Some(9);
            body = &body[1..];
        }
    } else if starts("aug") || q.starts_with('+') {
        fifth = Some(8);
        body = q.trim_start_matches("aug").trim_start_matches('+');
    } else if major_word {
        body = q
            .trim_start_matches("maj")
            .trim_start_matches("Maj")
            .trim_start_matches('M')
            .trim_start_matches('Δ')
            .trim_start_matches('^');
        // A major seventh chord (Δ alone, or with 7, 9, 11, 13).
        if body.is_empty() || body.starts_with(|c: char| c.is_ascii_digit()) {
            seventh = Some(11);
            body = body.trim_start_matches('7');
        }
    } else if starts("min") || starts("mi") || q.starts_with('m') || q.starts_with('-') {
        third = Some(3);
        body = q
            .trim_start_matches("min")
            .trim_start_matches("mi")
            .trim_start_matches('m')
            .trim_start_matches('-');
        if let Some(b) = body
            .strip_prefix("maj7")
            .or_else(|| body.strip_prefix("Maj7"))
            .or_else(|| body.strip_prefix("M7"))
        {
            seventh = Some(11);
            body = b;
        }
    }
    // Numbers and words that follow: 5, 6, 7, 9, 11, 13, sus, add, alterations.
    let mut b = body;
    while !b.is_empty() {
        let lb = b.to_lowercase();
        if lb.starts_with("sus2") {
            third = None;
            extra.push(2);
            b = take(4, b);
        } else if lb.starts_with("sus4") {
            third = None;
            extra.push(5);
            b = take(4, b);
        } else if lb.starts_with("sus") {
            third = None;
            extra.push(5);
            b = take(3, b);
        } else if lb.starts_with("add9") || lb.starts_with("add2") {
            extra.push(14);
            b = take(4, b);
        } else if lb.starts_with("add11") || lb.starts_with("add4") {
            extra.push(17);
            b = take(if lb.starts_with("add11") { 5 } else { 4 }, b);
        } else if lb.starts_with("13") {
            seventh.get_or_insert(10);
            extra.extend([14, 21]);
            b = take(2, b);
        } else if lb.starts_with("11") {
            seventh.get_or_insert(10);
            extra.extend([14, 17]);
            b = take(2, b);
        } else if let Some(alt) = [
            "b5", "#5", "b9", "#9", "#11", "b13", "♭5", "♯5", "♭9", "♯9", "♯11", "♭13",
        ]
        .iter()
        .find(|a| b.starts_with(**a))
        {
            let sharp = alt.starts_with('#') || alt.starts_with('♯');
            let degree: u32 = alt
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            match (degree, sharp) {
                (5, false) => fifth = Some(6),
                (5, true) => fifth = Some(8),
                (9, false) => extra.push(13),
                (9, true) => extra.push(15),
                (11, _) => extra.push(18),
                (13, _) => extra.push(20),
                _ => {}
            }
            b = take(alt.len(), b);
        } else if lb.starts_with('9') {
            seventh.get_or_insert(10);
            extra.push(14);
            b = take(1, b);
        } else if lb.starts_with('7') {
            seventh.get_or_insert(10);
            b = take(1, b);
        } else if lb.starts_with('6') {
            extra.push(9);
            b = take(1, b);
        } else if lb.starts_with('5') {
            third = None;
            b = take(1, b);
        } else {
            // Anything else (a word, an unknown sign) is skipped.
            let n = b.chars().next().map_or(1, char::len_utf8);
            b = take(n, b);
        }
    }
    let mut intervals = vec![0u8];
    intervals.extend(third);
    intervals.extend(fifth);
    intervals.extend(seventh);
    intervals.extend(extra);
    intervals.sort_unstable();
    intervals.dedup();
    Some(ChordTones {
        root,
        intervals,
        bass,
    })
}

impl ChordTones {
    /// MIDI keys to play: the bass note low (from G2 up), the chord above it in close position
    /// (root from F3 up), at most five notes over the bass.
    pub fn keys(&self) -> Vec<u8> {
        let low = |pc: u8, from: u8| -> u8 { from + (pc + 12 - from % 12) % 12 };
        let mut keys = vec![low(self.bass, 43)];
        let root = low(self.root, 53);
        for i in self.intervals.iter().take(5) {
            let k = root.saturating_add(*i);
            if k <= 108 && !keys.contains(&k) {
                keys.push(k);
            }
        }
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::chord_tones;

    fn iv(s: &str) -> (u8, Vec<u8>, u8) {
        let t = chord_tones(s).unwrap_or_else(|| panic!("{s}"));
        (t.root, t.intervals, t.bass)
    }

    #[test]
    fn common_chords() {
        assert_eq!(iv("C"), (0, vec![0, 4, 7], 0));
        assert_eq!(iv("Em"), (4, vec![0, 3, 7], 4));
        assert_eq!(iv("F#/E"), (6, vec![0, 4, 7], 4));
        assert_eq!(iv("Bbmaj7"), (10, vec![0, 4, 7, 11], 10));
        assert_eq!(iv("D/C"), (2, vec![0, 4, 7], 0));
        assert_eq!(iv("Asus4"), (9, vec![0, 5, 7], 9));
        assert_eq!(iv("Dsus2"), (2, vec![0, 2, 7], 2));
        assert_eq!(iv("G7"), (7, vec![0, 4, 7, 10], 7));
        assert_eq!(iv("Hm7"), (11, vec![0, 3, 7, 10], 11));
        assert_eq!(iv("C7/9"), (0, vec![0, 4, 7, 10, 14], 0));
        assert_eq!(iv("Bdim7"), (11, vec![0, 3, 6, 9], 11));
        assert_eq!(iv("Cm7b5"), (0, vec![0, 3, 6, 10], 0));
        assert_eq!(iv("Caug"), (0, vec![0, 4, 8], 0));
        assert_eq!(iv("Cmaj7"), (0, vec![0, 4, 7, 11], 0));
        assert_eq!(iv("CmMaj7"), (0, vec![0, 3, 7, 11], 0));
        assert_eq!(iv("Eb6"), (3, vec![0, 4, 7, 9], 3));
        assert_eq!(iv("G7(b9)"), (7, vec![0, 4, 7, 10, 13], 7));
        assert_eq!(chord_tones("N.C."), None);
        assert_eq!(chord_tones(""), None);
    }

    #[test]
    fn voicing() {
        let k = chord_tones("C/E").map(|t| t.keys()).unwrap_or_default();
        assert_eq!(k, vec![52, 60, 64, 67]);
        for s in ["C", "F#m7", "Bb13", "Hm", "G#dim7/B"] {
            let k = chord_tones(s).map(|t| t.keys()).unwrap_or_default();
            assert!(
                k.len() >= 3 && k.iter().all(|x| (36..=96).contains(x)),
                "{s}: {k:?}"
            );
        }
    }
}
