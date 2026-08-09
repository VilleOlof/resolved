use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
};

use crate::Error;

/// `./lua_module` compiled from the `build.rs` script
pub(crate) static LUA_MODULE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/prebuilt/lua_module.dll"
));
/// The name of the `lua_module` compiled module
pub(crate) const MODULE_NAME: &str = "vinci";
/// The default path on windows to `fuscript.exe`
pub(crate) const DEFAULT_FUSCRIPT: &str =
    "C:/Program Files/Blackmagic Design/DaVinci Resolve/fuscript.exe";

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
