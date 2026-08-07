use std::time::Duration;

use mlua::prelude::*;
use serde::Serialize;
use tiny_http::{Request, Response, Server};

#[mlua::lua_module]
fn vinci(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("start", lua.create_function(start)?)?;
    Ok(exports)
}

fn start(lua: &Lua, port: u16) -> LuaResult<()> {
    setup_globals(lua)?;

    let server = Server::http(format!("0.0.0.0:{port}")).unwrap();

    ready();

    for mut request in server.incoming_requests() {
        let res = match handle_req(lua, &mut request) {
            Err(e) => Response::from_string(format!("something went wrong: {e:?}")),
            Ok(buf) => Response::from_data(buf),
        }
        .with_status_code(200);

        let _ = request.respond(res);
    }

    Ok(())
}

fn setup_globals(lua: &Lua) -> LuaResult<()> {
    let resolve: LuaAnyUserData = lua.load("return Resolve()").eval()?;
    let globals = lua.globals();
    globals.set("self", resolve.clone())?;
    globals.set("resolve", resolve)?;

    Ok(())
}

fn handle_req(lua: &Lua, request: &mut Request) -> Result<Vec<u8>, RequestError> {
    let mut input = String::new();
    request.as_reader().read_to_string(&mut input)?;

    let lua_code = lua.load(input.trim());

    let eval_i = std::time::Instant::now();
    let value: LuaValue = lua_code.eval()?;
    let eval_time = eval_i.elapsed();

    let buffer = rmp_serde::to_vec(&ScriptResponse { value, eval_time })?;

    Ok(buffer)
}

fn ready() {
    // this is for the client library to know that the script env has fully started
    println!("vinci_starting");
}

#[derive(Debug, Serialize)]
struct ScriptResponse {
    value: LuaValue,
    eval_time: Duration,
}

#[derive(Debug, thiserror::Error)]
enum RequestError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Rmp(#[from] rmp_serde::encode::Error),
    #[error(transparent)]
    Lua(#[from] mlua::Error),
}
