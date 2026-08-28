use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use parking_lot::RwLock;
use resolved_shared::{ScriptResponse, instance_dir, shmem_path};
use serde::de::DeserializeOwned;
use tokio::{
    fs::{create_dir, write},
    process::Child,
    sync::{Mutex, MutexGuard},
};

use crate::{
    Error, ItemRef, ItemRefList, ResolveConfig, Script, cleanup,
    packet::ShmemClient,
    script_handler::{
        LUA_MODULE, LUA_MODULE_TRACING, MODULE_NAME, Pipe, dll_script, handle_module_request,
        new_module_pipe, new_pipe, spawn_script_server, write_config,
    },
};

macro_rules! log_script_resposne {
    ($script:expr, $eval:expr, $name:literal) => {
        #[cfg(feature = "tracing")]
        {
            let args = $script.args.len();
            let with = &$script.with;
            let script = &$script.lua;
            tracing::trace!(eval_time = ?$eval, ?args, ?with, ?script, $name);
        }
    };
}

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
    id: u32,
    /// The default timeout for [`Script`]'s if they haven't specified their own timeout.
    default_timeout: Duration,
    /// If the module was shutdown or if the module in someway shutdown, this is set to `true`
    cancelled: Arc<RwLock<bool>>,
    /// Instances of the shared memory and pipe used for requests
    packet_handler: Mutex<PacketHandler>,
    /// The pipe used for setting up the module
    _module_pipe: Pipe,
    /// The script binary that holds the module
    child: Child,
}

#[derive(Debug)]
pub(crate) struct PacketHandler {
    pub(crate) shmem: ShmemClient,
    pub(crate) pipe: Pipe,
}

// when the inner arc'd resolve instance is fully dropped then we can discard the module and bg tasks
impl Drop for InnerResolve {
    fn drop(&mut self) {
        self.cancel();
        // just try to kill the module in anyway possible:
        let _ = self.child.start_kill();
    }
}

