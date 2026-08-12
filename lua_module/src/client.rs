use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    time::Duration,
};

use resolved_shared::PrePacket;

use crate::error::ModuleError;

#[derive(Debug)]
pub struct Client(TcpStream);
impl Client {
    pub fn new(port: u16) -> std::io::Result<Self> {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
        let client = TcpStream::connect(addr)?;
        Ok(Self(client))
    }

    pub fn write_port(&mut self, port: u16) -> Result<(), ModuleError> {
        self.0.write(&[PrePacket::Ready as u8])?;
        self.0.write(&port.to_be_bytes())?;
        self.0.flush()?;
        Ok(())
    }

    pub fn write_err(&mut self, err: String) -> std::io::Result<()> {
        self.0.write(&[PrePacket::Error as u8])?;
        self.0.write(&(err.len() as u32).to_be_bytes())?;
        self.0.write(&err.into_bytes())?;
        self.0.flush()?;
        Ok(())
    }

    pub fn write_noresolve(&mut self) -> Result<(), ModuleError> {
        self.0.write(&[PrePacket::NoResolve as u8])?;
        self.0.flush()?;
        Ok(())
    }

    pub fn write_ping(&mut self) -> std::io::Result<()> {
        self.0.write(&[PrePacket::Ping as u8])?;
        self.0.flush()?;
        Ok(())
    }

    pub fn read_pong(&mut self) -> std::io::Result<()> {
        let mut buf = [0u8; 1];
        self.0.read_exact(&mut buf)?;
        let packet_type = PrePacket::from_u8(buf[0]).unwrap();
        if packet_type != PrePacket::Pong {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Not a pong packet".to_string(),
            ));
        }
        Ok(())
    }

    pub fn set_read_timeout(&mut self, time: Duration) {
        self.0
            .set_read_timeout(Some(time))
            .expect("failed to set read_timeout");
    }
}
