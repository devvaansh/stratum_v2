//! Pool Client - Stratum V2 Job Declaration Protocol

pub mod sv2_messages;

use bytes::BytesMut;
use noise_sv2::{Initiator, NoiseCodec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::common::{Event, CoinbaseOut, Sv2Error, Result};
use sv2_messages::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConnConfig {
    pub address: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handshake {
    Init,
    Connected,
    Sent,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclState {
    NeedToken,
    AwaitToken { req: u32 },
    Ready,
    Pending { req: u32 },
    AwaitTx { req: u32 },
}

#[derive(Debug, Clone)]
struct PendingDecl {
    tpl_id: u64,
    req_id: u32,
    txs: Vec<Vec<u8>>,
    txids: Vec<[u8; 32]>,
    #[allow(dead_code)]
    nonce: u64,
    #[allow(dead_code)]
    sent_at: Instant,
}

pub struct PoolClient {
    cfg: PoolConnConfig,
    bus_tx: broadcast::Sender<Event>,
    bus_rx: broadcast::Receiver<Event>,
    hs_state: Handshake,
    decl_state: DeclState,
    token: Option<Vec<u8>>,
    req_seq: u32,
    hash_nonce: u64,
    pending: HashMap<u32, PendingDecl>,
    blk_version: u32,
    blk_height: u64,
    coinbase_val: u64,
}

impl PoolClient {
    pub fn new(
        cfg: PoolConnConfig,
        bus_tx: broadcast::Sender<Event>,
        bus_rx: broadcast::Receiver<Event>,
    ) -> Self {
        Self {
            cfg,
            bus_tx,
            bus_rx,
            hs_state: Handshake::Init,
            decl_state: DeclState::NeedToken,
            token: None,
            req_seq: 0,
            hash_nonce: rand::random(),
            pending: HashMap::new(),
            blk_version: 0x20000000,
            blk_height: 0,
            coinbase_val: 0,
        }
    }

    fn next_req(&mut self) -> u32 {
        self.req_seq = self.req_seq.wrapping_add(1);
        self.req_seq
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Pool client starting");

        loop {
            let addr: SocketAddr = self
                .cfg
                .address
                .parse()
                .map_err(|e| Sv2Error::PoolConnection(format!("bad addr: {}", e)))?;

            let _ = self.bus_tx.send(Event::PoolConnecting);
            info!("Connecting to {}", addr);

            let stream = match TcpStream::connect(addr).await {
                Ok(s) => s,
                Err(e) => {
                    error!("Connect failed: {}", e);
                    let _ = self.bus_tx.send(Event::PoolDown);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            self.hs_state = Handshake::Connected;
            let _ = self.bus_tx.send(Event::PoolUp);
            info!("TCP connected");

            match self.handshake(stream).await {
                Ok((s, codec)) => {
                    info!("Noise handshake done");
                    self.hs_state = Handshake::Done;
                    let _ = self.bus_tx.send(Event::HandshakeDone);

                    self.decl_state = DeclState::NeedToken;
                    self.token = None;

                    if let Err(e) = self.run_protocol(s, codec).await {
                        error!("Protocol error: {}", e);
                        let _ = self.bus_tx.send(Event::Err(e.to_string()));
                    }
                }
                Err(e) => {
                    error!("Handshake failed: {}", e);
                    let _ = self.bus_tx.send(Event::HandshakeErr(e.to_string()));
                    self.hs_state = Handshake::Init;
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    async fn handshake(&mut self, mut stream: TcpStream) -> Result<(TcpStream, NoiseCodec)> {
        let _ = self.bus_tx.send(Event::Handshaking);
        info!("Starting Noise NX");

        let mut init = Initiator::new(None);

        // Step 0: Generate and send ephemeral public key
        let msg0 = init
            .step_0()
            .map_err(|e| Sv2Error::NoiseHandshake(format!("step0: {:?}", e)))?;

        debug!("Sending {} bytes", msg0.len());
        stream
            .write_all(&msg0)
            .await
            .map_err(|e| Sv2Error::NoiseHandshake(format!("send: {}", e)))?;

        self.hs_state = Handshake::Sent;

        // Read responder's message (contains their keys + signature)
        let mut buf = vec![0u8; 1024];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| Sv2Error::NoiseHandshake(format!("recv: {}", e)))?;

        if n == 0 {
            return Err(Sv2Error::NoiseHandshake("closed".into()));
        }

        buf.truncate(n);
        debug!("Received {} bytes", n);

        // Step 2: Process responder message and get codec
        // noise_sv2 expects exactly 234 bytes for the handshake response
        const EXPECTED_LEN: usize = 234;
        if buf.len() < EXPECTED_LEN {
            return Err(Sv2Error::NoiseHandshake(
                format!("response too short: {} < {}", buf.len(), EXPECTED_LEN)
            ));
        }
        
        let mut response: [u8; EXPECTED_LEN] = [0u8; EXPECTED_LEN];
        response.copy_from_slice(&buf[..EXPECTED_LEN]);
        
        let codec = init
            .step_2(response)
            .map_err(|e| Sv2Error::NoiseHandshake(format!("step2: {:?}", e)))?;

        info!("Encrypted channel ready");
        Ok((stream, codec))
    }

    async fn run_protocol(&mut self, stream: TcpStream, mut codec: NoiseCodec) -> Result<()> {
        info!("Running SV2 protocol");

        let (mut rd, mut wr) = stream.into_split();
        let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(32);

        self.request_token(&out_tx, &mut codec, &mut wr).await?;

        let mut buf = BytesMut::with_capacity(65536);

        loop {
            tokio::select! {
                res = rd.read_buf(&mut buf) => {
                    match res {
                        Ok(0) => {
                            error!("Pool closed connection");
                            return Err(Sv2Error::PoolConnection("closed".into()));
                        }
                        Ok(n) => {
                            debug!("Read {} bytes", n);
                            self.process_data(&mut buf, &mut codec, &out_tx).await?;
                        }
                        Err(e) => {
                            error!("Read error: {}", e);
                            return Err(Sv2Error::Io(e));
                        }
                    }
                }

                Some(data) = out_rx.recv() => {
                    let mut enc = data;
                    codec.encrypt(&mut enc)
                        .map_err(|e| Sv2Error::Framing(format!("encrypt: {:?}", e)))?;
                    wr.write_all(&enc).await.map_err(Sv2Error::Io)?;
                    debug!("Wrote {} encrypted bytes", enc.len());
                }

                Ok(ev) = self.bus_rx.recv() => {
                    self.handle_event(ev, &out_tx).await?;
                }
            }
        }
    }

    async fn process_data(
        &mut self,
        buf: &mut BytesMut,
        codec: &mut NoiseCodec,
        out_tx: &mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        const HDR: usize = 6;

        while buf.len() >= HDR {
            let ext = u16::from_le_bytes([buf[0], buf[1]]);
            let mtype = buf[2];
            let mlen = u32::from_le_bytes([buf[3], buf[4], buf[5], 0]) as usize;

            let total = HDR + mlen;
            if buf.len() < total {
                break;
            }

            let frame = buf.split_to(total);
            
            let payload = if mlen > 0 {
                let mut data = frame[HDR..].to_vec();
                codec.decrypt(&mut data)
                    .map_err(|e| Sv2Error::Framing(format!("decrypt: {:?}", e)))?;
                data
            } else {
                Vec::new()
            };

            self.handle_msg(ext, mtype, &payload, out_tx).await?;
        }

        Ok(())
    }

    async fn handle_msg(
        &mut self,
        ext: u16,
        mtype: u8,
        data: &[u8],
        out_tx: &mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        debug!("SV2 msg: ext=0x{:04X}, type=0x{:02X}, len={}", ext, mtype, data.len());

        match mtype {
            msg_types::ALLOC_TOKEN_OK => {
                self.on_token_ok(data).await?;
            }
            msg_types::DECL_JOB_OK => {
                self.on_job_ok(data).await?;
            }
            msg_types::DECL_JOB_ERR => {
                self.on_job_err(data).await?;
            }
            msg_types::IDENTIFY_TXS => {
                self.on_identify_txs(data, out_tx).await?;
            }
            msg_types::PROVIDE_TXS_OK => {
                self.on_txs_ok(data).await?;
            }
            _ => {
                warn!("Unknown msg type: 0x{:02X}", mtype);
            }
        }

        Ok(())
    }

    async fn request_token<W: AsyncWriteExt + Unpin>(
        &mut self,
        _out_tx: &mpsc::Sender<Vec<u8>>,
        codec: &mut NoiseCodec,
        wr: &mut W,
    ) -> Result<()> {
        let rid = self.next_req();
        
        let msg = AllocToken::new(rid, "sv2-jdc", 8);
        let payload = msg.serialize()?;
        let frame = build_frame(msg_types::ALLOC_TOKEN, DECL_EXT, &payload);

        let mut enc = frame;
        codec.encrypt(&mut enc)
            .map_err(|e| Sv2Error::Framing(format!("encrypt: {:?}", e)))?;
        wr.write_all(&enc).await.map_err(Sv2Error::Io)?;

        self.decl_state = DeclState::AwaitToken { req: rid };
        info!("Requested token (req={})", rid);

        Ok(())
    }

    async fn on_token_ok(&mut self, data: &[u8]) -> Result<()> {
        let msg = AllocTokenOk::parse(data)?;
        
        info!("Got token: req={}, len={}, async={}",
            msg.req_id, msg.token.len(), msg.async_ok);

        self.token = Some(msg.token);
        self.decl_state = DeclState::Ready;

        let _ = self.bus_tx.send(Event::PoolUp);
        Ok(())
    }

    async fn on_job_ok(&mut self, data: &[u8]) -> Result<()> {
        let msg = DeclJobOk::parse(data)?;
        
        info!("Job OK: req={}, token_len={}", msg.req_id, msg.new_token.len());

        if !msg.new_token.is_empty() {
            self.token = Some(msg.new_token.clone());
        }

        if let Some(p) = self.pending.remove(&msg.req_id) {
            let _ = self.bus_tx.send(Event::JobOk {
                tpl_id: p.tpl_id,
                token: msg.new_token,
            });
        }

        self.decl_state = DeclState::Ready;
        Ok(())
    }

    async fn on_job_err(&mut self, data: &[u8]) -> Result<()> {
        let msg = DeclJobErr::parse(data)?;
        
        error!("Job failed: req={}, code={:?}, msg={}",
            msg.req_id, msg.code, msg.details);

        if let Some(p) = self.pending.remove(&msg.req_id) {
            let _ = self.bus_tx.send(Event::JobFailed {
                tpl_id: p.tpl_id,
                reason: format!("{:?}: {}", msg.code, msg.details),
            });
        }

        self.decl_state = DeclState::Ready;
        Ok(())
    }

    async fn on_identify_txs(
        &mut self,
        data: &[u8],
        out_tx: &mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        let msg = IdentifyTxs::parse(data)?;
        
        info!("Pool wants {} txs for req={}", msg.positions.len(), msg.req_id);

        let p = self.pending.get(&msg.req_id)
            .ok_or_else(|| Sv2Error::InvalidState(
                format!("no pending for req={}", msg.req_id)
            ))?;

        let mut txs = Vec::new();
        for &pos in &msg.positions {
            if let Some(tx) = p.txs.get(pos as usize) {
                txs.push(tx.clone());
            } else {
                warn!("Invalid tx position {}", pos);
            }
        }

        let resp = ProvideTxs { req_id: msg.req_id, txs: txs.clone() };
        let payload = resp.serialize()?;
        let frame = build_frame(msg_types::PROVIDE_TXS, DECL_EXT, &payload);

        out_tx.send(frame).await.map_err(|_| Sv2Error::ChannelSend)?;

        info!("Sent {} txs", txs.len());
        self.decl_state = DeclState::AwaitTx { req: msg.req_id };
        Ok(())
    }

    async fn on_txs_ok(&mut self, data: &[u8]) -> Result<()> {
        let msg = ProvideTxsOk::parse(data)?;
        info!("Txs accepted for req={}", msg.req_id);
        Ok(())
    }

    async fn handle_event(&mut self, ev: Event, out_tx: &mpsc::Sender<Vec<u8>>) -> Result<()> {
        match ev {
            Event::DeclareJob { tpl_id, outputs, txs } => {
                self.declare_job(tpl_id, outputs, txs, out_tx).await?;
            }

            Event::NewTemplate { height, fees, .. } => {
                self.blk_height = height;
                self.coinbase_val = fees + 312500000;
            }

            Event::Shutdown => {
                info!("Pool client shutting down");
                return Err(Sv2Error::Shutdown);
            }

            _ => {}
        }

        Ok(())
    }

    async fn declare_job(
        &mut self,
        tpl_id: u64,
        outputs: Vec<CoinbaseOut>,
        txs: Vec<Vec<u8>>,
        out_tx: &mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        let tok = match &self.token {
            Some(t) => t.clone(),
            None => {
                warn!("No token, skipping declaration");
                return Ok(());
            }
        };

        if self.decl_state != DeclState::Ready {
            debug!("Not ready: {:?}", self.decl_state);
            return Ok(());
        }

        let rid = self.next_req();
        
        info!("Declaring job: tpl={}, req={}, txs={}", tpl_id, rid, txs.len());

        let txids: Vec<[u8; 32]> = txs.iter().map(|t| calc_txid(t)).collect();
        let nonce = self.hash_nonce;
        let shorts: Vec<u64> = txids.iter().map(|id| calc_short_hash(id, nonce)).collect();
        let hash_list = calc_tx_list_hash(&txs);

        let script = outputs
            .first()
            .map(|o| o.script_pubkey.clone())
            .unwrap_or_else(|| vec![0x6A]);

        let prefix = build_cb_prefix(self.blk_version, self.blk_height, b"sv2-jdc");
        let suffix = build_cb_suffix(self.coinbase_val, &script, None);

        let job = DeclJob {
            req_id: rid,
            token: tok,
            version: self.blk_version,
            cb_prefix: prefix,
            cb_suffix: suffix,
            hash_nonce: nonce,
            short_hashes: shorts,
            tx_list_hash: hash_list,
            extra: Vec::new(),
        };

        let payload = job.serialize()?;
        let frame = build_frame(msg_types::DECL_JOB, DECL_EXT, &payload);

        let tx_count = txs.len();
        self.pending.insert(rid, PendingDecl {
            tpl_id,
            req_id: rid,
            txs,
            txids,
            nonce,
            sent_at: Instant::now(),
        });

        out_tx.send(frame).await.map_err(|_| Sv2Error::ChannelSend)?;

        self.decl_state = DeclState::Pending { req: rid };

        let _ = self.bus_tx.send(Event::JobSent { tpl_id, txs: tx_count });
        info!("Job sent: req={}", rid);
        
        Ok(())
    }
}
