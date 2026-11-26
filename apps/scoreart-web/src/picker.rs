//! Choosing a file in the browser, in ScoreArt's look.
//!
//! A click on *Open* opens the system's file dialog right away. Browsers allow that only shortly
//! after a click or key press; when the moment has passed, a small window in ScoreArt's colours
//! asks for one more click (on a real button of the page, which browsers accept) or takes a file
//! dropped onto it.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Promise, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, DragEvent, Element, File, HtmlElement, HtmlInputElement};

use scoreart_ui_egui::i18n::t;

/// What the window says and which files it takes.
pub struct Ask {
    /// File name extensions without the dot.
    pub extensions: &'static [&'static str],
    /// The heading of the window ("Open score").
    pub title: &'static str,
    /// What kind of files, under the heading.
    pub what: &'static str,
    /// The page is dark.
    pub dark: bool,
}

/// Lets the user choose a file; its name and contents, or None when cancelled.
pub async fn pick(ask: Ask) -> Option<(String, Vec<u8>)> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let body = document.body()?;
    let input: HtmlInputElement = document.create_element("input").ok()?.dyn_into().ok()?;
    input.set_type("file");
    let accept: Vec<String> = ask.extensions.iter().map(|e| format!(".{e}")).collect();
    input.set_accept(&accept.join(","));
    input.set_id("scoreart-file-input");
    // Hidden off the page, not with display:none (which some browsers do not open).
    let _ = input.set_attribute(
        "style",
        "position:fixed;left:-10000px;top:0;width:1px;height:1px;opacity:0;pointer-events:none",
    );
    let _ = input.set_attribute("tabindex", "-1");
    let _ = input.set_attribute("aria-hidden", "true");
    let _ = body.append_child(&input);

    // Resolved with the chosen File, or null.
    let done: Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
    let promise = {
        let done = done.clone();
        Promise::new(&mut move |resolve, _reject| {
            *done.borrow_mut() = Some(resolve);
        })
    };
    let finish = {
        let done = done.clone();
        move |v: JsValue| {
            if let Some(f) = done.borrow_mut().take() {
                let _ = f.call1(&JsValue::NULL, &v);
            }
        }
    };
    let mut keep: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();
    {
        let changed = finish.clone();
        let input2 = input.clone();
        let on_change = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            let file = input2.files().and_then(|l| l.get(0));
            changed(file.map_or(JsValue::NULL, JsValue::from));
        });
        let _ = input.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref());
        keep.push(on_change);
        let cancelled = finish.clone();
        let on_cancel =
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| cancelled(JsValue::NULL));
        let _ = input.add_event_listener_with_callback("cancel", on_cancel.as_ref().unchecked_ref());
        keep.push(on_cancel);
    }

    // Straight to the system's dialog while the click still counts; else the window.
    let overlay = if user_active(&window) {
        input.click();
        None
    } else {
        window_for(&document, &body, &input, &ask, &finish, &mut keep)
    };

    let chosen = JsFuture::from(promise).await.ok();
    if let Some(o) = overlay {
        o.remove();
    }
    input.remove();
    drop(keep);
    let file: File = chosen?.dyn_into().ok()?;
    let name = file.name();
    let buf = JsFuture::from(file.array_buffer()).await.ok()?;
    Some((name, Uint8Array::new(&buf).to_vec()))
}

/// Whether the browser still counts the last click or key press (so a file dialog may open).
fn user_active(window: &web_sys::Window) -> bool {
    let nav = window.navigator();
    Reflect::get(&nav, &JsValue::from_str("userActivation"))
        .ok()
        .filter(|a| !a.is_undefined() && !a.is_null())
        .and_then(|a| Reflect::get(&a, &JsValue::from_str("isActive")).ok())
        .and_then(|v| v.as_bool())
        // Browsers without the API: try; if they refuse, the next click opens the window.
        .unwrap_or(true)
}

