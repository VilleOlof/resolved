use mlua::Error;

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("No registry key with this id({0}) was found")]
    NoRegistryKeyWithId(u64),
    #[error("A MsgPacket of byte {0} is not valid")]
    InvalidPacketType(u8),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Rmp(#[from] rmp_serde::encode::Error),
    #[error(transparent)]
    Lua(#[from] mlua::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("No ip found")]
    NoIp,

    #[error(transparent)]
    Lua(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Any(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}
