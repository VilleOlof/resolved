use std::{fmt::Debug, sync::Arc};

use reqwest::{Client, Url};
use serde::de::DeserializeOwned;
use tempfile::TempDir;

use crate::{
    Error, ItemRef, ScriptResponse,
    script::{
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
#[derive(Clone)]
pub struct Resolve {
    port: u16,
    pub(crate) url: Url,
    pub(crate) client: Client,
    pub(crate) temp_dir: Arc<TempDir>,
}

impl Debug for Resolve {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolve")
            .field("port", &self.port)
            .field("url", &self.url.as_str())
            .field("client", &())
            .field("temp_dir", &self.temp_dir)
            .finish()
    }
}

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

impl Resolve {
    /// Creates a new [`Resolve`] connection instance.  
    ///
    /// This creates a temporary directory with relevant script and dlls files.\
    /// Launches `fuscript` and starts a local script server that [`Resolve`] can use.
    ///
    /// # Errors
    /// If the internal lua module fails to reach *DaVinci Resolve* or the creation of this instance fails
    pub async fn new() -> Result<Self, Error> {
        let client = Client::builder().user_agent(APP_USER_AGENT).build()?;
        let temp_dir = Arc::new(TempDir::new()?);

        let port = start(&temp_dir).await?;
        let url = Url::parse(&format!("http://127.0.0.1:{port}"))?;

        let s = Self {
            port,
            url,
            client,
            temp_dir,
        };
        Ok(s)
    }

    /// Send and execute a piece of `lua` code to *DaVinci Resolve*.  
    ///
    /// This code has full context to it's Scripting API.  
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
    /// ## Example
    ///
    /// #### Lua Script
    /// ```lua
    /// local pm = self:GetProjectManager()
    /// local p = pm:GetCurrentProject()
    /// local t = p:GetCurrentTimeline()
    /// return t:GetName()
    /// ```
    ///
    /// #### Rust Code
    /// ```ignore
    /// let script = "<code above>";
    /// let resolve = Resolve::new().await?;
    /// let timeline_name = resolve.execute::<String>(script).await?;
    /// ```
    ///
    /// # Errors
    /// If the *lua code* fails for some reason it will be returned here
    pub async fn execute<T>(&self, lua_script: impl Into<String>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        match self.send_execute(lua_script.into()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(value),
        }
    }

    pub(crate) async fn execute_with<T>(
        &self,
        item: &ItemRef,
        lua_script: impl Into<String>,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        match self.send_execute_with(item.id(), lua_script.into()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(value),
        }
    }

    pub async fn store(&self, lua_script: impl Into<String>) -> Result<ItemRef, Error> {
        match self.send_store(lua_script.into()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(ItemRef::new(self.clone(), value)),
        }
    }

    pub(crate) async fn store_with(
        &self,
        item: &ItemRef,
        lua_script: impl Into<String>,
    ) -> Result<ItemRef, Error> {
        match self.send_store_with(item.id(), lua_script.into()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(ItemRef::new(self.clone(), value)),
        }
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
