use std::{borrow::Cow, time::Duration};

use resolved_shared::ArgType;
use serde::Serialize;

use crate::{Error, ItemRef, ItemRefList, owned_script::OwnedScript};

/// A piece of lua code with optional arguments.
///
/// A [`Script`] is to be sent to [`Resolve`](crate::Resolve) with it's [`execute`](crate::Resolve::execute) and [`store`](crate::Resolve::store) functions.\
/// For most scenarios, you can just use a [`str`]/[`String`] for the input argument to these methods.  
///
/// But if you now wanted to execute some code with some variables or arguments from *Rust*,\
/// you'll need to use the [`Script::new`] and it's builder functions *(or the [`script!`](resolved_macros::script) macro!)*.  
///
/// There are four different types of arguments:  
/// - **`Arg`**\
///   Values added with [`arg`](Script::arg) will be pushed to the global `arg` table in the lua enviroment.  
/// - **`ArgRef`**\
///   With the normal `Arg`, you can't specify an [`ItemRef`] and thus some previous stored variable data.\
///   But with [`arg_ref`](Script::arg_ref), you can specify an [`ItemRef`] to push to the global `arg` table.\
///   Note that this [`ItemRef`] must derive from the same [`Resolve`](crate::Resolve) instance.
/// - **`NamedArg`**\
///   Instead of pushing values to `arg`, this will simply put the value in specified global variable.\
///   Using [`named_arg`](Script::named_arg) with a `key` and `value` argument will satisfy this.\
///   Note that you can't name your variable `self`, as that is either assigned to `Resolve()` or the executed [`ItemRef`].
/// - **`NamedArgRef`**\
///   Behaves the same as `NamedArg` but with an [`ItemRef`] as the value instead, use [`named_arg_ref`](Script::named_arg_ref) for this.
///
/// ## Examples
///
/// ### Simple
/// ```ignore
/// let resolve = Resolve::new().await?;
///
/// resolve.execute::<()>("self:Quit()").await?;
/// ```
///
/// ### Arguments
/// ```ignore
/// let resolve = Resolve::new().await?;
///
/// let script = Script::new("return a + b")
///     .named_arg("a", 10)?
///     .named_arg("b", 10)?;
///
/// let value: i32 = resolve.execute(script).await?;
/// assert_eq!(20, value);
/// ```
///
/// For an owned variant with no references, see: [`OwnedScript`] and [`as_owned`](Script::as_owned)
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Script<'c> {
    /// The code to be loaded and executed in the lua module
    pub(crate) lua: Cow<'c, str>,
    /// List of all arguments to be sent with the code, can be any of the 4 different ones.  
    pub(crate) args: Vec<ArgData<'c>>,
    /// An optional [`ItemRef`] which `self` will be set to if specified.\
    /// Can only be set by [`execute_with`](crate::Resolve::execute_with)/[`store_with`](crate::Resolve::store_with) functions on [`Resolve`](crate::Resolve)
    pub(crate) with: Option<&'c ItemRef>,
    /// Timeout for the scripts execution time.
    /// The time for the module to send back a response
    ///
    /// If not specified it will use the default configured timeout in the [`Resolve`](crate::Resolve) instance
    pub(crate) timeout: Option<Duration>,
    /// Discards the returned value, returning `()`
    pub(crate) discard: bool,
}

/// The different types of argument.  
///
/// `Arg` / `ArgRef` are pushed to the global `arg` variable.\
/// `NamedArg` / `NamedArgRef` are added as global variables
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ArgData<'s> {
    Arg(Cow<'s, [u8]>),
    ArgRef(ItemRef),
    NamedArg { key: &'s str, value: Cow<'s, [u8]> },
    NamedArgRef { key: &'s str, value: ItemRef },
}

impl ArgData<'_> {
    /// Returns the general type of an argument, used to deserialize the data in the module
    pub const fn arg_type(&self) -> u8 {
        (match self {
            ArgData::Arg(_) => ArgType::Arg,
            ArgData::ArgRef(_) => ArgType::ArgRef,
            ArgData::NamedArg { key: _, value: _ } => ArgType::NamedArg,
            ArgData::NamedArgRef { key: _, value: _ } => ArgType::NamedArgRef,
        }) as u8
    }
}

impl<'c> Script<'c> {
    /// Create a new [`Script`] with no arguments specified.
    #[inline]
    pub fn new<S>(lua_script: S) -> Self
    where
        S: Into<Cow<'c, str>>,
    {
        Self::new_with_capacity(lua_script, 0)
    }

    /// Creates a new [`Script`] with a set capacity to the internal arguments list
    #[inline]
    pub fn new_with_capacity<S>(lua_script: S, arg_cap: usize) -> Self
    where
        S: Into<Cow<'c, str>>,
    {
        Self {
            lua: lua_script.into(),
            with: Option::default(),
            args: Vec::with_capacity(arg_cap),
            timeout: Option::default(),
            discard: bool::default(),
        }
    }

    /// The timeout on the scripts execution
    #[inline]
    #[must_use]
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// Changes the timeout of the scrips execution
    ///
    /// Beaware of setting this too low, if you timeout and the module takes a long time to execute your code.\
    /// And you send another request, the first execution is still running and you will get a [`Error::WrongHandle`] error.\
    /// Scripts never gets killed prematurely even if you timeout on the client.
    #[inline]
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// A list of all *named* arguments that has been added to this [`Script`]
    #[inline]
    #[must_use]
    pub fn named_args(&self) -> Vec<&str> {
        self.args
            .iter()
            .filter_map(|x| match x {
                ArgData::Arg(_) | ArgData::ArgRef(_) => None,
                ArgData::NamedArg { key, value: _ } | ArgData::NamedArgRef { key, value: _ } => {
                    Some(*key)
                }
            })
            .collect()
    }

