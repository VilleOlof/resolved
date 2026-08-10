use std::sync::Arc;

use futures::future::join_all;
use serde::de::DeserializeOwned;
use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinError,
};

use crate::{Error, Resolve};

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
        let mut handles = Vec::with_capacity(amount);

        for _ in 0..amount {
            handles.push(tokio::task::spawn(async { Resolve::new().await }));
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

    pub async fn execute<T>(&self, lua_script: impl Into<String>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let permit = self.inner.permits.acquire().await?;

        let instance = {
            let mut inst = self.inner.instances.lock().await;
            inst.pop().ok_or(Error::OutOfSyncSemaphore)?
        };

        let value_result = instance.execute::<T>(lua_script).await;

        {
            self.inner.instances.lock().await.push(instance);
        }

        drop(permit);

        Ok(value_result?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pool() -> Result<(), Error> {
        let pool = PooledResolve::new(4).await?;
        let mut v = Vec::with_capacity(64);
        for _ in 0..64 {
            let p = pool.clone();
            v.push(tokio::task::spawn(async move {
                p.execute::<String>("return resolve:GetVersionString()")
                    .await
                    .unwrap()
            }));
        }
        let all: Vec<String> = join_all(v).await.into_iter().map(|x| x.unwrap()).collect();

        assert_eq!(64, all.len());

        Ok(())
    }
}
