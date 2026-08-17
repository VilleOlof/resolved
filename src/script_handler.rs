use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use parking_lot::RwLock;
use resolved_shared::{PipeListenerTokio, PipeTokio, PrePacket, ResolveConfig};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
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
pub(crate) fn dll_script(path: &Path, id: u32) -> LuaCode {
    use itoa::Buffer;
    format!(
        r#"
package.cpath = package.cpath .. [[;{}/?.dll]]
require("{}").start({})"#,
        path.to_string_lossy(),
        MODULE_NAME,
        Buffer::new().format(id)
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

/// Spawns the lua module using the scripting binary specified in [`fuscript`] and the script specified
pub(crate) async fn spawn_script_server(
    script_path: &Path,
    cancelled: Arc<RwLock<bool>>,
) -> Result<Child, Error> {
    let fuscript = fuscript()?;
    let script_path = script_path.display().to_string();

    #[cfg(feature = "tracing")]
    tracing::trace!(?fuscript, "Starting module");

    let mut cmd = Command::new(fuscript);
    cmd.arg("-q").args([script_path]);

    #[cfg(not(debug_assertions))]
    {
        cmd.stdout(std::process::Stdio::piped());
    }

    match cmd.spawn() {
        Err(e) => {
            eprintln!("{e:?}");
            *cancelled.write() = true;
            return Err(e.into());
        }
        Ok(handle) => return Ok(handle),
    }
}

/// Handles the connection when starting up the lua module, writing the configuration and awaiting it's ready packet.
pub(crate) async fn handle_module_request(
    module_pipe: &mut PipeTokio,
    pipe: PipeListenerTokio,
) -> Result<PipeTokio, Error> {
    async fn read_err(pipe: &mut PipeTokio) -> Result<Error, Error> {
        let len = pipe.read_u32().await?;
        let mut s = vec![0; len as usize];
        pipe.read_exact(&mut s).await?;
        let err = String::from_utf8(s)?;
        Ok(Error::LuaModuleErr(err))
    }

    let sleep = tokio::time::sleep(MODULE_TIMEOUT);
    tokio::pin!(sleep);

    // timeout, or if the module pipe connects (module is ready), or the module errors in setup
    select! {
        () = &mut sleep => {
            Err(Error::ModuleTimeout)
        }
        pipe = pipe.accept() => return Ok(pipe?),
        p = module_pipe.read_u8() => {
            let packet_type = PrePacket::from_u8(p?).ok_or(Error::InvalidPacketType)?;
            match packet_type {
                PrePacket::Ready => unimplemented!(),
                PrePacket::NoResolve => Err(Error::UnableToReachDavinciResolve),
                PrePacket::Error => Err(read_err(module_pipe).await?),
                _ => unreachable!("ping/pong requests can't be sent yet")
            }
        }
    }
}

pub(crate) async fn write_config(
    pipe: &mut PipeTokio,
    config: &ResolveConfig,
    shmem_path: &Path,
) -> Result<(), Error> {
    pipe.write_u8(PrePacket::Configuration as u8).await?;

    pipe.write_u8(u8::from(config.reset_globals)).await?;

    let shmem_path = shmem_path.display().to_string();
    pipe.write_u32(u32::try_from(shmem_path.len())?).await?;
    pipe.write_all(&shmem_path.into_bytes()).await?;
    pipe.flush().await?;

    #[cfg(feature = "tracing")]
    tracing::trace!(?config, "Sent configuration");

    Ok(())
}
