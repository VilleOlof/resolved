#![doc = include_str!("../README.md")]

#[cfg(not(windows))]
compile_error!("resolved only supports windows. see #why-windows-only in readme");

mod cleanup;
mod config;
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

pub use config::{Globals, ResolveConfig};
pub use error::Error;
pub use item_ref::ItemRef;
pub use item_ref_list::{ItemRefList, RefList};
pub use owned_script::OwnedScript;
pub use resolve::Resolve;
pub use script::{Script, ToLuaRef};
pub use traits::{ResolveExecute, ResolveStore};

/// Common types to fully utilize the crate
pub mod prelude {
    pub use super::{Error, Globals, ItemRef, Resolve, ResolveConfig, Script};
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
/// let version: String = resolve.execute(script! { return self:GetVersionString() }?).await?;
///
/// // Reference rust variables with '$'
/// let (a, b) = (52, 91);
/// let result: i32 = resolve.execute(script! { return $a * $b }?).await?;
/// assert_eq!(4732, result);
///
/// // Reference other lua values (ItemRef) with '@'
/// let page = resolve.store(script! { return self:GetCurrentPage() }?).await?;
/// resolve.execute(script! { self:OpenPage(@page) }?).await?;
/// ```
///
/// # Errors
///
/// This macro always returns a `Result<Script<'c>, Error>` which must be handled.\
/// The macro can fail if the provided value for an argument fails to serialize.
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
