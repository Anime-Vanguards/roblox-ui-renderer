//! Sibling order replay for `rur compare` (port of `runtime/engine/order.luau`; RUR_ORDER = the
//! Studio dump the compare wrote, studio-nodes.json). A list built by iterating a table keyed by
//! tables or instances (vide `values()` keys its scopes by the list's values) comes out in an
//! address-dependent order: Luau hashes pointers, so Studio's GetChildren order for it changes
//! between sessions and ours between processes. No emulation reproduces that order, so the compare
//! replays Studio's the way it replays its clock and random seed: after the run, the children that
//! pair one to one with the dump's are put in the dump's order. Deterministic GetChildren orders
//! (vide `create`'s positional children, Clone) match Studio's already.

use std::rc::Rc;

use mlua::prelude::*;
use rustc_hash::FxHashMap;
use serde_json::{Value as Json, json};

use crate::engine::{
    Engine, InstanceId, K, NameId,
    lua::{convert, instance as lua_instance},
    value::Value,
};

/// Signatures stop after this many texts / images (enough to tell rows apart)
const MAX_PARTS: usize = 32;

/// Pairing cost of two same-named siblings that show different content (logical px otherwise)
const CONTENT_MISMATCH: f64 = 1e6;

struct Entry {
    name: String,
    path: String,
    folder: bool,
    node: Option<Json>,
    children: Vec<usize>,
}

#[derive(Default)]
struct Summary {
    parents: u64,
    moved: u64,
    unpaired: u64,
}

/// The dump lists GuiBase2d objects depth first in GetChildren order with "/"-joined paths
/// relative to the story target; Folders are walked through without an entry of their own (their
/// name is a path segment), so they get one here. Entries live in `arena`; index 0 is the root.
fn build_tree(nodes: &[Json]) -> Vec<Entry> {
    let mut arena = vec![Entry { name: String::new(), path: String::new(), folder: true, node: None, children: Vec::new() }];
    let mut stack: Vec<usize> = vec![0];
    for node in nodes {
        let path = match node.get("path") {
            Some(Json::String(path)) => path.clone(),
            _ => continue,
        };
        while stack.len() > 1 {
            let top = &arena[*stack.last().unwrap()].path;
            let prefix = format!("{top}/");
            if path.as_bytes().starts_with(prefix.as_bytes()) {
                break;
            }
            stack.pop();
        }
        let mut parent = *stack.last().unwrap();
        let rest = path.get(arena[parent].path.len() + 1..).unwrap_or("");
        let segments: Vec<&str> = rest.split('/').collect();
        for segment in &segments[..segments.len() - 1] {
            let last = arena[parent].children.last().copied();
            let reuse = last.filter(|&l| arena[l].folder && arena[l].name == *segment);
            let folder = match reuse {
                Some(existing) => existing,
                None => {
                    let path = format!("{}/{}", arena[parent].path, segment);
                    arena.push(Entry { name: segment.to_string(), path, folder: true, node: None, children: Vec::new() });
                    let index = arena.len() - 1;
                    arena[parent].children.push(index);
                    index
                }
            };
            parent = folder;
            stack.push(folder);
        }
        let name = segments.last().copied().unwrap_or("").to_string();
        arena.push(Entry { name, path, folder: false, node: Some(node.clone()), children: Vec::new() });
        let index = arena.len() - 1;
        arena[parent].children.push(index);
        stack.push(index);
    }
    arena
}

/// Luau tostring of a decoded JSON scalar.
fn tostring(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        Json::Number(n) => {
            let v = n.as_f64().unwrap_or(0.0);
            if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
        }
        Json::Bool(b) => b.to_string(),
        Json::Null => "nil".into(),
        _ => "table".into(),
    }
}

/// The first run of digits of an asset URI (else the text).
fn asset_key(text: &str) -> String {
    let bytes = text.as_bytes();
    let Some(start) = bytes.iter().position(u8::is_ascii_digit) else { return text.to_string() };
    let end = bytes[start..].iter().position(|b| !b.is_ascii_digit()).map_or(bytes.len(), |n| start + n);
    text[start..end].to_string()
}

struct Context<'a> {
    lua: &'a Lua,
    engine: &'a Rc<Engine>,
    target: (f64, f64),
    gui_base: Option<u32>,
    folder: Option<u32>,
    text_classes: Vec<u32>,
    image_classes: Vec<u32>,
    image: Option<NameId>,
}