impl PartialEq<Resolve> for Resolve {
    fn eq(&self, other: &Resolve) -> bool {
        self.id() == other.id()
    }
}
impl Eq for Resolve {}

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
        Self::new_with_config(&ResolveConfig::default()).await
    }

    /// Creates a new [`Resolve`] instance with the specified [`ResolveConfig`].
    ///
    /// # Errors
    /// - If it fails to create a temporary directory
    /// - The setup server communication fails
    /// - The module startup fails
    pub async fn new_with_config(config: &ResolveConfig) -> Result<Self, Error> {
        if !config.skip_cleanup {
            cleanup::check().await?;
        }

        #[cfg(feature = "tracing")]
        let creation_time = std::time::Instant::now();

        let id = fastrand::u32(..);

        #[cfg(feature = "tracing")]
        let span = tracing::trace_span!("new_resolve", id);
        #[cfg(feature = "tracing")]
        let _enter = span.enter();

        let instance_dir = instance_dir(id);
        create_dir(&instance_dir).await?;

        let cancelled = Arc::new(RwLock::new(false));

        let shmem = ShmemClient::new(shmem_path(&instance_dir))?;

        let (child, module_pipe, pipe) =
            start(&instance_dir, config, cancelled.clone(), id).await?;

        let packet_handler = Mutex::new(PacketHandler { shmem, pipe });

        #[cfg(feature = "tracing")]
        let creation_time = creation_time.elapsed();
        #[cfg(feature = "tracing")]
        tracing::trace!(?creation_time, "Created resolve client");

        Ok(Self {
            inner: Arc::new(InnerResolve {
                id,
                default_timeout: config.timeout,
                cancelled,
                packet_handler,
                _module_pipe: module_pipe,
                child,
            }),
        })
    }

    /// Returns a unique `id` to this specific [`Resolve`] instance, can be used to check if two instances are the same or not.
    #[inline]
    #[must_use]
    pub fn id(&self) -> u32 {
        self.inner.id
    }

    /// Returns the directory which this instance will place it's temporary files.\
    /// Including log files generated by the module when enabling `tracing` in [`ResolveConfig`]
    #[inline]
    #[must_use]
    pub fn dir(&self) -> PathBuf {
        instance_dir(self.id())
    }

    #[inline]
    pub(crate) fn cancelled(&self) -> bool {
        *self.inner.cancelled.read()
    }

    #[inline]
    pub(crate) fn timeout(&self) -> Duration {
        self.inner.default_timeout
    }

    #[inline]
    pub(crate) async fn packet_handler(&self) -> MutexGuard<'_, PacketHandler> {
        self.inner.packet_handler.lock().await
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
    /// ### Discard values
    /// Some values can't be serialized back to you *(`userdata` for example)*.\
    /// To use `userdata` objects you can use [`ItemRef`]'s and [`store`](Resolve::store) which only returns a reference to values.
    ///
    /// But sometimes you might never ever want to serialize a value and don't care for the returned value.\
    /// In this case you can specify [`Void`] as the return type and the value will be discarded in the module.\
    /// This skips serializing the value at all, great if you *just* want to execute something.
    ///
    /// ```rust ignore
    /// let resolve = Resolve::new().await?;
    /// let nothing = resolve.execute::<Void>("return resolve:GetProjectManager()").await?;
    /// assert_eq(Void, nothing);
    /// ```
    /// In this example, `GetProjectManager` returns a `userdata` which again, we can't serialize.
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent
    pub async fn execute<T>(&self, script: impl Into<Script<'_>>) -> Result<T, Error>
    where
        T: DeserializeOwned + 'static,
    {
        let mut script = script.into();
        script.discard = Void::is_void::<T>();
        match self.send_execute(&script).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::UnableToReachResolve => Err(Error::UnableToReachDavinciResolve),
            ScriptResponse::Ok {
                value,
                #[allow(unused_variables, reason = "used when 'tracing' is enabled")]
                eval_time,
            } => {
                log_script_resposne!(script, eval_time, "execute");
                Ok(value)
            }
        }
    }

    /// Store a reference to `Lua` value in `Rust`
    ///
    /// Instead of returning some value, you get an [`ItemRef`].
    /// This is just an **id** that resolves to the stored value when executing.
    ///
    /// You can store any value as an [`ItemRef`], a number, a function, or even an instance of a *timeline*!\
    /// Except for `nil`, in that scenario this returns [`Error::NilItemRef`].
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
    /// If the module executing the code fails, if the script can't be sent or if the returned value is `nil`
    pub async fn store(&self, script: impl Into<Script<'_>>) -> Result<ItemRef, Error> {
        self.store_option(script).await?.ok_or(Error::NilItemRef)
    }

    /// Maybe stores a reference to `Lua` value in `Rust`
    ///
    /// If the returned value is `nil`, this will return `None`.
    ///
    /// Look at [`store`](Resolve::store) for more info.
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent
    pub async fn store_option(
        &self,
        script: impl Into<Script<'_>>,
    ) -> Result<Option<ItemRef>, Error> {
        let script = script.into();
        match self.send_store(&script).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::UnableToReachResolve => Err(Error::UnableToReachDavinciResolve),
            ScriptResponse::Ok {
                value,
                #[allow(unused_variables, reason = "used when 'tracing' is enabled")]
                eval_time,
            } => {
                log_script_resposne!(script, eval_time, "store");
                Ok(value.map(|v| unsafe { ItemRef::new(self.clone(), v) }))
            }
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
        self.store_list_option(script)
            .await?
            .ok_or(Error::NilItemRef)
    }

    /// Maybe stores a reference to `Lua` values in `Rust`
    ///
    /// If the returned value is `nil`, this will return `None`
    ///
    /// Look at [`store`](Resolve::store_list) for more info.
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent.\
    /// Or if the returned value from lua was not a *table*
    pub async fn store_list_option(
        &self,
        script: impl Into<Script<'_>>,
    ) -> Result<Option<ItemRefList>, Error> {
        let script = script.into();
        match self.send_store_table(&script).await? {
            resolved_shared::ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::UnableToReachResolve => Err(Error::UnableToReachDavinciResolve),
            resolved_shared::ScriptResponse::Ok {
                value,
                #[allow(unused_variables, reason = "used when 'tracing' is enabled")]
                eval_time,
            } => {
                log_script_resposne!(script, eval_time, "store_list");
                Ok(value.map(|(source, list)| {
                    ItemRefList::new(
                        unsafe { ItemRef::new(self.clone(), source) },
                        list.into_iter()
                            .map(|x| unsafe { ItemRef::new(self.clone(), x) })
                            .collect(),
                    )
                }))
            }
        }
    }

    /// Get all values from a referenced table
    ///
    /// If you want all references from a table, see [`store_list`](Resolve::store_list).\
    /// Or if you want all values directly, see [`table_values`](Resolve::table_values)
    ///
    /// The value stored in the [`ItemRef`] must be of type `Table` in lua.
    ///
    /// ## Example
    ///
    /// The following table:
    /// ```lua
    /// return { a = 1, b = 2, c = 3 }
    /// ```
    /// would return `[1, 2, 3]` and `T` would be of type [`i32`] here. *(or any integer)*
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent,
    /// or if the referenced [`ItemRef`] is **not** a table
    pub async fn table_values<T>(&self, item: &ItemRef) -> Result<Vec<T>, Error>
    where
        T: DeserializeOwned,
    {
        if self.id() != item.resolve().id() {
            return Err(Error::MismatchedItemRef(self.id(), item.resolve().id()));
        }

        match self.send_table_values(item.id()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::UnableToReachResolve => Err(Error::UnableToReachDavinciResolve),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(value),
        }
    }

    /// Get all keys from a referenced table
    ///
    /// If you want all values from a table, see [`store_list`](Resolve::store_list).  
    ///
    /// The value stored in the [`ItemRef`] must be of type `Table` in lua.
    ///
    /// ## Example
    ///
    /// The following table:
    /// ```lua
    /// return { a = 1, b = 2, c = 3 }
    /// ```
    /// would return `["a", "b", "c"]` and `T` would be of type [`String`] here.
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent,
    /// or if the referenced [`ItemRef`] is **not** a table
    pub async fn table_keys<T>(&self, item: &ItemRef) -> Result<Vec<T>, Error>
    where
        T: DeserializeOwned,
    {
        if self.id() != item.resolve().id() {
            return Err(Error::MismatchedItemRef(self.id(), item.resolve().id()));
        }

        match self.send_table_keys(item.id()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::UnableToReachResolve => Err(Error::UnableToReachDavinciResolve),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(value),
        }
    }

    /// Returns the referenced value directly.
    pub(crate) async fn item_value<T>(&self, item: &ItemRef) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        match self.send_item_value(item.id()).await? {
            ScriptResponse::Err(e) => Err(Error::LuaModuleErr(e)),
            ScriptResponse::UnableToReachResolve => Err(Error::UnableToReachDavinciResolve),
            ScriptResponse::Ok {
                value,
                eval_time: _,
            } => Ok(value),
        }
    }

    /// Execute a [`Script`] with an [`ItemRef`] as `self`
    pub(crate) async fn execute_with<'c, T>(
        &self,
        item: &'c ItemRef,
        script: impl Into<Script<'c>>,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned + 'static,
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

    pub(crate) async fn store_option_with<'c>(
        &self,
        item: &'c ItemRef,
        script: impl Into<Script<'c>>,
    ) -> Result<Option<ItemRef>, Error> {
        let mut script = script.into();
        script = script.with(item)?;
        self.store_option(script).await
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

    pub(crate) async fn store_list_option_with<'c>(
        &self,
        item: &'c ItemRef,
        script: impl Into<Script<'c>>,
    ) -> Result<Option<ItemRefList>, Error> {
        let mut script = script.into();
        script = script.with(item)?;
        self.store_list_option(script).await
    }

    /// Shutdowns the connected module.  
    ///
    /// Any other calls to this [`Resolve`] client and it's references will always return an [`Error::ModuleNotRunning`].
    ///
    /// # Errors
    /// If the `shutdown` packet fails to send to the module
    ///
    /// # Safety
    /// All functions become null and void and does nothing other than return errors.
    pub async unsafe fn shutdown(&self) -> Result<(), Error> {
        self.send_shutdown().await?;
        self.inner.cancel();
        Ok(())
    }
}

