use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener},
    sync::{Arc, Mutex},
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

type SharedClient = Arc<Mutex<Client>>;

#[mlua::lua_module]
fn vinci(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("start", lua.create_function(start)?)?;
    Ok(exports)
}

fn start(lua: &Lua, (port, timeout_ms): (u16, u64)) -> LuaResult<()> {
    let client = Arc::new(Mutex::new(Client::new(port).unwrap()));
    match _start(lua, client.clone(), timeout_ms) {
        Ok(unit) => Ok(unit),
        Err(e) => {
            client.lock().unwrap().write_err(e.to_string()).unwrap();
            match e {
                ModuleError::Lua(l) => Err(l),
                other => Err(LuaError::external(other)),
            }
        }
    }
}

/// Creates a deep clone of the table instead of just the handle
fn clone_table(lua: &Lua, t: &LuaTable) -> Result<LuaTable, ModuleError> {
    let r = lua.create_table()?;
    for v in t.pairs::<LuaValue, LuaValue>() {
        let (k, v) = v?;
        r.set(k, v)?;
    }
    Ok(r)
}

fn sleep(_: &Lua, millis: u64) -> LuaResult<()> {
    std::thread::sleep(Duration::from_millis(millis));
    Ok(())
}

fn _start(lua: &Lua, client: SharedClient, timeout_ms: u64) -> Result<(), ModuleError> {
    let globals_ref = lua.globals();
    let resolve = resolve(lua, client.clone())?;
    globals_ref.set("resolve", &resolve)?;
    globals_ref.set("sleep", lua.create_function(sleep)?)?;

    lua.set_globals(globals_ref.clone())?;

    let host = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));
    let server = TcpListener::bind(host)?;

    let module_port = server.local_addr()?.port();
    {
        client.lock().unwrap().write_port(module_port)?;
    }

    ping_requester(client.clone(), timeout_ms);

    let mut item_ref_handler = ItemRefHandler::new(lua);

    for stream in server.incoming() {
        let mut request = Request::new(stream?);
        lua.set_globals(clone_table(lua, &globals_ref)?)?;

        let res = match handle_req(lua, &mut item_ref_handler, &resolve, &mut request) {
            Err(e) => serialize_err(e.to_string()).expect("Failed to serialize err string"),
            Ok(buf) => buf,
        };

        let _ = request.send(res);
    }

    Ok(())
}

/// If the client doesn't recieve back a `Pong` within 3 seconds, it is assumed to have died and we should also exit
fn ping_requester(client: SharedClient, timeout_ms: u64) {
    use std::{
        process::exit,
        thread::{sleep, spawn},
    };
    let interval = Duration::from_millis(timeout_ms);
    spawn(move || {
        {
            client.lock().unwrap().set_read_timeout(interval);
        }
        loop {
            sleep(interval);
            {
                let mut c = client.lock().unwrap();
                if let Err(_e) = c.write_ping() {
                    exit(0);
                }
                if let Err(_e) = c.read_pong() {
                    exit(0);
                }
            }
        }
    });
}

fn resolve(lua: &Lua, client: SharedClient) -> Result<LuaAnyUserData, ModuleError> {
    match lua.load("return Resolve()").eval() {
        Ok(r) => Ok(r),
        Err(e) => {
            client.lock().unwrap().write_noresolve()?;
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
