use pontia_core::{Error, Result};
use serde::{Serialize, de::DeserializeOwned};

pub(super) fn encode(items: &[impl Serialize]) -> Result<(i64, Vec<u8>)> {
    let bytes = serde_json::to_vec(items)?;
    if bytes.len() < 256 {
        return Ok((0, bytes));
    }
    Ok((1, zstd::stream::encode_all(&bytes[..], 3)?))
}

pub(super) fn decode<T: DeserializeOwned>(encoding: i64, bytes: &[u8]) -> Result<Vec<T>> {
    match encoding {
        0 => Ok(serde_json::from_slice(bytes)?),
        1 => Ok(serde_json::from_reader(zstd::Decoder::new(bytes)?)?),
        _ => Err(Error::Domain("unknown timeline cache encoding".into())),
    }
}
