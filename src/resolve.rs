use std::{
    fmt::Debug,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
    time::Duration,
};

use resolved_shared::ScriptResponse;
use serde::de::DeserializeOwned;
use tempfile::TempDir;
use tokio::{fs::write, task::JoinHandle};

use crate::{
    Error, ItemRef, Script,
    script_handler::{
        LUA_MODULE, MODULE_NAME, dll_script, handle_module_request, spawn_script_server,
        start_client_server, start_ping_responder,
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
    pub(crate) host: Arc<ModuleAddr>,
    /// We just hold onto this so it doesnt run its Drop function and remove the files until Resolve is dropped
    pub(crate) _temp_dir: Arc<TempDir>,
    id: u64,
    /// We need to store the handle to the background task that responds to pings from the lua module,
    /// so we can properly abort the task when this instance is dropped
    _ping_responder: Arc<PingResponder>,
}

// We need to wrap `host` and `ping_responder` since they are in an Arc
// we cant just drop on Resolve itself since those references the inner Arc's still live on
// But the fields can never leave the Resolve instance
// but a clone of a resolve instance can get dropped
// and then we dont want to drop anything, only when the inner arc'd values get dropped
// so the last remaining resolve instance which still holds an arc ref
// so we wrap them so we can impl Drop and properly send a shutdown signal and abort the pong task

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModuleAddr(pub(crate) SocketAddr);
impl Drop for ModuleAddr {
    fn drop(&mut self) {
        let _ = Resolve::send_shutdown(&self.0);
    }
}
#[derive(Debug)]
pub(crate) struct PingResponder(pub(crate) JoinHandle<()>);
impl Drop for PingResponder {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl PartialEq<Resolve> for Resolve {
    fn eq(&self, other: &Resolve) -> bool {
        self.id() == other.id()
    }
}

impl Resolve {
    pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

    /// Creates a new [`Resolve`] connection instance.  
    ///
    /// This creates a temporary directory with relevant script and dlls files.\
    /// Launches `fuscript` and starts a local script server that [`Resolve`] can use.
    ///
    /// # Errors
    /// If the internal lua module fails to reach *DaVinci Resolve* or the creation of this instance fails
    pub async fn new() -> Result<Self, Error> {
        Self::new_with_timeout(Self::DEFAULT_TIMEOUT).await
    }

    pub async fn new_with_timeout(timeout: Duration) -> Result<Self, Error> {
        let timeout_ms = timeout.as_millis() as u64;

        let temp_dir = TempDir::new()?;

        let (port, ping_responder) = start(&temp_dir, timeout_ms).await?;
        let host = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));

        let id = fastrand::u64(..);

        Ok(Self {
            host: Arc::new(ModuleAddr(host)),
            _temp_dir: Arc::new(temp_dir),
            id,
            _ping_responder: Arc::new(PingResponder(ping_responder)),
        })
    }

    /// Returns a unique `id` to this specific [`Resolve`] instance, can be used to check if two instances are the same or not.
    #[inline]
    pub fn id(&self) -> u64 {
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
            } => Ok(unsafe { ItemRef::new(self.clone(), value) }),
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
async fn start(temp_dir: &TempDir, timeout_ms: u64) -> Result<(u16, JoinHandle<()>), Error> {
    let dll = temp_dir.path().join(format!("{MODULE_NAME}.dll"));
    write(&dll, LUA_MODULE).await?;

    let script = dll_script(temp_dir.path());
    let script_path = temp_dir.path().join("script.lua");
    write(&script_path, &script).await?;

    let (listener, port) = start_client_server().await?;

    spawn_script_server(&script_path, port, timeout_ms).await?;
    let (mut stream, _addr) = listener.accept().await?;

    let port = handle_module_request(&mut stream).await?;

    let handle = start_ping_responder(stream).await;

    Ok((port, handle))
}
