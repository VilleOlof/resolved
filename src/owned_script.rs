use std::{borrow::Cow, time::Duration};

use serde::Serialize;

use crate::{Error, ItemRef, Script, script::ArgData};

/// An `Owned` variant of [`Script`]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct OwnedScript {
    pub(crate) lua: String,
    pub(crate) args: Vec<OwnedArgData>,
    pub(crate) with: Option<ItemRef>,
    pub(crate) timeout: Option<Duration>,
}

/// An `Owned` variant of [`Arg`]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum OwnedArgData {
    Arg(Vec<u8>),
    ArgRef(ItemRef),
    NamedArg { key: String, value: Vec<u8> },
    NamedArgRef { key: String, value: ItemRef },
}

impl OwnedScript {
    /// Create a new [`Script`] with no arguments specified.
    #[inline]
    pub fn new<S: Into<String>>(lua_script: S) -> Self {
        Self {
            lua: lua_script.into(),
            with: None,
            args: Vec::new(),
            timeout: None,
        }
    }

    // we notably dont need to implement .with since all Scripts, owned or not gets converted into a ref Script before getting used internally in Resolve

    /// Pushes `value` to the global `arg` variable.
    ///
    /// # Errors
    /// If it can't properly serialize `value`, this will fail
    #[inline]
    pub fn arg<S: Serialize>(mut self, value: &S) -> Result<Self, Error> {
        let arg = OwnedArgData::Arg(Script::ser(value)?);
        self.args.push(arg);
        Ok(self)
    }

    /// Pushes an [`ItemRef`] to the global `arg` variable.
    ///
    /// # Errors
    /// Even tho this returns an error, this actually can't error\
    /// *(this is for a standard arg API)*
    #[inline]
    pub fn arg_ref(mut self, item_ref: ItemRef) -> Result<Self, Error> {
        let arg = OwnedArgData::ArgRef(item_ref);
        self.args.push(arg);
        Ok(self)
    }

    /// Sets the global variable of `key` to `value`
    ///
    /// # Errors
    /// If it can't properly serialize `value`, this will fail
    #[inline]
    pub fn named_arg<K: Into<String>, S: Serialize>(
        mut self,
        key: K,
        value: &S,
    ) -> Result<Self, Error> {
        let arg = OwnedArgData::NamedArg {
            key: key.into(),
            value: Script::ser(value)?,
        };
        self.args.push(arg);
        Ok(self)
    }

    /// Sets the global variable of `key` to an [`ItemRef`]
    ///
    /// # Errors
    /// Even tho this returns an error, this actually can't error\
    /// *(this is for a standard arg API)*
    #[inline]
    pub fn named_arg_ref<K: Into<String>>(
        mut self,
        key: K,
        item_ref: ItemRef,
    ) -> Result<Self, Error> {
        let arg = OwnedArgData::NamedArgRef {
            key: key.into(),
            value: item_ref,
        };
        self.args.push(arg);
        Ok(self)
    }
}

// ###> as_owned and as_ref impls: <###

// Arg and OwnedArg doesnt need to pub their functions as their core type arent even exposed
impl ArgData<'_> {
    /// Returns an [`OwnedArg`] variant of this [`Arg`], this behaves like a clone
    #[must_use]
    fn as_owned(&self) -> OwnedArgData {
        match self {
            Self::Arg(v) => OwnedArgData::Arg(v.to_vec()),
            Self::ArgRef(v) => OwnedArgData::ArgRef((*v).clone()),
            Self::NamedArg { key, value } => OwnedArgData::NamedArg {
                key: key.to_string(),
                value: value.to_vec(),
            },
            Self::NamedArgRef { key, value } => OwnedArgData::NamedArgRef {
                key: key.to_string(),
                value: (*value).clone(),
            },
        }
    }
}

impl OwnedArgData {
    /// Returns a [`Arg`] that references the data in this [`OwnedArg`]
    #[must_use]
    fn as_ref(&self) -> ArgData<'_> {
        match self {
            Self::Arg(v) => ArgData::Arg(Cow::Borrowed(v)),
            Self::ArgRef(v) => ArgData::ArgRef(v),
            Self::NamedArg { key, value } => ArgData::NamedArg {
                key,
                value: Cow::Borrowed(value),
            },
            Self::NamedArgRef { key, value } => ArgData::NamedArgRef { key, value },
        }
    }
}

impl Script<'_> {
    /// Returns an [`OwnedScript`] variant of this [`Script`], this behaves like a clone
    #[must_use]
    pub fn as_owned(&self) -> OwnedScript {
        OwnedScript {
            lua: self.lua.to_string(),
            args: self.args.iter().map(ArgData::as_owned).collect(),
            with: self.with.cloned(),
            timeout: self.timeout,
        }
    }
}

impl OwnedScript {
    /// Returns a [`Script`] that references the data in this [`OwnedScript`]
    #[must_use]
    pub fn as_ref(&self) -> Script<'_> {
        Script {
            lua: Cow::Borrowed(&self.lua),
            args: self.args.iter().map(|x| x.as_ref()).collect(),
            with: self.with.as_ref(),
            timeout: self.timeout,
        }
    }
}
