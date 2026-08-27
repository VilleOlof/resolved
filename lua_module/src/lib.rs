use std::{
    io::{Read, Write},
    path::Path,
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

mod log;

use crate::{
    client::{Client, connect_pipe},
    reader::ShmemReader,
    request::serialize_err,
};
use error::{ModuleError, RequestError};
use handler::handle_req;
use item_ref::ItemRefHandler;
use resolved_shared::{
    ModuleConfig, PipeFlag, ShmemConf, ShmemData, instance_dir, shmem_path, shmem_struct,
};

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

    #[cfg(feature = "tracing")]
    log::init(id);

    let client = Arc::new(Mutex::new(client));
    match _start(lua, id, client.clone(), config) {
        Ok(unit) => Ok(unit),
        Err(e) => {
            error!(%e, "_start failed");
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

/// The real start functions once the outer one has connected to the module and gathered the configuration
fn _start(
    lua: &Lua,
    id: u32,
    client: SharedClient,
    config: ModuleConfig,
) -> Result<(), ModuleError> {
    info!(?id, ?config, "Started module setup");

    let globals_ref = lua.globals();
    let resolve = resolve(lua, client.clone())?;
    globals_ref.set(GLOBAL_RESOLVE, &resolve)?;
    globals_ref.set(GLOBAL_SLEEP, lua.create_function(sleep)?)?;

    let user_globals = config.globals;
    for (k, v) in &user_globals {
        let lua_value = lua.to_value(&v)?;
        globals_ref.set(k.as_str(), lua_value)?;
    }

    lua.set_globals(globals_ref.clone())?;
    info!(?user_globals, "globals added");

    let mut item_ref_handler = ItemRefHandler::new(lua);
    let mut buffers = Buffers::default();
    let mut shmem = ShmemModule::new(&shmem_path(&instance_dir(id)))?;

    let mut pipe = connect_pipe(id)?;
    let mut wait_buf = [0u8; 1];

    info!("Ready, connected to pipe");

    loop {
        if let Err(e) = pipe.read_exact(&mut wait_buf) {
            match e.kind() {
                // pipe was closed, client shutdown
                std::io::ErrorKind::UnexpectedEof => {
                    warn!("pipe was closed, client shutdown or crashed");
                    return Ok(());
                }
                _ => return Err(e.into()),
            }
        }

        if PipeFlag::ClientSent as u8 != wait_buf[0] {
            return Err(ModuleError::InvalidPipeFlag(
                PipeFlag::ClientSent as u8,
                wait_buf[0],
            ));
        }
        let handle = shmem.get_handle();
        debug!(?handle, "new request");

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
            config.function_check.as_ref(),
        ) {
            Err(e) => serialize_err(e.to_string()).expect("Failed to serialize err string"),
            Ok(buf) => buf,
        };

        if let Err(_) = shmem.write_data(&res) {
            // if the owner was wrong, the client must have changed it so we do nothing
            warn!(
                "Tried to write the response back but the shared memory owner was wrong, client probably timeout'd so we relax"
            );
            continue;
        }
        // if the above write failed we wouldnt attempt to write the handle
        // so we can safely always assume to write it here
        shmem.set_handle(handle);
        pipe.write_all(&[PipeFlag::ModuleSent as u8])?;
        pipe.flush()?;
        debug!(?handle, "wrote and flushed to client");
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

fn table_values(table: &LuaTable) -> LuaResult<Vec<LuaValue>> {
    let mut t = Vec::with_capacity(table.len()? as usize);
    for pair in table.pairs::<LuaValue, LuaValue>() {
        t.push(pair?.1);
    }
    Ok(t)
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
    pub fn new(path: &Path) -> Result<Self, ModuleError> {
        let _schmem = ShmemConf::new().flink(path).open()?;
        info!("Opened shared memory");
        let ptr = _schmem.as_ptr();
        Ok(Self { _schmem, ptr })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_clone() -> Result<(), ModuleError> {
        let lua = Lua::new();

        let table_1 = lua.create_table()?;
        table_1.set("A", "B")?;

        let table_1_clone = clone_table(&lua, &table_1)?;

        // should have different underlying references
        assert_ne!(table_1.to_pointer(), table_1_clone.to_pointer());

        // the native .clone is the same underlying
        let table_1_same = table_1.clone();
        assert_eq!(table_1.to_pointer(), table_1_same.to_pointer());

        let table_2 = lua.create_table()?;
        assert_ne!(table_1, table_2);

        Ok(())
    }

    #[test]
    fn keys() -> LuaResult<()> {
        let lua = Lua::new();

        let table = lua.create_table()?;
        table.set("age", 31)?;
        table.set("expires", 5)?;

        let mut keys = table_keys(&table)?
            .iter()
            .map(|x| x.as_string().unwrap().to_string_lossy())
            .collect::<Vec<String>>();
        keys.sort();
        assert_eq!(vec!["age", "expires"], keys);

        table.remove("age")?;
        assert_eq!(
            vec!["expires"],
            table_keys(&table)?
                .iter()
                .map(|x| x.as_string().unwrap().to_string_lossy())
                .collect::<Vec<String>>()
        );

        let table = lua.create_table()?;
        assert!(table_keys(&table)?.is_empty());

        table.push("bleh")?;
        assert_eq!(
            vec![1],
            table_keys(&table)?
                .iter()
                .map(|x| x.as_i32().unwrap())
                .collect::<Vec<i32>>()
        );

        Ok(())
    }

    #[test]
    fn run_lua_code() -> Result<(), RequestError> {
        let lua = Lua::new();

        let (val, time) = execute(&lua, r#""cool!""#)?;
        assert_eq!("cool!", val.as_string().unwrap().to_string_lossy());
        assert!(time > Duration::ZERO);

        let (val, time) = execute(&lua, "local a = 5\nreturn 1")?;
        assert_eq!(1, val.as_i32().unwrap());
        assert!(time > Duration::ZERO);

        Ok(())
    }
}
