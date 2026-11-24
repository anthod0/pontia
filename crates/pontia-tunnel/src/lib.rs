mod client;
pub mod protocol;
mod transport;
pub mod ws_frame;
pub use ws_frame::WsTunnelFrame;

pub use client::RemoteClient;
pub use transport::{
    DeviceRequestHandler, TunnelConnection, TunnelError, TunnelRuntime, connect_edge, serve_device,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Protocol(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
