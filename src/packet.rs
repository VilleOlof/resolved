use std::{path::Path, time::Duration};

use resolved_shared::{
    MsgPacket, PipeFlag, SIZE, ScriptResponse, ShmemConf, ShmemData, ShmemOwner, shmem_struct,
};
use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{Error, Resolve, Script, put::ShmemPut, resolve::PacketHandler};

macro_rules! log_id {
    ($id:expr, $name:literal) => {
        #[cfg(feature = "tracing")]
        tracing::trace!("id(s)" = ?$id, $name);
    };
}

shmem_struct!(ShmemClient, (Client => Module));

impl ShmemClient {
    pub fn new<S: AsRef<Path>>(path: S) -> Result<Self, Error> {
        let path = path.as_ref();
        if path.try_exists()? {
            std::fs::remove_file(path)?;
        }

        let _schmem = ShmemConf::new().size(SIZE).flink(path).create()?;
        let ptr = _schmem.as_ptr();

        Ok(Self { _schmem, ptr })
    }
}

impl Resolve {
    /// Send a packet to the module, this creates a new connection to it with the following data and packet
    async fn send_packet<F, R, T>(
        &self,
        packet: MsgPacket,
        specified_timeout: Option<Duration>,
        body: F,
        response: R,
    ) -> Result<T, Error>
    where
        F: FnOnce(&mut ShmemPut) -> Result<(), Error>,
        R: FnOnce(&[u8]) -> Result<T, Error>,
        T: DeserializeOwned,
    {
        async fn wait(handler: &mut PacketHandler) -> Result<(), Error> {
            let res = handler.pipe.read_u8().await?;
            if PipeFlag::ModuleSent as u8 != res {
                return Err(Error::InvalidPipeFlag(PipeFlag::ModuleSent as u8, res));
            }

            Ok(())
        }

        if self.cancelled() {
            return Err(Error::ModuleNotRunning);
        }

        let mut handler = self.packet_handler().await;

        let handle = {
            let mut id = [0; 4];
            fastrand::fill(&mut id);
            id
        };
        handler.shmem.set_handle(handle);

        let mut put = ShmemPut::new(&mut handler.shmem);

        #[cfg(feature = "tracing")]
        let span = {
            // we only want to enter after the .buffers since that holds an exlusive mutex so actual work is happening
            tracing::trace_span!("send_packet", ?packet)
        };
        #[cfg(feature = "tracing")]
        let _enter = span.enter();

        #[cfg(feature = "tracing")]
        let time = std::time::Instant::now();

        put.set_packet(packet);
        body(&mut put)?;
        let _data_len = put.finish();

        #[cfg(feature = "tracing")]
        let (time, write) = (std::time::Instant::now(), time.elapsed());

        handler.shmem.set_owner(ShmemClient::SIBLING_ID);
        handler.pipe.write_u8(PipeFlag::ClientSent as u8).await?;
        handler.pipe.flush().await?;

        #[cfg(feature = "tracing")]
        let (time, flush) = (std::time::Instant::now(), time.elapsed());
        #[cfg(feature = "tracing")]
        let data_len = _data_len;
        #[cfg(feature = "tracing")]
        tracing::trace!(data_len, "Sent packet");

        let timeout = specified_timeout.unwrap_or(self.timeout());
        if let Ok(w) = tokio::time::timeout(timeout, wait(&mut handler)).await {
            w?;
        } else {
            // we take ownership even if fail
            // module silently errors on this
            handler.shmem.set_owner(ShmemOwner::Client);
            return Err(Error::ScriptTimeout(timeout));
        }

        // we generate a handle, set it
        // module copies it and sets it at the end of it's request
        // and thus we read it back in
        // and if they somehow dont match we are dealing with 2 different requests
        // the data wont match and were fucked
        //
        // this can happen if we send a request and timeout
        // and while the module is processing the first request, we send another
        // so the ownership is still at the module so it sees no issues
        // so it writes its data and signals to us and now we read the data from request #1 but we are on request #2
        // by the module writing it the handle it copied we can ensure that we match request
        let stored_handle = handler.shmem.get_handle();
        if handle != stored_handle {
            handler.shmem.set_owner(ShmemOwner::Client);
            return Err(Error::WrongHandle(handle, stored_handle));
        }

        let data = handler.shmem.read_data()?;

        #[cfg(feature = "tracing")]
        let request = time.elapsed();
        #[cfg(feature = "tracing")]
        let len = data.len();
        #[cfg(feature = "tracing")]
        tracing::trace!(?write, ?flush, ?request, len, "Received packet");

        response(data)
    }

