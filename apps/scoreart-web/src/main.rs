//! ScoreArt in the browser.
//!
//! Query parameters:
//! - `?host=parent` — embedded in an `<iframe>` by a same-origin page (e.g. a Nextcloud app).
//!   A `postMessage` bridge replaces Open/Save:
//!   - `scoreart:ready` (to the parent) `{ version }`: listening, send a score now.
//!   - `scoreart:open` (from the parent) `{ name, bytes }` (`ArrayBuffer` or `Uint8Array`).
//!   - `scoreart:save` (to the parent) `{ name, bytes }` for Save and every export.
//!   - `scoreart:dirty` (to the parent) `{ dirty }` when unsaved changes appear or go away.
//!
//!   - `scoreart:config` (from the parent, before or after `open`) — what the host's administrator
//!     set up: `{ internet: bool, libraries: [{ name, enabled, base?, instruments? }],
//!     files: [{ name, url, autoload }] }`. `libraries` names catalogue libraries: hidden when
//!     `enabled` is false; with `base` (a same-origin path ending in `/`, laid out like the
//!     library's repository) the listed `instruments` are fetched from there instead of the
//!     internet. `internet: false` allows only such server copies. `files` are sound files
//!     (.sf2, .sfz, zip) on the server, loaded at start when `autoload`, else on request.
//!
//!   Only the parent window on the page's own origin is heard or answered.
//! - `?lang=de` — the interface language the host wants (e.g. the Nextcloud user's).
//! - `?new` — start with an empty piano score instead of the sample.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

#[cfg(target_arch = "wasm32")]
mod audio;
#[cfg(target_arch = "wasm32")]
mod midi;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("scoreart-web runs in the browser: cd apps/scoreart-web && trunk serve");
}