impl Context<'_> {
    /// The children the dump would list: GuiBase2d objects and Folders
    fn listed(&self, id: InstanceId) -> Vec<InstanceId> {
        let state = self.engine.state.borrow();
        state.dom.at(id).children.iter().copied().filter(|&child| self.is_listed(child)).collect()
    }

    fn is_listed(&self, id: InstanceId) -> bool {
        let class = self.engine.state.borrow().dom.at(id).class;
        self.gui_base.is_some_and(|c| self.engine.reflection.class(class).is_a(c)) || Some(class) == self.folder
    }

    /// What a subtree shows (its texts and image ids, sorted: the order inside may differ as well)
    fn studio_parts(&self, arena: &[Entry], entry: usize, parts: &mut Vec<String>) {
        if parts.len() >= MAX_PARTS {
            return;
        }
        if let Some(node) = &arena[entry].node {
            if let Some(text) = node.get("text").filter(|t| !t.is_null()) {
                parts.push(format!("T{}", tostring(text)));
            }
            if let Some(Json::String(image)) = node.get("props").and_then(|p| p.get("image")) {
                if !image.is_empty() {
                    parts.push(format!("I{}", asset_key(image)));
                }
            }
        }
        for &child in &arena[entry].children {
            self.studio_parts(arena, child, parts);
        }
    }

    fn offline_parts(&self, id: InstanceId, parts: &mut Vec<String>) {
        if parts.len() >= MAX_PARTS {
            return;
        }
        {
            let state = self.engine.state.borrow();
            let class = state.dom.at(id).class;
            if self.text_classes.contains(&class) {
                let text = match state.dom.raw_get(id, K::Text as NameId) {
                    Some(Value::String(text)) => text.to_str_lossy().into_owned(),
                    Some(Value::Nil) | None => String::new(),
                    Some(other) => format!("{other:?}"),
                };
                parts.push(format!("T{text}"));
            } else if self.image_classes.contains(&class) {
                let image = self.image.and_then(|n| state.dom.raw_get(id, n));
                let text = match image {
                    Some(Value::String(text)) if !text.as_bytes().is_empty() => Some(text.to_str_lossy().into_owned()),
                    Some(Value::Content(content)) => Some(content.uri().map(|u| u.to_str_lossy().into_owned()).unwrap_or_default()),
                    _ => None,
                };
                if let Some(text) = text {
                    let key = asset_key(&text);
                    if !key.is_empty() {
                        parts.push(format!("I{key}"));
                    }
                }
            }
        }
        for child in self.listed(id) {
            self.offline_parts(child, parts);
        }
    }

    fn vector(&self, id: InstanceId, name: &str) -> Option<(f64, f64)> {
        let key = LuaValue::String(self.lua.create_string(name).ok()?);
        let value = lua_instance::get(self.lua, self.engine, id, &key).ok()?;
        let vector = convert::from_lua(&value).as_vector2()?;
        Some((vector.x as f64, vector.y as f64))
    }

    /// Studio's rect of a dump entry against ours (AbsolutePosition / AbsoluteSize, both relative to
    /// the story target); Folders have none
    fn rect_distance(&self, node: Option<&Json>, id: InstanceId) -> f64 {
        let Some(node) = node else { return 0.0 };
        let class = self.engine.state.borrow().dom.at(id).class;
        if !self.gui_base.is_some_and(|c| self.engine.reflection.class(class).is_a(c)) {
            return 0.0;
        }
        let (px, py) = self.vector(id, "AbsolutePosition").unwrap_or((0.0, 0.0));
        let (sx, sy) = self.vector(id, "AbsoluteSize").unwrap_or((0.0, 0.0));
        let field = |key: &str| node.get(key).and_then(Json::as_f64).unwrap_or(f64::NAN);
        (px - (field("x") - self.target.0)).abs()
            + (py - (field("y") - self.target.1)).abs()
            + (sx - field("w")).abs()
            + (sy - field("h")).abs()
    }

    fn name_of(&self, id: InstanceId) -> Vec<u8> {
        self.engine.state.borrow().dom.at(id).name.as_bytes().to_vec()
    }

    /// Pairs Studio's children of one parent with ours: by name, same-named ones by what they
    /// show, then by place (greedy, cheapest first). Returns (entry, instance) pairs.
    fn pair(&self, arena: &[Entry], entries: &[usize], proxies: &[InstanceId]) -> FxHashMap<usize, InstanceId> {
        let mut pairs = FxHashMap::default();
        let mut groups: Vec<(Vec<u8>, Vec<usize>, Vec<InstanceId>)> = Vec::new();
        for &entry in entries {
            let name = arena[entry].name.as_bytes();
            match groups.iter_mut().find(|g| g.0 == name) {
                Some(group) => group.1.push(entry),
                None => groups.push((name.to_vec(), vec![entry], Vec::new())),
            }
        }
        for &proxy in proxies {
            let name = self.name_of(proxy);
            if let Some(group) = groups.iter_mut().find(|g| g.0 == name) {
                group.2.push(proxy);
            }
        }
        for (_, studio, ours) in groups {
            if studio.len() == 1 && ours.len() == 1 {
                pairs.insert(studio[0], ours[0]);
                continue;
            }
            let signature = |mut parts: Vec<String>| {
                parts.sort();
                parts.join("\0")
            };
            let our_signatures: Vec<String> = ours
                .iter()
                .map(|&proxy| {
                    let mut parts = Vec::new();
                    self.offline_parts(proxy, &mut parts);
                    signature(parts)
                })
                .collect();
            // (cost, position, index)
            let mut candidates: Vec<(f64, usize, usize, usize)> = Vec::new();
            for (position, &entry) in studio.iter().enumerate() {
                let mut parts = Vec::new();
                self.studio_parts(arena, entry, &mut parts);
                let studio_signature = signature(parts);
                for (index, &proxy) in ours.iter().enumerate() {
                    let cost = if our_signatures[index] == studio_signature { 0.0 } else { CONTENT_MISMATCH };
                    let cost = cost + self.rect_distance(arena[entry].node.as_ref(), proxy);
                    candidates.push((cost, position + 1, index + 1, entry));
                }
            }
            // Ties keep the dump's order, then ours
            candidates.sort_by(|a, b| {
                a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
            });
            let mut taken = vec![false; ours.len() + 1];
            for (_, _, index, entry) in candidates {
                if !pairs.contains_key(&entry) && !taken[index] {
                    pairs.insert(entry, ours[index - 1]);
                    taken[index] = true;
                }
            }
        }
        pairs
    }

    fn replay(&self, arena: &[Entry], entry: usize, id: InstanceId, summary: &mut Summary) {
        let proxies = self.listed(id);
        let children = &arena[entry].children;
        let pairs = self.pair(arena, children, &proxies);
        let mut paired = 0;
        for child in children {
            if pairs.contains_key(child) {
                paired += 1;
            } else {
                summary.unpaired += 1;
            }
        }

        // Only a one-to-one match is reordered: the listed children take the listed slots of the
        // children array in the dump's order (layouts, constraints and decorations keep their places)
        if paired == children.len() && paired == proxies.len() && paired > 1 {
            let mut moved = 0;
            {
                let listed: Vec<bool> = {
                    let state = self.engine.state.borrow();
                    state.dom.at(id).children.iter().map(|&c| self.is_listed(c)).collect()
                };
                let mut state = self.engine.state.borrow_mut();
                let instance = state.dom.at_mut(id);
                let mut slot = 0;
                for (index, is_listed) in listed.into_iter().enumerate() {
                    if is_listed {
                        let wanted = pairs[&children[slot]];
                        slot += 1;
                        if wanted != instance.children[index] {
                            moved += 1;
                        }
                        instance.children[index] = wanted;
                    }
                }
                if moved > 0 {
                    instance.child_version = instance.child_version.wrapping_add(1);
                    state.dom.structure_version += 1;
                }
            }
            if moved > 0 {
                summary.parents += 1;
                summary.moved += moved;
                // Hooks.HierarchyChanged(child, parent, parent) for the layout and export caches
                let hooks = self.engine.hooks.borrow();
                let mut state = self.engine.state.borrow_mut();
                for &child in &proxies {
                    for hook in &hooks.hierarchy_changed {
                        hook(&mut state, child, Some(id), Some(id));
                    }
                }
            }
        }

        for &child in children {
            if let Some(&matched) = pairs.get(&child) {
                self.replay(arena, child, matched, summary);
            }
        }
    }
}

