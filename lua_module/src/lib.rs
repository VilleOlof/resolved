use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener},
    time::Duration,
};

use mlua::prelude::*;

mod client;
mod error;
mod handler;
mod item_ref;
mod request;

use error::{ModuleError, RequestError};
use handler::handle_req;
use item_ref::ItemRefHandler;

use crate::{
    client::Client,
    request::{Request, serialize_err},
};

#[mlua::lua_module]
fn vinci(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("start", lua.create_function(start)?)?;
    Ok(exports)
}

fn start(lua: &Lua, port: u16) -> LuaResult<()> {
    let mut client = Client::new(port).unwrap();
    match _start(lua, &mut client) {
        Ok(unit) => Ok(unit),
        Err(e) => {
            client.write_err(e.to_string()).unwrap();
            match e {
                ModuleError::Lua(l) => Err(l),
                other => Err(LuaError::external(other)),
            }
        }
    }
}

fn _start(lua: &Lua, client: &mut Client) -> Result<(), ModuleError> {
    let globals_ref = lua.globals().clone();
    let resolve = resolve(lua, client)?;
    globals_ref.set("resolve", &resolve)?;

    lua.set_globals(globals_ref.clone())?;

    let host = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));
    let server = TcpListener::bind(host)?;

    let module_port = server.local_addr()?.port();
    client.write_port(module_port)?;

    let mut item_ref_handler = ItemRefHandler::new(lua);

    for stream in server.incoming() {
        let mut request = Request::new(stream?);
        lua.set_globals(globals_ref.clone())?;

        let res = match handle_req(lua, &mut item_ref_handler, &resolve, &mut request) {
            Err(e) => serialize_err(e.to_string()).expect("Failed to serialize err string"),
            Ok(buf) => buf,
        };

        let _ = request.send(res);
    }

    Ok(())
}

fn resolve(lua: &Lua, client: &mut Client) -> Result<LuaAnyUserData, ModuleError> {
    match lua.load("return Resolve()").eval() {
        Ok(r) => Ok(r),
        Err(e) => {
            client.write_noresolve()?;
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
