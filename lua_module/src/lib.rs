use std::{
    io::{self, Write},
    time::Duration,
};

use mlua::prelude::*;
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
            Err(e) => {
                let s = e.to_string();
                let res = rmp_serde::to_vec(&ScriptResponse::Err(s))
                    .expect("Failed to serialize err string");
                Response::from_data(res)
            }
            Ok(buf) => Response::from_data(buf),
        }
        .with_status_code(200);

        let _ = request.respond(res);
    }

    Ok(())
}

fn setup_globals(lua: &Lua) -> LuaResult<()> {
    let resolve: LuaAnyUserData = match lua.load("return Resolve()").eval() {
        Ok(r) => r,
        Err(e) => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(&[99, 99, 99, 99, 99, 99, 99, 99]).unwrap();
            stdout.flush().unwrap();
            return Err(e);
        }
    };
    let globals = lua.globals();
    globals.set("self", resolve.clone())?;
    globals.set("resolve", resolve)?;

    Ok(())
}

/// Handles a specific request
fn handle_req(lua: &Lua, request: &mut Request) -> Result<Vec<u8>, RequestError> {
    let mut input = String::new();
    request.as_reader().read_to_string(&mut input)?;

    let lua_code = lua.load(input.trim());

    let eval_i = std::time::Instant::now();
    let value: LuaValue = lua_code.eval()?;
    let eval_time = eval_i.elapsed();

    let buffer = rmp_serde::to_vec(&ScriptResponse::Ok { value, eval_time })?;

    Ok(buffer)
}

/// Tells the client that this server is ready and can accept incoming requests
fn ready() {
    // this is for the client library to know that the script env has fully started
    let mut stdout = io::stdout().lock();
    stdout.write_all(&[10, 20, 30, 40, 50, 60, 70, 80]).unwrap();
    stdout.flush().unwrap();
}

#[derive(Debug, serde::Serialize)]
pub enum ScriptResponse {
    Err(String),
    Ok {
        value: LuaValue,
        eval_time: Duration,
    },
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