/// The window in ScoreArt's look: heading, a button that opens the system's dialog, a place to
/// drop a file, and Cancel (also Escape or a click beside the window).
fn window_for(
    document: &Document,
    body: &HtmlElement,
    input: &HtmlInputElement,
    ask: &Ask,
    finish: &(impl Fn(JsValue) + Clone + 'static),
    keep: &mut Vec<Closure<dyn FnMut(web_sys::Event)>>,
) -> Option<Element> {
    let (bg, fg, soft, line, accent, accent_soft, shade) = if ask.dark {
        (
            "#24242a",
            "#e8e8ee",
            "#a0a0ab",
            "#3a3a44",
            "#9171f8",
            "#3b2f6b",
            "rgba(0,0,0,0.55)",
        )
    } else {
        (
            "#f8f8fa",
            "#1f1f24",
            "#6b6b76",
            "#dcdce3",
            "#7048e8",
            "#e5dbff",
            "rgba(20,20,30,0.35)",
        )
    };
    let overlay = document.create_element("div").ok()?;
    overlay.set_id("scoreart-picker");
    let css = format!(
        "#scoreart-picker{{position:fixed;inset:0;z-index:1000;display:flex;align-items:center;justify-content:center;\
background:{shade};font-family:system-ui,-apple-system,'Segoe UI',Ubuntu,sans-serif;animation:sa-in .15s ease-out}}\
#scoreart-picker .card{{background:{bg};color:{fg};border:1px solid {line};border-radius:12px;padding:22px 24px 18px;\
width:min(380px,calc(100vw - 32px));box-shadow:0 18px 50px rgba(0,0,0,.25)}}\
#scoreart-picker h2{{margin:0 0 4px;font-size:17px;font-weight:600}}\
#scoreart-picker p{{margin:0 0 16px;font-size:13px;color:{soft}}}\
#scoreart-picker .drop{{display:flex;flex-direction:column;align-items:center;gap:10px;padding:20px 12px;\
border:1.5px dashed {line};border-radius:10px;transition:border-color .1s,background .1s}}\
#scoreart-picker .drop.over{{border-color:{accent};background:{accent_soft}}}\
#scoreart-picker .choose{{display:inline-block;background:{accent};color:#fff;border-radius:8px;padding:9px 18px;\
font-size:14px;font-weight:600;cursor:pointer;user-select:none}}\
#scoreart-picker .choose:hover{{filter:brightness(1.08)}}\
#scoreart-picker .choose:focus{{outline:none}}\
#scoreart-picker .choose:focus-visible,#scoreart-picker .cancel:focus-visible{{outline:2px solid {accent};outline-offset:2px}}\
#scoreart-picker .hint{{font-size:12px;color:{soft}}}\
#scoreart-picker .row{{display:flex;justify-content:flex-end;margin-top:14px}}\
#scoreart-picker .cancel{{background:transparent;color:{fg};border:1px solid {line};border-radius:8px;padding:7px 14px;\
font-size:13px;cursor:pointer;font-family:inherit}}\
#scoreart-picker .cancel:hover{{background:{line}}}\
@keyframes sa-in{{from{{opacity:0}}to{{opacity:1}}}}"
    );
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    overlay.set_inner_html(&format!(
        "<style>{css}</style><div class=\"card\" role=\"dialog\" aria-modal=\"true\">\
<h2>{title}</h2><p>{what}</p>\
<div class=\"drop\"><label class=\"choose\" for=\"scoreart-file-input\" tabindex=\"0\">{choose}</label>\
<span class=\"hint\">{hint}</span></div>\
<div class=\"row\"><button class=\"cancel\" type=\"button\">{cancel}</button></div></div>",
        title = esc(t(ask.title)),
        what = esc(t(ask.what)),
        choose = esc(t("Choose file…")),
        hint = esc(t("or drop it here")),
        cancel = esc(t("Cancel")),
    ));
    let _ = body.append_child(&overlay);

    let listen = |el: &Element, ev: &str, f: Closure<dyn FnMut(web_sys::Event)>, keep: &mut Vec<_>| {
        let _ = el.add_event_listener_with_callback(ev, f.as_ref().unchecked_ref());
        keep.push(f);
    };
    // Cancel: the button, a click beside the card, Escape.
    if let Ok(Some(b)) = overlay.query_selector(".cancel") {
        let finish = finish.clone();
        listen(
            &b,
            "click",
            Closure::new(move |_e: web_sys::Event| finish(JsValue::NULL)),
            keep,
        );
    }
    {
        let finish = finish.clone();
        let o = overlay.clone();
        listen(
            &overlay,
            "click",
            Closure::new(move |e: web_sys::Event| {
                if e.target()
                    .and_then(|t| t.dyn_into::<Element>().ok())
                    .is_some_and(|t| t == o)
                {
                    finish(JsValue::NULL);
                }
            }),
            keep,
        );
    }
    {
        let finish = finish.clone();
        let input = input.clone();
        listen(
            &overlay,
            "keydown",
            Closure::new(move |e: web_sys::Event| {
                let key = Reflect::get(&e, &JsValue::from_str("key"))
                    .ok()
                    .and_then(|k| k.as_string());
                match key.as_deref() {
                    Some("Escape") => finish(JsValue::NULL),
                    Some("Enter") | Some(" ") => input.click(),
                    _ => {}
                }
            }),
            keep,
        );
    }
    if let Ok(Some(label)) = overlay.query_selector(".choose")
        && let Ok(l) = label.dyn_into::<HtmlElement>()
    {
        let _ = l.focus();
    }
    // Dropping a file onto the window.
    if let Ok(Some(drop)) = overlay.query_selector(".drop") {
        for (ev, over) in [("dragenter", true), ("dragover", true), ("dragleave", false)] {
            let d = drop.clone();
            listen(
                &drop,
                ev,
                Closure::new(move |e: web_sys::Event| {
                    e.prevent_default();
                    let _ = if over {
                        d.class_list().add_1("over")
                    } else {
                        d.class_list().remove_1("over")
                    };
                }),
                keep,
            );
        }
        let finish = finish.clone();
        listen(
            &drop,
            "drop",
            Closure::new(move |e: web_sys::Event| {
                e.prevent_default();
                let file = e
                    .dyn_into::<DragEvent>()
                    .ok()
                    .and_then(|d| d.data_transfer())
                    .and_then(|t| t.files())
                    .and_then(|l| l.get(0));
                if let Some(f) = file {
                    finish(JsValue::from(f));
                }
            }),
            keep,
        );
    }
    Some(overlay)
}
