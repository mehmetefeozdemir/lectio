//! How the browser's screen-reader layer (ADR-027) describes each egui
//! widget in HTML: egui's AccessKit tree → one `Spec` per node worth
//! reading (tag, ARIA role, name and state), in reading order. This part
//! has no browser code, so it is tested on the desktop; `web/a11y.rs`
//! applies the specs to real elements.

use std::collections::HashMap;

use eframe::egui::accesskit::{Node, NodeId, Role, Toggled, TreeUpdate};

/// One element of the mirror.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    /// HTML tag: `button`, `input`, `textarea`, `div` or `span`.
    pub tag: &'static str,
    /// ARIA role, when the tag alone doesn't say it.
    pub role: Option<&'static str>,
    /// `type` of an `<input>`.
    pub input_type: Option<&'static str>,
    pub name: String,
    /// Text and headings carry their words as content; controls as `aria-label`.
    pub as_text: bool,
    /// The current text of a text field.
    pub value: Option<String>,
    /// `aria-*` attributes (without the prefix), e.g. `("checked", "true")`.
    pub states: Vec<(&'static str, String)>,
    /// Left, top, width, height in CSS pixels.
    pub bounds: Option<[f64; 4]>,
    /// Activating it (VO-Space, Enter) should click the egui widget.
    pub clickable: bool,
}

/// What a screen reader says for a role, in announcements.
fn role_word(role: Role) -> &'static str {
    match role {
        Role::Button | Role::DefaultButton => "button",
        Role::CheckBox => "checkbox",
        Role::Switch => "switch",
        Role::RadioButton => "radio button",
        Role::TextInput | Role::MultilineTextInput | Role::SearchInput | Role::PasswordInput => "text field",
        Role::Slider => "slider",
        Role::ComboBox => "pop-up button",
        Role::Tab => "tab",
        Role::Heading => "heading",
        Role::Image => "image",
        Role::ProgressIndicator => "progress",
        Role::Link => "link",
        _ => "",
    }
}

/// (tag, role, input type, as text) for the roles the mirror shows.
fn element_for(role: Role) -> Option<(&'static str, Option<&'static str>, Option<&'static str>, bool)> {
    Some(match role {
        Role::Button | Role::DefaultButton => ("button", None, None, false),
        Role::CheckBox => ("div", Some("checkbox"), None, false),
        Role::Switch => ("div", Some("switch"), None, false),
        Role::RadioButton => ("div", Some("radio"), None, false),
        Role::TextInput => ("input", None, Some("text"), false),
        Role::SearchInput => ("input", None, Some("search"), false),
        Role::PasswordInput => ("input", None, Some("password"), false),
        Role::MultilineTextInput => ("textarea", None, None, false),
        Role::Slider => ("div", Some("slider"), None, false),
        Role::ComboBox => ("div", Some("combobox"), None, false),
        Role::Tab => ("div", Some("tab"), None, false),
        Role::Heading => ("div", Some("heading"), None, true),
        Role::Image | Role::Canvas => ("div", Some("img"), None, false),
        Role::ProgressIndicator => ("div", Some("progressbar"), None, false),
        Role::Meter => ("div", Some("meter"), None, false),
        Role::Link => ("div", Some("link"), None, false),
        Role::ListItem => ("div", Some("listitem"), None, false),
        Role::Window | Role::Dialog => ("div", Some("dialog"), None, false),
        Role::Label | Role::Paragraph | Role::Unknown | Role::GenericContainer => ("span", None, None, true),
        _ => return None,
    })
}

/// A node's name: its own label, else the labels it points to, else its value.
fn name_of(node: &Node, labels: &HashMap<NodeId, String>) -> String {
    if let Some(label) = node.label().filter(|l| !l.trim().is_empty()) {
        return label.trim().to_owned();
    }
    let joined: Vec<&str> = node.labelled_by().iter().filter_map(|id| labels.get(id).map(String::as_str)).collect();
    if !joined.is_empty() {
        return joined.join(" ");
    }
    node.value().map(|v| v.trim().to_owned()).unwrap_or_default()
}

