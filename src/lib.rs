#![doc = include_str!("../readme.md")]

#[cfg(not(windows))]
compile_error!(
    "'resolved' only works on windows due to dll's, paths and the way the library is structured with lua modules"
);

mod error;
mod item_ref;
mod packet;
mod pool;
mod resolve;
mod script;

pub use error::Error;
pub use item_ref::ItemRef;
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
            .execute::<String>("return resolve:GetVersionString()".to_string())
            .await?;

        assert!(!ver.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn item_ref() -> Result<(), Error> {
        let resolve = Resolve::new().await?;

        let project_manager = resolve.store("return self:GetProjectManager()").await?;
        let project = project_manager
            .store("return self:GetCurrentProject()")
            .await?;
        let timeline = project.store("return self:GetCurrentTimeline()").await?;

        let p_name = project
            .execute::<String>(r#"return self:GetName()"#)
            .await?;
        let t_name = timeline
            .execute::<String>(r#"return self:GetName()"#)
            .await?;
        println!("{p_name:?}:{t_name:?}");

        Ok(())
    }
}
