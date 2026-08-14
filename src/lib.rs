#![doc = include_str!("../readme.md")]

#[cfg(not(windows))]
compile_error!(
    "'resolved' only works on windows due to dll's, paths and the way the library is structured with lua modules. see #why-windows-only in the readme"
);

mod error;
mod item_ref;
mod item_ref_list;
mod owned_script;
mod packet;
mod resolve;
mod script;
mod script_handler;
mod traits;

#[cfg(feature = "pool")]
mod pool;
#[cfg(feature = "pool")]
pub use pool::PooledResolve;

pub use error::Error;
pub use item_ref::ItemRef;
pub use item_ref_list::{ItemRefList, RefList};
pub use owned_script::OwnedScript;
pub use resolve::Resolve;
pub use resolved_shared::ResolveConfig;
pub use script::Script;
pub use traits::{ResolveExecute, ResolveStore};

/// Common types to fully utilize the crate
pub mod prelude {
    pub use super::{Error, ItemRef, Resolve, Script};
    pub type ResolveResult<T> = std::result::Result<T, Error>;

    #[cfg(feature = "macros")]
    pub use resolved_macros::script;

    #[cfg(feature = "pool")]
    pub use super::PooledResolve;
}

#[cfg(feature = "macros")]
/// Write `Lua` code in `Rust` that can reference *Rust* variables directly.  
///
/// Rust values must implement [`Serialize`](https://docs.rs/serde/latest/serde/trait.Serialize.html).\
/// Notably, [`ItemRef`] doesn't implement `Serialize`, but use `#` instead of `$` to reference it.
///
/// ## Example
///
/// ```ignore
/// let resolve = Resolve::new().await?;
///
/// // Write lua directly
/// let version: String = resolve.execute(script! { return self:GetVersionString() }).await?;
///
/// // Reference rust variables with '$'
/// let (a, b) = (52, 91);
/// let result: i32 = resolve.execute(script! { return $a * $b }).await?;
/// assert_eq!(4732, result);
///
/// // Reference other lua values (ItemRef) with '#'
/// let page = resolve.store(script! { return self:GetCurrentPage() }).await?;
/// resolve.execute(script! { self:OpenPage(#page) }).await?;
/// ```
///
/// ## Syntax Issues
///
/// Since the Rust tokenizer will tokenize Lua code, this imposes some restrictions.
/// The main thing to remember is:
///
/// - Use double quoted strings (`""`) instead of single quoted strings (`''`).
///
///   (Single quoted strings only work if they contain a single character, since in Rust,
///   `'a'` is a character literal).
///
/// Other minor limitations:
///
/// - Certain escape codes in string literals don't work. (Specifically: `\a`, `\b`, `\f`, `\v`,
///   `\123` (octal escape codes), `\u`, and `\U`).
///
///   These are accepted: : `\\`, `\n`, `\t`, `\r`, `\xAB` (hex escape codes), and `\0`.
///
/// - The `//` (floor division) operator is unusable, as its start a comment.
///
/// Everything else should work.
// https://github.com/mlua-rs/mlua/blob/main/docs/chunk.md ^
pub use resolved_macros::script;

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
// - [/] rerun benchmarks, redo them a bit to make more sense and easier to display those numbers
//      and show some of those numbers in readme, like time to start a resolve instance
//      and the average time to execute a script (without starting the instance), and fuck async benchmarking, pools work just fine in tests
// - [X] tidy up cargo.toml's
//      - [] need to fix shared & macros subcrates cargo.tomls
// - [X] make create ready for release
//          root `resolved` and `resolved_shared` needs to be published as we cant use paths in dependencies i think?
//          `build_lib` and `lua_module` are prebuilt and done before publishing
//          make sure prebuilts are included in built package etc
// - [/] use the crate from a new binary crate externally and try and use it (rebuild clipboard crate?)
// - [] macro tests
// - [/] Some solution for references which is a sequence in lua
//      we would want to be able to iterate over each reference in rust
//      like get all timeline clips
//      then iterate over them in rust with each iter having that specific item ref with it
//      if we have some special seq fn, the module should validate that it is indeed a table of some sort
// - [/] Fix ItemRef Clone and dropping
//      if you clone an itemref and just drop one of them, all of its clones are also invalid
// - [] ItemRef and nil? what if .store fails to return a ref on purpose in script?
// - [] tracing feature, really useful to have trace logs in internal for debugging
// - [] tests on ItemRef cloning and dropping
//      if we got 2 cloned ItemRefs and drop one, wait, then the other one should still have a valid id to a value
// - [] shit ton of tests on ItemRefList
// - [] reference ItemRefList in someway in readme
