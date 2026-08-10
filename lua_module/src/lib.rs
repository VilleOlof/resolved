use std::{io::Write, net::TcpStream, time::Duration};

use mlua::prelude::*;
use resolved_shared::{PrePacket, ScriptResponse};
use tiny_http::{Response, Server};

mod error;
mod item_ref;
mod packet;
mod request;

use error::{ModuleError, RequestError};
use item_ref::ItemRefHandler;
use request::handle_req;

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
            client.write(&[PrePacket::Error as u8]).unwrap();
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
    let globals_ref = lua.globals().clone();
    let resolve = resolve(lua, client)?;
    globals_ref.set("resolve", &resolve)?;

    lua.set_globals(globals_ref.clone())?;

    let server = Server::http("0.0.0.0:0")?;
    let module_port = server
        .server_addr()
        .to_ip()
        .ok_or(ModuleError::NoIp)?
        .port();

    client.write(&[PrePacket::Ready as u8])?;
    client.write(&module_port.to_be_bytes())?;
    client.flush()?;

    let mut item_ref_handler = ItemRefHandler::new(lua);

    for mut request in server.incoming_requests() {
        lua.set_globals(globals_ref.clone())?;

        let res = match handle_req(lua, &mut item_ref_handler, &resolve, &mut request) {
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

fn resolve(lua: &Lua, stream: &mut TcpStream) -> Result<LuaAnyUserData, ModuleError> {
    match lua.load("return Resolve()").eval() {
        Ok(r) => Ok(r),
        Err(e) => {
            stream.write(&[PrePacket::NoResolve as u8])?;
            stream.flush()?;
            return Err(ModuleError::Lua(e));
        }
    }
}

fn execute(lua: &Lua, code: String) -> Result<(LuaValue, Duration), RequestError> {
    let lua_code = lua.load(code.trim());
    let eval_i = std::time::Instant::now();
    let value: LuaValue = lua_code.eval()?;
    Ok((value, eval_i.elapsed()))
}
