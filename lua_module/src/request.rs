use mlua::prelude::*;
use resolved_shared::MsgPacket;
use tiny_http::Request;

use crate::{error::RequestError, execute, item_ref::ItemRefHandler, packet::*};

const SELF: &str = "self";

/// Handles a specific request
pub fn handle_req(
    lua: &Lua,
    item_ref_handler: &mut ItemRefHandler,
    resolve: &LuaAnyUserData,
    request: &mut Request,
) -> Result<Vec<u8>, RequestError> {
    let mut reader = request.as_reader();
    let packet_type = read_packet(&mut reader)?;

    match packet_type {
        MsgPacket::Execute => {
            let input = read_string(reader)?;

            lua.globals().set(SELF, resolve)?;
            let (value, eval_time) = execute(lua, input)?;
            Ok(serialize_values(value, eval_time)?)
        }
        MsgPacket::Store => {
            let input = read_string(reader)?;

            lua.globals().set(SELF, resolve)?;
            let (value, eval_time) = execute(lua, input)?;
            let id = item_ref_handler.insert(value)?;

            Ok(serialize_values(id, eval_time)?)
        }
        MsgPacket::StoreWith => {
            let id = read_u64(reader)?;
            let input = read_string(reader)?;

            lua.globals()
                .set(SELF, item_ref_handler.get::<LuaValue>(id)?)?;
            let (value, eval_time) = execute(lua, input)?;
            let id = item_ref_handler.insert(value)?;

            Ok(serialize_values(id, eval_time)?)
        }
        MsgPacket::ExecuteWith => {
            let id = read_u64(reader)?;
            let input = read_string(reader)?;

            lua.globals()
                .set(SELF, item_ref_handler.get::<LuaValue>(id)?)?;
            let (value, eval_time) = execute(lua, input)?;

            Ok(serialize_values(value, eval_time)?)
        }
        MsgPacket::DropItem => {
            let id = read_u64(reader)?;
            item_ref_handler.remove(id)?;
            Ok(Vec::new())
        }
    }
}
