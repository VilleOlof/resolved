use std::{sync::Arc, time::Duration};

use futures::future::join_all;
use serde::de::DeserializeOwned;
use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinError,
};

use crate::{Error, Resolve, Script, script::Arg};

/// A pool of [`Resolve`] instances,\
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

#[derive(Debug)]
struct InternalPool {
    instances: Mutex<Vec<Resolve>>,
    permits: Semaphore,
}

impl PooledResolve {
    pub async fn new(amount: usize) -> Result<Self, Error> {
        Self::new_with_timeout(amount, Resolve::DEFAULT_TIMEOUT).await
    }

    pub async fn new_with_timeout(amount: usize, timeout: Duration) -> Result<Self, Error> {
        let mut handles = Vec::with_capacity(amount);

        for _ in 0..amount {
            handles.push(tokio::spawn(async move {
                Resolve::new_with_timeout(timeout).await
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

    pub(crate) async fn while_lock<T: DeserializeOwned>(
        script: Script<'_>,
        instance: &Resolve,
    ) -> Result<T, Error> {
        for arg in &script.args {
            match arg {
                Arg::ArgRef(_) | Arg::NamedArgRef { key: _, value: _ } => {
                    return Err(Error::CantHoldReferenceInPool);
                }
                _ => continue,
            }
        }

        Ok(instance.execute::<T>(script).await?)
    }

    pub async fn execute<T>(&self, script: impl Into<Script<'_>>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let permit = self.inner.permits.acquire().await?;

        let instance = {
            let mut inst = self.inner.instances.lock().await;
            inst.pop().ok_or(Error::OutOfSyncSemaphore)?
        };

        let script = script.into();
        // we dont want to propogate this error until we have
        // returned out instance so its not gone forever in the pool
        let value_result = Self::while_lock(script, &instance).await;

        {
            self.inner.instances.lock().await.push(instance);
        }

        drop(permit);

        Ok(value_result?)
    }
}
