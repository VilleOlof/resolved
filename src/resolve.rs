use std::{fmt::Debug, path::Path, process::Stdio, sync::Arc, time::Duration};

use bytes::Bytes;
use reqwest::{Client, Url};
use resolved_shared::PacketType;
use serde::de::DeserializeOwned;
use tempfile::TempDir;
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    process::Command,
    select,
};

use crate::{
    Error, ScriptResponse,
    script::{LUA_MODULE, MODULE_NAME, dll_script, fuscript},
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
        let client = Client::builder().user_agent(APP_USER_AGENT).build()?;
        let temp_dir = Arc::new(TempDir::new()?);

        let port = Self::start(&temp_dir).await?;
        let url = Url::parse(&format!("http://127.0.0.1:{port}"))?;

        let s = Self {
            port,
            url,
            client,
            temp_dir,
        };
        Ok(s)
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

async fn start_client_server() -> Result<(TcpListener, u16), Error> {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

async fn spawn_script_server(script_path: &Path, port: u16) -> Result<(), Error> {
    let fuscript = fuscript()?;
    let script_path = script_path.display().to_string();
    let port = port.to_string();
    tokio::spawn(async move {
        Command::new(fuscript)
            .arg("-q")
            .args([script_path, port])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
    })
    .await?;

    Ok(())
}

pub(crate) const MODULE_TIMEOUT: Duration = Duration::from_secs(3);

async fn handle_module_request(stream: &mut TcpStream) -> Result<u16, Error> {
    async fn read_err(stream: &mut TcpStream) -> Result<Error, Error> {
        let len = stream.read_u32().await?;
        let mut s = vec![0; len as usize];
        stream.read_exact(&mut s).await?;
        let err = String::from_utf8(s)?;
        Ok(Error::LuaModuleErr(err))
    }

    let sleep = tokio::time::sleep(MODULE_TIMEOUT);
    tokio::pin!(sleep);

    select! {
        _ = &mut sleep => {
            Err(Error::ModuleTimeout)
        }
        p = stream.read_u8() => {
            let packet_type = PacketType::from_u8(p?).ok_or(Error::InvalidPacketType)?;
            match packet_type {
                PacketType::Ready => Ok(stream.read_u16().await?),
                PacketType::NoResolve => Err(Error::UnableToReachDavinciResolve),
                PacketType::Error => Err(read_err(stream).await?)
            }
        }
    }
}
