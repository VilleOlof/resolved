use crate::script_handler::MODULE_TIMEOUT;

/// Any error that can occur while using `resolved`
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Lua module was unable to reach DaVinci Resolve. Are you sure it's open?")]
    UnableToReachDavinciResolve,
    #[error("Something went wrong when trying to call fuscript, buf: {0:?}")]
    FuscriptFailed([u8; 8]),
    #[error("Failed to grab stdout")]
    NoStdout,
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
    MismatchedItemRef(u64, u64),
    #[error(
        "Can't have ItemRef's has arguments when executing with a PooledResolve. This is since it doesn't implement '.store', thus no references can derive from it"
    )]
    CantHoldReferenceInPool,
    #[error("The module for some reason was either shutdown or panic'd")]
    ModuleNotRunning,
    #[error("Tried to get a ItemRef but got nil")]
    NilItemRef,

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
}
