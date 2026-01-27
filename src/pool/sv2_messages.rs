//! Stratum V2 Job Declaration Protocol Messages

use crate::common::{Sv2Error, Result};
use sha2::{Sha256, Digest};

pub mod msg_types {
    pub const ALLOC_TOKEN: u8 = 0x50;
    pub const ALLOC_TOKEN_OK: u8 = 0x51;
    pub const DECL_JOB: u8 = 0x52;
    pub const DECL_JOB_OK: u8 = 0x53;
    pub const DECL_JOB_ERR: u8 = 0x54;
    pub const IDENTIFY_TXS: u8 = 0x55;
    pub const PROVIDE_TXS: u8 = 0x56;
    pub const PROVIDE_TXS_OK: u8 = 0x57;
}

pub const DECL_EXT: u16 = 0x0002;

// ============================================================================
// AllocateMiningJobToken (0x50)
// ============================================================================

#[derive(Debug, Clone)]
pub struct AllocToken {
    pub req_id: u32,
    pub user: String,
    pub min_nonce2: u16,
}

impl AllocToken {
    pub fn new(req_id: u32, user: &str, min_nonce2: u16) -> Self {
        Self { req_id, user: user.into(), min_nonce2 }
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.req_id.to_le_bytes());
        
        let ubytes = self.user.as_bytes();
        if ubytes.len() > 255 {
            return Err(Sv2Error::Serialization("user too long".into()));
        }
        buf.push(ubytes.len() as u8);
        buf.extend_from_slice(ubytes);
        buf.extend_from_slice(&self.min_nonce2.to_le_bytes());
        
        Ok(buf)
    }
}

// ============================================================================
// AllocateMiningJobTokenSuccess (0x51)
// ============================================================================

#[derive(Debug, Clone)]
pub struct AllocTokenOk {
    pub req_id: u32,
    pub token: Vec<u8>,
    pub max_cb_extra: u32,
    pub async_ok: bool,
    pub constraints: Vec<CbConstraint>,
}

impl AllocTokenOk {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(Sv2Error::Serialization("too short".into()));
        }
        
        let mut pos = 0;
        let req_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        pos += 4;
        
        if pos >= data.len() {
            return Err(Sv2Error::Serialization("missing token len".into()));
        }
        let tlen = data[pos] as usize;
        pos += 1;
        
        if pos + tlen > data.len() {
            return Err(Sv2Error::Serialization("truncated token".into()));
        }
        let token = data[pos..pos + tlen].to_vec();
        pos += tlen;
        
        if pos + 4 > data.len() {
            return Err(Sv2Error::Serialization("missing max_cb_extra".into()));
        }
        let max_cb_extra = u32::from_le_bytes([
            data[pos], data[pos + 1], data[pos + 2], data[pos + 3]
        ]);
        pos += 4;
        
        if pos >= data.len() {
            return Err(Sv2Error::Serialization("missing async flag".into()));
        }
        let async_ok = data[pos] != 0;
        
        Ok(Self {
            req_id,
            token,
            max_cb_extra,
            async_ok,
            constraints: Vec::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CbConstraint {
    pub script: Vec<u8>,
}

// ============================================================================
// DeclareMiningJob (0x52)
// ============================================================================

#[derive(Debug, Clone)]
pub struct DeclJob {
    pub req_id: u32,
    pub token: Vec<u8>,
    pub version: u32,
    pub cb_prefix: Vec<u8>,
    pub cb_suffix: Vec<u8>,
    pub hash_nonce: u64,
    pub short_hashes: Vec<u64>,
    pub tx_list_hash: [u8; 32],
    pub extra: Vec<u8>,
}

impl DeclJob {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        
        buf.extend_from_slice(&self.req_id.to_le_bytes());
        
        if self.token.len() > 255 {
            return Err(Sv2Error::Serialization("token too long".into()));
        }
        buf.push(self.token.len() as u8);
        buf.extend_from_slice(&self.token);
        
        buf.extend_from_slice(&self.version.to_le_bytes());
        
        let plen = self.cb_prefix.len() as u16;
        buf.extend_from_slice(&plen.to_le_bytes());
        buf.extend_from_slice(&self.cb_prefix);
        
        let slen = self.cb_suffix.len() as u16;
        buf.extend_from_slice(&slen.to_le_bytes());
        buf.extend_from_slice(&self.cb_suffix);
        
        buf.extend_from_slice(&self.hash_nonce.to_le_bytes());
        
        let count = self.short_hashes.len() as u16;
        buf.extend_from_slice(&count.to_le_bytes());
        for h in &self.short_hashes {
            buf.extend_from_slice(&h.to_le_bytes());
        }
        
        buf.extend_from_slice(&self.tx_list_hash);
        
        let elen = self.extra.len() as u16;
        buf.extend_from_slice(&elen.to_le_bytes());
        buf.extend_from_slice(&self.extra);
        
        Ok(buf)
    }
}

// ============================================================================
// DeclareMiningJobSuccess (0x53)
// ============================================================================

#[derive(Debug, Clone)]
pub struct DeclJobOk {
    pub req_id: u32,
    pub new_token: Vec<u8>,
}

impl DeclJobOk {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(Sv2Error::Serialization("too short".into()));
        }
        
        let req_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        
        let tlen = data.get(4).copied().unwrap_or(0) as usize;
        let new_token = if tlen > 0 && data.len() > 5 + tlen {
            data[5..5 + tlen].to_vec()
        } else {
            Vec::new()
        };
        
        Ok(Self { req_id, new_token })
    }
}

