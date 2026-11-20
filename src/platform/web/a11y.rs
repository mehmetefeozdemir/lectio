//! Screen readers in the browser (ADR-027, fixes P-013).
//!
//! egui draws on one `<canvas>`, so the page has nothing a screen reader can
//! read. This egui plugin turns on AccessKit, receives egui's accessibility
//! tree after every frame (`output_hook`), and keeps one invisible element
//! per readable widget in an overlay above the canvas, with the matching
//! role, name and state (`a11y_map`). The overlay ignores the mouse, so
//! pointer use is unchanged.
//!
//! The other direction: activating a mirrored control (VoiceOver's VO-Space,
//! or Enter in a screen reader's browse mode) sends egui an AccessKit
//! `Click`; typing into a mirrored text field sends `SetValue`. Both go in
//! through `input_hook` before the next frame. When keyboard focus moves
//! inside the app (Tab), a polite live region says what is now focused.
//!
//! DOM objects can't be shared between threads, and a plugin must be
//! `Send`, so the elements live in a thread-local (the page has one thread).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use eframe::egui::accesskit::{Action, ActionData, ActionRequest, NodeId, Role, TreeId};
use eframe::egui::{self, Context, FullOutput, RawInput};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Event, HtmlElement, HtmlInputElement};

use crate::platform::a11y_map::{Spec, announcement, mirror};

const OVERLAY_ID: &str = "lectio_a11y";
const LIVE_ID: &str = "lectio_a11y_live";
const OVERLAY_STYLE: &str = "position:fixed;left:0;top:0;width:100%;height:100%;pointer-events:none;overflow:hidden;opacity:0;";
const LIVE_STYLE: &str = "position:fixed;left:-10000px;width:1px;height:1px;overflow:hidden;";

struct Mirrored {
    element: HtmlElement,
    spec: Option<Spec>,
    _listeners: Vec<Closure<dyn FnMut(Event)>>,
}

#[derive(Default)]
struct State {
    overlay: Option<HtmlElement>,
    live: Option<HtmlElement>,
    elements: HashMap<NodeId, Mirrored>,
    order: Vec<NodeId>,
    focus: Option<NodeId>,
    pending: Vec<ActionRequest>,
    ctx: Option<Context>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The plugin; all its state is in `STATE`.
pub struct ScreenReaderMirror;

impl egui::Plugin for ScreenReaderMirror {
    fn debug_name(&self) -> &'static str {
        "lectio-screen-reader-mirror"
    }

    fn setup(&mut self, ctx: &Context) {
        ctx.enable_accesskit();
        STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.ctx = Some(ctx.clone());
            if let Some(document) = document() {
                state.overlay = make_region(&document, OVERLAY_ID, OVERLAY_STYLE, None);
                state.live = make_region(&document, LIVE_ID, LIVE_STYLE, Some("polite"));
            }
        });
    }

    fn input_hook(&mut self, ctx: &Context, input: &mut RawInput) {
        // egui only builds the tree while AccessKit is enabled.
        ctx.enable_accesskit();
        let requests: Vec<ActionRequest> = STATE.with(|s| std::mem::take(&mut s.borrow_mut().pending));
        input.events.extend(requests.into_iter().map(egui::Event::AccessKitActionRequest));
    }

    fn output_hook(&mut self, _ctx: &Context, output: &mut FullOutput) {
        if let Some(update) = output.platform_output.accesskit_update.as_ref() {
            let roles: HashMap<NodeId, Role> = update.nodes.iter().map(|(id, n)| (*id, n.role())).collect();
            let specs = mirror(update);
            STATE.with(|s| s.borrow_mut().sync(specs, update.focus, &roles));
        }
    }
}

fn document() -> Option<Document> {
    web_sys::window().and_then(|w| w.document())
}

fn make_region(document: &Document, id: &str, style: &str, live: Option<&str>) -> Option<HtmlElement> {
    let element: HtmlElement = document.create_element("div").ok()?.dyn_into().ok()?;
    element.set_id(id);
    element.set_attribute("style", style).ok()?;
    if let Some(politeness) = live {
        element.set_attribute("aria-live", politeness).ok()?;
        element.set_attribute("role", "status").ok()?;
    }
    document.body()?.append_child(&element).ok()?;
    Some(element)
}

/// Queues an AccessKit request for egui's next frame and asks for that frame.
fn request(target: NodeId, action: Action, data: Option<ActionData>) {
    let ctx = STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.pending.push(ActionRequest { action, target_tree: TreeId::ROOT, target_node: target, data });
        state.ctx.clone()
    });
    if let Some(ctx) = ctx {
        ctx.request_repaint();
    }
}

