use std::{io::Write, net::SocketAddr};

use bytes::BufMut;
use resolved_shared::{MsgPacket, ScriptResponse};
use serde::de::DeserializeOwned;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::{Error, Resolve, Script};

macro_rules! log_script {
    ($script:expr, $name:literal) => {
        #[cfg(feature = "tracing")]
        {
            let args = $script.args.len();
            let with = &$script.with;
            let script = &$script.lua;
            tracing::trace!(?script, ?args, ?with, $name);
        }
    };
}

macro_rules! log_id {
    ($id:expr, $name:literal) => {
        #[cfg(feature = "tracing")]
        tracing::trace!("id(s)" = ?$id, $name);
    };
}

impl Resolve {
    /// Send a packet to the module, this creates a new connection to it with the following data and packet
    async fn send_packet<F, R, T>(
        &self,
        packet: MsgPacket,
        cap: usize,
        body: F,
        response: R,
    ) -> Result<T, Error>
    where
        F: FnOnce(&mut Vec<u8>) -> Result<(), Error>,
        R: FnOnce(&[u8]) -> Result<T, Error>,
        T: DeserializeOwned,
    {
        if self.cancelled() {
            return Err(Error::ModuleNotRunning);
        }

        let cap = cap + 1;
        let mut buffers = self.buffers().await;
        buffers.packet_write.reserve(cap);

        #[cfg(feature = "tracing")]
        let span = {
            let port = self.host().port();
            // we only want to enter after the .buffers since that holds an exlusive mutex so actual work is happening
            tracing::trace_span!("send_packet", ?packet, ?port)
        };
        #[cfg(feature = "tracing")]
        let _enter = span.enter();

        buffers.packet_write.push(packet as u8);
        body(&mut buffers.packet_write)?;
        let data_len = buffers.packet_write.len() as u64;

        let mut conn = TcpStream::connect(self.host()).await?;
        conn.write_u64(data_len).await?;
        conn.write_all(&buffers.packet_write).await?;

        #[cfg(feature = "tracing")]
        tracing::trace!(data_len, cap, "Sent packet");

        let len = usize::try_from(conn.read_u64().await?)?;
        buffers.packet_read.reserve(len);
        // .buffers() cleared the buf, so the resize always starts from 0
        buffers.packet_read.resize(len, 0u8);
        conn.read_exact(&mut buffers.packet_read).await?;

        #[cfg(feature = "tracing")]
        tracing::trace!(len, "Received packet");

        response(&buffers.packet_read)
    }

    /// Send a [`MsgPacket::Execute`] packet to the module
    pub(crate) async fn send_execute<T>(
        &self,
        script: Script<'_>,
    ) -> Result<ScriptResponse<T>, Error>
    where
        T: DeserializeOwned,
    {
        log_script!(script, "send_execute");
        self.send_packet(
            MsgPacket::Execute,
            script.size_hint(),
            |data| {
                data.put(&script.serialize()?[..]);
                Ok(())
            },
            |buf| Ok(rmp_serde::from_slice(buf)?),
        )
        .await
    }

    /// Send a [`MsgPacket::Store`] packet to the module
    pub(crate) async fn send_store(
        &self,
        script: Script<'_>,
    ) -> Result<ScriptResponse<Option<u64>>, Error> {
        log_script!(script, "send_store");
        self.send_packet(
            MsgPacket::Store,
            script.size_hint(),
            |data| {
                data.put(&script.serialize()?[..]);
                Ok(())
            },
            |buf| Ok(rmp_serde::from_slice(buf)?),
        )
        .await
    }

    /// Send a [`MsgPacket::StoreTable`] packet to the module
    pub(crate) async fn send_store_table(
        &self,
        script: Script<'_>,
    ) -> Result<ScriptResponse<(u64, Vec<u64>)>, Error> {
        log_script!(script, "send_store_table");
        self.send_packet(
            MsgPacket::StoreTable,
            script.size_hint(),
            |data| {
                data.put(&script.serialize()?[..]);
                Ok(())
            },
            |buf| Ok(rmp_serde::from_slice(buf)?),
        )
        .await
    }

    /// Send a [`MsgPacket::DropItem`] packet to the module
    pub(crate) async fn send_drop_item(&self, id: u64) -> Result<(), Error> {
        log_id!(id, "send_drop_item");
        self.send_packet(
            MsgPacket::DropItem,
            size_of::<u64>(),
            |data| {
                data.put_u64(id);
                Ok(())
            },
            |_| Ok(()),
        )
        .await
    }

    /// Send a [`MsgPacket::DropMany`] packet to the module
    pub(crate) async fn send_drop_items(&self, ids: Vec<u64>) -> Result<(), Error> {
        log_id!(ids, "send_drop_items");
        self.send_packet(
            MsgPacket::DropMany,
            (size_of::<u64>() * ids.len()) + size_of::<u32>(),
            |data| {
                data.put_u32(u32::try_from(ids.len())?);
                for id in ids {
                    data.put_u64(id);
                }
                Ok(())
            },
            |_| Ok(()),
        )
        .await
    }

    /// Send a [`MsgPacket::TableKeys`] packet to the module
    pub(crate) async fn send_table_keys<T>(&self, id: u64) -> Result<ScriptResponse<Vec<T>>, Error>
    where
        T: DeserializeOwned,
    {
        log_id!(id, "send_table_keys");
        self.send_packet(
            MsgPacket::TableKeys,
            size_of::<u64>(),
            |data| {
                data.put_u64(id);
                Ok(())
            },
            |buf| Ok(rmp_serde::from_slice(buf)?),
        )
        .await
    }

    /// Send a [`MsgPacket::ItemValue`] packet to the module
    pub(crate) async fn send_item_value<T>(&self, id: u64) -> Result<ScriptResponse<T>, Error>
    where
        T: DeserializeOwned,
    {
        log_id!(id, "send_item_value");
        self.send_packet(
            MsgPacket::ItemValue,
            size_of::<u64>(),
            |data| {
                data.put_u64(id);
                Ok(())
            },
            |buf| Ok(rmp_serde::from_slice(buf)?),
        )
        .await
    }

    /// Send a [`MsgPacket::Shutdown`] packet to the module
    ///
    /// This is not async so it can be sent through [`Drop`],
    /// it writes the most barebones packet it can just to shutdown
    pub(crate) fn send_shutdown(host: &SocketAddr) -> Result<(), Error> {
        let data = [MsgPacket::Shutdown as u8];

        #[cfg(feature = "tracing")]
        {
            let packet = MsgPacket::Shutdown;
            let port = host.port();
            tracing::trace!(?packet, port, "sending shutdown");
        }

        let mut conn = std::net::TcpStream::connect(host)?;
        conn.write_all(&(data.len() as u64).to_be_bytes())?;
        conn.write_all(&data)?;
        conn.flush()?;

        Ok(())
    }
}
