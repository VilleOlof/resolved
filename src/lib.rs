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
mod script_handler;

pub use error::Error;
pub use item_ref::ItemRef;
pub use pool::PooledResolve;
pub use resolve::Resolve;
pub use resolved_shared::ScriptResponse;
pub use script::Script;

pub mod prelude {
    pub use super::{Error, ItemRef, PooledResolve, Resolve, Script};
    pub type ResolveResult<T> = std::result::Result<T, Error>;
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn simple() -> Result<(), Error> {
        let resolve = Resolve::new().await?;

        let t = std::time::Instant::now();
        let ver = resolve
            .execute::<String>("return resolve:GetVersionString()".to_string())
            .await?;

        println!("{:?}", t.elapsed());
        assert!(!ver.is_empty());
        println!("{ver:?}");

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

#[tokio::test]
async fn arg_test() -> Result<(), Error> {
    let resolve = Resolve::new().await?;

    let script = Script::new("return arg[1] + idx")
        .arg(5)?
        .named_arg("idx", 9)?;

    println!("{:?}", script.clone().serialize());

    let t = std::time::Instant::now();
    let result = resolve.execute::<i32>(script).await?;
    println!("{:?}: {result:?}", t.elapsed());

    println!(
        "{:?}",
        resolve
            .execute::<i32>(
                Script::new("return a + b")
                    .named_arg("a", 85)?
                    .named_arg("b", 1)?,
            )
            .await?
    );

    Ok(())
}

#[tokio::test]
async fn arg_with_test() -> Result<(), Error> {
    let resolve = Resolve::new().await?;

    let pm = resolve.store("return self:GetProjectManager()").await?;
    let project = pm.store("return self:GetCurrentProject()").await?;
    let timeline = project.store("return self:GetCurrentTimeline()").await?;

    tokio::time::sleep(std::time::Duration::from_secs(4)).await;

    let success: bool = project
        .execute(
            Script::new("return self:SetCurrentTimeline(timeline)")
                .named_arg_ref("timeline", &timeline)?,
        )
        .await?;

    println!("{success}");

    Ok(())
}

// TODO:
// - [] docs on all execute and store functions and on remaining public items
// - [] create a `dummy` crate which is a dummy `fuscript.exe` binary which replicates its behavior
//      so we can set FUSCRIPT in testing so we dont have to rely on davinci resolve during testing of client+module
//      should require a `test-util` or something feature flag which changes .execute to require `Default`
//      bound on their return value, so all execute will always return the default value
//      as the dummy wont know what functions exist or what values should be returned
//      we just want to use this dummy binary to test the networking, packets, script, serializing and references etc.
// - [] rerun benchmarks, redo them a bit to make more sense and easier to display those numbers
//      and show some of those numbers in readme, like time to start a resolve instance
//      and the average time to execute a script (without starting the instance)
// - [] tidy up cargo.toml's
// - [] fix architecture.md
// - [] make create ready for release
//          root `resolved` and `resolved_shared` needs to be published as we cant use paths in dependencies i think?
//          `build_lib` and `lua_module` are prebuilt and done before publishing
//          make sure prebuilts are included in built package etc
