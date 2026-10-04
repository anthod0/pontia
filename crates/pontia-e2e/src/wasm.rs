use http::{HeaderMap, HeaderName, HeaderValue};
use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::{
    BrowserHandshake, BrowserIdentity, BrowserSession, Error, MAX_RECORD_PLAINTEXT, RecordDecoder,
    RecordEncoder, Result, VERSION,
    bhttp::{self, Decoder, Encoder, Event, Head, Kind},
};

impl From<Error> for JsValue {
    fn from(error: Error) -> Self {
        js_sys::Error::new(&error.to_string()).into()
    }
}

#[wasm_bindgen]
pub struct Identity {
    inner: Option<BrowserIdentity>,
}
#[wasm_bindgen]
impl Identity {
    #[wasm_bindgen(constructor)]
    pub fn new() -> std::result::Result<Identity, JsValue> {
        Ok(Self {
            inner: Some(BrowserIdentity::generate()?),
        })
    }
    pub fn public_key(&self) -> std::result::Result<Vec<u8>, JsValue> {
        Ok(self
            .inner
            .as_ref()
            .ok_or(Error::Closed)?
            .public_key()
            .to_vec())
    }
    pub fn start(
        &mut self,
        device_public: &[u8],
        capability: &[u8],
    ) -> std::result::Result<Handshake, JsValue> {
        let identity = self.inner.take().ok_or(Error::Closed)?;
        let public = device_public.try_into().map_err(|_| Error::Protocol)?;
        Ok(Handshake {
            inner: Some(identity.start(public, capability)?),
        })
    }
}

#[wasm_bindgen]
pub struct Handshake {
    inner: Option<BrowserHandshake>,
}
#[wasm_bindgen]
impl Handshake {
    pub fn bytes(&self) -> std::result::Result<Vec<u8>, JsValue> {
        Ok(self
            .inner
            .as_ref()
            .ok_or(Error::Closed)?
            .request_bytes()
            .to_vec())
    }
    pub fn confirm(&mut self, response: &[u8]) -> std::result::Result<Session, JsValue> {
        let handshake = self.inner.take().ok_or(Error::Closed)?;
        Ok(Session {
            inner: handshake.confirm(response)?,
        })
    }
}

#[wasm_bindgen]
pub struct Session {
    inner: BrowserSession,
}
#[wasm_bindgen]
impl Session {
    pub fn request(
        &self,
        method: &str,
        path: &str,
        fields: &Array,
    ) -> std::result::Result<Request, JsValue> {
        let headers = headers(fields)?;
        let head = Head::Request {
            method: method.parse().map_err(|_| Error::Protocol)?,
            uri: path.parse().map_err(|_| Error::Protocol)?,
            headers,
        };
        let (bhttp_encoder, head) = Encoder::new(&head)?;
        let (context, mut encoder, decoder) = self.inner.request()?;
        let mut first = vec![VERSION];
        first.extend_from_slice(&context.session_id);
        first.extend_from_slice(&context.request_id);
        first.extend_from_slice(&encoder.seal(&head)?);
        Ok(Request {
            first,
            inner: Some(RequestState {
                encoder,
                decoder,
                bhttp_encoder,
                bhttp_decoder: Decoder::new(Kind::Response),
                pending: Vec::new(),
                offset: 0,
                upload_finished: false,
                response_finished: false,
            }),
        })
    }
}

fn headers(fields: &Array) -> Result<HeaderMap> {
    if fields.length() > 256 {
        return Err(Error::Capacity);
    }
    let mut headers = HeaderMap::new();
    let mut size = 0;
    for field in fields.iter() {
        if !Array::is_array(&field) {
            return Err(Error::Protocol);
        }
        let pair = Array::from(&field);
        if pair.length() != 2 {
            return Err(Error::Protocol);
        }
        let name = pair.get(0).as_string().ok_or(Error::Protocol)?;
        let value = pair.get(1).as_string().ok_or(Error::Protocol)?;
        size += name.len() + value.len();
        if size > bhttp::MAX_HEAD_BYTES {
            return Err(Error::Capacity);
        }
        headers.append(
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| Error::Protocol)?,
            HeaderValue::from_bytes(
                &value
                    .chars()
                    .map(|c| u8::try_from(u32::from(c)).map_err(|_| Error::Protocol))
                    .collect::<Result<Vec<_>>>()?,
            )
            .map_err(|_| Error::Protocol)?,
        );
    }
    Ok(headers)
}

struct RequestState {
    encoder: RecordEncoder,
    decoder: RecordDecoder,
    bhttp_encoder: Encoder,
    bhttp_decoder: Decoder,
    pending: Vec<u8>,
    offset: usize,
    upload_finished: bool,
    response_finished: bool,
}

