use bytes::{BufMut, Bytes};
use resolved_shared::{MsgPacket, ScriptResponse};
use serde::de::DeserializeOwned;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::{Error, Resolve, Script};

impl Resolve {
    async fn send_packet<F>(&self, packet: MsgPacket, body: F) -> Result<Bytes, Error>
    where
        F: FnOnce(&mut Vec<u8>) -> Result<(), Error>,
    {
        let mut data = Vec::with_capacity(4);
        data.push(packet as u8);
        body(&mut data)?;
        let data_len = data.len() as u64;

        let mut conn = TcpStream::connect(self.host.as_ref()).await?;
        conn.write_u64(data_len).await?;
        conn.write_all(&data).await?;

        let len = conn.read_u64().await? as usize;
        let mut buf = vec![0u8; len];
        conn.read_exact(&mut buf).await?;

        Ok(Bytes::from_owner(buf))
    }

    pub(crate) async fn send_execute<T>(
        &self,
        script: Script<'_>,
    ) -> Result<ScriptResponse<T>, Error>
    where
        T: DeserializeOwned,
    {
        let bytes = self
            .send_packet(MsgPacket::Execute, |data| {
                data.put(&script.serialize()?[..]);
                Ok(())
            })
            .await?;

        Ok(rmp_serde::from_slice(&bytes)?)
    }

    pub(crate) async fn send_store(
        &self,
        script: Script<'_>,
    ) -> Result<ScriptResponse<u64>, Error> {
        let bytes = self
            .send_packet(MsgPacket::Store, |data| {
                data.put(&script.serialize()?[..]);
                Ok(())
            })
            .await?;

        Ok(rmp_serde::from_slice(&bytes)?)
    }

    pub(crate) async fn send_drop_item(&self, id: u64) -> Result<(), Error> {
        self.send_packet(MsgPacket::DropItem, |data| {
            data.put_u64(id);
            Ok(())
        })
        .await?;
        Ok(())
    }
}
