use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) static LUA_MODULE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/lua_module.dll"));
pub(crate) const MODULE_NAME: &str = "vinci";
pub(crate) const READY_CALL: &str = "vinci_starting";
const DEFAULT_FUSCRIPT: &str = "C:/Program Files/Blackmagic Design/DaVinci Resolve/fuscript.exe";

#[derive(Debug, serde::Deserialize)]
pub struct ScriptResponse<T> {
    pub value: T,
    eval_time: Duration,
}

impl<T> ScriptResponse<T> {
    #[inline(always)]
    pub fn value(self) -> T {
        self.value
    }

    #[inline(always)]
    pub fn eval_time(&self) -> Duration {
        self.eval_time
    }
}

type LuaCode = String;

/// `{path}/?.dll` so the directory that should contain it
pub fn dll_script(path: &Path) -> LuaCode {
    format!(
        r#"
package.cpath = package.cpath .. [[;{}/?.dll]]
require("{}").start(arg[1])"#,
        path.to_string_lossy(),
        MODULE_NAME
    )
}

pub fn fuscript() -> PathBuf {
    match env::var("FUSCRIPT") {
        Ok(s) => PathBuf::from(s),
        Err(VarError::NotPresent) => PathBuf::from(DEFAULT_FUSCRIPT),
        Err(VarError::NotUnicode(s)) => panic!("Not Unicode: {s:?}"),
    }
}
