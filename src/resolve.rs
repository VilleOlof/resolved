use std::{
    fmt::Debug,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};

use resolved_shared::{ResolveConfig, ScriptResponse};
use serde::de::DeserializeOwned;
use tempfile::TempDir;
use tokio::{
    fs::write,
    sync::{Mutex, MutexGuard},
    task::JoinHandle,
};

use crate::{
    Error, ItemRef, ItemRefList, Script,
    script_handler::{
        LUA_MODULE, MODULE_NAME, dll_script, handle_module_request, spawn_script_server,
        start_client_server, start_ping_responder,
    },
};

/// A connection to *`DaVinci Resolve`*.
///
/// Used to run `lua` code with it's Scripting API available.
///
/// ## Globals
/// `resolve` will always point to the value returned from `Resolve()`, which is the root of the **Scripting API** in *`DaVinci Resolve`*.\
/// This is so you don't have to call it yourself everytime.
///
/// Depending on the context that a [`Script`] was executed from, `self` will be the current active instance.\
/// When executing from [`Resolve`], `self` is the root, so `resolve`.\
/// When executing from [`ItemRef`], `self` is the stored value, which can be anything.
///
/// ## Single-Threaded  
/// The script server that this spins up can only accept requests one at a time.\
/// If you wish to send multiple scripts to execute at the same time, start a new [`Resolve`] instance and use that.
///
/// Or you can use [`PooledResolve`](crate::PooledResolve) to start multiple instances at the same time
/// and use any available on when executing. Look at it's doc for more info.
///
/// ## Clone
/// The internal connection to *`DaVinci Resolve`* is the same if you were to run `.clone()` on [`Resolve`].\
/// So [`Resolve`] can be cheaply cloned and passed around.
#[derive(Debug, Clone)]
pub struct Resolve {
    /// All inner data for the instance, wrapped in Arc to be cheaply cloned and referenced,  
    inner: Arc<InnerResolve>,
}

#[derive(Debug)]
struct InnerResolve {
    /// The unique id for this specific instance and connection to `DaVinci Resolve`
    id: u64,
    /// The host to connect to the lua module linked to this instance
    host: SocketAddr,
    /// Buffers while reading and writing packets
    buffers: Mutex<Buffers>,
    /// We just hold onto this so it doesnt run its Drop function and remove the files until Resolve is dropped
    _temp_dir: TempDir,
    /// We need to store the handle to the background task that responds to pings from the lua module,
    /// so we can properly abort the task when this instance is dropped
    ping_responder: JoinHandle<()>,
}

/// Internal buffers that [`Resolve`] can reuse to save allocations
#[derive(Default)]
pub(crate) struct Buffers {
    pub(crate) packet_write: Vec<u8>,
    pub(crate) packet_read: Vec<u8>,
}

impl Buffers {
    pub(crate) fn clear_all(&mut self) {
        self.packet_write.clear();
        self.packet_read.clear();
    }
}

impl Debug for Buffers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // we dont want to spam Debug display with shit ton of the latest bytes
        f.write_str("<Buffers>")
    }
}

// when the inner arc'd resolve instance is fully dropped then we can discard the module and bg tasks
impl Drop for InnerResolve {
    fn drop(&mut self) {
        let _ = Resolve::send_shutdown(&self.host);
        self.ping_responder.abort();
    }
}

impl PartialEq<Resolve> for Resolve {
    fn eq(&self, other: &Resolve) -> bool {
        self.id() == other.id()
    }
}

impl std::hash::Hash for Resolve {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl Resolve {
    /// Creates a new [`Resolve`] connection instance.  
    ///
    /// This creates a temporary directory with relevant script and dlls files.\
    /// Launches `fuscript` and starts a local script server that [`Resolve`] can use.
    ///
    /// # Errors
    /// If the internal lua module fails to reach *`DaVinci Resolve`* or the creation of this instance fails
    pub async fn new() -> Result<Self, Error> {
        Self::new_with_config(&ResolveConfig::DEFAULT).await
    }

