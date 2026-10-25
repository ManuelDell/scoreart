//! Browser shell: file picking, downloads, the `?host=parent` bridge and the eframe web runner.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use scoreart_engine::Session;
use scoreart_ui_egui::{Host, ScoreApp};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

const CANVAS_ID: &str = "scoreart_canvas";
const LOADING_ID: &str = "scoreart_loading";

type Inbox = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

fn query_param(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    if params.has(name) {
        Some(params.get(name).unwrap_or_default())
    } else {
        None
    }
}

/// The same-origin parent page that framed us, if any.
#[derive(Clone)]
struct Bridge {
    parent: web_sys::Window,
    origin: String,
    dirty: Rc<Cell<Option<bool>>>,
}

impl Bridge {
    fn new() -> Option<Bridge> {
        let window = web_sys::window()?;
        let parent = window.parent().ok().flatten()?;
        if js_sys::Object::is(&parent, &window) {
            return None;
        }
        let origin = window.location().origin().ok()?;
        Some(Bridge {
            parent,
            origin,
            dirty: Rc::default(),
        })
    }

    fn post(&self, fields: &[(&str, JsValue)], transfer: Option<&JsValue>) {
        let msg = js_sys::Object::new();
        for (k, v) in fields {
            let _ = js_sys::Reflect::set(&msg, &JsValue::from_str(k), v);
        }
        let result = match transfer {
            Some(t) => self
                .parent
                .post_message_with_transfer(&msg, &self.origin, &js_sys::Array::of1(t)),
            None => self.parent.post_message(&msg, &self.origin),
        };
        if result.is_err() {
            web_sys::console::error_1(&JsValue::from_str("scoreart: postMessage to the parent failed"));
        }
    }

    fn post_save(&self, name: &str, bytes: &[u8]) {
        let buf = js_sys::Uint8Array::from(bytes).buffer();
        self.post(
            &[
                ("type", "scoreart:save".into()),
                ("name", name.into()),
                ("bytes", buf.clone().into()),
            ],
            Some(&buf.into()),
        );
    }

    fn report_dirty(&self, dirty: bool) {
        if self.dirty.get() != Some(dirty) {
            self.dirty.set(Some(dirty));
            self.post(
                &[("type", "scoreart:dirty".into()), ("dirty", dirty.into())],
                None,
            );
        }
    }

    /// Hears `scoreart:open` from the parent (same origin, parent window only).
    fn listen(&self, inbox: Inbox, ctx: egui::Context) {
        let origin = self.origin.clone();
        let parent = self.parent.clone();
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            if e.origin() != origin {
                return;
            }
            let from_parent = e
                .source()
                .map(|s| js_sys::Object::is(&s, &parent))
                .unwrap_or(false);
            if !from_parent {
                return;
            }
            let data = e.data();
            let get = |k: &str| js_sys::Reflect::get(&data, &JsValue::from_str(k)).ok();
            if get("type").and_then(|t| t.as_string()).as_deref() != Some("scoreart:open") {
                return;
            }
            let name = get("name")
                .and_then(|n| n.as_string())
                .unwrap_or_else(|| "score.musicxml".into());
            let Some(bytes) = get("bytes") else { return };
            let bytes = js_sys::Uint8Array::new(&bytes).to_vec();
            inbox.borrow_mut().push((name, bytes));
            ctx.request_repaint();
        });
        if let Some(w) = web_sys::window() {
            let _ = w.add_event_listener_with_callback("message", on_message.as_ref().unchecked_ref());
        }
        on_message.forget();
    }
}

struct WebHost {
    inbox: Inbox,
    ctx: egui::Context,
    bridge: Option<Bridge>,
    audio: crate::audio::WebAudio,
    rate: u32,
}

fn download(name: &str, bytes: &[u8]) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("application/octet-stream");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

