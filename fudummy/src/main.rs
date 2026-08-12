use std::{fs::read_to_string, path::PathBuf};

use mlua::prelude::*;

fn main() {
    let args = Args::load();

    let script = read_to_string(&args.script_path).expect("failed to read script file");
    let lua = unsafe { Lua::unsafe_new() };

    let arg_table = lua.create_table().expect("failed to create arg table");
    arg_table
        .push(LuaValue::Integer(args.port as i64))
        .expect("failed to push to arg");
    arg_table
        .push(LuaValue::Integer(args.timeout as i64))
        .expect("failed to push to arg");

    lua.globals()
        .set("arg", arg_table)
        .expect("failed to set arg global");

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

struct Resolve;
impl LuaUserData for Resolve {}

struct Args {
    script_path: PathBuf,
    port: u16,
    timeout: u64,
}

impl Args {
    fn load() -> Self {
        let mut args = std::env::args().skip(1);
        let _quiet = args.next().expect("no -q flag");
        let path = args.next().expect("no script path");
        let port = args.next().expect("no port");
        let timeout = args.next().expect("no timeout");

        let script_path = PathBuf::from(path);
        let port = port.parse::<u16>().unwrap();
        let timeout = timeout.parse::<u64>().unwrap();

        Self {
            script_path,
            port,
            timeout,
        }
    }
}
