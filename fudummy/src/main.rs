use std::{fs::read_to_string, path::PathBuf};

use mlua::prelude::*;

fn main() {
    let args = Args::load();

    let script = read_to_string(&args.script_path).expect("failed to read script file");
    let lua = unsafe { Lua::unsafe_new() };

    // lua module always loads a `Resolve()` function so we poly-fill it with a noop
    let dummy_resolve = lua
        .create_function(|_, _: ()| Ok(Resolve))
        .expect("failed to create polyfill resolve fn");
    lua.globals()
        .set("Resolve", dummy_resolve)
        .expect("failed to set resolve global fn");

    lua.load(script)
        .exec()
        .expect("failed to execute loaded script");
}

/// Userdata so we can give the proper values to the module
struct Resolve;
impl LuaUserData for Resolve {}

/// Same arguments as fuscript, but only those that we ever give it
struct Args {
    script_path: PathBuf,
}

impl Args {
    fn load() -> Self {
        let mut args = std::env::args().skip(1);
        let _quiet = args.next().expect("no -q flag"); // we just discard this
        let path = args.next().expect("no script path");

        let script_path = PathBuf::from(path);

        Self { script_path }
    }
}
