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
    /// The rolling `id` used in the lua module to retrieve the RegistryKey with the referenced value.
    pub(crate) id: u64,
    /// The [`Resolve`] instance which this [`ItemRef`] was taken from.\
    /// The [`Option`] here is nothing to worry about and only to easier impl [`Drop`].\
    /// [`Resolve`] can always be used during the lifetime of the [`ItemRef`] with [`resolve`](ItemRef::resolve).
    pub(crate) resolve: Option<Resolve>,
}

impl PartialEq<ItemRef> for ItemRef {
    fn eq(&self, other: &ItemRef) -> bool {
        // we need to check both ids, resolve partialeq already goes direct to id
        self.id() == other.id() && self.resolve() == other.resolve()
    }
}

impl ItemRef {
    /// Creates a new [`ItemRef`] with it's lua module `id` and the [`Resolve`] instance it was retrieved from.
    ///
    /// # Safety  
    /// This is unsafe since if you were to mismatch the id and which [`Resolve`] instance it was gathered from then it would be undefined behavior.
    /// Only ever use this if you know for a fact that the `id` you pass it derives from the same [`Resolve`] instance and hasn't already been dropped in the module.
    #[inline]
    pub unsafe fn new(resolve: Resolve, id: u64) -> Self {
        Self {
            id,
            resolve: Some(resolve),
        }
    }

    /// Returns the unique `id` for this reference
    #[inline]
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

    pub async fn execute<T>(&self, script: impl Into<Script<'_>>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.resolve().execute_with(self, script).await
    }

    pub async fn store(&self, script: impl Into<Script<'_>>) -> Result<ItemRef, Error> {
        self.resolve().store_with(self, script).await
    }

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
        let resolve =
            std::mem::replace(&mut self.resolve, None).expect("resolve must exist on drop");
        let id = self.id;
        return ItemRef::sync_manual_drop(resolve, id);
    }
}