    /// Removes the specified *named* arg with a `key`
    #[inline]
    pub fn remove_named_arg(&mut self, key: &'c str) {
        self.args.retain(|x| match x {
            ArgData::Arg(_) | ArgData::ArgRef(_) => true,
            ArgData::NamedArgRef {
                key: arg_key,
                value: _,
            }
            | ArgData::NamedArg {
                key: arg_key,
                value: _,
            } => *arg_key != key,
        });
    }

    /// If we add a `with` [`ItemRef`], we need to validate that no previous pushed arguments have mismatched resolve ids
    ///
    /// We dont need to check ids when we push arguments since .with is internal to the crate,
    /// and .with can only be written to after the consumer has given away ownership of the script object,
    /// thus they cant push arguments after .with has been maybe written to. so we only need to check when adding .with
    pub(crate) fn check_args(&self, item_ref: &'c ItemRef) -> Result<(), Error> {
        let id = item_ref.resolve().id();
        for arg in &self.args {
            let arg_id = match arg {
                ArgData::ArgRef(r) => r.resolve().id(),
                ArgData::NamedArgRef { key: _, value } => value.resolve().id(),
                _ => continue,
            };

            if id != arg_id {
                return Err(Error::MismatchedItemRef(id, arg_id));
            }
        }
        Ok(())
    }

    /// We dont want to expose this as it can cause item ref confusion between [`Resolve`](crate::Resolve) instances.
    /// By only keeping with private to crate, consumer has to call execute and make non-with script objects on the [`ItemRef`] item
    /// which will call with for them so it ensures it has the correct [`ItemRef`] for its instance
    pub(crate) fn with(mut self, item_ref: &'c ItemRef) -> Result<Self, Error> {
        self.check_args(item_ref)?;

        self.with = Some(item_ref);
        Ok(self)
    }

    /// Pushes `value` to the global `arg` variable.
    ///
    /// # Errors
    /// If it can't properly serialize `value`, this will fail
    #[inline]
    pub fn arg<S: Serialize>(mut self, value: &S) -> Result<Self, Error> {
        let arg = ArgData::Arg(Cow::Owned(Self::ser(value)?));
        self.args.push(arg);
        Ok(self)
    }

    /// Pushes an [`ItemRef`] or [`ItemRefList`] to the global `arg` variable.
    ///
    /// # Errors
    /// Even tho this returns an error, this actually can't error\
    /// *(this is for a standard arg API and for the macros to work easier)*
    #[inline]
    pub fn arg_ref(mut self, item_ref: impl ToLuaRef) -> Result<Self, Error> {
        let arg = ArgData::ArgRef(item_ref.to_ref());
        self.args.push(arg);
        Ok(self)
    }

    /// Sets the global variable of `key` to `value`
    ///
    /// # Errors
    /// If it can't properly serialize `value`, this will fail
    #[inline]
    pub fn named_arg<S: Serialize>(mut self, key: &'c str, value: &S) -> Result<Self, Error> {
        self.remove_named_arg(key);

        let arg = ArgData::NamedArg {
            key,
            value: Cow::Owned(Self::ser(value)?),
        };
        self.args.push(arg);
        Ok(self)
    }

    /// Sets the global variable of `key` to an [`ItemRef`] or [`ItemRefList`]
    ///
    /// # Errors
    /// Even tho this returns an error, this actually can't error\
    /// *(this is for a standard arg API and for the macros to work easier)*
    #[inline]
    pub fn named_arg_ref(mut self, key: &'c str, item_ref: impl ToLuaRef) -> Result<Self, Error> {
        self.remove_named_arg(key);

        let arg = ArgData::NamedArgRef {
            key,
            value: item_ref.to_ref(),
        };
        self.args.push(arg);
        Ok(self)
    }

    /// Serialize a value to a buffer
    #[inline]
    pub(crate) fn ser<S: Serialize>(value: &S) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec(value)?)
    }
}

impl<'c, T> From<T> for Script<'c>
where
    T: Into<Cow<'c, str>>,
{
    fn from(value: T) -> Self {
        Self::new(value.into())
    }
}

impl<T> From<T> for OwnedScript
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Self::new(value.into())
    }
}

impl<'c> From<&'c OwnedScript> for Script<'c> {
    fn from(value: &'c OwnedScript) -> Self {
        value.as_ref()
    }
}

impl<'c> From<Script<'c>> for OwnedScript {
    fn from(value: Script<'c>) -> Self {
        value.as_owned()
    }
}

impl std::fmt::Display for Script<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.lua)
    }
}
impl std::fmt::Display for OwnedScript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.lua)
    }
}

/// References which can be referenced by a single reference
pub trait ToLuaRef {
    fn to_ref(&self) -> ItemRef;
}

impl ToLuaRef for ItemRef {
    fn to_ref(&self) -> ItemRef {
        self.clone()
    }
}
impl ToLuaRef for ItemRefList {
    fn to_ref(&self) -> ItemRef {
        self.source()
    }
}

impl<T: ToLuaRef> ToLuaRef for &T {
    fn to_ref(&self) -> ItemRef {
        (*self).to_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new() -> Result<(), Error> {
        let script = Script::new("return 1");

        assert_eq!("return 1", script.lua);
        Ok(())
    }

    #[tokio::test]
    async fn arg() -> Result<(), Error> {
        let script = Script::new("return 1").arg(&95)?;
        assert_eq!(1, script.args.len());
        Ok(())
    }

    #[tokio::test]
    async fn round_trip() {
        let a_script = Script::new("");
        let owned = a_script.as_owned();
        let b_script = owned.as_ref();
        assert_eq!(a_script, b_script);
    }
}
