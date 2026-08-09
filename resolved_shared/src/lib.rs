use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum PacketType {
    Ready = 0,
    NoResolve = 1,
    Error = 2,
}

impl PacketType {
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::Ready,
            1 => Self::NoResolve,
            2 => Self::Error,
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
