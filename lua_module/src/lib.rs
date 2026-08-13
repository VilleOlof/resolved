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
use resolved_shared::ResolveConfig;

use crate::{
    client::Client,
    request::{Request, serialize_err},
};

type SharedClient = Arc<Mutex<Client>>;

/// Entry point of the entire module
#[mlua::lua_module]
fn vinci(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("start", lua.create_function(start)?)?;
    Ok(exports)
}

/// Starts off the entire module from the lua script, the port should be the port of the client crate's client_server
fn start(lua: &Lua, port: u16) -> LuaResult<()> {
    let mut client = Client::new(port).unwrap();
    let config = client.read_config().unwrap();
    let client = Arc::new(Mutex::new(client));
    match _start(lua, client.clone(), config) {
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

/// an accurate sleep, mostly used in the benchmarks to simulate work being done, but may be useful for others
fn sleep(_: &Lua, millis: u64) -> LuaResult<()> {
    std::thread::sleep(Duration::from_millis(millis));
    Ok(())
}

/// The real start functions once the outer one has connected to the module and gathered the configuration
fn _start(lua: &Lua, client: SharedClient, config: ResolveConfig) -> Result<(), ModuleError> {
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

    ping_requester(client.clone(), config.timeout);

    let mut item_ref_handler = ItemRefHandler::new(lua);

    for stream in server.incoming() {
        let mut request = Request::new(stream?);
        if config.reset_globals {
            lua.set_globals(clone_table(lua, &globals_ref)?)?;
        }

        let res = match handle_req(lua, &mut item_ref_handler, &resolve, &mut request) {
            Err(e) => serialize_err(e.to_string()).expect("Failed to serialize err string"),
            Ok(buf) => buf,
        };

        let _ = request.send(res);
    }

    Ok(())
}

/// If the client doesn't recieve back a `Pong` within 3 seconds, it is assumed to have died and we should also exit
fn ping_requester(client: SharedClient, timeout: Duration) {
    use std::{
        process::exit,
        thread::{sleep, spawn},
    };
    spawn(move || {
        {
            client.lock().unwrap().set_read_timeout(timeout);
        }
        loop {
            sleep(timeout);
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

/// Returns the root of the Scripting API
fn resolve(lua: &Lua, client: SharedClient) -> Result<LuaAnyUserData, ModuleError> {
    match lua.load("return Resolve()").eval() {
        Ok(r) => Ok(r),
        Err(e) => {
            client.lock().unwrap().write_noresolve()?;
            return Err(ModuleError::Lua(e));
        }
    }
}

/// Executes some lua code and times it
fn execute(lua: &Lua, code: &str) -> Result<(LuaValue, Duration), RequestError> {
    let lua_code = lua.load(code.trim());
    let eval_i = std::time::Instant::now();
    let value: LuaValue = lua_code.eval()?;
    Ok((value, eval_i.elapsed()))
}
