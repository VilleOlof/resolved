use std::io::{Read, Write};

use resolved_shared::{ModuleConfig, PrePacket, module_pipe_path, pipe_path};

use crate::error::ModuleError;

/// The [`TcpStream`] for [`PrePacket`]'s and packets sent outside the normal request execution loop
#[derive(Debug)]
pub struct Client(Pipe);

impl Client {
    /// Creates a new client and connect to localhost:<port>
    pub fn new(id: u32) -> std::io::Result<Self> {
        let pipe = connect_module_pipe(id)?;
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

    fn read_u32(&mut self) -> std::io::Result<u32> {
        let mut buf = [0u8; size_of::<u32>()];
        self.0.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    fn read_u8(&mut self) -> std::io::Result<u8> {
        let mut buf = [0u8; size_of::<u8>()];
        self.0.read_exact(&mut buf)?;
        Ok(u8::from_be_bytes(buf))
    }

    fn read_string(&mut self, len: u32) -> std::io::Result<String> {
        let buf = self.read_buf(len)?;
        Ok(String::from_utf8(buf).expect("string had invalid utf8"))
    }

    fn read_buf(&mut self, len: u32) -> std::io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        self.0.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Reads the specifiec configurations from the client
    pub fn read_config(&mut self) -> Result<ModuleConfig, ModuleError> {
        let mut buf = [0u8; 1];
        self.0.read_exact(&mut buf)?;
        let packet_type = PrePacket::from_u8(buf[0]).expect("invalid packet type byte");
        if packet_type != PrePacket::Configuration {
            panic!("invalid packet type")
        }

        let reset_globals = {
            let mut buf = [0u8; 1];
            self.0.read_exact(&mut buf)?;
            buf[0] == 1
        };

        let globals = {
            let len = self.read_u32()?;

            let mut globals = Vec::with_capacity(len as usize);
            for _ in 0..len {
                let key_len = self.read_u32()?;
                let key = self.read_string(key_len)?;

                let buf_len = self.read_u32()?;
                let buf = self.read_buf(buf_len)?;
                let value: rmpv::Value = rmp_serde::from_slice(&buf)?;

                globals.push((key, value));
            }

            globals
        };

        let function_check = {
            let is_some = self.read_u8()? == 1;
            if is_some {
                let f_len = self.read_u32()?;
                let f = self.read_string(f_len)?;
                Some(f)
            } else {
                None
            }
        };

        Ok(ModuleConfig {
            reset_globals,
            globals,
            function_check,
        })
    }
}

use interprocess::os::windows::named_pipe::{self, DuplexPipeStream, pipe_mode};

pub type Pipe = named_pipe::PipeStream<pipe_mode::Bytes, pipe_mode::Bytes>;

/// Connects to the request pipe for events related to shared memory access
pub fn connect_pipe(id: u32) -> std::io::Result<DuplexPipeStream<pipe_mode::Bytes>> {
    DuplexPipeStream::connect_by_path(pipe_path(id))
}

/// Connects to the higher up module pipe before setting up things
pub fn connect_module_pipe(id: u32) -> std::io::Result<DuplexPipeStream<pipe_mode::Bytes>> {
    DuplexPipeStream::connect_by_path(module_pipe_path(id))
}
