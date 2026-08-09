#![doc = include_str!("../readme.md")]

#[cfg(not(windows))]
compile_error!(
    "'resolved' only works on windows due to dll's, paths and the way the library is structured with lua modules"
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
    async fn simple() -> Result<(), Error> {
        let resolve = Resolve::new().await?;

        let ver = resolve
            .execute::<String>("return self:GetVersionString()".to_string())
            .await?;

        assert!(!ver.is_empty());

        Ok(())
    }
}
