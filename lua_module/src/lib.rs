use std::{io::Write, net::TcpStream};

use mlua::prelude::*;
use resolved_shared::{PacketType, ScriptResponse};
use tiny_http::{Request, Response, Server};

#[mlua::lua_module]
fn vinci(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("start", lua.create_function(start)?)?;
    Ok(exports)
}

fn start(lua: &Lua, port: u16) -> LuaResult<()> {
    let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    match _start(lua, &mut client) {
        Ok(unit) => Ok(unit),
        Err(e) => {
            // write packet type 'error' with a length prefixed str of the formatted err
            let str = e.to_string();
            client.write(&[PacketType::Error as u8]).unwrap();
            client.write(&(str.len() as u32).to_be_bytes()).unwrap();
            client.write(&str.into_bytes()).unwrap();
            client.flush().unwrap();
            match e {
                ModuleError::Lua(l) => Err(l),
                other => Err(LuaError::external(other)),
            }
        }
    }
}

fn _start(lua: &Lua, client: &mut TcpStream) -> Result<(), ModuleError> {
    setup_globals(lua, client)?;

    let server = Server::http("0.0.0.0:0")?;
    let module_port = server
        .server_addr()
        .to_ip()
        .ok_or(ModuleError::NoIp)?
        .port();

    client.write(&[PacketType::Ready as u8])?;
    client.write(&module_port.to_be_bytes())?;
    client.flush()?;

    for mut request in server.incoming_requests() {
        let res = match handle_req(lua, &mut request) {
            Err(e) => {
                let s = e.to_string();
                let res = rmp_serde::to_vec(&ScriptResponse::<()>::Err(s))
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

fn setup_globals(lua: &Lua, stream: &mut TcpStream) -> Result<(), ModuleError> {
    let resolve: LuaAnyUserData = match lua.load("return Resolve()").eval() {
        Ok(r) => r,
        Err(e) => {
            stream.write(&[PacketType::NoResolve as u8])?;
            stream.flush()?;
            return Err(ModuleError::Lua(e));
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

#[derive(Debug, thiserror::Error)]
enum RequestError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Rmp(#[from] rmp_serde::encode::Error),
    #[error(transparent)]
    Lua(#[from] mlua::Error),
}

#[derive(Debug, thiserror::Error)]
enum ModuleError {
    #[error("No ip found")]
    NoIp,

    #[error(transparent)]
    Lua(#[from] LuaError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Any(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}
