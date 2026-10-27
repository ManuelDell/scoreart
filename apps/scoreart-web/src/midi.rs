//! MIDI input in the browser (Web MIDI API) and small settings in `localStorage`.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

type Queue = Rc<RefCell<Vec<Vec<u8>>>>;

#[derive(Default)]
pub struct WebMidi {
    /// The `MIDIAccess` once the browser granted it.
    access: Rc<RefCell<Option<JsValue>>>,
    requested: bool,
    queue: Queue,
    /// The open input: its name, the `MIDIInput` and the message handler kept alive.
    open: Option<(String, JsValue, Closure<dyn FnMut(JsValue)>)>,
}

/// The `MIDIInput` objects of a `MIDIAccess`.
fn inputs(access: &JsValue) -> Vec<JsValue> {
    let Ok(map) = js_sys::Reflect::get(access, &"inputs".into()) else {
        return Vec::new();
    };
    let values = js_sys::Reflect::get(&map, &"values".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(&map).ok());
    let Some(values) = values else { return Vec::new() };
    match js_sys::try_iter(&values) {
        Ok(Some(it)) => it.filter_map(Result::ok).collect(),
        _ => Vec::new(),
    }
}

fn name_of(port: &JsValue) -> String {
    js_sys::Reflect::get(port, &"name".into())
        .ok()
        .and_then(|n| n.as_string())
        .unwrap_or_default()
}

fn set_handler(port: &JsValue, f: Option<&js_sys::Function>) {
    let v = f.map_or(JsValue::NULL, |f| f.clone().into());
    let _ = js_sys::Reflect::set(port, &"onmidimessage".into(), &v);
}

impl WebMidi {
    /// Device names; the first call asks the browser for access (the list fills in later).
    pub fn ports(&mut self, ctx: &egui::Context) -> Vec<String> {
        if self.access.borrow().is_none() && !self.requested {
            self.requested = true;
            let Some(nav) = web_sys::window().map(|w| w.navigator()) else {
                return Vec::new();
            };
            let request = js_sys::Reflect::get(&nav, &"requestMIDIAccess".into())
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                .and_then(|f| f.call0(&nav).ok())
                .and_then(|p| p.dyn_into::<js_sys::Promise>().ok());
            if let Some(promise) = request {
                let slot = self.access.clone();
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(access) = wasm_bindgen_futures::JsFuture::from(promise).await {
                        *slot.borrow_mut() = Some(access);
                        ctx.request_repaint();
                    }
                });
            }
            return Vec::new();
        }
        self.access
            .borrow()
            .as_ref()
            .map(|a| inputs(a).iter().map(name_of).collect())
            .unwrap_or_default()
    }

    pub fn open(&mut self, want: Option<&str>, ctx: &egui::Context) -> Result<(), String> {
        if let Some((_, port, _)) = self.open.take() {
            set_handler(&port, None);
        }
        let Some(want) = want else { return Ok(()) };
        let access = self.access.borrow().clone();
        let Some(access) = access else {
            self.ports(ctx);
            return Err("Allow MIDI access in the browser, then choose the device again.".into());
        };
        let port = inputs(&access)
            .into_iter()
            .find(|p| name_of(p) == want)
            .ok_or_else(|| format!("MIDI device {want} not found"))?;
        let queue = self.queue.clone();
        let ctx = ctx.clone();
        let handler = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
            let data = js_sys::Reflect::get(&ev, &"data".into())
                .ok()
                .and_then(|d| d.dyn_into::<js_sys::Uint8Array>().ok())
                .map(|d| d.to_vec());
            if let Some(d) = data {
                let mut q = queue.borrow_mut();
                if q.len() < 4096 {
                    q.push(d);
                }
                ctx.request_repaint();
            }
        });
        set_handler(&port, Some(handler.as_ref().unchecked_ref()));
        self.open = Some((want.to_string(), port, handler));
        Ok(())
    }

    pub fn connected(&self) -> Option<String> {
        self.open.as_ref().map(|(n, ..)| n.clone())
    }

    pub fn take(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.queue.borrow_mut())
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub fn load_setting(name: &str) -> Option<String> {
    storage()?.get_item(&format!("scoreart.{name}")).ok()?
}

pub fn save_setting(name: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(&format!("scoreart.{name}"), value);
    }
}
