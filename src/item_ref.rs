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

impl ItemRef {
    /// Creates a new [`ItemRef`] with it's lua module `id` and the [`Resolve`] instance it was retrieved from.
    #[inline]
    pub(crate) fn new(resolve: Resolve, id: u64) -> Self {
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
}

impl Drop for ItemRef {
    fn drop(&mut self) {
        let resolve =
            std::mem::replace(&mut self.resolve, None).expect("resolve must exist on drop");
        let id = self.id;
        tokio::spawn(async move {
            if let Err(e) = resolve.send_drop_item(id).await {
                eprintln!("failed to drop item ref: {e:?}");
            }
        });
    }
}
