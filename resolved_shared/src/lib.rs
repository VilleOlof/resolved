use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum PrePacket {
    /// Sent by the module to tell the client that it's server is ready for connections.  
    /// The module returns its own port back to the client
    Ready = 0,
    /// Sent by module to tell the client that it was unable to reach/get the Resolve() object
    NoResolve = 1,
    /// Sent by the module to tell the client that some error happened while setting up the module
    /// The error formatted as a string is sent back as the response.
    Error = 2,
}

impl PrePacket {
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::Ready,
            1 => Self::NoResolve,
            2 => Self::Error,
            _ => return None,
        })
    }
}

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
}

impl MsgPacket {
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::Execute,
            1 => Self::Store,
            2 => Self::DropItem,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ArgType {
    Arg = 0,
    ArgRef = 1,
    NamedArg = 2,
    NamedArgRef = 3,
}

impl ArgType {
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
    Ok { value: T, eval_time: Duration },
}

impl<T> ScriptResponse<T> {
    #[inline(always)]
    pub fn value(self) -> Option<T> {
        match self {
            Self::Err(_) => None,
            Self::Ok {
                value,
                eval_time: _,
            } => Some(value),
        }
    }

    #[inline(always)]
    pub fn eval_time(&self) -> Option<&Duration> {
        match self {
            Self::Err(_) => None,
            Self::Ok {
                value: _,
                eval_time,
            } => Some(eval_time),
        }
    }

    #[inline(always)]
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
