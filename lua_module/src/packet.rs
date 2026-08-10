use std::time::Duration;

use resolved_shared::{MsgPacket, ScriptResponse};
use serde::Serialize;

use crate::error::RequestError;

type Reader<'s> = dyn std::io::Read + 's;

macro_rules! read_num {
    ($n:ty, $f:ident) => {
        pub fn $f(reader: &mut Reader) -> Result<$n, RequestError> {
            let mut buf = [0u8; size_of::<$n>()];
            reader.read_exact(&mut buf)?;
            Ok(<$n>::from_be_bytes(buf))
        }
    };
}

read_num!(u8, read_u8);
read_num!(u32, read_u32);
read_num!(u64, read_u64);

pub fn read_packet(reader: &mut Reader) -> Result<MsgPacket, RequestError> {
    let raw = read_u8(reader)?;
    let packet_type = MsgPacket::from_u8(raw).ok_or(RequestError::InvalidPacketType(raw))?;
    Ok(packet_type)
}

pub fn read_string(reader: &mut Reader) -> Result<String, RequestError> {
    let len = read_u32(reader)?;

    let mut str = vec![0u8; len as usize];
    reader.read_exact(&mut str)?;
    let str = String::from_utf8(str)?;

    Ok(str)
}

pub fn serialize_values<T: Serialize>(
    value: T,
    eval_time: Duration,
) -> Result<Vec<u8>, RequestError> {
    Ok(rmp_serde::to_vec(&ScriptResponse::Ok { value, eval_time })?)
}
