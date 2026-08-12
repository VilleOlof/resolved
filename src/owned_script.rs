use std::borrow::Cow;

use serde::Serialize;

use crate::{Error, ItemRef, Script, script::Arg};

/// An `Owned` variant of [`Script`]
#[derive(Debug, Clone)]
pub struct OwnedScript {
    lua: String,
    args: Vec<OwnedArg>,
    with: Option<ItemRef>,
}

#[derive(Debug, Clone)]
pub(crate) enum OwnedArg {
    Arg(Vec<u8>),
    ArgRef(ItemRef),
    NamedArg { key: String, value: Vec<u8> },
    NamedArgRef { key: String, value: ItemRef },
}

impl OwnedScript {
    /// Create a new [`Script`] with no arguments specified.
    pub fn new<S: Into<String>>(lua_script: S) -> Self {
        Self {
            lua: lua_script.into(),
            with: None,
            args: Vec::new(),
        }
    }

    // we notably dont need to implement .with since all Scripts, owned or not gets converted into a ref Script before getting used internally in Resolve

    /// Pushes `value` to the global `arg` variable.
    pub fn arg<S: Serialize>(mut self, value: S) -> Result<Self, Error> {
        let arg = OwnedArg::Arg(Script::ser(value)?);
        self.args.push(arg);
        Ok(self)
    }

    /// Pushes an [`ItemRef`] to the global `arg` variable.
    pub fn arg_ref(mut self, item_ref: ItemRef) -> Result<Self, Error> {
        let arg = OwnedArg::ArgRef(item_ref);
        self.args.push(arg);
        Ok(self)
    }

    /// Sets the global variable of `key` to `value`
    pub fn named_arg<K: Into<String>, S: Serialize>(
        mut self,
        key: K,
        value: S,
    ) -> Result<Self, Error> {
        let arg = OwnedArg::NamedArg {
            key: key.into(),
            value: Script::ser(value)?,
        };
        self.args.push(arg);
        Ok(self)
    }

    /// Sets the global variable of `key` to an [`ItemRef`]
    pub fn named_arg_ref<K: Into<String>>(
        mut self,
        key: K,
        item_ref: ItemRef,
    ) -> Result<Self, Error> {
        let arg = OwnedArg::NamedArgRef {
            key: key.into(),
            value: item_ref,
        };
        self.args.push(arg);
        Ok(self)
    }
}

// ###> as_owned and as_ref impls: <###

// Arg and OwnedArg doesnt need to pub their functions as their core type arent even exposed
impl Arg<'_> {
    fn as_owned(&self) -> OwnedArg {
        match self {
            Self::Arg(v) => OwnedArg::Arg(v.to_vec()),
            Self::ArgRef(v) => OwnedArg::ArgRef((*v).clone()),
            Self::NamedArg { key, value } => OwnedArg::NamedArg {
                key: key.to_string(),
                value: value.to_vec(),
            },
            Self::NamedArgRef { key, value } => OwnedArg::NamedArgRef {
                key: key.to_string(),
                value: (*value).clone(),
            },
        }
    }
}

impl OwnedArg {
    fn as_ref<'s>(&'s self) -> Arg<'s> {
        match self {
            Self::Arg(v) => Arg::Arg(Cow::Borrowed(v)),
            Self::ArgRef(v) => Arg::ArgRef(v),
            Self::NamedArg { key, value } => Arg::NamedArg {
                key: key,
                value: Cow::Borrowed(value),
            },
            Self::NamedArgRef { key, value } => Arg::NamedArgRef { key: key, value },
        }
    }
}

impl Script<'_> {
    pub fn as_owned(&self) -> OwnedScript {
        OwnedScript {
            lua: self.lua.to_string(),
            args: self.args.iter().map(|x| x.as_owned()).collect(),
            with: self.with.cloned(),
        }
    }
}

impl OwnedScript {
    pub fn as_ref<'s>(&'s self) -> Script<'s> {
        Script {
            lua: Cow::Borrowed(&self.lua),
            args: self.args.iter().map(|x| x.as_ref()).collect(),
            with: self.with.as_ref(),
        }
    }
}
