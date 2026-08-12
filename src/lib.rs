#![doc = include_str!("../readme.md")]

#[cfg(not(windows))]
compile_error!(
    "'resolved' only works on windows due to dll's, paths and the way the library is structured with lua modules"
);

mod error;
mod item_ref;
mod owned_script;
mod packet;
mod pool;
mod resolve;
mod script;
mod script_handler;
mod traits;

pub use error::Error;
pub use item_ref::ItemRef;
pub use owned_script::OwnedScript;
pub use pool::PooledResolve;
pub use resolve::Resolve;
pub use script::Script;
pub use traits::{ResolveExecute, ResolveStore};

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
            .execute::<String>("return resolve:GetVersionString()")
            .await?;

        println!("{:?}", t.elapsed());
        assert!(!ver.is_empty());
        println!("{ver:?}");

        Ok(())
    }
}

// TODO:
// - [] docs on all execute and store functions and on remaining public items
//      mention global sleep fn in lua
// - [X] create a `dummy` crate which is a dummy `fuscript.exe` binary which replicates its behavior
//      so we can set FUSCRIPT in testing so we dont have to rely on davinci resolve during testing of client+module
//      should require a `test-util` or something feature flag which changes .execute to require `Default`
//      bound on their return value, so all execute will always return the default value
//      as the dummy wont know what functions exist or what values should be returned
//      we just want to use this dummy binary to test the networking, packets, script, serializing and references etc.
// - [/] rerun benchmarks, redo them a bit to make more sense and easier to display those numbers
//      and show some of those numbers in readme, like time to start a resolve instance
//      and the average time to execute a script (without starting the instance)
// - [] tidy up cargo.toml's
// - [] performance checking
//      like the port/timeout ms tostring / from string(in dummy) could maybe use those libs that is used in snbt?
//      like fast type conversion
// - [] mlua chunk! macro but output is Script (and maybe special syntax for normal capture vs ItemRef?)
//          rust variable becomes name and all is named_arg with the value
// - [] fix architecture.md
// - [] make create ready for release
//          root `resolved` and `resolved_shared` needs to be published as we cant use paths in dependencies i think?
//          `build_lib` and `lua_module` are prebuilt and done before publishing
//          make sure prebuilts are included in built package etc
