use bytes::{BufMut, Bytes};
use resolved_shared::{MsgPacket, ScriptResponse};
use serde::de::DeserializeOwned;

use crate::{Error, Resolve};

impl Resolve {
    async fn send_packet<F>(&self, packet: MsgPacket, body: F) -> Result<Bytes, Error>
    where
        F: FnOnce(&mut Vec<u8>),
    {
        let mut data = Vec::with_capacity(4);
        data.push(packet as u8);
        body(&mut data);

        let req = self.client.post(self.url.clone()).body(data);
        let res = req.send().await?;
        Ok(res.bytes().await?)
    }

    pub(crate) async fn send_execute<T>(
        &self,
        lua_script: String,
    ) -> Result<ScriptResponse<T>, Error>
    where
        T: DeserializeOwned,
    {
        let bytes = self
            .send_packet(MsgPacket::Execute, |data| {
                write_string(data, lua_script);
            })
            .await?;

        Ok(rmp_serde::from_slice(&bytes)?)
    }

    pub(crate) async fn send_execute_with<T>(
        &self,
        id: u64,
        lua_script: String,
    ) -> Result<ScriptResponse<T>, Error>
    where
        T: DeserializeOwned,
    {
        let bytes = self
            .send_packet(MsgPacket::ExecuteWith, |data| {
                data.put_u64(id);
                write_string(data, lua_script);
            })
            .await?;

        Ok(rmp_serde::from_slice(&bytes)?)
    }

    pub(crate) async fn send_store(
        &self,
        lua_script: String,
    ) -> Result<ScriptResponse<u64>, Error> {
        let bytes = self
            .send_packet(MsgPacket::Store, |data| {
                write_string(data, lua_script);
            })
            .await?;

        Ok(rmp_serde::from_slice(&bytes)?)
    }

    pub(crate) async fn send_store_with(
        &self,
        id: u64,
        lua_script: String,
    ) -> Result<ScriptResponse<u64>, Error> {
        let bytes = self
            .send_packet(MsgPacket::StoreWith, |data| {
                data.put_u64(id);
                write_string(data, lua_script);
            })
            .await?;

        Ok(rmp_serde::from_slice(&bytes)?)
    }

    pub(crate) async fn send_drop_item(&self, id: u64) -> Result<(), Error> {
        self.send_packet(MsgPacket::DropItem, |data| data.put_u64(id))
            .await?;
        Ok(())
    }
}

fn write_string(data: &mut Vec<u8>, string: String) {
    data.put_u32(string.len() as u32);
    data.extend(string.into_bytes());
}
