use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
    time::Duration,
};

use crate::Error;

/// `./lua_module` compiled from the `build.rs` script
pub(crate) static LUA_MODULE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/prebuilt/lua_module.dll"
));
/// The name of the `lua_module` compiled module
pub(crate) const MODULE_NAME: &str = "vinci";
/// Sent by the script server in stdout if the module is ready to accept requests  
pub(crate) const READY_CALL: [u8; 8] = [10, 20, 30, 40, 50, 60, 70, 80];
/// Sent by the script server in stdout if the module failed to reach DaVinci Resolve
pub(crate) const RESOLVE_FAILED: [u8; 8] = [99, 99, 99, 99, 99, 99, 99, 99];
/// The default path on windows to `fuscript.exe`
pub(crate) const DEFAULT_FUSCRIPT: &str =
    "C:/Program Files/Blackmagic Design/DaVinci Resolve/fuscript.exe";

/// A response from the script server ran in the lua module.  
///
/// Contains either the `Error String`, or the `value` and time it took to evaluate the specified script
#[derive(Debug, serde::Deserialize)]
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

type LuaCode = String;

/// Generates a `.lua` script that sets the cpath to contain the specified `.dll` directory and starts the internal lua module.\
/// `{path}/?.dll` so the directory that should contain it
pub(crate) fn dll_script(path: &Path) -> LuaCode {
    format!(
        r#"
package.cpath = package.cpath .. [[;{}/?.dll]]
require("{}").start(arg[1])"#,
        path.to_string_lossy(),
        MODULE_NAME
    )
}

/// Returns the path to `fuscript.exe`.  
///
/// If `FUSCRIPT` in `$PATH` is not set, it will use the default path ([`DEFAULT_FUSCRIPT`])
pub(crate) fn fuscript() -> Result<PathBuf, Error> {
    match env::var("FUSCRIPT") {
        Ok(s) => Ok(PathBuf::from(s)),
        Err(VarError::NotPresent) => Ok(PathBuf::from(DEFAULT_FUSCRIPT)),
        Err(e) => Err(e.into()),
    }
}
