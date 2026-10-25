//! `cargo run -p scoreart-render --example sample_svg -- in.musicxml out-prefix`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: sample_svg in.musicxml out-prefix");
        return;
    };
    let bytes = std::fs::read(input).unwrap_or_default();
    let score = match scoreart_musicxml::from_bytes(&bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    let style = scoreart_engrave::Style::default();
    let l = scoreart_engrave::layout(&score, &style);
    for (i, svg) in scoreart_render::svg_pages(&l, &style).iter().enumerate() {
        let _ = std::fs::write(format!("{out}-{}.svg", i + 1), svg);
    }
}
