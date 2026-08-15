use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use parking_lot::RwLock;
use resolved_shared::{PrePacket, ResolveConfig};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    process::Command,
    select,
    task::JoinHandle,
};

use crate::Error;

/// `./lua_module` compiled from the `build.rs` script
pub(crate) static LUA_MODULE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/prebuilt/lua_module.dll"
));
/// The name of the `lua_module` compiled module
pub(crate) const MODULE_NAME: &str = "vinci";
/// The default path on windows to `fuscript.exe`
pub(crate) const DEFAULT_FUSCRIPT: &str =
    "C:/Program Files/Blackmagic Design/DaVinci Resolve/fuscript.exe";
pub(crate) const MODULE_TIMEOUT: Duration = Duration::from_secs(10);

type LuaCode = String;

/// Generates a `.lua` script that sets the cpath to contain the specified `.dll` directory and starts the internal lua module.\
/// `{path}/?.dll` so the directory that should contain it
pub(crate) fn dll_script(path: &Path, port: u16) -> LuaCode {
    format!(
        r#"
package.cpath = package.cpath .. [[;{}/?.dll]]
require("{}").start({})"#,
        path.to_string_lossy(),
        MODULE_NAME,
        itoa::Buffer::new().format(port)
    )
}

/// Returns the path to `fuscript.exe`.  
///
/// If `FUSCRIPT` in `$PATH` is not set, it will use the default path ([`DEFAULT_FUSCRIPT`])
pub(crate) fn fuscript() -> Result<PathBuf, Error> {
    match env::var("FUSCRIPT") {
        Ok(s) => Ok(PathBuf::from(s)),
        Err(VarError::NotPresent) => Ok(PathBuf::from(DEFAULT_FUSCRIPT)),
        Err(e) => Err(e.into()),
    }
}

/// Starts the client > module server to communicate configurations and when the module is ready.
pub(crate) async fn start_client_server() -> Result<(TcpListener, u16), Error> {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();

    #[cfg(feature = "tracing")]
    tracing::trace!(port, "Started client server");

    Ok((listener, port))
}

/// Spawns the lua module using the scripting binary specified in [`fuscript`] and the script specified
pub(crate) async fn spawn_script_server(
    script_path: &Path,
    cancelled: Arc<RwLock<bool>>,
) -> Result<(), Error> {
    let fuscript = fuscript()?;
    let script_path = script_path.display().to_string();

    #[cfg(feature = "tracing")]
    tracing::trace!(?fuscript, "Starting module");

    tokio::spawn(async move {
        if let Err(e) = Command::new(fuscript)
            .arg("-q")
            .args([script_path])
            .stdout(Stdio::piped())
            .spawn()
        {
            eprintln!("{e:?}");
            *cancelled.write() = true;
        }
    })
    .await?;

    Ok(())
}

/// Handles the connection when starting up the lua module, writing the configuration and awaiting it's ready packet.
pub(crate) async fn handle_module_request(
    stream: &mut TcpStream,
    config: &ResolveConfig,
) -> Result<u16, Error> {
    async fn read_err(stream: &mut TcpStream) -> Result<Error, Error> {
        let len = stream.read_u32().await?;
        let mut s = vec![0; len as usize];
        stream.read_exact(&mut s).await?;
        let err = String::from_utf8(s)?;
        Ok(Error::LuaModuleErr(err))
    }

    async fn write_config(stream: &mut TcpStream, config: &ResolveConfig) -> Result<(), Error> {
        stream.write_u8(PrePacket::Configuration as u8).await?;
        stream
            .write_u32(u32::try_from(config.timeout.as_millis())?)
            .await?;
        stream.write_u8(u8::from(config.reset_globals)).await?;
        stream.flush().await?;

        #[cfg(feature = "tracing")]
        tracing::trace!(?config, "Sent configuration");

        Ok(())
    }

    write_config(stream, config).await?;

    let sleep = tokio::time::sleep(MODULE_TIMEOUT);
    tokio::pin!(sleep);

    select! {
        () = &mut sleep => {
            Err(Error::ModuleTimeout)
        }
        p = stream.read_u8() => {
            let packet_type = PrePacket::from_u8(p?).ok_or(Error::InvalidPacketType)?;
            match packet_type {
                PrePacket::Ready => Ok(stream.read_u16().await?),
                PrePacket::NoResolve => Err(Error::UnableToReachDavinciResolve),
                PrePacket::Error => Err(read_err(stream).await?),
                _ => unreachable!("ping/pong requests can't be sent yet")
            }
        }
    }
}

/// Starts the Ping/Pong background task responder
pub(crate) async fn start_ping_responder(mut stream: TcpStream) -> JoinHandle<()> {
    async fn respond(stream: &mut TcpStream) {
        let packet_type =
            PrePacket::from_u8(stream.read_u8().await.unwrap()).expect("invalid prepacket type");
        assert!(
            packet_type == PrePacket::Ping,
            "unexpected packet type while ping pong"
        );
        stream
            .write_u8(PrePacket::Pong as u8)
            .await
            .expect("failed to write Pong byte");
        stream.flush().await.expect("failed to flush pong packet");
    }

    tokio::spawn(async move {
        loop {
            respond(&mut stream).await;
        }
    })
}
