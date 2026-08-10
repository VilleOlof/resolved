use std::{collections::HashMap, num::Wrapping};

use mlua::prelude::*;

use crate::RequestError;

pub struct ItemRefHandler<'a> {
    lua: &'a Lua,
    latest_id: Wrapping<u64>,
    keys: HashMap<u64, LuaRegistryKey>,
}

impl<'a> ItemRefHandler<'a> {
    pub fn new(lua: &'a Lua) -> Self {
        Self {
            lua,
            latest_id: Wrapping(0),
            keys: HashMap::new(),
        }
    }

    pub fn insert(&mut self, value: impl IntoLua) -> Result<u64, RequestError> {
        let key = self.lua.create_registry_value(value)?;
        self.latest_id += 1;
        self.keys.insert(self.latest_id.0, key);
        Ok(self.latest_id.0)
    }

    pub fn get<T: FromLua>(&self, id: u64) -> Result<T, RequestError> {
        let key = self
            .keys
            .get(&id)
            .ok_or(RequestError::NoRegistryKeyWithId(id))?;
        let value = self.lua.registry_value::<T>(key)?;
        Ok(value)
    }

    pub fn remove(&mut self, id: u64) -> Result<(), RequestError> {
        let key = self
            .keys
            .remove(&id)
            .ok_or(RequestError::NoRegistryKeyWithId(id))?;
        self.lua.remove_registry_value(key)?;
        Ok(())
    }
}
