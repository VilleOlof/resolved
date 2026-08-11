use std::{
    fmt::Debug,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};

use serde::de::DeserializeOwned;
use tempfile::TempDir;

use crate::{
    Error, ItemRef, Script, ScriptResponse,
    script_handler::{
        LUA_MODULE, MODULE_NAME, dll_script, handle_module_request, spawn_script_server,
        start_client_server,
    },
};

/// A connection to *DaVinci Resolve*.\
/// Used to run `lua` code with it's Scripting API available.
///
/// ## Lua globals
/// `self` and `resolve` both point to the global `Resolve()` instance to DaVinci Resolve's Scripting API root.\
/// ```lua
/// self:Quit()
/// -- or
/// resolve:Quit()
/// -- both calls the same thing
/// ```
/// This is so you don't have to call `Resolve()` every time yourself, since it can never be invalid and never change.
///
/// ## Single-Threaded  
/// The script server that this spins up can only accept requests one at a time.\
/// If you wish to send multiple scripts to execute at the same time, start a new [`Resolve`] instance and use that.
///
/// ## Clone
/// The internal connection to *DaVinci Resolve* is the same if you were to run `.clone()` on [`Resolve`].
#[derive(Debug, Clone)]
pub struct Resolve {
    // TODO: maybe add a unique id to every resolve instance so itemrefs can be caught earlier and properly error
    // if the itemref was taken from another instance
    pub(crate) host: Arc<SocketAddr>,
    /// We just hold onto this so it doesnt run its Drop function and remove the files until Resolve is dropped
    pub(crate) _temp_dir: Arc<TempDir>,
    id: u64,
}

impl Resolve {
    /// Creates a new [`Resolve`] connection instance.  
    ///
    /// This creates a temporary directory with relevant script and dlls files.\
    /// Launches `fuscript` and starts a local script server that [`Resolve`] can use.
    ///
    /// # Errors
    /// If the internal lua module fails to reach *DaVinci Resolve* or the creation of this instance fails
    pub async fn new() -> Result<Self, Error> {
        let temp_dir = TempDir::new()?;

        let port = start(&temp_dir).await?;
        let host = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));

        let id = fastrand::u64(..);

        Ok(Self {
            host: Arc::new(host),
            _temp_dir: Arc::new(temp_dir),
            id,
        })
    }

    #[inline]
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub async fn execute<T>(&self, script: impl Into<Script<'_>>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        match self.send_execute(script.into()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(value),
        }
    }

    pub async fn store(&self, script: impl Into<Script<'_>>) -> Result<ItemRef, Error> {
        match self.send_store(script.into()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(ItemRef::new(self.clone(), value)),
        }
    }

    pub(crate) async fn execute_with<T>(
        &self,
        item: &ItemRef,
        script: impl Into<Script<'_>>,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let mut script = script.into();
        script = script.with(item)?;
        self.execute(script).await
    }

    pub(crate) async fn store_with(
        &self,
        item: &ItemRef,
        script: impl Into<Script<'_>>,
    ) -> Result<ItemRef, Error> {
        let mut script = script.into();
        script = script.with(item)?;
        self.store(script).await
    }
}

/// Writes the saved `.dll` and the generated `.lua` script to a temporary directory.\
/// Which it then uses to start `fuscript.exe` with said script and the specified port.\
///
/// # Errors
/// If the lua module fails to reach *DaVinci Resolve* or for some other reason the lua module itself crashes before starting the http server.
///
async fn start(temp_dir: &TempDir) -> Result<u16, Error> {
    let dll = temp_dir.path().join(format!("{MODULE_NAME}.dll"));
    tokio::fs::write(&dll, LUA_MODULE).await?;

    let script = dll_script(temp_dir.path());
    let script_path = temp_dir.path().join("script.lua");
    tokio::fs::write(&script_path, &script).await?;

    let (listener, port) = start_client_server().await?;

    spawn_script_server(&script_path, port).await?;
    let (mut stream, _addr) = listener.accept().await?;

    handle_module_request(&mut stream).await
}