/// Order.Apply: puts the story target's subtree in the dump's sibling order. Returns the summary
/// ({parents, moved, unpaired[, error]}); an unreadable dump changes nothing.
pub fn apply(lua: &Lua, engine: &Rc<Engine>, target: InstanceId, file: &str) -> LuaResult<Json> {
    let dump = std::fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|text| crate::engine::lune_json::decode(&text));
    let dump = match dump {
        Ok(dump @ Json::Object(_)) => dump,
        Ok(other) => return Ok(json!({ "parents": 0, "moved": 0, "unpaired": 0, "error": other.to_string() })),
        Err(error) => return Ok(json!({ "parents": 0, "moved": 0, "unpaired": 0, "error": error })),
    };
    let offset = match dump.get("target") {
        Some(Json::Array(values)) => (values.first().and_then(Json::as_f64).unwrap_or(0.0), values.get(1).and_then(Json::as_f64).unwrap_or(0.0)),
        _ => (0.0, 0.0),
    };
    let nodes = dump.get("nodes").and_then(Json::as_array).cloned().unwrap_or_default();
    let arena = build_tree(&nodes);
    let reflection = &engine.reflection;
    let context = Context {
        lua,
        engine,
        target: offset,
        gui_base: reflection.class_id("GuiBase2d"),
        folder: reflection.class_id("Folder"),
        text_classes: ["TextLabel", "TextButton", "TextBox"].iter().filter_map(|c| reflection.class_id(c)).collect(),
        image_classes: ["ImageLabel", "ImageButton"].iter().filter_map(|c| reflection.class_id(c)).collect(),
        image: reflection.name_id("Image"),
    };
    let mut summary = Summary::default();
    context.replay(&arena, 0, target, &mut summary);
    Ok(json!({ "parents": summary.parents, "moved": summary.moved, "unpaired": summary.unpaired }))
}