impl Host for WebHost {
    fn request_open(&mut self) {
        let inbox = self.inbox.clone();
        let ctx = self.ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let file = rfd::AsyncFileDialog::new()
                .add_filter("MusicXML", &["musicxml", "mxl", "xml"])
                .pick_file()
                .await;
            if let Some(f) = file {
                let name = f.file_name();
                let bytes = f.read().await;
                inbox.borrow_mut().push((name, bytes));
                ctx.request_repaint();
            }
        });
    }

    fn take_opened(&mut self) -> Option<(String, Vec<u8>)> {
        self.inbox.borrow_mut().pop()
    }

    fn save(&mut self, name: &str, bytes: Vec<u8>, _save_as: bool) -> Option<String> {
        if let Some(b) = &self.bridge {
            b.post_save(name, &bytes);
            return Some(name.to_string());
        }
        match download(name, &bytes) {
            Ok(()) => Some(name.to_string()),
            Err(_) => None,
        }
    }

    fn embedded(&self) -> bool {
        self.bridge.is_some()
    }

    fn audio_rate(&self) -> u32 {
        self.rate
    }

    fn audio_start(&mut self, synth: scoreart_synth::Synth) -> Result<(), String> {
        self.audio.start(synth)
    }

    fn audio_stop(&mut self) {
        self.audio.stop();
    }

    fn audio_position(&mut self) -> Option<f64> {
        self.audio.position()
    }

    fn tick(&mut self) {
        self.audio.pump();
        if self.audio.active() {
            self.ctx.request_repaint();
        }
    }
}

struct WebApp {
    ui: ScoreApp,
    bridge: Option<Bridge>,
    inbox: Inbox,
}

impl eframe::App for WebApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Files dropped on the page.
        let dropped = ui.ctx().input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        for f in dropped {
            let inbox = self.inbox.clone();
            let ctx = ui.ctx().clone();
            wasm_bindgen_futures::spawn_local(async move {
                let name = f
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "dropped.musicxml".into());
                if let Ok(bytes) = f.bytes_async().await {
                    inbox.borrow_mut().push((name, bytes));
                    ctx.request_repaint();
                }
            });
        }
        self.ui.ui(ui);
        if let Some(b) = &self.bridge {
            b.report_dirty(self.ui.session.dirty);
        }
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let title = self.ui.title();
            if doc.title() != title {
                doc.set_title(&title);
            }
        }
    }
}

pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Some(canvas) = document
            .get_element_by_id(CANVAS_ID)
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            web_sys::console::error_1(&JsValue::from_str("missing <canvas id=\"scoreart_canvas\">"));
            return;
        };
        let bridge = if query_param("host").as_deref() == Some("parent") {
            Bridge::new()
        } else {
            None
        };
        let app_bridge = bridge.clone();
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    let inbox: Inbox = Rc::default();
                    let mut session = Session::default();
                    if app_bridge.is_some() || query_param("new").is_some() {
                        let _ = session.execute("file.new", &serde_json::json!({ "measures": 16 }));
                        session.dirty = false;
                    }
                    if let Some(b) = &app_bridge {
                        b.listen(inbox.clone(), cc.egui_ctx.clone());
                    }
                    let host = WebHost {
                        inbox: inbox.clone(),
                        ctx: cc.egui_ctx.clone(),
                        bridge: app_bridge.clone(),
                        audio: crate::audio::WebAudio::default(),
                        rate: 44_100,
                    };
                    let mut host = host;
                    host.rate = host.audio.rate();
                    let ui = ScoreApp::new(&cc.egui_ctx, session, Box::new(host));
                    if let Some(b) = &app_bridge {
                        b.post(
                            &[
                                ("type", "scoreart:ready".into()),
                                ("version", env!("CARGO_PKG_VERSION").into()),
                            ],
                            None,
                        );
                    }
                    Ok(Box::new(WebApp {
                        ui,
                        bridge: app_bridge,
                        inbox,
                    }))
                }),
            )
            .await;
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!(
                    "ScoreArt could not start: {}",
                    e.as_string().unwrap_or_else(|| "WebGL2 is required".into())
                )),
            }
        }
    });
}
