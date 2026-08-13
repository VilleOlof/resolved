use std::{collections::HashMap, num::Wrapping};

use mlua::prelude::*;

use crate::RequestError;

/// Handles references to lua values and looks up registry keys with their ids
pub struct ItemRefHandler<'a> {
    /// the lua vm
    lua: &'a Lua,
    /// the running id, a new id is this +1
    latest_id: Wrapping<u64>,
    /// All ids and their mapping to their [`LuaRegistryKey`]
    keys: HashMap<u64, LuaRegistryKey>,
}

impl<'a> ItemRefHandler<'a> {
    /// Creates a new empty [`ItemRefHandler`] that starts at id 0 (1 on the first one)
    pub fn new(lua: &'a Lua) -> Self {
        Self {
            lua,
            latest_id: Wrapping(0),
            keys: HashMap::new(),
        }
    }

    /// Insert a new value to be stored in the registry, the returned id can be used to retrieve the value back
    pub fn insert(&mut self, value: impl IntoLua) -> Result<u64, RequestError> {
        let key = self.lua.create_registry_value(value)?;
        self.latest_id += 1;
        self.keys.insert(self.latest_id.0, key);
        Ok(self.latest_id.0)
    }

    /// Retrieve the value from the registry based on the id
    pub fn get<T: FromLua>(&self, id: u64) -> Result<T, RequestError> {
        let key = self
            .keys
            .get(&id)
            .ok_or(RequestError::NoRegistryKeyWithId(id))?;
        let value = self.lua.registry_value::<T>(key)?;
        Ok(value)
    }

    /// Remove a registry value and it's mapping with an id
    pub fn remove(&mut self, id: u64) -> Result<(), RequestError> {
        let key = self
            .keys
            .remove(&id)
            .ok_or(RequestError::NoRegistryKeyWithId(id))?;
        self.lua.remove_registry_value(key)?;
        Ok(())
    }
}
