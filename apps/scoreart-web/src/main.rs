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
//!   Only the parent window on the page's own origin is heard or answered.
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
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("scoreart-web runs in the browser: cd apps/scoreart-web && trunk serve");
}