fn states(node: &Node, role: Role) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if node.is_disabled() {
        out.push(("disabled", "true".to_owned()));
    }
    if let Some(toggled) = node.toggled() {
        let value = match toggled {
            Toggled::True => "true",
            Toggled::False => "false",
            Toggled::Mixed => "mixed",
        };
        // A button that is on or off is "pressed"; checkboxes and switches are "checked".
        out.push((if role == Role::Button { "pressed" } else { "checked" }, value.to_owned()));
    }
    if let Some(selected) = node.is_selected() {
        out.push((if role == Role::Button { "pressed" } else { "selected" }, selected.to_string()));
    }
    if let Some(level) = node.level().filter(|_| role == Role::Heading) {
        out.push(("level", level.to_string()));
    }
    for (key, value) in [("valuenow", node.numeric_value()), ("valuemin", node.min_numeric_value()), ("valuemax", node.max_numeric_value())] {
        if let Some(v) = value {
            out.push((key, v.to_string()));
        }
    }
    if let Some(expanded) = node.is_expanded() {
        out.push(("expanded", expanded.to_string()));
    }
    out
}

/// The spec for one node, or `None` if a screen reader has nothing to read there.
pub fn spec(node: &Node, labels: &HashMap<NodeId, String>) -> Option<Spec> {
    if node.is_hidden() {
        return None;
    }
    let role = node.role();
    let (tag, aria_role, input_type, as_text) = element_for(role)?;
    let name = name_of(node, labels);
    let is_field = matches!(tag, "input" | "textarea");
    // Containers and text without words would only add silence.
    if name.is_empty() && !is_field && matches!(role, Role::Label | Role::Paragraph | Role::Unknown | Role::GenericContainer) {
        return None;
    }
    // A heading's level defaults to 2 (egui headings are section titles).
    let mut states = states(node, role);
    if role == Role::Heading && !states.iter().any(|(k, _)| *k == "level") {
        states.push(("level", "2".to_owned()));
    }
    let bounds = node.bounds().map(|r| [r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0]);
    Some(Spec {
        tag,
        role: aria_role,
        input_type,
        name,
        as_text,
        value: if is_field { Some(node.value().unwrap_or_default().to_owned()) } else { None },
        states,
        bounds,
        clickable: node.supports_action(eframe::egui::accesskit::Action::Click),
    })
}

/// Every node worth reading, in reading order (depth first from the root).
pub fn mirror(update: &TreeUpdate) -> Vec<(NodeId, Spec)> {
    let nodes: HashMap<NodeId, &Node> = update.nodes.iter().map(|(id, n)| (*id, n)).collect();
    let labels: HashMap<NodeId, String> =
        update.nodes.iter().filter_map(|(id, n)| n.label().or(n.value()).map(|l| (*id, l.trim().to_owned()))).collect();
    let root = update.tree.as_ref().map(|t| t.root).or_else(|| update.nodes.first().map(|(id, _)| *id));
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = root.into_iter().collect();
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(&id) else { continue };
        if Some(id) != root
            && let Some(spec) = spec(node, &labels)
        {
            out.push((id, spec));
        }
        stack.extend(node.children().iter().rev());
    }
    out
}

