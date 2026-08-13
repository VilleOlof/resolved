use std::{io::Write, net::SocketAddr};

use bytes::BufMut;
use resolved_shared::{MsgPacket, ScriptResponse};
use serde::de::DeserializeOwned;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::{Error, Resolve, Script};

impl Resolve {
    /// Send a packet to the module, this creates a new connection to it with the following data and packet
    async fn send_packet<F>(&self, packet: MsgPacket, cap: usize, body: F) -> Result<Vec<u8>, Error>
    where
        F: FnOnce(&mut Vec<u8>) -> Result<(), Error>,
    {
        let mut data = Vec::with_capacity(cap + 1);
        data.push(packet as u8);
        body(&mut data)?;
        let data_len = data.len() as u64;

        let mut conn = TcpStream::connect(self.host()).await?;
        conn.write_u64(data_len).await?;
        conn.write_all(&data).await?;

        let len = usize::try_from(conn.read_u64().await?)?;
        let mut buf = vec![0u8; len];
        conn.read_exact(&mut buf).await?;

        Ok(buf)
    }

    /// Send a [`MsgPacket::Execute`] packet to the module
    pub(crate) async fn send_execute<T>(
        &self,
        script: Script<'_>,
    ) -> Result<ScriptResponse<T>, Error>
    where
        T: DeserializeOwned,
    {
        let bytes = self
            .send_packet(MsgPacket::Execute, script.size_hint(), |data| {
                data.put(&script.serialize()?[..]);
                Ok(())
            })
            .await?;

        Ok(rmp_serde::from_slice(&bytes)?)
    }

    /// Send a [`MsgPacket::Store`] packet to the module
    pub(crate) async fn send_store(
        &self,
        script: Script<'_>,
    ) -> Result<ScriptResponse<u64>, Error> {
        let bytes = self
            .send_packet(MsgPacket::Store, script.size_hint(), |data| {
                data.put(&script.serialize()?[..]);
                Ok(())
            })
            .await?;

        Ok(rmp_serde::from_slice(&bytes)?)
    }

    /// Send a [`MsgPacket::DropItem`] packet to the module
    pub(crate) async fn send_drop_item(&self, id: u64) -> Result<(), Error> {
        self.send_packet(MsgPacket::DropItem, size_of::<u64>(), |data| {
            data.put_u64(id);
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Send a [`MsgPacket::Shutdown`] packet to the module
    ///
    /// This is not async so it can be sent through [`Drop`],
    /// it writes the most barebones packet it can just to shutdown
    pub(crate) fn send_shutdown(host: &SocketAddr) -> Result<(), Error> {
        let data = [MsgPacket::Shutdown as u8];

        let mut conn = std::net::TcpStream::connect(host)?;
        conn.write_all(&(data.len() as u64).to_be_bytes())?;
        conn.write_all(&data)?;
        conn.flush()?;

        Ok(())
    }
}