#[wasm_bindgen]
pub struct Request {
    first: Vec<u8>,
    inner: Option<RequestState>,
}
#[wasm_bindgen]
impl Request {
    /// Single-use prefix plus encrypted bHTTP head. Does not close upload direction.
    pub fn first_bytes(&mut self) -> std::result::Result<Vec<u8>, JsValue> {
        if self.first.is_empty() {
            return Err(Error::Closed.into());
        }
        Ok(std::mem::take(&mut self.first))
    }
    pub fn content(&mut self, content: &[u8]) -> std::result::Result<Vec<u8>, JsValue> {
        let result = self.content_inner(content);
        if result.is_err() {
            self.inner = None;
        }
        Ok(result?)
    }
    fn content_inner(&mut self, content: &[u8]) -> Result<Vec<u8>> {
        let state = self.inner.as_mut().ok_or(Error::Closed)?;
        let bytes = state.bhttp_encoder.content(content)?;
        let mut result = Vec::new();
        for chunk in bytes.chunks(MAX_RECORD_PLAINTEXT) {
            result.extend_from_slice(&state.encoder.seal(chunk)?);
        }
        Ok(result)
    }
    pub fn finish_upload(&mut self) -> std::result::Result<Vec<u8>, JsValue> {
        let result = self.finish_upload_inner();
        if result.is_err() {
            self.inner = None;
        }
        Ok(result?)
    }
    fn finish_upload_inner(&mut self) -> Result<Vec<u8>> {
        let state = self.inner.as_mut().ok_or(Error::Closed)?;
        let mut result = state.encoder.seal(&state.bhttp_encoder.finish()?)?;
        result.extend_from_slice(&state.encoder.finish()?);
        state.upload_finished = true;
        if state.response_finished {
            self.inner = None;
        }
        Ok(result)
    }
    /// Consume at most one encrypted record. Drain next_event before feeding its suffix.
    pub fn receive(&mut self, ciphertext: &[u8]) -> std::result::Result<usize, JsValue> {
        let result = self.receive_inner(ciphertext);
        if result.is_err() {
            self.inner = None;
        }
        Ok(result?)
    }
    fn receive_inner(&mut self, ciphertext: &[u8]) -> Result<usize> {
        let state = self.inner.as_mut().ok_or(Error::Closed)?;
        if !state.pending.is_empty() {
            return Err(Error::Protocol);
        }
        let (consumed, plaintext) = state.decoder.feed(ciphertext)?;
        if let Some(plaintext) = plaintext {
            state.pending = plaintext;
            state.offset = 0;
        }
        Ok(consumed)
    }
    /// FFI events: [0, status, header-pairs], [1, Uint8Array], or [2].
    /// These are process-local values, never a wire HTTP envelope.
    pub fn next_event(&mut self) -> std::result::Result<Option<Array>, JsValue> {
        let result = self.next_event_inner();
        if result.is_err() {
            self.inner = None;
        }
        Ok(result?)
    }
    fn next_event_inner(&mut self) -> Result<Option<Array>> {
        let state = self.inner.as_mut().ok_or(Error::Closed)?;
        while state.offset < state.pending.len() {
            let (consumed, event) = state.bhttp_decoder.feed(&state.pending[state.offset..])?;
            state.offset += consumed;
            if state.offset == state.pending.len() {
                state.pending.clear();
                state.offset = 0;
            }
            if let Some(event) = event {
                let output = Array::new();
                match event {
                    Event::Head(Head::Response { status, headers }) => {
                        output.push(&0.into());
                        output.push(&status.into());
                        let fields = Array::new();
                        for (name, value) in &headers {
                            let pair = Array::new();
                            pair.push(&name.as_str().into());
                            let text: String = value
                                .as_bytes()
                                .iter()
                                .map(|byte| char::from(*byte))
                                .collect();
                            pair.push(&text.into());
                            fields.push(&pair);
                        }
                        output.push(&fields);
                    }
                    Event::Head(_) => return Err(Error::Protocol),
                    Event::Content(bytes) => {
                        output.push(&1.into());
                        output.push(&Uint8Array::from(bytes.as_slice()));
                    }
                    Event::End => {
                        output.push(&2.into());
                    }
                }
                return Ok(Some(output));
            }
        }
        Ok(None)
    }
    pub fn finish_response(&mut self) -> std::result::Result<(), JsValue> {
        let result = self.finish_response_inner();
        if result.is_err() {
            self.inner = None;
        }
        Ok(result?)
    }
    fn finish_response_inner(&mut self) -> Result<()> {
        let state = self.inner.as_mut().ok_or(Error::Closed)?;
        if !state.pending.is_empty() || state.response_finished {
            return Err(Error::Protocol);
        }
        state.decoder.finish()?;
        state.bhttp_decoder.finish()?;
        state.response_finished = true;
        if state.upload_finished {
            self.inner = None;
        }
        Ok(())
    }
}