/// What the live region says when keyboard focus moves: "Start recording, button".
pub fn announcement(spec: &Spec, role: Role) -> String {
    let word = role_word(role);
    let state = spec.states.iter().find(|(k, _)| matches!(*k, "checked" | "pressed" | "selected")).map(|(_, v)| v.as_str());
    let state = match state {
        Some("true") => ", on",
        Some("false") => ", off",
        _ => "",
    };
    match (spec.name.is_empty(), word.is_empty()) {
        (false, false) => format!("{}, {word}{state}", spec.name),
        (false, true) => spec.name.clone(),
        (true, _) => word.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::accesskit::{Action, Rect, TreeId, TreeInfo};

    use super::*;

    fn node(role: Role) -> Node {
        Node::new(role)
    }

    /// A window with a button, a toggled checkbox, a heading, a text field,
    /// a nameless container, and a switch named by a separate label.
    fn tree() -> TreeUpdate {
        let mut root = node(Role::Window);
        root.set_children(vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4), NodeId(5), NodeId(7)]);
        let mut button = node(Role::Button);
        button.set_label("Start recording");
        button.add_action(Action::Click);
        button.set_bounds(Rect { x0: 10.0, y0: 20.0, x1: 110.0, y1: 50.0 });
        let mut check = node(Role::CheckBox);
        check.set_label("In collection Midterm 1");
        check.set_toggled(Toggled::True);
        let mut heading = node(Role::Heading);
        heading.set_label("Midterm 1");
        let mut field = node(Role::TextInput);
        field.set_label("Note text");
        field.set_value("BFS uses a queue");
        let mut group = node(Role::GenericContainer);
        group.set_children(vec![NodeId(6)]);
        let mut caption = node(Role::Label);
        caption.set_label("Show times");
        let mut switch = node(Role::Switch);
        switch.push_labelled_by(NodeId(6));
        switch.set_toggled(Toggled::False);
        let nodes = vec![
            (NodeId(0), root),
            (NodeId(1), button),
            (NodeId(2), check),
            (NodeId(3), heading),
            (NodeId(4), field),
            (NodeId(5), group),
            (NodeId(6), caption),
            (NodeId(7), switch),
        ];
        TreeUpdate { nodes, tree: Some(TreeInfo::new(NodeId(0))), tree_id: TreeId::ROOT, focus: NodeId(1) }
    }

    #[test]
    fn the_mirror_lists_readable_nodes_in_reading_order_without_the_root() {
        let names: Vec<String> = mirror(&tree()).into_iter().map(|(_, s)| s.name).collect();
        assert_eq!(names, vec!["Start recording", "In collection Midterm 1", "Midterm 1", "Note text", "Show times", "Show times"]);
    }

    #[test]
    fn controls_get_the_matching_html_and_state() {
        let specs: HashMap<NodeId, Spec> = mirror(&tree()).into_iter().collect();
        let button = &specs[&NodeId(1)];
        assert_eq!((button.tag, button.clickable, button.bounds), ("button", true, Some([10.0, 20.0, 100.0, 30.0])));
        let check = &specs[&NodeId(2)];
        assert_eq!((check.role, check.states.clone()), (Some("checkbox"), vec![("checked", "true".to_owned())]));
        let heading = &specs[&NodeId(3)];
        assert!(heading.as_text && heading.states.contains(&("level", "2".to_owned())));
        let field = &specs[&NodeId(4)];
        assert_eq!((field.tag, field.input_type, field.value.as_deref()), ("input", Some("text"), Some("BFS uses a queue")));
    }

    #[test]
    fn a_control_without_its_own_label_is_named_by_the_label_it_points_to() {
        let specs: HashMap<NodeId, Spec> = mirror(&tree()).into_iter().collect();
        assert_eq!(specs[&NodeId(7)].name, "Show times");
        assert!(!specs.contains_key(&NodeId(5)), "a nameless container adds nothing");
    }

    #[test]
    fn focus_announcements_say_name_role_and_state() {
        let specs: HashMap<NodeId, Spec> = mirror(&tree()).into_iter().collect();
        assert_eq!(announcement(&specs[&NodeId(1)], Role::Button), "Start recording, button");
        assert_eq!(announcement(&specs[&NodeId(2)], Role::CheckBox), "In collection Midterm 1, checkbox, on");
        assert_eq!(announcement(&specs[&NodeId(7)], Role::Switch), "Show times, switch, off");
    }
}
