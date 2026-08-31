use std::time::Duration;

use serde::Serialize;

pub use crate::cleanup::CleanupConfig;

/// Configuration for [`Resolve`](crate::Resolve) instances
///
/// Can be used to increase the internal ping timeout and or if it should reset globals after every execution.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveConfig {
    /// The default timeout for `Script`s if they don't specify their own timeout
    pub timeout: Duration,
    /// If the module should reset the lua globals between every request.
    /// For short, small requests, this can increase performance by a good bit.
    /// You just need to make sure you use `local` variables in lua and don't clutter the global table to mess with different scripts
    pub reset_globals: bool,
    /// Global variables that will always exist.  
    ///
    /// These globals won't care for `reset_globals` as that is only for script executed globals.
    pub globals: Globals,
    /// Enables trace logging in the lua module, logs will be written to a file in the instance's temporary directory ([`dir`](crate::Resolve::dir))
    pub tracing: bool,
    /// Every so often when you create a [`Resolve`](crate::Resolve) instance, it will run a background cleanup job to remove stale files created by the crate.  
    ///
    /// This configures when and if that cleanup runs.
    pub cleanup: CleanupConfig,
    /// Specifies a root Scripting API function to check if it exists during each execution request.\
    /// If this specifed function does *not* exist, the module will assume that `DaVinci Resolve` is unreachable.\
    /// This makes the functions return a proper [`Error::UnableToReachDavinciResolve`](crate::Error::UnableToReachDavinciResolve) error if so.
    ///
    /// This is not enabled by default as it's a bit slow for simple calls, but a good default to use is: [`DEFAULT_RESOLVE_AVAILABLE_FUNCTION`](ResolveConfig::DEFAULT_RESOLVE_AVAILABLE_FUNCTION).
    ///
    /// Roughly, this adds one extra internal call to verify
    pub is_resolve_available: Option<String>,
    /// The timeout time for when the lua module is initializing itself.
    pub module_init_timeout: Duration,
}

impl ResolveConfig {
    /// A function that i'm sure is to exist in all and previous versions of the Scripting API.
    pub const DEFAULT_RESOLVE_AVAILABLE_FUNCTION: &str = "Quit";

    /// Default configuration except that globals don't get reset
    #[inline]
    #[must_use]
    pub fn keep_globals() -> Self {
        Self {
            reset_globals: false,
            ..Default::default()
        }
    }
}

// builders >
impl ResolveConfig {
    /// Set's `timeout` to the specified value.
    #[inline]
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    /// Set's `reset_globals` to the specified value.
    #[inline]
    #[must_use]
    pub fn reset_globals(mut self, reset_globals: bool) -> Self {
        self.reset_globals = reset_globals;
        self
    }
    /// Set's `globals` to the specified value.
    #[inline]
    #[must_use]
    pub fn globals(mut self, globals: Globals) -> Self {
        self.globals = globals;
        self
    }
    /// Set's `tracing` to the specified value.
    #[inline]
    #[must_use]
    pub fn tracing(mut self, tracing: bool) -> Self {
        self.tracing = tracing;
        self
    }
    /// Set's `cleanup` to the specified value.
    #[inline]
    #[must_use]
    pub fn cleanup(mut self, cleanup: CleanupConfig) -> Self {
        self.cleanup = cleanup;
        self
    }
    /// Set's `is_resolve_available` to the specified value.
    #[inline]
    #[must_use]
    pub fn is_resolve_available(mut self, is_resolve_available: Option<String>) -> Self {
        self.is_resolve_available = is_resolve_available;
        self
    }
}

impl Default for ResolveConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            reset_globals: true,
            globals: Globals::default(),
            tracing: false,
            cleanup: CleanupConfig::default(),
            is_resolve_available: None,
            module_init_timeout: Duration::from_secs(25),
        }
    }
}

/// Used to add global variables to [`ResolveConfig`].  
///
/// These globals won't ever get reset no matter what and always exist when executing scripts
#[derive(Clone, PartialEq)]
pub struct Globals {
    pub(crate) list: Vec<(String, Vec<u8>)>,
}

impl Globals {
    /// A new [`Globals`] with no globals specified
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self { list: Vec::new() }
    }

    /// A new [`Globals`] with `n` capacity for globals
    #[inline]
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            list: Vec::with_capacity(n),
        }
    }

    /// How many globals have been added already
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Adds a new global `value` with it's name being `key`
    ///
    /// # Errors
    /// If the serializing of the value fails
    pub fn add<T: Serialize, S: Into<String>>(
        &mut self,
        key: S,
        value: &T,
    ) -> Result<(), rmp_serde::encode::Error> {
        self.list.push((key.into(), rmp_serde::to_vec(value)?));
        Ok(())
    }
}

impl std::fmt::Debug for Globals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Globals")
            .field(
                "list",
                &format!(
                    "{:?}",
                    self.list.iter().map(|s| &s.0).collect::<Vec<&String>>()
                ),
            )
            .finish()
    }
}

impl Default for Globals {
    fn default() -> Self {
        Self::new()
    }
}
