use thiserror::Error;

#[derive(Error, Debug)]
pub enum Sv2Error {
    #[error("RPC: {0}")]
    BitcoinRpc(#[from] bitcoincore_rpc::Error),

    #[error("Pool: {0}")]
    PoolConnection(String),

    #[error("Noise: {0}")]
    NoiseHandshake(String),

    #[error("Frame: {0}")]
    Framing(String),

    #[error("Codec: {0}")]
    Codec(String),

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Send failed")]
    ChannelSend,

    #[error("Recv failed")]
    ChannelRecv,

    #[error("Bad state: {0}")]
    InvalidState(String),

    #[error("Template: {0}")]
    TemplateBuilding(String),

    #[error("Serialize: {0}")]
    Serialization(String),

    #[error("Shutdown")]
    Shutdown,
}

pub type Result<T> = std::result::Result<T, Sv2Error>;
