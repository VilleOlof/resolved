#![doc = include_str!("../readme.md")]

#[cfg(not(windows))]
compile_error!(
    "'resolved' only works on windows due to dll's, paths and the way the library is structured with lua modules. see #why-windows-only in the readme"
);

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
// - [-] remove ResolveConfig timeout, since its on the script object and isnt used
//      maybe look if we could possibly add some more configs?
// - [] sanity check with friends if this seems good and get someone to try using it
// - [X] move interprocess dep to workspace and only add tokio feature on crate & not in module
// - [X] some mention of the unsafe code and which 3 files it exists in (shared/mem, lua_module/reader, src/put)
// - [] tests for resolve_shared
// - [] tests for lua_module
// - [] tests for shmem
// - [X] add a 4-8 byte random request id which must be validated for the request to be valid
// - [X] set globals with resolveconfig, make their configs into functions since heap now with hashmap
// - [X] optional tracing+sub for module, compile one with tracing and one without
//      this would add 566kb extra into the final binary
//      Option<bool> for if tracing in config
//      if None then if tracing feature it would be true which would load the tracing dll
//          if not tracing feature and none then load normal dll
//      if Some(val) then use val for if it should be enabled
// - [X] move log file to some more permanant directory?
//      like the temp dir will get removed on drop so hard to read if enabled
//      something like: "AppData/*/resolved/<t<unix_time>_i<id>>.log"
//      update docs on where file is located
//      !! since cleanup is now every x files and fresh ones wont get deleted this is fine
// - [X] timeout in resolveconfig should be default and script should have Option<Duration>
// - [X] dir() fn on resolve to return tempdir stored
// - [] pedantic
// - [X] temp_dir cant remove vinci.dll since its still in-use since module is still running when client drops
//      this leaves files which arent removed
//      could have outer start function try and delete the folder since we got it in config

//      drop TempFile as a dep and place new random directories in a permanant directory which we know
//      then every time this library is init (global async task is spawned?)
//      or everytime you start an instance, if that dir has more than like 100 foldrs
//      it goes and spawns a BG task that tries to remove everything that it can that is older 1 minute
// - [X] add a .lock file to resolved_root which is held during cleanup
//      so other instances in maybe other tasks/programs dont act while someone else is cleaning
//      this check on the lock should be before they even count files at all
// - [] think if we can add more options just in case

// add a 4-8 random byte seq as id into the shared mem layout, module stores a copy during request and at response (on one of the parts) of the id in shmem doesn’t match the stored they are diff requests and data is mismatched, so error
// this can happen if the client sends a request, times out and begins a new request before the first one can respond, since the ownership will still be the module it will write back the old response but never read in the new request and just overwrite it