// ============================================================================
// DeclareMiningJobError (0x54)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclErrCode {
    BadToken = 0x01,
    BadParams = 0x02,
    Stale = 0x03,
    Unknown = 0xFF,
}

impl From<u8> for DeclErrCode {
    fn from(v: u8) -> Self {
        match v {
            0x01 => Self::BadToken,
            0x02 => Self::BadParams,
            0x03 => Self::Stale,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeclJobErr {
    pub req_id: u32,
    pub code: DeclErrCode,
    pub details: String,
}

impl DeclJobErr {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            return Err(Sv2Error::Serialization("too short".into()));
        }
        
        let req_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let code = DeclErrCode::from(data[4]);
        
        let dlen = data.get(5).copied().unwrap_or(0) as usize;
        let details = if dlen > 0 && data.len() > 6 + dlen {
            String::from_utf8_lossy(&data[6..6 + dlen]).into()
        } else {
            String::new()
        };
        
        Ok(Self { req_id, code, details })
    }
}

// ============================================================================
// IdentifyTransactions (0x55)
// ============================================================================

#[derive(Debug, Clone)]
pub struct IdentifyTxs {
    pub req_id: u32,
    pub positions: Vec<u16>,
}

impl IdentifyTxs {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(Sv2Error::Serialization("too short".into()));
        }
        
        let req_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        
        let mut positions = Vec::new();
        if data.len() > 6 {
            let cnt = u16::from_le_bytes([data[4], data[5]]) as usize;
            let mut pos = 6;
            for _ in 0..cnt {
                if pos + 2 <= data.len() {
                    positions.push(u16::from_le_bytes([data[pos], data[pos + 1]]));
                    pos += 2;
                }
            }
        }
        
        Ok(Self { req_id, positions })
    }
}

// ============================================================================
// ProvideMissingTransactions (0x56)
// ============================================================================

#[derive(Debug, Clone)]
pub struct ProvideTxs {
    pub req_id: u32,
    pub txs: Vec<Vec<u8>>,
}

impl ProvideTxs {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        
        buf.extend_from_slice(&self.req_id.to_le_bytes());
        
        let cnt = self.txs.len() as u16;
        buf.extend_from_slice(&cnt.to_le_bytes());
        
        for tx in &self.txs {
            let len = tx.len() as u32;
            buf.push((len & 0xFF) as u8);
            buf.push(((len >> 8) & 0xFF) as u8);
            buf.push(((len >> 16) & 0xFF) as u8);
            buf.extend_from_slice(tx);
        }
        
        Ok(buf)
    }
}

// ============================================================================
// ProvideMissingTransactionsSuccess (0x57)
// ============================================================================

#[derive(Debug, Clone)]
pub struct ProvideTxsOk {
    pub req_id: u32,
}

impl ProvideTxsOk {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(Sv2Error::Serialization("too short".into()));
        }
        Ok(Self {
            req_id: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        })
    }
}

// ============================================================================
// Frame Builder
// ============================================================================

pub fn build_frame(mtype: u8, ext: u16, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&ext.to_le_bytes());
    frame.push(mtype);
    
    let len = payload.len() as u32;
    frame.push((len & 0xFF) as u8);
    frame.push(((len >> 8) & 0xFF) as u8);
    frame.push(((len >> 16) & 0xFF) as u8);
    
    frame.extend_from_slice(payload);
    frame
}

// ============================================================================
// Crypto utilities
// ============================================================================

pub fn calc_short_hash(txid: &[u8; 32], nonce: u64) -> u64 {
    let mut h = Sha256::new();
    h.update(&nonce.to_le_bytes());
    h.update(txid);
    let out = h.finalize();
    
    u64::from_le_bytes([
        out[0], out[1], out[2], out[3],
        out[4], out[5], out[6], out[7],
    ])
}

pub fn calc_txid(raw: &[u8]) -> [u8; 32] {
    let h1 = Sha256::digest(raw);
    let h2 = Sha256::digest(&h1);
    
    let mut id = [0u8; 32];
    id.copy_from_slice(&h2);
    id.reverse();
    id
}

pub fn calc_tx_list_hash(txs: &[Vec<u8>]) -> [u8; 32] {
    let mut h = Sha256::new();
    
    for tx in txs {
        h.update(&calc_txid(tx));
    }
    
    let h1 = h.finalize();
    let h2 = Sha256::digest(&h1);
    
    let mut out = [0u8; 32];
    out.copy_from_slice(&h2);
    out
}

// ============================================================================
// Coinbase builder
// ============================================================================

