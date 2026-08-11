use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use resolved_shared::PrePacket;
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    process::Command,
    select,
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
pub(crate) fn dll_script(path: &Path) -> LuaCode {
    format!(
        r#"
package.cpath = package.cpath .. [[;{}/?.dll]]
require("{}").start(arg[1])"#,
        path.to_string_lossy(),
        MODULE_NAME
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

pub(crate) async fn start_client_server() -> Result<(TcpListener, u16), Error> {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

pub(crate) async fn spawn_script_server(script_path: &Path, port: u16) -> Result<(), Error> {
    let fuscript = fuscript()?;
    let script_path = script_path.display().to_string();
    let port = port.to_string();
    tokio::spawn(async move {
        Command::new(fuscript)
            .arg("-q")
            .args([script_path, port])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
    })
    .await?;

    Ok(())
}

pub(crate) async fn handle_module_request(stream: &mut TcpStream) -> Result<u16, Error> {
    async fn read_err(stream: &mut TcpStream) -> Result<Error, Error> {
        let len = stream.read_u32().await?;
        let mut s = vec![0; len as usize];
        stream.read_exact(&mut s).await?;
        let err = String::from_utf8(s)?;
        Ok(Error::LuaModuleErr(err))
    }

    let sleep = tokio::time::sleep(MODULE_TIMEOUT);
    tokio::pin!(sleep);

    select! {
        _ = &mut sleep => {
            Err(Error::ModuleTimeout)
        }
        p = stream.read_u8() => {
            let packet_type = PrePacket::from_u8(p?).ok_or(Error::InvalidPacketType)?;
            match packet_type {
                PrePacket::Ready => Ok(stream.read_u16().await?),
                PrePacket::NoResolve => Err(Error::UnableToReachDavinciResolve),
                PrePacket::Error => Err(read_err(stream).await?)
            }
        }
    }
}
