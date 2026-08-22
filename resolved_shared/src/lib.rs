use std::{path::PathBuf, sync::LazyLock, time::Duration};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

mod mem;

pub use mem::*;

/// Packets sent by the client and module before the module starts accepting requests / outside the normal request handling
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum PrePacket {
    /// Sent by module to tell the client that it was unable to reach/get the `Resolve()` object
    NoResolve = 0,
    /// Sent by the module to tell the client that some error happened while setting up the module
    /// The error formatted as a string is sent back as the response.
    Error = 1,
    /// Sent by the client to the module with the specified configuration
    Configuration = 2,
}

impl PrePacket {
    #[must_use]
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::NoResolve,
            1 => Self::Error,
            2 => Self::Configuration,
            _ => return None,
        })
    }
}

/// Packets sent during a normal execution request to the module
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum MsgPacket {
    /// Client sends a piece of lua code to execute and return the value back to the client
    Execute = 0,
    /// Client sends a piece of lua code to execute and instead of sending back the value,
    /// The module will store the value in the lua registry and return back a unique id to that value
    Store = 1,
    /// A reference to a registry item was dropped on the client so it needs to be removed in the module
    DropItem = 2,
    /// Explicitly tell the lua module to exit
    Shutdown = 3,
    /// Behaves the same as Store, but the returned value must be a table,
    /// where each value will be inserted into the item ref handler.
    /// So the user can easily iterate over all refs in a table
    StoreTable = 4,
    /// Drops a whole list of registry ids at once
    DropMany = 5,
    /// Returns all keys from a table,
    /// Used in `ItemRefList`
    TableKeys = 6,
    /// Returns the value stored in an `ItemRef`
    ItemValue = 7,
}

impl MsgPacket {
    #[must_use]
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::Execute,
            1 => Self::Store,
            2 => Self::DropItem,
            3 => Self::Shutdown,
            4 => Self::StoreTable,
            5 => Self::DropMany,
            6 => Self::TableKeys,
            7 => Self::ItemValue,
            _ => return None,
        })
    }
}

/// Different types of arguments that some piece of lua code can have along side it
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ArgType {
    Arg = 0,
    ArgRef = 1,
    NamedArg = 2,
    NamedArgRef = 3,
}

impl ArgType {
    #[must_use]
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::Arg,
            1 => Self::ArgRef,
            2 => Self::NamedArg,
            3 => Self::NamedArgRef,
            _ => return None,
        })
    }
}

/// A response from the script server ran in the lua module.  
///
/// Contains either the `Error String`, or the `value` and time it took to evaluate the specified script
#[derive(Debug, Serialize, Deserialize)]
pub enum ScriptResponse<T> {
    Err(String),
    Ok {
        /// The value returned from the module
        value: T,
        /// How long it took to execute the lua code
        eval_time: Duration,
    },
}

impl<T> ScriptResponse<T> {
    #[inline]
    pub fn value(self) -> Option<T> {
        match self {
            Self::Err(_) => None,
            Self::Ok {
                value,
                eval_time: _,
            } => Some(value),
        }
    }

    #[inline]
    pub fn eval_time(&self) -> Option<&Duration> {
        match self {
            Self::Err(_) => None,
            Self::Ok {
                value: _,
                eval_time,
            } => Some(eval_time),
        }
    }

    #[inline]
    pub fn err(self) -> Option<String> {
        match self {
            Self::Err(e) => Some(e),
            Self::Ok {
                value: _,
                eval_time: _,
            } => None,
        }
    }
}

/// Configuration sent from the client to the module, contains a subset* of `ResolveConfig` and extra information
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleConfig {
    pub reset_globals: bool,
    pub globals: Vec<(String, rmpv::Value)>,
}

pub static RESOLVED_ROOT: LazyLock<PathBuf> =
    LazyLock::new(|| BaseDirs::new().unwrap().data_local_dir().join("resolved"));

#[inline]
pub fn instance_dir(id: u32) -> PathBuf {
    RESOLVED_ROOT.join(itoa::Buffer::new().format(id))
}