    /// Creates a new [`Resolve`] instance with the specified [`ResolveConfig`].
    ///
    /// # Errors
    /// - If it fails to create a temporary directory
    /// - The setup server communication fails
    /// - The module startup fails
    pub async fn new_with_config(config: &ResolveConfig) -> Result<Self, Error> {
        let temp_dir = TempDir::new()?;

        let (port, ping_responder) = start(&temp_dir, config).await?;
        let host = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
        let buffers = Mutex::new(Buffers::default());

        let id = fastrand::u64(..);

        Ok(Self {
            inner: Arc::new(InnerResolve {
                id,
                buffers,
                host,
                _temp_dir: temp_dir,
                ping_responder,
            }),
        })
    }

    /// Returns a unique `id` to this specific [`Resolve`] instance, can be used to check if two instances are the same or not.
    #[inline]
    #[must_use]
    pub fn id(&self) -> u64 {
        self.inner.id
    }

    #[inline]
    pub(crate) fn host(&self) -> SocketAddr {
        self.inner.host
    }

    #[inline]
    pub(crate) async fn buffers(&self) -> MutexGuard<'_, Buffers> {
        let mut buffers = self.inner.buffers.lock().await;
        buffers.clear_all();
        buffers
    }

    /// Execute some `lua` code, the returned value in the code will be returned here.  
    ///
    /// Using [`Script`] (or it's [`script!`](resolved_macros::script) macro) you can pass in arguments to your code.\
    ///
    /// ## Globals  
    ///
    /// Instead of calling `Resolve()` every time to reach for the **Scripting API**, you can use `resolve`.
    /// `resolve` is always available in the global context no matter which `.execute` you run.  
    ///
    /// `self` on the other hand is special to your active instance.
    /// If you run [`execute`](Resolve::execute) from [`Resolve`], `self` will also be the value of the `resolve` global.
    /// But if you run [`execute`](ItemRef::execute) from an [`ItemRef`], that stored value will be `self`.
    ///
    /// `sleep(ms)` is also an available function.
    ///
    /// ## Examples
    ///
    /// ### Simple
    /// ```rust ignore
    /// let resolve = Resolve::new().await?;
    /// let version = resolve.execute::<String>(r#"return self:GetVersionString()"#).await?;
    /// assert!(!version.is_empty());
    /// ```
    ///
    /// ### Arguments
    /// ```rust ignore
    /// let resolve = Resolve::new().await?;
    /// let script = Script::new("return my_var + secret")
    ///     .named_arg("my_var", 5)?
    ///     .named_arg("secret", u8::MAX)?;
    /// let result = resolve.execute::<i32>(script).await?;
    /// assert_eq!(260, result);
    /// ```
    ///
    /// ### On Reference
    /// Look more at [`ItemRef`] and [`store`](Resolve::store) for more info on this.
    /// ```rust ignore
    /// let resolve = Resolve::new().await?;
    /// let pm = resolve.store("return self:GetProjectManager()").await?;
    /// pm.execute::<()>("self:SaveProject()").await?;
    /// ```
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent
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

    /// Store a reference to `Lua` value in `Rust`
    ///
    /// Instead of returning some value, you get an [`ItemRef`].
    /// This is just an **id** that resolves to the stored value when executing.
    ///
    /// You can store any value as an [`ItemRef`], a number, a function, or even an instance of a *timeline*!
    ///
    /// And you can also can [`execute`](ItemRef::execute) and [`.store`](ItemRef::store) on the [`ItemRef`] itself.
    /// in that case, the global variable `self` becomes the value of that [`ItemRef`]
    ///
    /// ## Example
    /// ```rust ignore
    /// let resolve = Resolve::new().await?;
    /// let page: ItemRef = resolve.store("return self:GetCurrentPage()").await?;
    ///
    /// resolve.execute::<()>(Script::new("self:OpenPage(arg[1])").arg_ref(&page)?).await?;
    /// ```
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent
    pub async fn store(&self, script: impl Into<Script<'_>>) -> Result<ItemRef, Error> {
        match self.send_store(script.into()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(unsafe { ItemRef::new(self.clone(), value) }),
        }
    }