fn create(document: &Document, id: NodeId, spec: &Spec) -> Option<Mirrored> {
    let element: HtmlElement = document.create_element(spec.tag).ok()?.dyn_into().ok()?;
    // Reachable by a screen reader's cursor, but not by Tab: Tab stays with egui.
    element.set_attribute("tabindex", "-1").ok()?;
    let mut listeners = Vec::new();
    if spec.clickable {
        let on_click = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            event.prevent_default();
            request(id, Action::Click, None);
        });
        element.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref()).ok()?;
        listeners.push(on_click);
    }
    if spec.value.is_some() {
        let field = element.clone();
        let on_input = Closure::<dyn FnMut(Event)>::new(move |_: Event| {
            let value = field.dyn_ref::<HtmlInputElement>().map_or_else(|| field.inner_text(), HtmlInputElement::value);
            request(id, Action::Focus, None);
            request(id, Action::SetValue, Some(ActionData::Value(value.into())));
        });
        element.add_event_listener_with_callback("input", on_input.as_ref().unchecked_ref()).ok()?;
        listeners.push(on_input);
    }
    Some(Mirrored { element, spec: None, _listeners: listeners })
}

/// Writes a spec onto its element (only called when something changed).
fn apply(element: &HtmlElement, spec: &Spec, document: &Document) -> Result<(), JsValue> {
    match spec.role {
        Some(role) => element.set_attribute("role", role)?,
        None => element.remove_attribute("role")?,
    }
    if let Some(kind) = spec.input_type {
        element.set_attribute("type", kind)?;
    }
    if spec.as_text {
        element.set_text_content(Some(&spec.name));
        element.remove_attribute("aria-label")?;
    } else {
        element.set_attribute("aria-label", &spec.name)?;
    }
    for key in ["disabled", "checked", "pressed", "selected", "level", "valuenow", "valuemin", "valuemax", "expanded"] {
        let attribute = format!("aria-{key}");
        match spec.states.iter().find(|(k, _)| *k == key) {
            Some((_, value)) => element.set_attribute(&attribute, value)?,
            None => element.remove_attribute(&attribute)?,
        }
    }
    if let (Some(value), Some(input)) = (&spec.value, element.dyn_ref::<HtmlInputElement>()) {
        // Don't overwrite what the user is typing right now.
        let typing = document.active_element().is_some_and(|active| active.is_same_node(Some(element)));
        if !typing && input.value() != *value {
            input.set_value(value);
        }
    }
    if let Some([left, top, width, height]) = spec.bounds {
        let style = format!("position:absolute;left:{left}px;top:{top}px;width:{width}px;height:{height}px;margin:0;padding:0;");
        element.set_attribute("style", &style)?;
    }
    Ok(())
}

impl State {
    fn sync(&mut self, specs: Vec<(NodeId, Spec)>, focus: NodeId, roles: &HashMap<NodeId, Role>) {
        let (Some(overlay), Some(document)) = (self.overlay.clone(), document()) else { return };
        let order: Vec<NodeId> = specs.iter().map(|(id, _)| *id).collect();
        let alive: HashSet<NodeId> = order.iter().copied().collect();
        self.elements.retain(|id, m| {
            let keep = alive.contains(id);
            if !keep {
                m.element.remove();
            }
            keep
        });
        let mut order_changed = order != self.order;
        for (id, spec) in specs {
            // A widget whose kind of element changed gets a new element.
            if self.elements.get(&id).is_some_and(|m| m.spec.as_ref().is_some_and(|old| old.tag != spec.tag)) {
                if let Some(old) = self.elements.remove(&id) {
                    old.element.remove();
                }
                order_changed = true;
            }
            let mirrored = match self.elements.entry(id) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let Some(made) = create(&document, id, &spec) else { continue };
                    order_changed = true;
                    e.insert(made)
                }
            };
            if mirrored.spec.as_ref() != Some(&spec) && apply(&mirrored.element, &spec, &document).is_ok() {
                mirrored.spec = Some(spec);
            }
        }
        // Reading order follows egui's tree; re-append only when it changed.
        if order_changed {
            for id in &order {
                if let Some(m) = self.elements.get(id) {
                    let _ = overlay.append_child(&m.element);
                }
            }
            self.order = order;
        }
        self.announce(focus, roles);
    }

    /// Says the newly focused widget's name and role in the live region.
    fn announce(&mut self, focus: NodeId, roles: &HashMap<NodeId, Role>) {
        if self.focus == Some(focus) {
            return;
        }
        self.focus = Some(focus);
        let (Some(live), Some(Some(spec)), Some(role)) =
            (&self.live, self.elements.get(&focus).map(|m| m.spec.as_ref()), roles.get(&focus))
        else {
            return;
        };
        live.set_text_content(Some(&announcement(spec, *role)));
    }
}
