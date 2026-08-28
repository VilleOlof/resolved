use serde::de::DeserializeOwned;

use crate::{Error, ItemRef, ItemRefList, Resolve, Script};

/// All types that you can call `.execute` on and run some lua code which returns the value.
///
/// - [`Resolve`]
/// - [`ItemRef`]
/// - [`PooledResolve`](crate::PooledResolve)
pub trait ResolveExecute {
    fn execute<'c, T: DeserializeOwned + 'static + Send>(
        &'c self,
        script: impl Into<Script<'c>> + Send,
    ) -> impl Future<Output = Result<T, Error>> + Send;
}

/// All types that you can call `.store` on and store a reference which is returned
///
/// - [`Resolve`]
/// - [`ItemRef`]
///
/// Note that [`PooledResolve`](crate::PooledResolve) can't be used to store and use references in,
/// if you could, it would need to sync all references across all lua modules in every instance,
/// this is unfeasable and would require too much syncing and extra house keeping to keep track of right.
pub trait ResolveStore {
    fn store<'c>(
        &'c self,
        script: impl Into<Script<'c>> + Send,
    ) -> impl Future<Output = Result<ItemRef, Error>> + Send;

    fn store_option<'c>(
        &'c self,
        script: impl Into<Script<'c>> + Send,
    ) -> impl Future<Output = Result<Option<ItemRef>, Error>> + Send;

    fn store_list<'c>(
        &'c self,
        script: impl Into<Script<'c>> + Send,
    ) -> impl Future<Output = Result<ItemRefList, Error>> + Send;

    fn store_list_option<'c>(
        &'c self,
        script: impl Into<Script<'c>> + Send,
    ) -> impl Future<Output = Result<Option<ItemRefList>, Error>> + Send;
}

macro_rules! impl_execute {
    ($( $n:path ),*) => {$(
        impl ResolveExecute for $n {
            fn execute<'c, T: DeserializeOwned + 'static + Send>(
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

            fn store_option<'c>(
                &'c self,
                script: impl Into<Script<'c>> + Send
            ) -> impl Future<Output = Result<Option<ItemRef>, Error>> + Send {
                self.store_option(script)
            }

            fn store_list<'c>(
                &'c self,
                script: impl Into<Script<'c>> + Send
            ) -> impl Future<Output = Result<ItemRefList, Error>> + Send {
                self.store_list(script)
            }

            fn store_list_option<'c>(
                &'c self,
                script: impl Into<Script<'c>> + Send
            ) -> impl Future<Output = Result<Option<ItemRefList>, Error>> + Send {
                self.store_list_option(script)
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
