use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub enum Event {
    NodeUp,
    NodeDown,
    NewTemplate {
        height: u64,
        txs: usize,
        fees: u64,
    },
    TemplateErr(String),

    PoolConnecting,
    PoolUp,
    PoolDown,
    Handshaking,
    HandshakeDone,
    HandshakeErr(String),
    
    JobSent {
        tpl_id: u64,
        txs: usize,
    },
    JobOk {
        tpl_id: u64,
        token: Vec<u8>,
    },
    JobFailed {
        tpl_id: u64,
        reason: String,
    },

    DeclareJob {
        tpl_id: u64,
        outputs: Vec<CoinbaseOut>,
        txs: Vec<Vec<u8>>,
    },

    Shutdown,
    Err(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinbaseOut {
    pub value: u64,
    pub script_pubkey: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum JobState {
    Pending,
    Sent { ts: SystemTime },
    Accepted { token: Vec<u8> },
    Rejected { reason: String },
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub node_up: bool,
    pub pool_up: bool,
    pub handshake_ok: bool,
    pub height: u64,
    pub templates: u64,
    pub declared: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub fees: u64,
    pub uptime: u64,
}
