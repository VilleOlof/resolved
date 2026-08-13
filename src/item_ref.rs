use serde::de::DeserializeOwned;

use crate::{Error, Resolve, Script};

/// A *reference* to a `Lua` value.  
///
/// When running `.execute` or `.store` on a [`ItemRef`].\
/// The global lua variable: `self` will be the referenced value this links to.  
///
/// Can also be used as an argument to a [`Script`] with [`arg_ref`](Script::arg_ref) or [`named_arg_ref`](Script::named_arg_ref).
#[derive(Debug, Clone)]
pub struct ItemRef {
    /// The rolling `id` used in the lua module to retrieve the `RegistryKey` with the referenced value.
    pub(crate) id: u64,
    /// The [`Resolve`] instance which this [`ItemRef`] was taken from.\
    /// The [`Option`] here is nothing to worry about and only to easier impl [`Drop`].\
    /// [`Resolve`] can always be used during the lifetime of the [`ItemRef`] with [`resolve`](ItemRef::resolve).
    pub(crate) resolve: Option<Resolve>,
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
            id,
            resolve: Some(resolve),
        }
    }

    /// Returns the unique `id` for this reference
    #[inline]
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Returns the [`Resolve`] instance which this [`ItemRef`] was taken from.
    #[inline]
    pub(crate) fn resolve(&self) -> &Resolve {
        self.resolve
            .as_ref()
            .expect("resolve must exist before drop")
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

    /// Store references to `Lua` values in `Rust`,
    /// global variable `self` is set to the value stored in the [`ItemRef`].
    ///
    /// Look at [`Resolve::store`] for more info on how it works.  
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent
    pub async fn store<'c>(&'c self, script: impl Into<Script<'c>>) -> Result<ItemRef, Error> {
        self.resolve().store_with(self, script).await
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
}

impl Drop for ItemRef {
    fn drop(&mut self) {
        let resolve = std::mem::take(&mut self.resolve).expect("resolve must exist on drop");
        ItemRef::sync_manual_drop(resolve, self.id);
    }
}

impl std::hash::Hash for ItemRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
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
