use mlua::Error;

/// Errors during a normal execution request
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("No registry key with this id({0}) was found")]
    NoRegistryKeyWithId(u64),
    #[error("A MsgPacket of byte {0} is not valid")]
    InvalidPacketType(u8),
    #[error("A ArgType of byte {0} is not valid")]
    InvalidArgType(u8),
    #[error("Tried to call StoreTable with a value that wasn't a table, got a: {0:?}")]
    NotATable(&'static str),
    #[error("Not enough enogh bytes in shared memory len")]
    NotEnoughBytesInMemory,
    #[error("unreachable message")]
    UnableToReachResolve,

    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    RmpEncode(#[from] rmp_serde::encode::Error),
    #[error(transparent)]
    RmpDecode(#[from] rmp_serde::decode::Error),
    #[error(transparent)]
    Lua(#[from] mlua::Error),
    #[error(transparent)]
    ShmemData(#[from] resolved_shared::ShmemDataError),
}

/// Errors during setup and outside normal requests
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("tried to get the global Resolve() function but got: {0}")]
    GlobalResolveWasNotAFunction(&'static str),
    #[error("Waited, got flag: {1:?} but expected {0:?}")]
    InvalidPipeFlag(u8, u8),

    #[error(transparent)]
    Lua(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Shmem(#[from] resolved_shared::ShmemError),
    #[error(transparent)]
    ShmemData(#[from] resolved_shared::ShmemDataError),
    #[error(transparent)]
    RmpDecode(#[from] rmp_serde::decode::Error),
}
