use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use mlua::prelude::*;

mod client;
mod error;
mod handler;
mod item_ref;
mod reader;
mod request;

use crate::{client::Client, reader::ShmemReader, request::serialize_err};
use error::{ModuleError, RequestError};
use handler::handle_req;
use item_ref::ItemRefHandler;
use resolved_shared::{ModuleConfig, PipeFlag, ShmemConf, ShmemData, shmem_struct};

/// The function the tiny starting lua script runs when loading this module
const MODULE_ENTRY_FUNCTION: &str = "start";
/// The function in the global scope that the Scripting API provides as the root
const RESOLVE_ENTRY_POINT: &str = "Resolve";
/// What ItemRefs and resolve client uses as their "own" context variable
const GLOBAL_SELF: &str = "self";
/// Nameless script arguments are pushed to this in globals
const GLOBAL_ARG: &str = "arg";
/// Global variable for the root of the Scripting API
const GLOBAL_RESOLVE: &str = "resolve";
/// Global function to sleep N milliseconds accurately
const GLOBAL_SLEEP: &str = "sleep";
const RESOLVE_FLAGS: &str = "__flags";

type SharedClient = Arc<Mutex<Client>>;

/// Entry point of the entire module
#[mlua::lua_module]
fn vinci(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set(MODULE_ENTRY_FUNCTION, lua.create_function(start)?)?;
    Ok(exports)
}

/// Starts off the entire module from the lua script, the port should be the port of the client crate's client_server
fn start(lua: &Lua, id: u32) -> LuaResult<()> {
    let mut client = Client::new(id).expect("failed to init client");

    let config = client.read_config().expect("failed to read configuration");
    let client = Arc::new(Mutex::new(client));
    match _start(lua, id, client.clone(), config) {
        Ok(unit) => Ok(unit),
        Err(e) => {
            // this will error if pipe is closed
            let _ = client
                .lock()
                .expect("client was poisoned")
                .write_err(e.to_string());
            match e {
                ModuleError::Lua(l) => Err(l),
                other => Err(LuaError::external(other)),
            }
        }
    }
}

/// Creates a deep clone of the table instead of just the handle
fn clone_table(lua: &Lua, t: &LuaTable) -> Result<LuaTable, ModuleError> {
    let r = lua.create_table_with_capacity(0, t.len()? as usize)?;
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

fn table_keys(table: &LuaTable) -> LuaResult<Vec<LuaValue>> {
    let mut t = Vec::with_capacity(table.len()? as usize);
    for pair in table.pairs::<LuaValue, LuaValue>() {
        t.push(pair?.0);
    }
    Ok(t)
}

/// The real start functions once the outer one has connected to the module and gathered the configuration
fn _start(
    lua: &Lua,
    id: u32,
    client: SharedClient,
    config: ModuleConfig,
) -> Result<(), ModuleError> {
    let globals_ref = lua.globals();
    let resolve = resolve(lua, client.clone())?;
    globals_ref.set(GLOBAL_RESOLVE, &resolve)?;
    globals_ref.set(GLOBAL_SLEEP, lua.create_function(sleep)?)?;

    lua.set_globals(globals_ref.clone())?;

    let mut item_ref_handler = ItemRefHandler::new(lua);
    let mut buffers = Buffers::default();
    let mut shmem = ShmemModule::new(&config.shmem_path)?;

    let mut pipe = resolved_shared::connect_pipe(id)?;
    let mut wait_buf = [0u8; 1];

    loop {
        if let Err(e) = pipe.read_exact(&mut wait_buf) {
            match e.kind() {
                // pipe was closed, client shutdown
                std::io::ErrorKind::UnexpectedEof => return Ok(()),
                _ => return Err(e.into()),
            }
        }

        if PipeFlag::ClientSent as u8 != wait_buf[0] {
            return Err(ModuleError::InvalidPipeFlag(
                PipeFlag::ClientSent as u8,
                wait_buf[0],
            ));
        }

        if config.reset_globals {
            lua.set_globals(clone_table(lua, &globals_ref)?)?;
        }
        buffers.clear();

        let mut reader = ShmemReader::new(&mut shmem);

        let res = match handle_req(
            lua,
            &mut reader,
            &mut item_ref_handler,
            &resolve,
            &mut buffers,
        ) {
            Err(e) => serialize_err(e.to_string()).expect("Failed to serialize err string"),
            Ok(buf) => buf,
        };

        if let Err(_) = shmem.write_data(&res) {
            // if the owner was wrong, the client must have changed it so we do nothing
            continue;
        }
        pipe.write_all(&[PipeFlag::ModuleSent as u8])?;
        pipe.flush()?;
    }
}

/// Returns the root of the Scripting API
fn resolve(lua: &Lua, client: SharedClient) -> Result<LuaAnyUserData, ModuleError> {
    let resolve_var = lua.globals().get::<LuaValue>(RESOLVE_ENTRY_POINT)?;
    let resolve_fn = resolve_var
        .as_function()
        .ok_or(ModuleError::GlobalResolveWasNotAFunction(
            resolve_var.type_name(),
        ))?;

    match resolve_fn.call::<LuaAnyUserData>(()) {
        Ok(r) => Ok(r),
        Err(e) => {
            client
                .lock()
                .expect("client was poisoned")
                .write_noresolve()?;
            return Err(ModuleError::Lua(e));
        }
    }
}

/// Executes some lua code and times it
fn execute(lua: &Lua, code: &str) -> Result<(LuaValue, Duration), RequestError> {
    let lua_code = lua.load(code);
    let eval_i = Instant::now();
    let value: LuaValue = lua_code.eval()?;
    let eval_time = eval_i.elapsed();
    Ok((value, eval_time))
}

#[derive(Debug, Default)]
struct Buffers {
    nameless_args: Vec<LuaValue>,
}

impl Buffers {
    pub fn clear(&mut self) {
        self.nameless_args.clear();
    }
}

shmem_struct!(ShmemModule, (Module => Client));

impl ShmemModule {
    pub fn new(path: &str) -> Result<Self, ModuleError> {
        let _schmem = ShmemConf::new().flink(path).open()?;
        let ptr = _schmem.as_ptr();
        Ok(Self { _schmem, ptr })
    }
}
