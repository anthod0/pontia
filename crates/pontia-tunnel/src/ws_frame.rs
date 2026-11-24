use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

pub const MAX_PREVIEW_WS_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WsTunnelFrame {
    Text(String),
    Binary(Bytes),
    Ping(Bytes),
    Pong(Bytes),
    Close(Option<(u16, String)>),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("WebSocket message exceeds 16 MiB")]
    MessageTooBig,
    #[error("invalid WebSocket tunnel frame")]
    Invalid,
    #[error("incomplete WebSocket tunnel frame")]
    Truncated,
}

fn check_header(kind: u8, len: usize) -> Result<(), FrameError> {
    if len > MAX_PREVIEW_WS_MESSAGE_BYTES {
        return Err(FrameError::MessageTooBig);
    }
    match kind {
        1 | 2 => Ok(()),
        3 | 4 if len <= 125 => Ok(()),
        5 if len <= 125 && len != 1 => Ok(()),
        _ => Err(FrameError::Invalid),
    }
}

impl WsTunnelFrame {
    pub fn encode(&self) -> Result<Bytes, FrameError> {
        let (kind, code, payload): (_, Option<u16>, &[u8]) = match self {
            Self::Text(text) => (1, None, text.as_bytes()),
            Self::Binary(bytes) => (2, None, bytes),
            Self::Ping(bytes) => (3, None, bytes),
            Self::Pong(bytes) => (4, None, bytes),
            Self::Close(None) => (5, None, &[]),
            Self::Close(Some((code, reason))) if CloseCode::from(*code).is_allowed() => {
                (5, Some(*code), reason.as_bytes())
            }
            Self::Close(Some(_)) => return Err(FrameError::Invalid),
        };
        let len = payload.len() + if code.is_some() { 2 } else { 0 };
        check_header(kind, len)?;
        let mut frame = BytesMut::with_capacity(5 + len);
        frame.put_u8(kind);
        frame.put_u32(len as u32);
        if let Some(code) = code {
            frame.put_u16(code);
        }
        frame.extend_from_slice(payload);
        Ok(frame.freeze())
    }

    /// Append incoming DATA to the buffer and decode until this returns None.
    pub fn decode(buffer: &mut BytesMut) -> Result<Option<Self>, FrameError> {
        if buffer.len() < 5 {
            return Ok(None);
        }
        let kind = buffer[0];
        let len = (&buffer[1..5]).get_u32() as usize;
        check_header(kind, len)?;
        if buffer.len() < 5 + len {
            return Ok(None);
        }
        buffer.advance(5);
        let mut payload = buffer.split_to(len).freeze();
        let text = |bytes: Bytes| String::from_utf8(bytes.into()).map_err(|_| FrameError::Invalid);
        Ok(Some(match kind {
            1 => Self::Text(text(payload)?),
            2 => Self::Binary(payload),
            3 => Self::Ping(payload),
            4 => Self::Pong(payload),
            5 if payload.is_empty() => Self::Close(None),
            5 => {
                let code = payload.get_u16();
                if !CloseCode::from(code).is_allowed() {
                    return Err(FrameError::Invalid);
                }
                Self::Close(Some((code, text(payload)?)))
            }
            _ => unreachable!("validated frame type"),
        }))
    }

    /// At end of stream, any bytes left after decoding are a partial frame.
    pub fn finish(buffer: &BytesMut) -> Result<(), FrameError> {
        if buffer.is_empty() {
            Ok(())
        } else {
            Err(FrameError::Truncated)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_coalesced_frames_and_partial_payloads() {
        let mut wire = BytesMut::from(&b"\x03\0\0\0\0\x05\0\0\0\x04\x03\xe8ok\x01\0\0\0\x02x"[..]);
        assert_eq!(
            WsTunnelFrame::decode(&mut wire),
            Ok(Some(WsTunnelFrame::Ping(Bytes::new())))
        );
        assert_eq!(
            WsTunnelFrame::decode(&mut wire),
            Ok(Some(WsTunnelFrame::Close(Some((1000, "ok".into())))))
        );
        assert_eq!(WsTunnelFrame::decode(&mut wire), Ok(None));
        assert_eq!(WsTunnelFrame::finish(&wire), Err(FrameError::Truncated));
        wire.extend_from_slice(b"y");
        assert_eq!(
            WsTunnelFrame::decode(&mut wire),
            Ok(Some(WsTunnelFrame::Text("xy".into())))
        );
        assert_eq!(WsTunnelFrame::finish(&wire), Ok(()));
    }

    #[test]
    fn rejects_invalid_headers_before_payload_and_malformed_payloads() {
        for wire in [
            &b"\x09\0\0\0\0"[..],
            b"\x03\0\0\0\x7e",
            b"\x04\0\0\0\x7e",
            b"\x05\0\0\0\x01",
            b"\x01\0\0\0\x01\xff",
            b"\x05\0\0\0\x03\x03\xe8\xff",
            b"\x05\0\0\0\x02\x03\xed",
        ] {
            assert_eq!(
                WsTunnelFrame::decode(&mut BytesMut::from(wire)),
                Err(FrameError::Invalid)
            );
        }
        let mut oversized = BytesMut::from(&b"\x02\x01\0\0\x01"[..]);
        assert_eq!(
            WsTunnelFrame::decode(&mut oversized),
            Err(FrameError::MessageTooBig)
        );
        let frame = WsTunnelFrame::Binary(Bytes::from(vec![7; MAX_PREVIEW_WS_MESSAGE_BYTES]));
        assert_eq!(
            WsTunnelFrame::decode(&mut BytesMut::from(frame.encode().unwrap().as_ref())),
            Ok(Some(frame))
        );
        assert_eq!(
            WsTunnelFrame::Pong(Bytes::from(vec![0; 126])).encode(),
            Err(FrameError::Invalid)
        );
    }
}
