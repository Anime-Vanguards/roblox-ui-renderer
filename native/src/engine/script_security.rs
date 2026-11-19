//! Engine members a game script or a plugin can't call, which the emulator has no behaviour for: in Roblox each raises
//! "The current thread cannot call '<name>' (lacking capability <level>)", where the emulator's stub for an
//! unemulated member returned nil. Roblox's own packages (Foundation, UIBlox, through their SafeFlags) call them under
//! pcall and fall back on that error, so the nil sent them down a path no client takes:
//!
//! - `GuiService:GetResolutionScale()` (RobloxScriptSecurity): FoundationImages picks its sprite sheets with it and
//!   compared the nil (`attempt to compare number < nil` in Generated/GetImageSetData); on the error it uses scale 1,
//!   as in a game.
//! - `DataModel:DefineFastFlag/Int/String` (RobloxScriptSecurity) and `GetFastFlag/Int/String`, `GetEngineFeature`
//!   (LocalUserSecurity): SafeFlags reads the "cannot call" error as "not a CoreScript" and turns every flag on but
//!   `Debug` ones, with the defaults for ints and strings; a nil turned every flag off.
//!
//! Securities and capabilities from the API dump (MaximumADHD/Roblox-Client-Tracker API-Dump.json, client 0.748).
use std::rc::Rc;

use crate::engine::{
    Engine,
    lua::raise,
    services::{Behaviors, method},
};

/// (class, member, the capability the error names)
const UNCALLABLE: [(&str, &str, &str); 8] = [
    ("GuiService", "GetResolutionScale", "RobloxScript"),
    ("DataModel", "DefineFastFlag", "RobloxScript"),
    ("DataModel", "DefineFastInt", "RobloxScript"),
    ("DataModel", "DefineFastString", "RobloxScript"),
    ("DataModel", "GetFastFlag", "LocalUser"),
    ("DataModel", "GetFastInt", "LocalUser"),
    ("DataModel", "GetFastString", "LocalUser"),
    ("DataModel", "GetEngineFeature", "LocalUser"),
];

pub fn install(engine: &Rc<Engine>, behaviors: &mut Behaviors) {
    let reflection = engine.reflection.clone();
    for (class, member, capability) in UNCALLABLE {
        let (Some(class), Some(name)) = (reflection.class_id(class), reflection.name_id(member)) else { continue };
        let message = format!("The current thread cannot call '{member}' (lacking capability {capability})");
        behaviors.add_method(class, name, method(move |_, _, _, _| raise(message.clone())));
    }
}
