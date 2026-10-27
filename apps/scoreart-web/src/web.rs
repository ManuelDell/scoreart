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
/// Sound libraries the user picked, read and parsed.
type SoundInbox = Rc<RefCell<Vec<Result<scoreart_synth::SoundBank, String>>>>;

/// Fetches a file (the catalogue's sources allow cross-origin reads).
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let err = |e: JsValue| format!("{url}: {e:?}");
    let window = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(err)?
        .dyn_into()
        .map_err(err)?;
    if !resp.ok() {
        return Err(format!("{url}: HTTP {}", resp.status()));
    }
    let buf = wasm_bindgen_futures::JsFuture::from(resp.array_buffer().map_err(err)?)
        .await
        .map_err(err)?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

/// What the host page (e.g. Nextcloud, set up by its administrator) allows and provides.
#[derive(Debug)]
pub struct HostConfig {
    /// Catalogue libraries may be fetched from the internet.
    internet: bool,
    /// Per catalogue library (by name): offered at all, and the server copy (base URL ending in
    /// `/`, laid out like the library's repository) with its instruments.
    libraries: std::collections::HashMap<String, LibraryRule>,
    /// Sound files on the server: name, URL, load at start.
    files: Vec<(String, String, bool)>,
}

#[derive(Debug, Default)]
struct LibraryRule {
    enabled: bool,
    base: Option<String>,
    instruments: Vec<String>,
}

impl Default for HostConfig {
    fn default() -> Self {
        HostConfig {
            internet: true,
            libraries: Default::default(),
            files: Vec::new(),
        }
    }
}

impl HostConfig {
    /// Reads a `scoreart:config` message (unknown or malformed fields keep the defaults).
    fn from_json(v: &serde_json::Value) -> HostConfig {
        let mut c = HostConfig::default();
        if let Some(b) = v.get("internet").and_then(|b| b.as_bool()) {
            c.internet = b;
        }
        for l in v
            .get("libraries")
            .and_then(|l| l.as_array())
            .into_iter()
            .flatten()
        {
            let Some(name) = l.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let base = l
                .get("base")
                .and_then(|b| b.as_str())
                .filter(|b| b.starts_with('/') && b.ends_with('/'))
                .map(str::to_string);
            let instruments = l
                .get("instruments")
                .and_then(|i| i.as_array())
                .into_iter()
                .flatten()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect();
            c.libraries.insert(
                name.to_string(),
                LibraryRule {
                    enabled: l.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                    base,
                    instruments,
                },
            );
        }
        for f in v.get("files").and_then(|f| f.as_array()).into_iter().flatten() {
            let (Some(name), Some(url)) = (
                f.get("name").and_then(|n| n.as_str()),
                f.get("url")
                    .and_then(|u| u.as_str())
                    .filter(|u| u.starts_with('/')),
            ) else {
                continue;
            };
            let auto = f.get("autoload").and_then(|a| a.as_bool()).unwrap_or(false);
            c.files.push((name.to_string(), url.to_string(), auto));
        }
        c
    }
}

/// Fetches a sound file the host provides and reads it as a library.
async fn load_host_file(name: String, url: String) -> Result<scoreart_synth::SoundBank, String> {
    let bytes = fetch_bytes(&url).await?;
    scoreart_synth::SoundBank::from_bytes(&name, bytes)
}

