use mlua::prelude::*;
use resolved_shared::MsgPacket;

use crate::{
    Buffers, Request,
    error::RequestError,
    item_ref::ItemRefHandler,
    request::{Payload, serialize_values},
};

/// global variable name for self instances
pub(crate) const SELF: &str = "self";

/// Handles a specific request
pub fn handle_req(
    lua: &Lua,
    item_ref_handler: &mut ItemRefHandler,
    resolve: &LuaAnyUserData,
    request: &mut Request,
    buffers: &mut Buffers,
) -> Result<Vec<u8>, RequestError> {
    request.read_payload(buffers)?;
    let mut payload = Payload::new(&buffers.payload);

    let packet_type = payload.packet_type()?;

    return match packet_type {
        MsgPacket::Execute => {
            let (value, eval_time) =
                payload.handle_script(lua, item_ref_handler, resolve, buffers)?;
            Ok(serialize_values(value, eval_time)?)
        }
        MsgPacket::Store => {
            let (value, eval_time) =
                payload.handle_script(lua, item_ref_handler, resolve, buffers)?;
            let id = item_ref_handler.insert(value)?;
            Ok(serialize_values(id, eval_time)?)
        }
        MsgPacket::DropItem => {
            let id = payload.u64()?;
            item_ref_handler.remove(id)?;
            Ok(Vec::new())
        }
        MsgPacket::Shutdown => std::process::exit(1),
        MsgPacket::StoreTable => {
            let (value, eval_time) =
                payload.handle_script(lua, item_ref_handler, resolve, buffers)?;

            let table = value
                .as_table()
                .ok_or(RequestError::NotATable(value.type_name()))?;
            let source_id = item_ref_handler.insert(table)?;
            let mut ids = Vec::with_capacity(table.len()? as usize);
            for pair in table.pairs::<LuaValue, LuaValue>() {
                let (_, v) = pair?;
                let id = item_ref_handler.insert(v)?;
                ids.push(id);
            }

            Ok(serialize_values((source_id, ids), eval_time)?)
        }
        MsgPacket::DropMany => {
            let len = payload.u32()?;
            for _ in 0..len {
                item_ref_handler.remove(payload.u64()?)?;
            }
            Ok(Vec::new())
        }
    };
}