impl InnerResolve {
    /// Writes to the internal state that it's cancelled
    pub(crate) fn cancel(&self) {
        *self.cancelled.write() = true;
    }
}

/// Makes an [`execute`](Resolve::execute) function discard it's return value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Void;
impl Void {
    /// Returns if the specified type `T` is [`Void`]
    #[inline]
    pub(crate) fn is_void<T: 'static>() -> bool {
        use std::any::TypeId;
        TypeId::of::<T>() == TypeId::of::<Self>()
    }
}

/// Writes the saved `.dll` and the generated `.lua` script to a temp directory.
/// The client then sends some configuration data and the module send back when
/// it's ready to accept connections and which port to send data to.
///
/// # Errors
/// If the lua module fails to reach *`DaVinci Resolve`* or for some other reason the lua module itself crashes before starting the http server.
async fn start(
    instance_dir: &Path,
    config: &ResolveConfig,
    cancelled: Arc<RwLock<bool>>,
    id: u32,
) -> Result<(Child, Pipe, Pipe), Error> {
    let module_pipe = new_module_pipe(id)?;
    let pipe = new_pipe(id)?;

    let dll = instance_dir.join(format!("{MODULE_NAME}.dll"));
    let raw_module = if config.tracing {
        LUA_MODULE_TRACING
    } else {
        LUA_MODULE
    };

    write(&dll, raw_module).await?;

    let script = dll_script(instance_dir, id);
    let script_path = instance_dir.join("script.lua");

    #[cfg(feature = "tracing")]
    tracing::trace!(script, "Startup script");

    write(&script_path, &script).await?;

    let child = spawn_script_server(&script_path, cancelled).await?;
    let mut module_pipe = module_pipe.accept().await?;

    #[cfg(feature = "tracing")]
    tracing::trace!("Module pipe connected");

    write_config(&mut module_pipe, config).await?;

    let pipe = handle_module_request(&mut module_pipe, pipe).await?;

    #[cfg(feature = "tracing")]
    tracing::trace!("Module started correctly, finished");

    Ok((child, module_pipe, pipe))
}
