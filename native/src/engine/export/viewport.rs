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

pub(super) fn export(ctx: &Ctx, out: &Export, id: InstanceId, record: &Record, node: &mut Map<String, Json>) {
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
    let mut stack = vec![(id, 0u32)];
    while let Some((current, parent)) = stack.pop() {
        let instance = ctx.state.dom.at(current);
        if current != id && (ctx.reflection.is_a(instance.class, "GuiBase2d") || ctx.reflection.is_a(instance.class, "LuaSourceContainer")) {
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
    let dimension = |n: f64| (n * out.pixel_scale).ceil().clamp(1.0, 2048.0) as u32;
    let descriptor = json!({
        "version":1, "width":dimension(record.w), "height":dimension(record.h),
        "camera":{"mode":"cFrame","components":components,"fov":camera.and_then(|c| get(c,"FieldOfView")).or_else(|| get(id,"CameraFieldOfView")).and_then(Value::as_number).unwrap_or(70.0)},
        "instances":instances,
    });
    let digest = Sha256::digest(serde_json::to_vec(&descriptor).unwrap());
    let uri = format!("rur-viewport://{}/{digest:x}", ctx.state.dom.at(id).serial);
    node.insert("image".into(), json!({"uri":uri,"color":ctx.color(get(id,"ImageColor3")),"transparency":get(id,"ImageTransparency").and_then(Value::as_number).unwrap_or(0.0),"scaleType":"Stretch","rectOffset":[0,0],"rectSize":[0,0]}));
    node.insert("viewport".into(), descriptor);
}
