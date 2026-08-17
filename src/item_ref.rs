use serde::de::DeserializeOwned;
use std::sync::{Arc, RwLock};

use crate::{Error, ItemRefList, Resolve, Script};

/// A *reference* to a `Lua` value.  
///
/// When running `.execute` or `.store` on a [`ItemRef`].\
/// The global lua variable: `self` will be the referenced value this links to.  
///
/// Can also be used as an argument to a [`Script`] with [`arg_ref`](Script::arg_ref) or [`named_arg_ref`](Script::named_arg_ref).
#[derive(Debug, Clone)]
pub struct ItemRef {
    /// The inner referenced id is wrapped in an Arc to support cloning.\
    /// We only want to send a drop packet when the reference is totally gone from the client (aka 0 references here).
    pub(crate) value: Arc<LuaRef>,
}

/// The inner id and resolve instance for an [`ItemRef`].  
///
/// We only implement our custom send drop packet [`Drop`] on this Arc'd inner value
#[derive(Debug)]
pub(crate) struct LuaRef {
    /// The rolling `id` used in the lua module to retrieve the `RegistryKey` with the referenced value.
    pub(crate) id: u64,
    /// The [`Resolve`] instance which this [`ItemRef`] was taken from.
    pub(crate) resolve: Option<Resolve>,
    /// If the reference has been dropped in the module.\
    /// If this is `true`: calling any code with this will cause a [`Error::LuaModuleErr`]
    pub(crate) dropped: RwLock<bool>,
}

impl ItemRef {
    /// Creates a new [`ItemRef`] with it's lua module `id` and the [`Resolve`] instance it was retrieved from.
    ///
    /// # Safety  
    /// This is unsafe since if you were to mismatch the id and which [`Resolve`] instance it was gathered from then it would be undefined behavior.
    /// Only ever use this if you know for a fact that the `id` you pass it derives from the same [`Resolve`] instance and hasn't already been dropped in the module.
    #[inline]
    #[must_use]
    pub unsafe fn new(resolve: Resolve, id: u64) -> Self {
        Self {
            value: Arc::new(LuaRef {
                id,
                resolve: Some(resolve),
                dropped: RwLock::new(false),
            }),
        }
    }

    /// Returns the unique `id` for this reference
    #[inline]
    #[must_use]
    pub fn id(&self) -> u64 {
        self.value.id
    }

    /// Returns if the reference has already been marked as dropped.\
    /// The registry key in the module will likely already have been removed then.
    #[inline]
    #[must_use]
    pub(crate) fn is_dropped(&self) -> bool {
        *self
            .value
            .dropped
            .read()
            .expect("itemref.dropped was poisoned")
    }

    /// Returns the [`Resolve`] instance which this [`ItemRef`] was taken from.
    #[inline]
    #[must_use]
    pub(crate) fn resolve(&self) -> Resolve {
        self.value
            .resolve
            .as_ref()
            .expect("resolve was taken from itemref")
            .clone()
    }

    /// Execute some `lua` code, setting `self` to the stored reference value and returning what the code returned.
    ///
    /// Look at [`Resolve::execute`] for more info on how it works.  
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent
    pub async fn execute<'c, T>(&'c self, script: impl Into<Script<'c>>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.resolve().execute_with(self, script).await
    }

    /// Store a reference to `Lua` value in `Rust`,
    /// global variable `self` is set to the value stored in the [`ItemRef`].
    ///
    /// Look at [`Resolve::store`] for more info on how it works.  
    ///
    /// # Errors
    /// If the module executing the code fails, if the script can't be sent or if the returned value is `nil`
    pub async fn store<'c>(&'c self, script: impl Into<Script<'c>>) -> Result<ItemRef, Error> {
        self.resolve().store_with(self, script).await
    }

    /// Maybe stores a reference to `Lua` value in `Rust`,
    /// global variable `self` is set to the value stored in the [`ItemRef`].
    ///
    /// If the returned value is `nil`, this will return `None`.
    ///
    /// Look at [`store`](Resolve::store) for more info.
    ///
    /// Errors
    /// If the module executing the code fails or if the script can't be sent
    pub async fn store_option<'c>(
        &'c self,
        script: impl Into<Script<'c>>,
    ) -> Result<Option<ItemRef>, Error> {
        self.resolve().store_option_with(self, script).await
    }

    /// Store multiple references to `Lua` values in `Rust`,
    ///
    /// Look at [`Resolve::store_list`] for more info on how it works.  
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent.\
    /// Or if the returned value from lua was not a *table*
    pub async fn store_list<'c>(
        &'c self,
        script: impl Into<Script<'c>>,
    ) -> Result<ItemRefList, Error> {
        self.resolve().store_list_with(self, script).await
    }

    /// Returns the referenced value directly.
    ///
    /// Reference is still valid, this just clones the value.
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent
    pub async fn value<T>(&self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.resolve().item_value(self).await
    }

    /// Spawns a background task to drop the [`ItemRef`] in the module
    pub(crate) fn sync_manual_drop(resolve: Resolve, id: u64) {
        tokio::spawn(async move { unsafe { Self::manual_drop(resolve, id).await } });
    }

    /// Sends a packet to the lua module to drop the reference in the lua context.\
    /// This can be used to ensure a reference is dropped before doing anything else.\
    /// Aka, this is blocking if you `.await` it since the normal [`Drop`] can finish anytime it wants in the background.
    ///
    /// # Safety  
    /// You must ensure that the `id` came from correct [`Resolve`] instance and that the value hasn't already been dropped
    ///
    /// # Error
    /// This will silently fail and print its err to stderr if it fails.
    pub async unsafe fn manual_drop(resolve: Resolve, id: u64) {
        if let Err(e) = resolve.send_drop_item(id).await {
            eprintln!("failed to drop item ref: {e:?}");
        }
    }

    /// Creates a so called `phantom` [`ItemRef`].  
    ///
    /// This reference doesn't exist in the module, it's `id` will never* get reached,
    /// and it has already been "dropped" in the client.
    #[inline]
    #[must_use]
    pub(crate) fn phantom(resolve: Resolve) -> Self {
        ItemRef {
            value: Arc::new(LuaRef {
                id: u64::MAX,
                resolve: Some(resolve),
                dropped: RwLock::new(true),
            }),
        }
    }
}

impl Drop for LuaRef {
    fn drop(&mut self) {
        let dropped = { *self.dropped.read().expect("itemref.dropped was poisoned") };
        if !dropped {
            *self.dropped.write().expect("itemref.dropped was poisoned") = true;
            let resolve = std::mem::take(&mut self.resolve).expect("resolve must exist on drop");
            ItemRef::sync_manual_drop(resolve, self.id);
        }
    }
}

impl std::hash::Hash for ItemRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
        self.resolve().id().hash(state);
    }
}

impl Eq for ItemRef {}

impl PartialEq<ItemRef> for ItemRef {
    fn eq(&self, other: &ItemRef) -> bool {
        // we need to check both ids, resolve partialeq already goes direct to id
        self.id() == other.id() && self.resolve() == other.resolve()
    }
}
