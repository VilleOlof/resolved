use std::time::Instant;

use mlua::prelude::*;
use resolved_shared::MsgPacket;

use crate::{
    Buffers, RESOLVE_FLAGS, error::RequestError, item_ref::ItemRefHandler, reader::ShmemReader,
    request::serialize_values, table_keys,
};

/// Handles a specific request
pub fn handle_req(
    lua: &Lua,
    reader: &mut ShmemReader,
    item_ref_handler: &mut ItemRefHandler,
    resolve: &LuaAnyUserData,
    buffers: &mut Buffers,
) -> Result<Vec<u8>, RequestError> {
    let packet_type = reader.get_packet()?;
    crate::info!(?packet_type, "request handler");

    return match packet_type {
        MsgPacket::Execute => {
            let (value, eval_time) =
                reader.handle_script(lua, item_ref_handler, resolve, buffers)?;
            Ok(serialize_values(value, eval_time)?)
        }
        MsgPacket::Store => {
            let (value, eval_time) =
                reader.handle_script(lua, item_ref_handler, resolve, buffers)?;
            if value.is_nil() {
                return Ok(serialize_values(None::<u64>, eval_time)?);
            }

            let id = item_ref_handler.insert(value)?;
            Ok(serialize_values(Some(id), eval_time)?)
        }
        MsgPacket::DropItem => {
            let id = reader.u64()?;
            item_ref_handler.remove(id)?;
            Ok(Vec::new())
        }
        MsgPacket::Shutdown => {
            crate::info!("shutting down from packet");
            std::process::exit(1);
        }
        MsgPacket::StoreTable => {
            let (value, eval_time) =
                reader.handle_script(lua, item_ref_handler, resolve, buffers)?;

            let table = value
                .as_table()
                .ok_or(RequestError::NotATable(value.type_name()))?;
            let source_id = item_ref_handler.insert(table)?;

            // remove any resolve inserted flags
            match table.get::<LuaValue>(RESOLVE_FLAGS)? {
                LuaValue::Nil => (),
                _ => {
                    crate::debug!("removed '__table' from table");
                    table.remove(RESOLVE_FLAGS)?
                }
            };

            let mut ids = Vec::with_capacity(table.len()? as usize);
            for pair in table.pairs::<LuaValue, LuaValue>() {
                let (_, v) = pair?;
                let id = item_ref_handler.insert(v)?;
                ids.push(id);
            }

            Ok(serialize_values((source_id, ids), eval_time)?)
        }
        MsgPacket::DropMany => {
            let len = reader.u32()?;

            // we only save the latest err and continue try to clear all ids even if an earlier id fails
            // we mostly want to save at least one err to show the user that something failed
            let mut err = None;
            for _ in 0..len {
                let id = reader.u64()?;
                if let Err(e) = item_ref_handler.remove(id) {
                    err = Some(e);
                }
            }

            if let Some(e) = err {
                return Err(e);
            }

            Ok(Vec::new())
        }
        MsgPacket::TableKeys => {
            let id = reader.u64()?;
            let value = item_ref_handler.get::<LuaValue>(id)?;
            let table = match value {
                LuaValue::Table(v) => v,
                _ => return Err(RequestError::NotATable(value.type_name())),
            };

            let time = Instant::now();
            let keys = table_keys(&table)?;
            Ok(serialize_values(keys, time.elapsed())?)
        }
        MsgPacket::ItemValue => {
            let id = reader.u64()?;
            let time = Instant::now();
            let value = item_ref_handler.get::<LuaValue>(id)?;
            Ok(serialize_values(value, time.elapsed())?)
        }
    };
}
