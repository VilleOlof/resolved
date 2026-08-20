use std::time::Duration;

use crate::script_handler::MODULE_TIMEOUT;

/// Any error that can occur while using `resolved`
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Lua module was unable to reach DaVinci Resolve. Are you sure it's open?")]
    UnableToReachDavinciResolve,
    #[error("Lua module failed: {0:?}")]
    LuaModuleErr(String),
    #[error("permits got out of sync with actual instances")]
    OutOfSyncSemaphore,
    #[error("Module didn't respond with any packet within {:?}", MODULE_TIMEOUT)]
    ModuleTimeout,
    #[error("Packet type from Module was invalid")]
    InvalidPacketType,
    #[error(
        "Tried to use an ItemRef on a foreign Resolve instance whose id didn't match: {0} != {1}"
    )]
    MismatchedItemRef(u32, u32),
    #[error(
        "Can't have ItemRef's has arguments when executing with a PooledResolve. This is since it doesn't implement '.store', thus no references can derive from it"
    )]
    CantHoldReferenceInPool,
    #[error("The module for some reason was either shutdown or panic'd")]
    ModuleNotRunning,
    #[error("Tried to get a ItemRef but got nil")]
    NilItemRef,
    #[error(
        "Tried to execute a script and didn't recieve a response within the timeout window: {0:?}"
    )]
    ScriptTimeout(Duration),
    #[error("Waited, got flag: {1:?} but expected {0:?}")]
    InvalidPipeFlag(u8, u8),
    #[error(
        "Got data from another request, this can happen if request #1 times out and before it can finish #1, you send a new #2 and then recieve the data from #1. expected handle: {0:?} and got {1:?}"
    )]
    WrongHandle([u8; 4], [u8; 4]),

    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    RmpDecode(#[from] rmp_serde::decode::Error),
    #[error(transparent)]
    RmpEncode(#[from] rmp_serde::encode::Error),
    #[error(transparent)]
    Var(#[from] std::env::VarError),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Acquire(#[from] tokio::sync::AcquireError),
    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    TryFromInt(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    Shmem(#[from] resolved_shared::ShmemError),
    #[error(transparent)]
    ShmemData(#[from] resolved_shared::ShmemDataError),
}
