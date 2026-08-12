use mlua::prelude::*;
use resolved_shared::MsgPacket;

use crate::{
    Request,
    error::RequestError,
    item_ref::ItemRefHandler,
    request::{Payload, serialize_values},
};

pub(crate) const SELF: &str = "self";

/// Handles a specific request
pub fn handle_req(
    lua: &Lua,
    item_ref_handler: &mut ItemRefHandler,
    resolve: &LuaAnyUserData,
    request: &mut Request,
) -> Result<Vec<u8>, RequestError> {
    let buf = request.read_payload()?;
    let mut payload = Payload::new(buf);

    let packet_type = payload.packet_type()?;

    return match packet_type {
        MsgPacket::Execute => {
            let (value, eval_time) = payload.handle_script(lua, item_ref_handler, resolve)?;
            Ok(serialize_values(value, eval_time)?)
        }
        MsgPacket::Store => {
            let (value, eval_time) = payload.handle_script(lua, item_ref_handler, resolve)?;
            let id = item_ref_handler.insert(value)?;
            Ok(serialize_values(id, eval_time)?)
        }
        MsgPacket::DropItem => {
            let id = payload.u64()?;
            item_ref_handler.remove(id)?;
            Ok(Vec::new())
        }
        MsgPacket::Shutdown => std::process::exit(1),
    };
}
