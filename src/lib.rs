#[cfg(not(windows))]
compile_error!(
    "vinci only works on windows due to dll's and the way the library is structured with lua modules"
);

mod error;
mod pool;
mod resolve;
mod script;

pub use error::Error;
pub use pool::PooledResolve;
pub use resolve::Resolve;
pub use resolved_shared::ScriptResponse;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test() {
        let resolve = Resolve::new().await.unwrap();

        let t = std::time::Instant::now();
        let s = resolve
            .deserialize::<String>("return self:GetVersionString()".to_string())
            .await
            .unwrap();
        let t = t.elapsed();
        println!("[{:?}]: {s:?}", t);
    }
}
