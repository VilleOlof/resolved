use serde::de::DeserializeOwned;

use crate::{Error, Resolve};

#[derive(Debug, Clone)]
pub struct ItemRef {
    pub(crate) id: u64,
    pub(crate) resolve: Option<Resolve>,
}

impl ItemRef {
    pub(crate) fn new(resolve: Resolve, id: u64) -> Self {
        Self {
            id,
            resolve: Some(resolve),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub async fn execute<T>(&self, lua_script: impl Into<String>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let resolve = self
            .resolve
            .as_ref()
            .expect("resolve must exist before drop");
        resolve.execute_with(self, lua_script).await
    }

    pub async fn store(&self, lua_script: impl Into<String>) -> Result<ItemRef, Error> {
        let resolve = self
            .resolve
            .as_ref()
            .expect("resolve must exist before drop");
        resolve.store_with(self, lua_script).await
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