    /// Send a [`MsgPacket::Execute`] packet to the module
    pub(crate) async fn send_execute<T>(
        &self,
        script: &Script<'_>,
    ) -> Result<ScriptResponse<T>, Error>
    where
        T: DeserializeOwned,
    {
        self.send_packet(
            MsgPacket::Execute,
            script.timeout(),
            |data| data.put_script(script),
            |buf| Ok(rmp_serde::from_slice(buf)?),
        )
        .await
    }

    /// Send a [`MsgPacket::Store`] packet to the module
    pub(crate) async fn send_store(
        &self,
        script: &Script<'_>,
    ) -> Result<ScriptResponse<Option<u64>>, Error> {
        self.send_packet(
            MsgPacket::Store,
            script.timeout(),
            |data| data.put_script(script),
            |buf| Ok(rmp_serde::from_slice(buf)?),
        )
        .await
    }

    /// Send a [`MsgPacket::StoreTable`] packet to the module
    pub(crate) async fn send_store_table(
        &self,
        script: &Script<'_>,
    ) -> Result<ScriptResponse<(u64, Vec<u64>)>, Error> {
        self.send_packet(
            MsgPacket::StoreTable,
            script.timeout(),
            |data| data.put_script(script),
            |buf| Ok(rmp_serde::from_slice(buf)?),
        )
        .await
    }

    /// Send a [`MsgPacket::DropItem`] packet to the module
    pub(crate) async fn send_drop_item(&self, id: u64) -> Result<(), Error> {
        let r = self
            .send_packet(
                MsgPacket::DropItem,
                None,
                |data| {
                    data.put_data(&id.to_be_bytes())?;
                    Ok(())
                },
                |_| Ok(()),
            )
            .await;
        log_id!(id, "send_drop_item");
        r
    }

    /// Send a [`MsgPacket::DropMany`] packet to the module
    pub(crate) async fn send_drop_items(&self, ids: &[u64]) -> Result<(), Error> {
        let r = self
            .send_packet(
                MsgPacket::DropMany,
                None,
                |data| {
                    data.put_data(&u32::try_from(ids.len())?.to_be_bytes())?;
                    for id in ids {
                        data.put_data(&id.to_be_bytes())?;
                    }
                    Ok(())
                },
                |_| Ok(()),
            )
            .await;
        log_id!(ids, "send_drop_items");
        r
    }

    /// Send a [`MsgPacket::TableKeys`] packet to the module
    pub(crate) async fn send_table_keys<T>(&self, id: u64) -> Result<ScriptResponse<Vec<T>>, Error>
    where
        T: DeserializeOwned,
    {
        let r = self
            .send_packet(
                MsgPacket::TableKeys,
                None,
                |data| {
                    data.put_data(&id.to_be_bytes())?;
                    Ok(())
                },
                |buf| Ok(rmp_serde::from_slice(buf)?),
            )
            .await;
        log_id!(id, "send_table_keys");
        r
    }

    /// Send a [`MsgPacket::TableValues`] packet to the module
    pub(crate) async fn send_table_values<T>(
        &self,
        id: u64,
    ) -> Result<ScriptResponse<Vec<T>>, Error>
    where
        T: DeserializeOwned,
    {
        let r = self
            .send_packet(
                MsgPacket::TableValues,
                None,
                |data| {
                    data.put_data(&id.to_be_bytes())?;
                    Ok(())
                },
                |buf| Ok(rmp_serde::from_slice(buf)?),
            )
            .await;
        log_id!(id, "send_table_values");
        r
    }

    /// Send a [`MsgPacket::ItemValue`] packet to the module
    pub(crate) async fn send_item_value<T>(&self, id: u64) -> Result<ScriptResponse<T>, Error>
    where
        T: DeserializeOwned,
    {
        let r = self
            .send_packet(
                MsgPacket::ItemValue,
                None,
                |data| {
                    data.put_data(&id.to_be_bytes())?;
                    Ok(())
                },
                |buf| Ok(rmp_serde::from_slice(buf)?),
            )
            .await;
        log_id!(id, "send_item_value");
        r
    }

    /// Send a [`MsgPacket::Shutdown`] packet to the module
    ///
    /// it writes the most barebones packet it can just to shutdown
    pub(crate) async fn send_shutdown(&self) -> Result<(), Error> {
        self.send_packet(
            MsgPacket::Shutdown,
            None,
            |_| /* <-- 2 story house */ Ok(()),
            |_| Ok(()),
        )
        .await?;
        #[cfg(feature = "tracing")]
        tracing::trace!("send_shutdown");

        Ok(())
    }
}