/// Downloads one catalogue instrument into the library's in-memory store, from `base` (the
/// publisher's repository, or a server copy laid out the same way).
async fn download_instrument(
    lib: &'static scoreart_synth::catalog::Library,
    base: String,
    instrument: String,
    mem: scoreart_synth::files::Mem,
    status: Rc<RefCell<Option<scoreart_ui_egui::DownloadStatus>>>,
    cancel: Rc<Cell<bool>>,
) -> Result<scoreart_synth::SoundBank, String> {
    use scoreart_synth::catalog::{self, NOTICE_FILE};
    use scoreart_synth::files::Files as _;
    let accepted = scoreart_ui_egui::acceptance_text(lib, (js_sys::Date::now() / 1000.0) as u64);
    let previous = mem
        .read(NOTICE_FILE)
        .map(|b| String::from_utf8_lossy(&b).to_string());
    mem.insert(
        NOTICE_FILE,
        catalog::notice(lib, &instrument, &accepted, previous.as_deref()).into_bytes(),
    );
    let mut p = catalog::Progress::default();
    for _round in 0..32 {
        let todo = catalog::next_files(lib, &mem, &instrument)?;
        if todo.is_empty() {
            return scoreart_synth::SoundBank::from_files(lib.name, Box::new(mem));
        }
        p.files_total = p.files_done + todo.len();
        for f in todo {
            if cancel.get() {
                return Err("download cancelled".into());
            }
            p.file = f.clone();
            if let Some(s) = status.borrow_mut().as_mut() {
                s.progress = p.clone();
            }
            let bytes = fetch_bytes(&catalog::file_url(&base, &f)).await?;
            p.bytes += bytes.len() as u64;
            mem.insert(&f, bytes);
            p.files_done += 1;
        }
    }
    Err("the instrument refers to too many nested files".into())
}

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
    /// The host's settings, once it sent them.
    config: Rc<RefCell<HostConfig>>,
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
            config: Rc::default(),
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

    /// Hears `scoreart:open` and `scoreart:config` from the parent (same origin, parent window
    /// only). Sound files the configuration marks for loading at start go to `sounds`.
    fn listen(&self, inbox: Inbox, sounds: SoundInbox, ctx: egui::Context) {
        let origin = self.origin.clone();
        let parent = self.parent.clone();
        let config = self.config.clone();
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
            let kind = get("type").and_then(|t| t.as_string());
            if kind.as_deref() == Some("scoreart:config") {
                let json = js_sys::JSON::stringify(&data)
                    .ok()
                    .and_then(|s| s.as_string())
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
                if let Some(v) = json {
                    let c = HostConfig::from_json(&v);
                    for (name, url, auto) in &c.files {
                        if *auto {
                            let (name, url, sounds, ctx) =
                                (name.clone(), url.clone(), sounds.clone(), ctx.clone());
                            wasm_bindgen_futures::spawn_local(async move {
                                let r = load_host_file(name, url).await;
                                sounds.borrow_mut().push(r);
                                ctx.request_repaint();
                            });
                        }
                    }
                    *config.borrow_mut() = c;
                    ctx.request_repaint();
                }
                return;
            }
            if kind.as_deref() != Some("scoreart:open") {
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
    sounds: SoundInbox,
    /// Downloaded catalogue files per library (kept for this browser session only).
    stores: Rc<RefCell<std::collections::HashMap<&'static str, scoreart_synth::files::Mem>>>,
    download: Rc<RefCell<Option<scoreart_ui_egui::DownloadStatus>>>,
    cancel: Rc<Cell<bool>>,
    midi: crate::midi::WebMidi,
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

impl WebHost {
    /// The host page's settings, when ScoreArt runs inside one.
    fn config(&self) -> Option<Rc<RefCell<HostConfig>>> {
        self.bridge.as_ref().map(|b| b.config.clone())
    }
}

impl Host for WebHost {
    fn request_open(&mut self) {
        let inbox = self.inbox.clone();
        let ctx = self.ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let file = rfd::AsyncFileDialog::new()
                .add_filter(
                    "Scores (MusicXML, MuseScore, MIDI)",
                    &["musicxml", "mxl", "xml", "mscz", "mscx", "mid", "midi"],
                )
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

    fn request_sounds(&mut self, _folder: bool) {
        let inbox = self.sounds.clone();
        let ctx = self.ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            // Browsers cannot open folders: SFZ libraries come as a zip with their samples.
            let file = rfd::AsyncFileDialog::new()
                .add_filter("Sound libraries (SoundFont .sf2, zip of SFZ)", &["sf2", "zip"])
                .pick_file()
                .await;
            if let Some(f) = file {
                let name = f.file_name();
                let bytes = f.read().await;
                let bank =
                    scoreart_synth::SoundBank::from_bytes(&name, bytes).map_err(|e| format!("{name}: {e}"));
                inbox.borrow_mut().push(bank);
                ctx.request_repaint();
            }
        });
    }

    fn take_sounds(&mut self) -> Option<Result<scoreart_synth::SoundBank, String>> {
        self.sounds.borrow_mut().pop()
    }

    fn can_download(&self) -> bool {
        true
    }

    fn download_sounds(&mut self, library: &'static scoreart_synth::catalog::Library, instrument: &str) {
        if self.download.borrow().is_some() {
            return;
        }
        let config = self.config();
        let rule = config.as_ref().and_then(|c| {
            c.borrow()
                .libraries
                .get(library.name)
                .map(|r| (r.base.clone(), r.instruments.contains(&instrument.to_string())))
        });
        let internet = config.as_ref().is_none_or(|c| c.borrow().internet);
        let base = match (rule, &library.source) {
            (Some((Some(base), true)), _) => base,
            (_, scoreart_synth::catalog::Source::Repository { base, .. }) if internet => base.to_string(),
            _ => return,
        };
        let mem = self.stores.borrow_mut().entry(library.id).or_default().clone();
        *self.download.borrow_mut() = Some(scoreart_ui_egui::DownloadStatus {
            library: library.name.to_string(),
            instrument: instrument.to_string(),
            progress: Default::default(),
        });
        self.cancel.set(false);
        let (status, cancel, inbox, ctx) = (
            self.download.clone(),
            self.cancel.clone(),
            self.sounds.clone(),
            self.ctx.clone(),
        );
        let instrument = instrument.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let r = download_instrument(library, base, instrument, mem, status.clone(), cancel)
                .await
                .map_err(|e| format!("{}: {e}", library.name));
            *status.borrow_mut() = None;
            inbox.borrow_mut().push(r);
            ctx.request_repaint();
        });
    }

    fn download_status(&self) -> Option<scoreart_ui_egui::DownloadStatus> {
        self.download.borrow().clone()
    }

    fn cancel_download(&mut self) {
        self.cancel.set(true);
    }

    fn host_language(&self) -> Option<String> {
        query_param("lang").filter(|l| !l.trim().is_empty())
    }

    fn library_allowed(&self, library: &str) -> bool {
        self.config()
            .is_none_or(|c| c.borrow().libraries.get(library).is_none_or(|r| r.enabled))
    }

    fn provided_instruments(&self, library: &str) -> Option<Vec<String>> {
        let c = self.config()?;
        let c = c.borrow();
        let r = c.libraries.get(library)?;
        r.base.as_ref().map(|_| r.instruments.clone())
    }

    fn internet_downloads(&self) -> bool {
        self.config().is_none_or(|c| c.borrow().internet)
    }

    fn host_sounds(&self) -> Vec<String> {
        self.config()
            .map(|c| c.borrow().files.iter().map(|f| f.0.clone()).collect())
            .unwrap_or_default()
    }

    fn load_host_sounds(&mut self, name: &str) {
        let Some(url) = self
            .config()
            .and_then(|c| c.borrow().files.iter().find(|f| f.0 == name).map(|f| f.1.clone()))
        else {
            return;
        };
        let (name, sounds, ctx) = (name.to_string(), self.sounds.clone(), self.ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let r = load_host_file(name, url).await;
            sounds.borrow_mut().push(r);
            ctx.request_repaint();
        });
    }

    fn midi_ports(&mut self) -> Vec<String> {
        self.midi.ports(&self.ctx)
    }

    fn midi_open(&mut self, port: Option<&str>) -> Result<(), String> {
        self.midi.open(port, &self.ctx)
    }

    fn midi_connected(&self) -> Option<String> {
        self.midi.connected()
    }

    fn take_midi(&mut self) -> Vec<Vec<u8>> {
        self.midi.take()
    }

    fn locale(&self) -> Option<String> {
        web_sys::window()?.navigator().language()
    }

    fn load_setting(&self, name: &str) -> Option<String> {
        crate::midi::load_setting(name)
    }

    fn save_setting(&mut self, name: &str, value: &str) {
        crate::midi::save_setting(name, value);
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
                    let sounds = SoundInbox::default();
                    let mut session = Session::default();
                    // The language first, so a new score gets its title in it.
                    use scoreart_ui_egui::i18n::{Lang, set_lang, t};
                    let lang = query_param("lang")
                        .or_else(|| crate::midi::load_setting("language"))
                        .or_else(|| web_sys::window().and_then(|w| w.navigator().language()));
                    set_lang(lang.map_or(Lang::En, |l| Lang::from_code(&l)));
                    if app_bridge.is_some() || query_param("new").is_some() {
                        let _ = session.execute(
                            "file.new",
                            &serde_json::json!({ "measures": 16, "title": t("Untitled score") }),
                        );
                        session.dirty = false;
                    }
                    if let Some(b) = &app_bridge {
                        b.listen(inbox.clone(), sounds.clone(), cc.egui_ctx.clone());
                    }
                    let host = WebHost {
                        inbox: inbox.clone(),
                        ctx: cc.egui_ctx.clone(),
                        bridge: app_bridge.clone(),
                        audio: crate::audio::WebAudio::default(),
                        rate: 44_100,
                        sounds,
                        stores: Rc::default(),
                        download: Rc::default(),
                        cancel: Rc::default(),
                        midi: Default::default(),
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
