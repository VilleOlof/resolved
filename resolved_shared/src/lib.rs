use std::{path::Path, time::Duration};

use serde::{Deserialize, Serialize};

mod mem;

pub use mem::*;

/// Packets sent by the client and module before the module starts accepting requests / outside the normal request handling
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum PrePacket {
    /// Sent by the module to tell the client that it's server is ready for connections.
    /// The module returns its own port back to the client
    Ready = 0,
    /// Sent by module to tell the client that it was unable to reach/get the `Resolve()` object
    NoResolve = 1,
    /// Sent by the module to tell the client that some error happened while setting up the module
    /// The error formatted as a string is sent back as the response.
    Error = 2,
    Ping = 3,
    Pong = 4,
    /// Sent by the client to the module with the specified configuration
    Configuration = 5,
}

impl PrePacket {
    #[must_use]
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::Ready,
            1 => Self::NoResolve,
            2 => Self::Error,
            3 => Self::Ping,
            4 => Self::Pong,
            5 => Self::Configuration,
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

/// Configuration for `Resolve` instances
///
/// Can be used to increase the internal ping timeout and or if it should reset globals after every execution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolveConfig {
    /// If the module don't get a pong packet back from the client in [`timeout`](ResolveConfig::timeout) time, it will exit.
    pub timeout: Duration,
    /// If the module should reset the lua globals between every request.
    /// For short, small requests, this can increase performance by a good bit.
    /// You just need to make sure you use `local` variables in lua and don't clutter the global table to mess with different scripts
    pub reset_globals: bool,
}

impl ResolveConfig {
    /// Default configuration for all instances
    pub const DEFAULT: Self = ResolveConfig {
        timeout: Duration::from_secs(60),
        reset_globals: true,
    };

    /// Default configuration except that globals don't get reset
    pub const KEEP_GLOBALS: Self = ResolveConfig {
        timeout: Duration::from_secs(60),
        reset_globals: false,
    };
}

impl Default for ResolveConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Configuration sent from the client to the module, contains a subset* of [`ResolveConfig`] and extra information
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleConfig {
    pub reset_globals: bool,
    pub shmem_path: String,
}

impl ModuleConfig {
    pub fn new(
        resolve_config: &ResolveConfig,
        shmem_path: &Path,
    ) -> Result<Self, std::num::TryFromIntError> {
        Ok(Self {
            reset_globals: resolve_config.reset_globals,
            shmem_path: shmem_path.display().to_string(),
        })
    }
}
