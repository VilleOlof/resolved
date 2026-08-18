use std::time::Duration;

use serde::Serialize;

/// Configuration for `Resolve` instances
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
    /// This skips this cleanup and never runs it when creating this instance
    pub skip_cleanup: bool,
}

impl ResolveConfig {
    /// Default configuration for all instances
    pub fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            reset_globals: true,
            globals: Globals::default(),
            tracing: false,
            skip_cleanup: false,
        }
    }

    /// Default configuration except that globals don't get reset
    pub fn keep_globals() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            reset_globals: false,
            globals: Globals::default(),
            tracing: false,
            skip_cleanup: false,
        }
    }
}

// builders >
impl ResolveConfig {
    /// Set's `timeout` to the specified value.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    /// Set's `reset_globals` to the specified value.
    pub fn reset_globals(mut self, reset_globals: bool) -> Self {
        self.reset_globals = reset_globals;
        self
    }
    /// Set's `globals` to the specified value.
    pub fn globals(mut self, globals: Globals) -> Self {
        self.globals = globals;
        self
    }
    /// Set's `tracing` to the specified value.
    pub fn tracing(mut self, tracing: bool) -> Self {
        self.tracing = tracing;
        self
    }
    /// Set's `skip_cleanup` to the specified value.
    pub fn skip_cleanup(mut self, skip_cleanup: bool) -> Self {
        self.skip_cleanup = skip_cleanup;
        self
    }
}

impl Default for ResolveConfig {
    fn default() -> Self {
        Self::default()
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
    pub fn new() -> Self {
        Self { list: Vec::new() }
    }

    /// A new [`Globals`] with `n` capacity for globals
    pub fn with_capacity(n: usize) -> Self {
        Self {
            list: Vec::with_capacity(n),
        }
    }

    /// How many globals have been added already
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Adds a new global `value` with it's name being `key`
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
