use std::io::{Read, Write};

use resolved_shared::{ModuleConfig, PrePacket};

use crate::error::ModuleError;

/// The [`TcpStream`] for [`PrePacket`]'s and packets sent outside the normal request execution loop
#[derive(Debug)]
pub struct Client(resolved_shared::PipeSync);

impl Client {
    /// Creates a new client and connect to localhost:<port>
    pub fn new(id: u32) -> std::io::Result<Self> {
        let pipe = resolved_shared::connect_module_pipe(id)?;
        Ok(Self(pipe))
    }

    /// Writes an error to the client
    pub fn write_err(&mut self, err: String) -> std::io::Result<()> {
        self.0.write(&[PrePacket::Error as u8])?;
        self.0.write(&(err.len() as u32).to_be_bytes())?;
        self.0.write(&err.into_bytes())?;
        self.0.flush()?;
        Ok(())
    }

    /// Writes a special error to the client that signals that the module was unable to connect to DaVinci Resolve
    pub fn write_noresolve(&mut self) -> Result<(), ModuleError> {
        self.0.write(&[PrePacket::NoResolve as u8])?;
        self.0.flush()?;
        Ok(())
    }

    /// Reads the specifiec configurations from the client
    pub fn read_config(&mut self) -> std::io::Result<ModuleConfig> {
        let mut buf = [0u8; 1];
        self.0.read_exact(&mut buf)?;
        let packet_type = PrePacket::from_u8(buf[0]).expect("invalid packet type byte");
        if packet_type != PrePacket::Configuration {
            panic!("invalid packet type")
        }

        let mut buf = [0u8; 1];
        self.0.read_exact(&mut buf)?;
        let reset_globals = buf[0] == 1;

        let mut buf = [0u8; size_of::<u32>()];
        self.0.read_exact(&mut buf)?;
        let shmem_path_len = u32::from_be_bytes(buf);

        let mut buf = vec![0u8; shmem_path_len as usize];
        self.0.read_exact(&mut buf)?;
        let shmem_path = String::from_utf8(buf).expect("shmem path had invalid utf8");

        Ok(ModuleConfig {
            reset_globals,
            shmem_path,
        })
    }
}
