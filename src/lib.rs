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
mod put;
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
/// Notably, [`ItemRef`] doesn't implement `Serialize`, but use `@` instead of `$` to reference it.
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
/// // Reference other lua values (ItemRef) with '@'
/// let page = resolve.store(script! { return self:GetCurrentPage() }).await?;
/// resolve.execute(script! { self:OpenPage(@page) }).await?;
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

// TODO:
// - [X*] stablize shmem & pipe packet handler
//      improve areas around it to better fit their new API
// - [X] rewrite bit of architecture
// - [/] look at all docs
// - [] remove ResolveConfig timeout, since its on the script object and isnt used
//      maybe look if we could possibly add some more configs?
// - [] sanity check with friends if this seems good and get someone to try using it
// - [] move interprocess dep to workspace and only add tokio feature on crate & not in module
// - [] some mention of the unsafe code and which 3 files it exists in (shared/mem, lua_module/reader, src/put)
