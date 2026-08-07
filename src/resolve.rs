use std::{process::Stdio, sync::Arc};

use bytes::Bytes;
use reqwest::{Client, Url};
use serde::de::DeserializeOwned;
use tempfile::TempDir;
use tokio::{io::AsyncReadExt, process::Command};

use crate::{
    ScriptResponse, random_port,
    script::{LUA_MODULE, MODULE_NAME, READY_CALL, dll_script, fuscript},
};

#[derive(Debug, Clone)]
pub struct Resolve {
    port: u16,
    url: Url,
    client: Client,
    temp_dir: Arc<TempDir>,
}

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

impl Resolve {
    pub async fn new() -> Self {
        let port = random_port().unwrap();
        Self::new_with_port(port).await
    }

    pub async fn new_with_port(port: u16) -> Self {
        let client = Client::builder()
            .user_agent(APP_USER_AGENT)
            .build()
            .unwrap();
        let url = Url::parse(&format!("http://127.0.0.1:{port}")).unwrap();
        let temp_dir = Arc::new(TempDir::new().unwrap());

        let s = Self {
            port,
            url,
            client,
            temp_dir,
        };
        s.start().await;
        s
    }

    async fn start(&self) {
        let dll = self.temp_dir.path().join(format!("{MODULE_NAME}.dll"));
        tokio::fs::write(&dll, LUA_MODULE).await.unwrap();

        let script = dll_script(self.temp_dir.path());
        let script_path = self.temp_dir.path().join("script.lua");
        tokio::fs::write(&script_path, &script).await.unwrap();

        let mut child = Command::new(fuscript())
            .arg("-q")
            .args([script_path.display().to_string(), self.port.to_string()])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let mut stdout = child.stdout.take().unwrap();

        let mut buf = [0; READY_CALL.len()];
        stdout.read_exact(&mut buf).await.unwrap();
        let output = String::from_utf8_lossy(&buf);

        if output != READY_CALL {
            panic!("somethings wrong")
        }

        // now we can return once we have the instance ready
    }

    ///
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
    /// let script = "...";
    /// let resolve = Resolve::new();
    /// let timeline_name = resolve.call::<String>(script);
    /// ```
    ///
    pub async fn execute<T: DeserializeOwned>(&self, lua_script: impl Into<String>) -> T {
        self.deserialize(lua_script.into()).await.value()
    }

    pub async fn deserialize<T: DeserializeOwned>(&self, lua_script: String) -> ScriptResponse<T> {
        let bytes = self.raw(lua_script).await;
        rmp_serde::from_slice(&bytes).unwrap()
    }

    pub async fn raw(&self, lua_script: String) -> Bytes {
        let req = self.client.post(self.url.clone()).body(lua_script);
        let res = req.send().await.unwrap();
        res.bytes().await.unwrap()
    }
}
