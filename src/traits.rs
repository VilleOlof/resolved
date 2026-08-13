use serde::de::DeserializeOwned;

use crate::{Error, ItemRef, Resolve, Script};

mod __seal__ {
    pub trait Sealed {}
}

impl __seal__::Sealed for Resolve {}
impl __seal__::Sealed for ItemRef {}
#[cfg(feature = "pool")]
impl __seal__::Sealed for crate::PooledResolve {}

/// All types that you can call `.execute` on and run some lua code which returns the value.
///
/// - [`Resolve`]
/// - [`PooledResolve`]
/// - [`ItemRef`]
pub trait ResolveExecute: __seal__::Sealed {
    fn execute<'c, T: DeserializeOwned + Send>(
        &'c self,
        script: impl Into<Script<'c>> + Send,
    ) -> impl Future<Output = Result<T, Error>> + Send;
}

/// All types that you can call `.store` on and store a reference which is returned
///
/// - [`Resolve`]
/// - [`ItemRef`]
///
/// Note that [`PooledResolve`] can't be used to store and use references in,
/// if you could, it would need to sync all references across all lua modules in every instance,
/// this is unfeasable and would require too much syncing and extra house keeping to keep track of right.
pub trait ResolveStore: __seal__::Sealed {
    fn store<'c>(
        &'c self,
        script: impl Into<Script<'c>> + Send,
    ) -> impl Future<Output = Result<ItemRef, Error>> + Send;
}

macro_rules! impl_execute {
    ($( $n:path ),*) => {$(
        impl ResolveExecute for $n {
            fn execute<'c, T: DeserializeOwned + Send>(
                &'c self,
                script: impl Into<Script<'c>> + Send,
            ) -> impl Future<Output = Result<T, Error>> + Send {
                self.execute(script)
            }
        }
    )*};
}
macro_rules! impl_store {
    ($( $n:path ),*) => {$(
        impl ResolveStore for $n {
            fn store<'c>(
                &'c self,
                script: impl Into<Script<'c>> + Send
            ) -> impl Future<Output = Result<ItemRef, Error>> + Send {
                self.store(script)
            }
        }
    )*};
}

impl_execute!(Resolve, ItemRef);
#[cfg(feature = "pool")]
impl_execute!(crate::PooledResolve);
// PooledResolve doesn't impl store since it would be unfeasable to get references
// across all instances in a pool and sync their registries.
impl_store!(Resolve, ItemRef);
