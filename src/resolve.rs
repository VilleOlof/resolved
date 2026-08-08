use std::{fmt::Debug, process::Stdio, sync::Arc};

use bytes::Bytes;
use reqwest::{Client, Url};
use serde::de::DeserializeOwned;
use tempfile::TempDir;
use tokio::{io::AsyncReadExt, process::Command};

use crate::{
    Error, ScriptResponse,
    port::random_port,
    script::{LUA_MODULE, MODULE_NAME, READY_CALL, RESOLVE_FAILED, dll_script, fuscript},
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
    url: Url,
    client: Client,
    temp_dir: Arc<TempDir>,
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
        let port = random_port().await?;
        Self::new_with_port(port).await
    }

    /// Creates a new [`Resolve`] connection instance with a specific port to the underlying script server.  
    pub async fn new_with_port(port: u16) -> Result<Self, Error> {
        let client = Client::builder().user_agent(APP_USER_AGENT).build()?;
        let url = Url::parse(&format!("http://127.0.0.1:{port}"))?;
        let temp_dir = Arc::new(TempDir::new()?);

        let s = Self {
            port,
            url,
            client,
            temp_dir,
        };
        s.start().await?;
        Ok(s)
    }

    /// Writes the saved `.dll` and the generated `.lua` script to a temporary directory.\
    /// Which it then uses to start `fuscript.exe` with said script and the specified port.\
    ///
    /// # Errors
    /// If the lua module fails to reach *DaVinci Resolve* or for some other reason the lua module itself crashes before starting the http server.
    ///
    async fn start(&self) -> Result<(), Error> {
        let dll = self.temp_dir.path().join(format!("{MODULE_NAME}.dll"));
        tokio::fs::write(&dll, LUA_MODULE).await?;

        let script = dll_script(self.temp_dir.path());
        let script_path = self.temp_dir.path().join("script.lua");
        tokio::fs::write(&script_path, &script).await?;

        let mut child = Command::new(fuscript()?)
            .arg("-q")
            .args([script_path.display().to_string(), self.port.to_string()])
            .stdout(Stdio::piped())
            .spawn()?;

        let mut stdout = child.stdout.take().ok_or(Error::NoStdout)?;

        let mut buf = [0; 8];
        stdout.read_exact(&mut buf).await?;

        if buf == READY_CALL {
            Ok(())
        } else if buf == RESOLVE_FAILED {
            Err(Error::UnableToReachDavinciResolve)
        } else {
            Err(Error::FuscriptFailed(buf))
        }
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
        match self.deserialize(lua_script.into()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(value),
        }
    }

    /// Reads in a `.lua` file and executes it in *DaVinci Resolve*  
    ///
    /// See [`Resolve::execute`] for more info on errors, global variables and examples
    pub async fn execute_file<T, P>(&self, file: impl AsRef<std::path::Path>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let contents = tokio::fs::read_to_string(file.as_ref()).await?;
        self.execute(contents).await
    }

    /// Send and execute a piece of `lua` code and returns the direct [`ScriptResponse`] from the script.
    pub async fn deserialize<T: DeserializeOwned>(
        &self,
        lua_script: String,
    ) -> Result<ScriptResponse<T>, Error> {
        let bytes = self.raw(lua_script).await?;
        Ok(rmp_serde::from_slice(&bytes)?)
    }

    /// Send and execute a piece of `lua` code and returns the raw bytes from the script server.
    async fn raw(&self, lua_script: String) -> Result<Bytes, Error> {
        let req = self.client.post(self.url.clone()).body(lua_script);
        let res = req.send().await?;
        Ok(res.bytes().await?)
    }
}
