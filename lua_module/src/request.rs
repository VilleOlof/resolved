use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use bytes::{Buf, Bytes};
use mlua::prelude::*;
use resolved_shared::{ArgType, MsgPacket, ScriptResponse};
use serde::Serialize;

use crate::{error::RequestError, execute, handler::SELF, item_ref::ItemRefHandler};

const ARG_GLOBAL: &str = "arg";

macro_rules! read_num {
    ($t:ty, $f:ident) => {
        /// Reads a number
        pub fn $f(&mut self) -> Result<$t, RequestError> {
            let mut buf = [0u8; size_of::<$t>()];
            self.0.read_exact(&mut buf)?;
            Ok(<$t>::from_be_bytes(buf))
        }
    };
}

/// An execution request
#[derive(Debug)]
pub struct Request(TcpStream);
impl Request {
    /// Wraps a [`TcpStream`]
    pub fn new(stream: TcpStream) -> Self {
        Self(stream)
    }

    /// Returns back a value to the client
    pub fn send(&mut self, buf: Vec<u8>) -> Result<(), RequestError> {
        self.0.write(&(buf.len() as u64).to_be_bytes())?;
        self.0.write_all(&buf)?;
        self.0.flush()?;
        Ok(())
    }

    read_num!(u64, read_u64);

    /// Reads the entire sent payload by the client
    pub fn read_payload(&mut self) -> Result<Vec<u8>, RequestError> {
        let len = self.read_u64()? as usize;
        let mut data = vec![0u8; len];
        self.0.read_exact(&mut data)?;
        Ok(data)
    }
}

/// Serializes a value and it's eval time to a buffer
pub fn serialize_values<T: Serialize>(
    value: T,
    eval_time: Duration,
) -> Result<Vec<u8>, RequestError> {
    Ok(rmp_serde::to_vec(&ScriptResponse::Ok { value, eval_time })?)
}

/// Serializes an error to a buffer
pub fn serialize_err(err: String) -> Result<Vec<u8>, RequestError> {
    Ok(rmp_serde::to_vec(&ScriptResponse::<()>::Err(err))?)
}

/// All the data sent during an execution, the ref, script, arguments and more
pub struct Payload(Bytes);
impl Payload {
    /// Wraps the buffer in [`Bytes`] internally
    pub fn new(data: Vec<u8>) -> Self {
        Self(Bytes::from_owner(data))
    }

    /// Handles the request and executes the provided script in the payload, returns back the lua value returned from the execution.
    pub fn handle_script(
        &mut self,
        lua: &Lua,
        item_ref_handler: &mut ItemRefHandler,
        resolve: &LuaAnyUserData,
    ) -> Result<(LuaValue, Duration), RequestError> {
        let is_ref = self.u8()? == 1;
        let ref_id = if is_ref { Some(self.u64()?) } else { None };

        let globals = lua.globals();

        let lua_code = self.string()?;
        let args_len = self.u32()?;

        let mut nameless_args = Vec::new();
        let mut global_args = Vec::new();
        for _ in 0..args_len {
            let raw_arg_type = self.u8()?;
            let arg_type =
                ArgType::from_u8(raw_arg_type).ok_or(RequestError::InvalidArgType(raw_arg_type))?;

            match arg_type {
                ArgType::Arg => {
                    let data_len = self.u32()? as usize;
                    let value = self.buf_into_lua_value(lua, data_len)?;
                    nameless_args.push(value);
                }
                ArgType::ArgRef => {
                    let id = self.u64()?;
                    let value = item_ref_handler.get::<LuaValue>(id)?;
                    nameless_args.push(value);
                }
                ArgType::NamedArg => {
                    let key = self.string()?;
                    let data_len = self.u32()? as usize;
                    let value = self.buf_into_lua_value(lua, data_len)?;
                    global_args.push(key.clone());
                    globals.set(key, value)?;
                }
                ArgType::NamedArgRef => {
                    let key = self.string()?;
                    let id = self.u64()?;
                    let value = item_ref_handler.get::<LuaValue>(id)?;
                    global_args.push(key.clone());
                    globals.set(key, value)?;
                }
            }
        }

        if !nameless_args.is_empty() {
            let arg =
                lua.create_table_from(nameless_args.iter().enumerate().map(|(i, x)| (i + 1, x)))?;

            globals.set(ARG_GLOBAL, &arg)?;
        }

        // set self after globals from arg so consumer cant override self
        match ref_id {
            Some(id) => {
                let value = item_ref_handler.get::<LuaValue>(id)?;
                globals.set(SELF, value)?;
            }
            None => globals.set(SELF, resolve)?,
        }

        let return_value = execute(lua, &lua_code)?;

        // we also need to reset the argument globals since
        // if a user has disabled reset_globals then their script arguments shouldnt clutter anyway
        for name in global_args {
            globals.remove(name)?;
        }
        // we dont need to remove SELF as its gonna be set next execution anyway

        Ok(return_value)
    }

    /// Reads the [`MsgPacket`] from the payload
    pub fn packet_type(&mut self) -> Result<MsgPacket, RequestError> {
        let raw = self.0.try_get_u8()?;
        let packet = MsgPacket::from_u8(raw).ok_or(RequestError::InvalidPacketType(raw))?;
        Ok(packet)
    }

    /// Reads an u64
    pub fn u64(&mut self) -> Result<u64, RequestError> {
        Ok(self.0.try_get_u64()?)
    }

    /// Reads an u32
    pub fn u32(&mut self) -> Result<u32, RequestError> {
        Ok(self.0.try_get_u32()?)
    }

    /// Reads an u8
    pub fn u8(&mut self) -> Result<u8, RequestError> {
        Ok(self.0.try_get_u8()?)
    }

    /// Reads a length-prefixed string
    pub fn string(&mut self) -> Result<String, RequestError> {
        let len = self.u32()?;
        let mut buf = vec![0u8; len as usize];
        self.0.copy_to_slice(&mut buf);
        let str = String::from_utf8(buf)?;
        Ok(str)
    }

    /// Converts a raw rmp buffer value into a [`LuaValue`] by converting it first to a [`rmpv::Value`] and then using the lua context
    fn buf_into_lua_value(&mut self, lua: &Lua, len: usize) -> Result<LuaValue, RequestError> {
        let buf = self.0.split_to(len);
        let temp_value = rmp_serde::from_slice::<rmpv::Value>(&buf)?;
        // we need lua to convert our buf to a LuaValue so we use rmpv::Value as a middleground for serde
        // best case scenario we could skip rmpv
        let value = lua.to_value(&temp_value)?;
        Ok(value)
    }
}
