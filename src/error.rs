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

    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    UrlParse(#[from] url::ParseError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    RmpDecode(#[from] rmp_serde::decode::Error),
    #[error(transparent)]
    Var(#[from] std::env::VarError),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Acquire(#[from] tokio::sync::AcquireError),
}