    /// Store multiple references to `Lua` values in `Rust`
    ///
    /// Instead of returning some value, you get an [`ItemRefList`].
    /// This is a list of **ids** that resolves into the stored value when executing on them.
    ///
    /// **The returned value in the `Lua` code but must be of type `Table`.**
    ///
    /// You can use [`.list()`](ItemRefList::list) on the [`ItemRefList`] to iterate over all [`ItemRef`]'s inside.
    ///
    /// ## Example
    /// ```rust ignore
    /// let resolve = Resolve::new().await?;
    /// let timeline = resolve.store(r#"
    ///     local pm = self:GetProjectManager()
    ///     local p = pm:GetCurrentProject()
    ///     return p:GetCurrentTimeline()
    /// "#).await?;
    ///
    /// // Once we have our timeline, we can get a list of references to *all* clips on video track 1
    /// let clips = timeline.store_list(r#"self:GetItemListInTrack("video", 1)"#).await?;
    /// for clip in &clips.list() {
    ///     let name: String = clip.execute("self:GetName()").await?;
    ///     println!("{name}");
    /// }
    /// ```
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent.\
    /// Or if the returned value from lua was not a *table*
    pub async fn store_list(&self, script: impl Into<Script<'_>>) -> Result<ItemRefList, Error> {
        match self.send_store_table(script.into()).await? {
            resolved_shared::ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            resolved_shared::ScriptResponse::Ok {
                value: (source, list),
                eval_time: _,
            } => Ok(ItemRefList::new(
                unsafe { ItemRef::new(self.clone(), source) },
                list.into_iter()
                    .map(|x| unsafe { ItemRef::new(self.clone(), x) })
                    .collect(),
            )),
        }
    }

    /// Execute a [`Script`] with an [`ItemRef`] as `self`
    pub(crate) async fn execute_with<'c, T>(
        &self,
        item: &'c ItemRef,
        script: impl Into<Script<'c>>,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let mut script = script.into();
        script = script.with(item)?;
        self.execute(script).await
    }

    /// Store a value with an [`ItemRef`] as `self`
    pub(crate) async fn store_with<'c>(
        &self,
        item: &'c ItemRef,
        script: impl Into<Script<'c>>,
    ) -> Result<ItemRef, Error> {
        let mut script = script.into();
        script = script.with(item)?;
        self.store(script).await
    }

    pub(crate) async fn store_list_with<'c>(
        &self,
        item: &'c ItemRef,
        script: impl Into<Script<'c>>,
    ) -> Result<ItemRefList, Error> {
        let mut script = script.into();
        script = script.with(item)?;
        self.store_list(script).await
    }
}

/// Writes the saved `.dll` and the generated `.lua` script to a temp directory.
/// The client then sends some configuration data and the module send back when
/// it's ready to accept connections and which port to send data to.
///
/// # Errors
/// If the lua module fails to reach *`DaVinci Resolve`* or for some other reason the lua module itself crashes before starting the http server.
async fn start(temp_dir: &TempDir, config: &ResolveConfig) -> Result<(u16, JoinHandle<()>), Error> {
    let dll = temp_dir.path().join(format!("{MODULE_NAME}.dll"));
    write(&dll, LUA_MODULE).await?;

    let (listener, port) = start_client_server().await?;

    let script = dll_script(temp_dir.path(), port);
    let script_path = temp_dir.path().join("script.lua");
    write(&script_path, &script).await?;

    spawn_script_server(&script_path).await?;
    let (mut stream, _addr) = listener.accept().await?;

    let port = handle_module_request(&mut stream, config).await?;

    let handle = start_ping_responder(stream).await;

    Ok((port, handle))
}
