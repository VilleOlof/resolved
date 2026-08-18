use std::time::Duration;

use mlua::prelude::*;
use resolved_shared::{ArgType, MsgPacket, ShmemData, data_offset, type_offset};

use crate::{
    Buffers, GLOBAL_ARG, GLOBAL_SELF, ShmemModule, error::RequestError, execute,
    item_ref::ItemRefHandler,
};

pub struct ShmemReader<'s> {
    shmem: &'s mut ShmemModule,
    cursor: usize,
    len: usize,
}

macro_rules! read_ptr_num {
    ($f:ident, $t:ty) => {
        pub fn $f(&mut self) -> Result<$t, RequestError> {
            if self.cursor + size_of::<$t>() > self.len {
                return Err(RequestError::NotEnoughBytesInMemory);
            }

            unsafe {
                let ptr = self.curr_ptr();
                let b = <$t>::from_be_bytes(
                    std::slice::from_raw_parts(ptr, size_of::<$t>())
                        .try_into()
                        .expect("<$t> makes this impossible since it must conform to it's size"),
                );
                self.cursor += size_of::<$t>();
                Ok(b)
            }
        }
    };
}

impl<'s> ShmemReader<'s> {
    pub fn new(shmem: &'s mut ShmemModule) -> Self {
        Self {
            len: shmem.get_len(),
            shmem,
            cursor: 0,
        }
    }

    fn curr_ptr(&self) -> *mut u8 {
        unsafe { self.shmem.ptr.add(data_offset() + self.cursor) }
    }

    pub fn get_packet(&self) -> Result<MsgPacket, RequestError> {
        unsafe {
            let ptr = self.shmem.ptr.add(type_offset());
            let byte = std::ptr::read_volatile(ptr);
            MsgPacket::from_u8(byte).ok_or(RequestError::InvalidPacketType(byte))
        }
    }

    pub fn u8(&mut self) -> Result<u8, RequestError> {
        if self.cursor + size_of::<u8>() > self.len {
            return Err(RequestError::NotEnoughBytesInMemory);
        }

        unsafe {
            let ptr = self.curr_ptr();
            let b = std::ptr::read_volatile(ptr);
            self.cursor += size_of::<u8>();
            Ok(b)
        }
    }

    read_ptr_num!(u32, u32);
    read_ptr_num!(u64, u64);

    pub fn slice(&mut self) -> Result<&'s [u8], RequestError> {
        unsafe {
            let len = self.u32()? as usize;

            if self.cursor + len > self.len {
                return Err(RequestError::NotEnoughBytesInMemory);
            }

            let ptr = self.curr_ptr();
            let s = std::slice::from_raw_parts(ptr, len);
            self.cursor += len;
            Ok(s)
        }
    }

    pub fn string(&mut self) -> Result<&'s str, RequestError> {
        unsafe {
            let s = self.slice()?;
            Ok(str::from_utf8_unchecked(s))
        }
    }

    pub fn handle_script(
        &mut self,
        lua: &Lua,
        item_ref_handler: &mut ItemRefHandler,
        resolve: &LuaAnyUserData,
        buffers: &mut Buffers,
    ) -> Result<(LuaValue, Duration), RequestError> {
        let is_ref = self.u8()? == 1;
        let ref_id = if is_ref { Some(self.u64()?) } else { None };
        crate::debug!(?is_ref, ?ref_id, "script with");

        let globals = lua.globals();

        let lua_code = self.string()?;
        let args_len = self.u32()?;

        let mut global_key_names = if args_len > 0 {
            Vec::with_capacity(args_len as usize)
        } else {
            Vec::new()
        };

        for _ in 0..args_len {
            let raw_arg_type = self.u8()?;
            let arg_type =
                ArgType::from_u8(raw_arg_type).ok_or(RequestError::InvalidArgType(raw_arg_type))?;

            match arg_type {
                ArgType::Arg => {
                    let value = self.buf_into_lua_value(lua)?;
                    buffers.nameless_args.push(value);
                }
                ArgType::ArgRef => {
                    let id = self.u64()?;
                    let value = item_ref_handler.get::<LuaValue>(id)?;
                    buffers.nameless_args.push(value);
                }
                ArgType::NamedArg => {
                    let key = self.string()?;
                    let value = self.buf_into_lua_value(lua)?;
                    global_key_names.push(key);
                    globals.set(key, value)?;
                }
                ArgType::NamedArgRef => {
                    let key = self.string()?;
                    let id = self.u64()?;
                    let value = item_ref_handler.get::<LuaValue>(id)?;
                    global_key_names.push(key);
                    globals.set(key, value)?;
                }
            }
        }

        #[cfg(feature = "tracing")]
        let nameless_count = buffers.nameless_args.len();
        crate::debug!(
            ?nameless_count,
            ?global_key_names,
            ?lua_code,
            "got script data"
        );

        if !buffers.nameless_args.is_empty() {
            let arg = lua.create_table_from(
                buffers
                    .nameless_args
                    .iter()
                    .enumerate()
                    .map(|(i, x)| (i + 1, x)),
            )?;

            globals.set(GLOBAL_ARG, &arg)?;
        }

        // set self after globals from arg so consumer cant override self
        match ref_id {
            Some(id) => {
                let value = item_ref_handler.get::<LuaValue>(id)?;
                globals.set(GLOBAL_SELF, value)?;
            }
            None => globals.set(GLOBAL_SELF, resolve)?,
        }

        let return_value = execute(lua, lua_code)?;
        #[cfg(feature = "tracing")]
        let (type_name, value, time) =
            (return_value.0.type_name(), &return_value.0, return_value.1);
        crate::info!(?type_name, ?time, ?value, "executed script");

        // we also need to reset the argument globals since
        // if a user has disabled reset_globals then their script arguments shouldnt clutter anyway
        for name in &global_key_names {
            globals.remove(*name)?;
        }
        crate::debug!(?global_key_names, "cleared global argument variables");
        // we dont need to remove SELF as its gonna be set next execution anyway

        Ok(return_value)
    }

    /// Converts a raw rmp buffer value into a [`LuaValue`] by converting it first to a [`rmpv::Value`] and then using the lua context
    fn buf_into_lua_value(&mut self, lua: &Lua) -> Result<LuaValue, RequestError> {
        let buf = self.slice()?;
        let temp_value = rmp_serde::from_slice::<rmpv::Value>(&buf)?;
        // we need lua to convert our buf to a LuaValue so we use rmpv::Value as a middleground for serde
        // best case scenario we could skip rmpv
        let value = lua.to_value(&temp_value)?;
        Ok(value)
    }
}
