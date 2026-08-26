use std::time::Duration;

use resolved_shared::ScriptResponse;
use serde::Serialize;

use crate::error::RequestError;

/// Serializes a value and it's eval time to a buffer
pub fn serialize_values<T: Serialize>(
    value: T,
    eval_time: Duration,
) -> Result<Vec<u8>, RequestError> {
    Ok(rmp_serde::to_vec(&ScriptResponse::Ok { value, eval_time })?)
}

/// Serializes an error to a buffer
pub fn serialize_err(err: String) -> Result<Vec<u8>, RequestError> {
    Ok(rmp_serde::to_vec(&ScriptResponse::<()>::Err(err))?)
}

/// Serializes a UnableToReachResolve error, special type to passthrough the error
pub fn serialize_noresolve() -> Result<Vec<u8>, RequestError> {
    Ok(rmp_serde::to_vec(
        &ScriptResponse::<()>::UnableToReachResolve,
    )?)
}
