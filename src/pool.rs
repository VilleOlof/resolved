use std::sync::Arc;

use futures::future::join_all;
use serde::de::DeserializeOwned;
use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinError,
};

use crate::{Error, Resolve, ResolveConfig, Script, script::ArgData};

/// A pool of [`Resolve`] instances.  
///
/// Whenever you run [`PooledResolve::execute`], it will grab one of the available [`Resolve`] instances and use it to run the specified code.
///
/// This helps to run multiple scripts at the same time without blocking since a single [`Resolve`] instance can only handle on script at a time.
///
/// ## Example
/// ```ignore
/// // 2-6 is usually a sweet range for most shorter stuff.
/// // If you have more, long running scripts then more would be better,
/// // You'll have to experiment and test yourself
/// let pool = PooledResolve::new(4).await?;
/// let page = pool.execute::<String>("return self:GetCurrentPage()").await?;
///
/// ```
#[derive(Debug, Clone)]
pub struct PooledResolve {
    inner: Arc<InternalPool>,
}

/// Holds the internal data of the pool and all of the instances
#[derive(Debug)]
struct InternalPool {
    instances: Mutex<Vec<Resolve>>,
    permits: Semaphore,
}

impl PooledResolve {
    /// Creates a new [`PooledResolve`] with `amount` instances in total.
    ///
    /// # Errors
    /// If any of the instances fail to properly setup or the tasks fail to join
    pub async fn new(amount: usize) -> Result<Self, Error> {
        Self::new_with_config(amount, ResolveConfig::default()).await
    }

    /// Creates a new [`PooledResolve`] with `amount` instances in it, and a [`ResolveConfig`] that is passed to all instances.
    ///
    /// # Errors
    /// If any of the instances fail to properly setup or the tasks fail to join
    pub async fn new_with_config(amount: usize, config: ResolveConfig) -> Result<Self, Error> {
        let mut handles = Vec::with_capacity(amount);

        let conf = Arc::new(config);
        for _ in 0..amount {
            let config = conf.clone();
            handles.push(tokio::spawn(async move {
                Resolve::new_with_config(config.as_ref()).await
            }));
        }

        let instances: Result<Result<Vec<Resolve>, Error>, JoinError> =
            join_all(handles).await.into_iter().collect();
        let instances = instances??;

        let pool = InternalPool {
            instances: Mutex::new(instances),
            permits: Semaphore::new(amount),
        };

        Ok(Self {
            inner: Arc::new(pool),
        })
    }

    /// Logic that runs when an instance has been grabbed from the pool
    pub(crate) async fn on_lock<T: DeserializeOwned + 'static>(
        script: Script<'_>,
        instance: &Resolve,
    ) -> Result<T, Error> {
        instance.execute::<T>(script).await
    }

    /// Execute some `lua` code with any of the available instances, the returned value in the code will be returned here.
    ///
    /// Look at [`Resolve::execute`] for more info on how it works.  
    ///
    /// This `execute` will behave the same, but of course it's grabbing a [`Resolve`] instance from a pool of them.
    /// When no instances are available and all are already taken, it will wait until theres one available to run.
    ///
    /// # Errors
    /// If it fails to acquire a permit to an instance, or if instance [`execute`](Resolve::execute) fails
    pub async fn execute<T>(&self, script: impl Into<Script<'_>>) -> Result<T, Error>
    where
        T: DeserializeOwned + 'static,
    {
        // we can enforce no-ref in pools before we even acquire a permit since Script doesnt know its destination fn
        let script = script.into();
        for arg in &script.args {
            match arg {
                ArgData::ArgRef(_) | ArgData::NamedArgRef { key: _, value: _ } => {
                    return Err(Error::CantHoldReferenceInPool);
                }
                _ => (),
            }
        }

        let permit = self.inner.permits.acquire().await?;

        let instance = {
            let mut inst = self.inner.instances.lock().await;
            inst.pop().ok_or(Error::OutOfSyncSemaphore)?
        };

        // we dont want to propogate this error until we have
        // returned out instance so its not gone forever in the pool
        let value_result = Self::on_lock(script, &instance).await;

        {
            self.inner.instances.lock().await.push(instance);
        }

        drop(permit);

        value_result
    }
}
