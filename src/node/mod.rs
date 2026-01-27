use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time;
use tracing::{debug, error, info, warn};

use crate::common::{Event, CoinbaseOut, Sv2Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinRpcConfig {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
    pub poll_interval: u64,
    pub min_fee_rate: f64,
}

pub struct BitcoinNode {
    cfg: BitcoinRpcConfig,
    rpc: Option<Client>,
    bus: broadcast::Sender<Event>,
    outputs: Vec<CoinbaseOut>,
    last_height: u64,
    tpl_seq: u64,
}

impl BitcoinNode {
    pub fn new(
        cfg: BitcoinRpcConfig,
        bus: broadcast::Sender<Event>,
        outputs: Vec<CoinbaseOut>,
    ) -> Self {
        Self {
            cfg,
            rpc: None,
            bus,
            outputs,
            last_height: 0,
            tpl_seq: 0,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Starting Bitcoin RPC handler");

        if let Err(e) = self.connect() {
            error!("RPC connect failed: {}", e);
            let _ = self.bus.send(Event::NodeDown);
            return Err(e);
        }

        let _ = self.bus.send(Event::NodeUp);
        info!("Connected to {}", self.cfg.rpc_url);

        let mut ticker = time::interval(Duration::from_secs(self.cfg.poll_interval));

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.poll_template().await {
                        error!("Template poll error: {}", e);
                        let _ = self.bus.send(Event::TemplateErr(e.to_string()));
                    }
                }
            }
        }
    }

    fn connect(&mut self) -> Result<()> {
        let auth = Auth::UserPass(
            self.cfg.rpc_user.clone(),
            self.cfg.rpc_password.clone(),
        );

        let client = Client::new(&self.cfg.rpc_url, auth)?;
        let chain = client.get_blockchain_info()?;
        
        info!("Chain: {}, height: {}", chain.chain, chain.blocks);

        self.last_height = chain.blocks;
        self.rpc = Some(client);
        Ok(())
    }

    async fn poll_template(&mut self) -> Result<()> {
        let client = self.rpc.as_ref().ok_or_else(|| {
            Sv2Error::PoolConnection("RPC not ready".into())
        })?;

        let chain = client.get_blockchain_info()?;
        let h = chain.blocks;

        if h > self.last_height {
            info!("New block at height {}", h);
            self.last_height = h;
        }

        let tpl = match self.fetch_template() {
            Ok(t) => t,
            Err(e) => {
                warn!("Template fetch failed: {}", e);
                return Err(e);
            }
        };

        debug!("Template: height={}, txs={}", tpl.height, tpl.txs.len());

        let fees: u64 = tpl.txs.iter().filter_map(|tx| tx.fee).sum();

        let _ = self.bus.send(Event::NewTemplate {
            height: tpl.height,
            txs: tpl.txs.len(),
            fees,
        });

        self.tpl_seq += 1;
        let tpl_id = self.tpl_seq;

        let raw_txs: Vec<Vec<u8>> = tpl
            .txs
            .iter()
            .filter_map(|tx| hex::decode(&tx.data).ok())
            .collect();

        let _ = self.bus.send(Event::DeclareJob {
            tpl_id,
            outputs: self.outputs.clone(),
            txs: raw_txs,
        });

        Ok(())
    }

    fn fetch_template(&self) -> Result<Template> {
        let client = self.rpc.as_ref().ok_or_else(|| {
            Sv2Error::PoolConnection("RPC not ready".into())
        })?;

        let raw: serde_json::Value = client.call(
            "getblocktemplate",
            &[serde_json::json!({ "rules": ["segwit"] })],
        )?;

        let tpl: Template = serde_json::from_value(raw)
            .map_err(|e| Sv2Error::Serialization(e.to_string()))?;

        Ok(tpl)
    }
}

#[derive(Debug, Deserialize)]
struct Template {
    version: u32,
    #[serde(rename = "previousblockhash")]
    prev_hash: String,
    #[serde(rename = "transactions")]
    txs: Vec<TxEntry>,
    #[serde(rename = "coinbasevalue")]
    coinbase_val: u64,
    target: String,
    #[serde(rename = "mintime")]
    min_time: u64,
    #[serde(rename = "curtime")]
    cur_time: u64,
    bits: String,
    height: u64,
}

#[derive(Debug, Deserialize)]
struct TxEntry {
    data: String,
    txid: String,
    hash: String,
    fee: Option<u64>,
    #[serde(default)]
    depends: Vec<usize>,
    weight: u64,
}