pub fn build_cb_prefix(ver: u32, height: u64, tag: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    
    buf.extend_from_slice(&ver.to_le_bytes());
    buf.push(0x00); // segwit marker
    buf.push(0x01); // segwit flag
    buf.push(0x01); // input count
    
    buf.extend_from_slice(&[0u8; 32]); // null prevout
    buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // prevout index
    
    let hscript = encode_height(height);
    let slen = hscript.len() + tag.len();
    
    if slen < 0xFD {
        buf.push(slen as u8);
    } else {
        buf.push(0xFD);
        buf.extend_from_slice(&(slen as u16).to_le_bytes());
    }
    
    buf.extend_from_slice(&hscript);
    buf.extend_from_slice(tag);
    
    buf
}

pub fn build_cb_suffix(value: u64, script: &[u8], witness: Option<&[u8; 32]>) -> Vec<u8> {
    let mut buf = Vec::new();
    
    buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // sequence
    
    let outs = if witness.is_some() { 2 } else { 1 };
    buf.push(outs);
    
    buf.extend_from_slice(&value.to_le_bytes());
    
    if script.len() < 0xFD {
        buf.push(script.len() as u8);
    } else {
        buf.push(0xFD);
        buf.extend_from_slice(&(script.len() as u16).to_le_bytes());
    }
    buf.extend_from_slice(script);
    
    if let Some(w) = witness {
        buf.extend_from_slice(&0u64.to_le_bytes());
        let wscript = witness_script(w);
        buf.push(wscript.len() as u8);
        buf.extend_from_slice(&wscript);
    }
    
    buf.push(0x01); // witness stack count
    buf.push(0x20); // 32 bytes
    buf.extend_from_slice(&[0u8; 32]); // witness nonce
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // locktime
    
    buf
}

fn encode_height(h: u64) -> Vec<u8> {
    let mut out = Vec::new();
    
    if h == 0 {
        out.push(0x00);
    } else if h <= 0x7F {
        out.push(0x01);
        out.push(h as u8);
    } else if h <= 0x7FFF {
        out.push(0x02);
        out.extend_from_slice(&(h as u16).to_le_bytes());
    } else if h <= 0x7FFFFF {
        out.push(0x03);
        out.push((h & 0xFF) as u8);
        out.push(((h >> 8) & 0xFF) as u8);
        out.push(((h >> 16) & 0xFF) as u8);
    } else {
        out.push(0x04);
        out.extend_from_slice(&(h as u32).to_le_bytes());
    }
    
    out
}

fn witness_script(commitment: &[u8; 32]) -> Vec<u8> {
    let mut s = Vec::new();
    s.push(0x6A); // OP_RETURN
    s.push(0x24); // push 36
    s.extend_from_slice(&[0xAA, 0x21, 0xA9, 0xED]);
    s.extend_from_slice(commitment);
    s
}

// ============================================================================
// Merkle tree
// ============================================================================

pub fn merkle_root(txids: &[[u8; 32]]) -> [u8; 32] {
    if txids.is_empty() {
        return [0u8; 32];
    }
    if txids.len() == 1 {
        return txids[0];
    }
    
    let mut level: Vec<[u8; 32]> = txids.to_vec();
    
    while level.len() > 1 {
        let mut next = Vec::new();
        
        for i in (0..level.len()).step_by(2) {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { left };
            next.push(merkle_pair(&left, &right));
        }
        
        level = next;
    }
    
    level[0]
}

fn merkle_pair(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut cat = Vec::with_capacity(64);
    cat.extend_from_slice(a);
    cat.extend_from_slice(b);
    
    let h1 = Sha256::digest(&cat);
    let h2 = Sha256::digest(&h1);
    
    let mut out = [0u8; 32];
    out.copy_from_slice(&h2);
    out
}

pub fn witness_commitment(nonce: &[u8; 32], root: &[u8; 32]) -> [u8; 32] {
    let mut cat = Vec::with_capacity(64);
    cat.extend_from_slice(root);
    cat.extend_from_slice(nonce);
    
    let h1 = Sha256::digest(&cat);
    let h2 = Sha256::digest(&h1);
    
    let mut out = [0u8; 32];
    out.copy_from_slice(&h2);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_alloc_token_serialize() {
        let msg = AllocToken::new(1, "miner", 8);
        let buf = msg.serialize().unwrap();
        assert_eq!(&buf[0..4], &1u32.to_le_bytes());
        assert_eq!(buf[4], 5);
    }
    
    #[test]
    fn test_frame_builder() {
        let payload = vec![0x01, 0x02, 0x03];
        let frame = build_frame(0x50, 0x0002, &payload);
        
        assert_eq!(&frame[0..2], &0x0002u16.to_le_bytes());
        assert_eq!(frame[2], 0x50);
        assert_eq!(frame[3], 3);
        assert_eq!(&frame[6..], &payload);
    }
    
    #[test]
    fn test_height_encoding() {
        assert_eq!(encode_height(0), vec![0x00]);
        assert_eq!(encode_height(1), vec![0x01, 0x01]);
        assert_eq!(encode_height(127), vec![0x01, 0x7F]);
        assert_eq!(encode_height(256), vec![0x02, 0x00, 0x01]);
    }
}
