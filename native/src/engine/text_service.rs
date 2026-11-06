//! TextService and ContentText through the text port (port of `runtime/engine/textservice.luau`),
//! installed by the GUI layer (engine/mod.rs GuiLayer).
//! Measured in Studio: the results are logical pixels of text laid out at the device pixel scale
//! (like TextBounds), wrap at the frame width, keep only the lines that fit the frame height, and
//! empty text is one line tall. GetTextSize rounds its integer font size; GetTextBoundsAsync with
//! Width 0 does not wrap.

use std::rc::Rc;

use mlua::prelude::*;
use rbx_dom_weak::types as dom;

use crate::engine::{
    Engine, GuiEngine, K, NameId,
    lua::{Flow, convert, raise, types::font_from_enum, values},
    services::{Behaviors, getter, method},
    text::{LayoutParams, TextEngine},
    value::{FontValue, Str, Value},
};

/// The engine's text engine (created on first use: Fonts.Preload).
pub fn with_text<T>(engine: &Engine, f: impl FnOnce(&mut TextEngine) -> T) -> T {
    let mut text = engine.text().borrow_mut();
    let text = text.get_or_insert_with(|| {
        TextEngine::new(crate::engine::text::FontPaths::for_repo(&engine.options.repo.to_string_lossy()))
    });
    f(text)
}

/// FontInternal.Parts: family URI, numeric weight and "normal" / "italic".
pub fn font_parts(engine: &Engine, font: &FontValue) -> (String, f64, &'static str) {
    let weight = engine.reflection.enum_item_info(font.weight).value;
    let style = if &*engine.reflection.enum_item_info(font.style).name == "Italic" { "italic" } else { "normal" };
    (font.family.to_string(), weight, style)
}

fn measure(engine: &Engine, text: &[u8], face: &FontValue, size: f64, width: f64, height: f64, rich: bool) -> dom::Vector2 {
    let pixel_scale = engine.options.pixel_scale;
    let (family, weight, style) = font_parts(engine, face);
    let wrapped = width < f64::INFINITY;
    with_text(engine, |text_engine| {
        let mut params = LayoutParams::new(text, &family, weight, style, size, pixel_scale);
        params.wrapped = wrapped;
        params.max_width = wrapped.then_some(width * pixel_scale);
        params.line_height = Some(1.0);
        let full = text_engine.layout(&params, rich);
        if !wrapped || height >= f64::INFINITY {
            return dom::Vector2::new((full.width / pixel_scale) as f32, (full.height / pixel_scale) as f32);
        }
        // Measured: the height keeps only the lines that fit the frame, the width spans all lines
        params.max_height = Some(height * pixel_scale);
        let clamped = text_engine.layout(&params, rich);
        dom::Vector2::new((full.width / pixel_scale) as f32, (clamped.height / pixel_scale) as f32)
    })
}

fn argument_error<T>(index: usize) -> Result<T, Flow> {
    // error(msg, 3) from ArgumentError: the script's line
    raise(format!("Argument {index} missing or nil"))
}

pub fn install(engine: &Rc<Engine>, behaviors: &mut Behaviors) {
    let reflection = engine.reflection.clone();
    let Some(service) = reflection.class_id("TextService") else { return };
    let n = |name: &str| reflection.name_id(name);

    if let Some(name) = n("GetTextSize") {
        behaviors.add_method(service, name, method(|lua, engine, _, args| {
            let text = match args.get(1) {
                LuaValue::String(s) => s.as_bytes().to_vec(),
                value @ (LuaValue::Number(_) | LuaValue::Integer(_)) => {
                    lua.coerce_string(value.clone())?.map(|s| s.as_bytes().to_vec()).unwrap_or_default()
                }
                _ => return argument_error(1),
            };
            let Some(size) = args.number(2) else { return argument_error(2) };
            let item = engine.reflection.enum_id("Font").and_then(|id| convert::coerce_enum(lua, engine, id, args.get(3)));
            let Some(item) = item else { return raise("Unable to cast value to Enum.Font") };
            let Value::Vector2(frame) = convert::from_lua(args.get(4)) else { return argument_error(4) };
            let face = font_from_enum(engine, item);
            let result = measure(engine, &text, &face, (size + 0.5).floor(), frame.x as f64, frame.y as f64, false);
            Ok(values([convert::to_lua(lua, engine, &Value::Vector2(result))?]))
        }));
    }
    if let Some(name) = n("GetTextBoundsAsync") {
        behaviors.add_method(service, name, method(|lua, engine, _, args| {
            let Some(params) = args.instance(1) else { return raise("Argument 1 missing or nil") };
            let (is_params, text, face, size, width, rich) = {
                let state = engine.state.borrow();
                let class = state.dom.at(params).class;
                let get = |name: &str| engine.reflection.name_id(name).and_then(|n| state.dom.raw_get(params, n).cloned());
                (
                    engine.reflection.is_a(class, "GetTextBoundsParams"),
                    get("Text").and_then(|v| v.as_str().cloned()).unwrap_or_default(),
                    match get("Font") {
                        Some(Value::Font(font)) => (*font).clone(),
                        _ => FontValue {
                            family: Str::from("rbxasset://fonts/families/SourceSansPro.json"),
                            weight: engine.reflection.enum_item("FontWeight", "Regular").unwrap_or(0),
                            style: engine.reflection.enum_item("FontStyle", "Normal").unwrap_or(0),
                        },
                    },
                    get("Size").and_then(|v| v.as_number()).unwrap_or(20.0),
                    get("Width").and_then(|v| v.as_number()).unwrap_or(0.0),
                    matches!(get("RichText"), Some(Value::Bool(true))),
                )
            };
            if !is_params {
                return raise("Argument 1 missing or nil");
            }
            let width = if width > 0.0 { width } else { f64::INFINITY };
            let result = measure(engine, text.as_bytes(), &face, size, width, f64::INFINITY, rich);
            Ok(values([convert::to_lua(lua, engine, &Value::Vector2(result))?]))
        }));
    }
    if let Some(name) = n("GetTextSizeOffsetAsync") {
        behaviors.add_method(service, name, method(|_, _, _, _| Ok(values([LuaValue::Number(0.0)]))));
    }
    if let Some(name) = n("SetResolutionScale") {
        behaviors.add_method(service, name, method(|_, _, _, _| Ok(values([]))));
    }

    // ContentText: rich text shows its content (invalid markup shows the source)
    for class_name in ["TextLabel", "TextButton", "TextBox"] {
        let (Some(class), Some(name)) = (reflection.class_id(class_name), n("ContentText")) else { continue };
        behaviors.add_getter(class, name, getter(|lua, engine, id| {
            let (text, rich) = {
                let state = engine.state.borrow();
                let text = state.dom.raw_get(id, K::Text as NameId).and_then(|v| v.as_str().cloned()).unwrap_or_default();
                (text, matches!(state.dom.raw_get(id, K::RichText as NameId), Some(Value::Bool(true))))
            };
            let content = if rich { with_text(engine, |t| t.content_text(text.as_bytes())) } else { text.as_bytes().to_vec() };
            Ok(LuaValue::String(lua.create_string(content)?))
        }));
    }
}
