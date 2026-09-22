use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
};

use pontia_core::{Error, Result};

use super::ResolvedAgentBinding;

pub(crate) fn source_len(source: &ResolvedAgentBinding) -> Result<usize> {
    let len = fs::metadata(&source.path)
        .map_err(|err| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: raw source {} is unavailable: {err}",
                source.path.display()
            ))
        })?
        .len();
    usize::try_from(len)
        .map_err(|_| Error::Domain("source too large for this platform".to_string()))
}

pub(crate) fn read_range_from_source(
    source: &ResolvedAgentBinding,
    start: usize,
    end: usize,
) -> Result<Vec<u8>> {
    let mut file = File::open(&source.path).map_err(|err| {
        Error::CapabilityUnavailable(format!(
            "source_unavailable: raw source {} is unavailable: {err}",
            source.path.display()
        ))
    })?;
    file.seek(SeekFrom::Start(start as u64)).map_err(|err| {
        Error::CapabilityUnavailable(format!(
            "source_unavailable: raw source {} is unavailable: {err}",
            source.path.display()
        ))
    })?;
    let mut bytes = vec![0; end.saturating_sub(start)];
    file.read_exact(&mut bytes).map_err(|err| {
        Error::CapabilityUnavailable(format!(
            "source_unavailable: raw source {} is unavailable: {err}",
            source.path.display()
        ))
    })?;
    Ok(bytes)
}
