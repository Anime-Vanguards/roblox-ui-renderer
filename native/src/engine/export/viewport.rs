//! A content-addressed 3D snapshot. The 3D helper reconstructs its DOM and ror (roblox-object-renderer)
//! draws it; the resulting
//! transparent texture takes the same clipping, tint and compositing path as every other image.
use super::*;
use rbx_dom_weak::types::Variant;
use sha2::{Digest, Sha256};

/// Unmodeled properties ROR reads; the rest (physics and mesh blobs) would only grow every snapshot
/// and its digest.
const FORWARDED_UNMODELED: &[&str] = &["InitialSize"];

fn value(ctx: &Ctx, value: &Value) -> Option<Variant> {
    Some(match value {
        Value::Bool(v) => Variant::Bool(*v),
        Value::Number(v) => Variant::Float64(*v),
        Value::String(v) => Variant::String(v.to_string()),
        Value::EnumItem(v) => Variant::Enum(rbx_dom_weak::types::Enum::from_u32(ctx.reflection.enum_item_info(*v).value as u32)),
        Value::Vector3(v) => Variant::Vector3(*v),
        Value::Vector2(v) => Variant::Vector2(*v),
        Value::Color3(v) => Variant::Color3(*v),
        Value::NumberRange(v) => Variant::NumberRange(*v),
        Value::NumberSequence(v) => Variant::NumberSequence((**v).clone()),
        Value::ColorSequence(v) => Variant::ColorSequence((**v).clone()),
        Value::Content(v) => Variant::ContentId(v.uri().map(ToString::to_string).unwrap_or_default().into()),
        Value::Dom(v) => (**v).clone(),
        _ => return None,
    })
}

pub(super) fn export(ctx: &Ctx, _out: &Export, id: InstanceId, record: &Record, node: &mut Map<String, Json>) {
    let get = |id, name| ctx.get_opt(id, ctx.reflection.name_id(name));
    let camera = get(id, "CurrentCamera").and_then(Value::as_instance).filter(|id| ctx.state.dom.get(*id).is_some());
    let camera_value = camera.and_then(|camera| get(camera, "CFrame")).or_else(|| get(id, "CameraCFrame"));
    let components = match camera_value {
        Some(Value::Dom(v)) => match v.as_ref() {
            Variant::CFrame(v) => {
                let p = v.position;
                let m = v.orientation;
                [p.x, p.y, p.z, m.x.x, m.x.y, m.x.z, m.y.x, m.y.y, m.y.z, m.z.x, m.z.y, m.z.z]
            }
            _ => [0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0., 1.],
        },
        _ => [0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0., 1.],
    };
    let mut instances = Vec::new();
    // The DataModel's MaterialService (another root of the snapshot): which built-in materials the world is
    // drawn with and the place's MaterialVariants. Studio draws a ViewportFrame's parts with the place's
    // materials: a place without Use2022Materials shows the pre-2022 ones in its frames as in its world
    // (Studio 0.747, fixtures/viewport-probe-materials.luau: flat Plastic, grey Brick, knit Fabric).
    let mut game = id;
    while let Some(parent) = ctx.state.dom.at(game).parent {
        game = parent;
    }
    let service = ctx.state.dom.at(game).children.iter().copied().find(|child| ctx.class_name(*child) == "MaterialService");
    let use_2022 = service
        .and_then(|service| ctx.get_opt(service, ctx.reflection.name_id("Use2022MaterialsXml")))
        .and_then(|value| match value {
            Value::Bool(on) => Some(*on),
            _ => None,
        })
        .unwrap_or(false);
    let mut stack = vec![(id, 0u32)];
    if let Some(service) = service {
        stack.insert(0, (service, 0));
    }
    while let Some((current, parent)) = stack.pop() {
        let instance = ctx.state.dom.at(current);
        if current != id && (ctx.reflection.is_a(instance.class, "GuiBase2d") || ctx.reflection.is_a(instance.class, "LuaSourceContainer")) {
            continue;
        }
        if Some(current) == service {
            instances.push(json!({"id":instance.serial,"parent":0,"class":"MaterialService","name":"MaterialService",
                "properties":{"Use2022MaterialsXml":serde_json::to_value(Variant::Bool(use_2022)).unwrap_or(Json::Null)},"references":{}}));
            stack.extend(instance.children.iter().rev().map(|child| (*child, instance.serial)));
            continue;
        }
        let mut properties = Map::new();
        let mut references = Map::new();
        for (name, property) in &instance.props {
            let name = ctx.reflection.name(name);
            if let Value::Instance(target) = property {
                if let Some(target) = ctx.state.dom.get(*target) {
                    references.insert(name.into(), json!(target.serial));
                }
            } else if let Some(property) = value(ctx, property) {
                properties.insert(name.into(), serde_json::to_value(property).unwrap_or(Json::Null));
            }
        }
        // What the model file held that the engine does not model. Without a MeshPart's InitialSize,
        // ror draws the mesh at its native size, so Size (ScaleTo, accessory sizing) is ignored and
        // a scaled model shows oversized parts.
        if let Some(unmodeled) = &instance.unmodeled {
            for (name, property) in unmodeled.iter() {
                if FORWARDED_UNMODELED.contains(&name) && !properties.contains_key(name) {
                    properties.insert(name.into(), serde_json::to_value(property).unwrap_or(Json::Null));
                }
            }
        }
        instances.push(json!({"id":instance.serial,"parent":parent,"class":ctx.class_name(current),"name":instance.name.to_string(),"properties":properties,"references":references}));
        stack.extend(instance.children.iter().rev().map(|child| (*child, instance.serial)));
    }
    if service.is_none() {
        // (serials count up from 1; this one is never an instance's)
        instances.push(json!({"id":u32::MAX,"parent":0,"class":"MaterialService","name":"MaterialService",
            "properties":{"Use2022MaterialsXml":serde_json::to_value(Variant::Bool(use_2022)).unwrap_or(Json::Null)},"references":{}}));
    }
    // Studio renders a ViewportFrame's world at the frame's logical size, whatever the device scale, and
    // stretches that picture over the frame like an image: at 1.5x a part's edge ramps over three device
    // pixels, the bilinear magnification of a one-texel edge (fixtures/viewport-probe-fade.luau, Studio 0.747).
    let dimension = |n: f64| n.ceil().clamp(1.0, 2048.0) as u32;
    let descriptor = json!({
        "version":1, "width":dimension(record.w), "height":dimension(record.h),
        "camera":{"mode":"cFrame","components":components,"fov":camera.and_then(|c| get(c,"FieldOfView")).or_else(|| get(id,"CameraFieldOfView")).and_then(Value::as_number).unwrap_or(70.0)},
        "instances":instances,
        // The frame's fill, which the picture is laid over: Studio draws the world into a target cleared to it
        // (cli/viewports.mjs layViewportBackground; the renderer then draws only the frame's border)
        "background":node.get("bg").cloned().unwrap_or(Json::Null),
    });
    let digest = Sha256::digest(serde_json::to_vec(&descriptor).unwrap());
    let uri = format!("rur-viewport://{}/{digest:x}", ctx.state.dom.at(id).serial);
    node.insert("image".into(), json!({"uri":uri,"color":ctx.color(get(id,"ImageColor3")),"transparency":get(id,"ImageTransparency").and_then(Value::as_number).unwrap_or(0.0),"scaleType":"Stretch","rectOffset":[0,0],"rectSize":[0,0]}));
    node.insert("viewport".into(), descriptor);
}
