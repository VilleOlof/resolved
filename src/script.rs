use std::borrow::Cow;

use bytes::{BufMut, BytesMut};
use resolved_shared::ArgType;
use serde::Serialize;

use crate::{Error, ItemRef};

/// A piece of lua code with optional arguments.
///
/// A [`Script`] is to be sent to [`Resolve`](crate::Resolve) with it's [`execute`](crate::Resolve::execute) and [`store`](crate::Resolve::store) functions.\
/// For most scenarios, you can just use a [`str`]/[`String`] for the input argument to these methods.  
///
/// But if you now wanted to execute some code with some variables or arguments from *Rust*,\
/// you'll need to use the [`Script::new`] and it's builder functions.  
///
/// There are four different types of arguments:  
/// - **`Arg`**\
///     Values added with [`arg`](Script::arg) will be pushed to the global `arg` table in the lua enviroment.  
/// - **`ArgRef`**\
///     With the normal `Arg`, you can't specify an [`ItemRef`] and thus some previous stored variable data.\
///     But with [`arg_ref`](Script::arg_ref), you can specify an [`ItemRef`] to push to the global `arg` table.\
///     Note that this [`ItemRef`] must derive from the same [`Resolve`](crate::Resolve) instance.
/// - **`NamedArg`**\
///     Instead of pushing values to `arg`, this will simply put the value in specified global variable.\
///     Using [`named_arg`](Script::named_arg) with a `key` and `value` argument will satisfy this.\
///     Note that you can't name your variable `self`, as that is either assigned to `Resolve()` or the executed [`ItemRef`].
/// - **`NamedArgRef`**\
///     Behaves the same as `NamedArg` but with an [`ItemRef`] as the value instead, use [`named_arg_ref`](Script::named_arg_ref) for this.
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
#[derive(Debug, Clone)]
pub struct Script<'c> {
    /// The code to be loaded and executed in the lua module
    lua: Cow<'c, str>,
    /// List of all arguments to be sent with the code, can be any of the 4 different ones.  
    args: Vec<Arg<'c>>,
    /// An optional [`ItemRef`] which `self` will be set to if specified.\
    /// Can only be set by [`execute_with`](crate::Resolve::execute_with)/[`store_with`](crate::Resolve::store_with) functions on [`Resolve`](crate::Resolve)
    pub(crate) with: Option<&'c ItemRef>,
}

/// The different types of argument.  
///
/// `Arg` / `ArgRef` are pushed to the global `arg` variable.\
/// `NamedArg` / `NamedArgRef` are added as global variables
#[derive(Debug, Clone)]
enum Arg<'s> {
    Arg(Vec<u8>),
    ArgRef(&'s ItemRef),
    NamedArg { key: &'s str, value: Vec<u8> },
    NamedArgRef { key: &'s str, value: &'s ItemRef },
}

impl Arg<'_> {
    pub const fn arg_type(&self) -> u8 {
        (match self {
            Arg::Arg(_) => ArgType::Arg,
            Arg::ArgRef(_) => ArgType::ArgRef,
            Arg::NamedArg { key: _, value: _ } => ArgType::NamedArg,
            Arg::NamedArgRef { key: _, value: _ } => ArgType::NamedArgRef,
        }) as u8
    }
}

impl<'c> Script<'c> {
    /// Create a new [`Script`] with no arguments specified.
    pub fn new<S>(lua_script: S) -> Self
    where
        S: Into<Cow<'c, str>>,
    {
        Self {
            lua: lua_script.into(),
            with: None,
            args: Vec::new(),
        }
    }

    /// This is the only validation we need to do with internal Resolve instance IDs and to make sure they are the same.  
    /// Since execute_with and those are internal and the only way to use those is via Script and .with
    /// We only need to validate those functions who take an ItemRef as an argumen in Script
    pub(crate) fn validate_item_ref_id(&self, item_ref: &'c ItemRef) -> Result<(), Error> {
        if let Some(with) = self.with {
            let a = with.resolve().id();
            let b = item_ref.resolve().id();

            if a != b {
                return Err(Error::MismatchedItemRef(a, b));
            }
        }

        Ok(())
    }

    /// We dont want to expose this as it can cause item ref confusion between resolve instances.  
    /// By only keeping with private to crate, consumer has to call execute and make non-with script objects on the ItemRef item
    /// which will call with for them so it ensures it has the correct itemref for its instance
    pub(crate) fn with(mut self, item_ref: &'c ItemRef) -> Result<Self, Error> {
        self.validate_item_ref_id(item_ref)?;

        self.with = Some(item_ref);
        Ok(self)
    }

    /// Pushes `value` to the global `arg` variable.
    pub fn arg<S: Serialize>(mut self, value: S) -> Result<Self, Error> {
        let arg = Arg::Arg(Self::ser(value)?);
        self.args.push(arg);
        Ok(self)
    }

    /// Pushes an [`ItemRef`] to the global `arg` variable.
    pub fn arg_ref(mut self, item_ref: &'c ItemRef) -> Result<Self, Error> {
        self.validate_item_ref_id(item_ref)?;

        let arg = Arg::ArgRef(item_ref);
        self.args.push(arg);
        Ok(self)
    }

    /// Sets the global variable of `key` to `value`
    pub fn named_arg<S: Serialize>(mut self, key: &'c str, value: S) -> Result<Self, Error> {
        let arg = Arg::NamedArg {
            key,
            value: Self::ser(value)?,
        };
        self.args.push(arg);
        Ok(self)
    }

    /// Sets the global variable of `key` to an [`ItemRef`]
    pub fn named_arg_ref(mut self, key: &'c str, item_ref: &'c ItemRef) -> Result<Self, Error> {
        self.validate_item_ref_id(item_ref)?;

        let arg = Arg::NamedArgRef {
            key,
            value: item_ref,
        };
        self.args.push(arg);
        Ok(self)
    }

    /// Serialize a value to a buffer
    fn ser<S: Serialize>(value: S) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec(&value)?)
    }

    /// Packs the [`Script`] into a serialized format.  
    ///
    /// `[if_with:u8,ref_id_if_with]`,
    /// `[str_len:u32;lua_script_str]`,
    /// `[args_len:u32]`,
    ///     `[arg_type:u8;arg_data]`,
    ///
    /// where `arg_data` can be just a value, a u64 for a ref, a len-prefixed string+value or len-prefixed string+u64
    pub(crate) fn serialize(self) -> Result<Vec<u8>, Error> {
        fn string(data: &mut BytesMut, str: &str) {
            data.put_u32(str.len() as u32);
            data.put(str.as_bytes());
        }

        let mut data = BytesMut::new();

        data.put_u8(self.with.is_some() as u8);
        if let Some(item) = self.with {
            data.put_u64(item.id());
        }

        string(&mut data, &self.lua);
        data.put_u32(self.args.len() as u32);

        for arg in self.args {
            data.put_u8(arg.arg_type());

            match arg {
                Arg::Arg(arg) => {
                    data.put_u32(arg.len() as u32);
                    data.put(&arg[..]);
                }
                Arg::ArgRef(arg) => data.put_u64(arg.id()),
                Arg::NamedArg { key, value } => {
                    string(&mut data, &key);
                    data.put_u32(value.len() as u32);
                    data.put(&value[..]);
                }
                Arg::NamedArgRef { key, value } => {
                    string(&mut data, &key);
                    data.put_u64(value.id());
                }
            }
        }

        Ok(data.to_vec())
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
