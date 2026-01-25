mod common;
mod node;
mod pool;
mod ui;

use common::{AppMessage, CoinbaseOutput, JdcError, Result};
use node::{NodeActor, NodeConfig};
use pool::{PoolActor, PoolConfig};
use ui::UiActor;

use config::Config;
use serde::Deserialize;
use tokio::sync::broadcast;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Application configuration
#[derive(Debug, Deserialize)]
struct AppConfig {
    bitcoin_node: NodeConfig,
    pool: PoolConfig,
    jdc: JdcConfig,
    logging: LoggingConfig,
}

#[derive(Debug, Deserialize)]
struct JdcConfig {
    coinbase_outputs: Vec<CoinbaseOutputConfig>,
    min_fee_rate: f64,
    max_template_size: usize,
}

#[derive(Debug, Deserialize)]
struct CoinbaseOutputConfig {
    value: u64,
    script_pubkey: String,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = load_config()?;

    // Initialize logging
    init_logging(&config.logging)?;

    info!("Starting Stratum V2 Job Declarator Client");
    info!("Configuration loaded successfully");

    // Parse coinbase outputs
    let coinbase_outputs = parse_coinbase_outputs(&config.jdc.coinbase_outputs)?;

    // Create message passing channels
    // Using broadcast channel for fanout pattern (one-to-many)
    let (tx, _) = broadcast::channel::<AppMessage>(100);

    // Spawn Node Actor
    let node_actor = NodeActor::new(
        config.bitcoin_node.clone(),
        tx.clone(),
        coinbase_outputs.clone(),
    );
    let node_handle = tokio::spawn(async move {
        if let Err(e) = node_actor.run().await {
            error!("Node actor error: {}", e);
        }
    });

    // Spawn Pool Actor
    let pool_actor = PoolActor::new(
        config.pool.clone(),
        tx.clone(),
        tx.subscribe(),
    );
    let pool_handle = tokio::spawn(async move {
        if let Err(e) = pool_actor.run().await {
            error!("Pool actor error: {}", e);
        }
    });

    // Spawn UI Actor (runs in main thread for terminal control)
    let ui_actor = UiActor::new(tx.subscribe());
    let ui_result = ui_actor.run().await;

    // When UI exits (user presses 'q'), shutdown other actors
    info!("Shutting down...");
    let _ = tx.send(AppMessage::Shutdown);

    // Wait for actors to finish with timeout
    let shutdown_timeout = tokio::time::Duration::from_secs(5);
    tokio::select! {
        _ = node_handle => info!("Node actor terminated"),
        _ = pool_handle => info!("Pool actor terminated"),
        _ = tokio::time::sleep(shutdown_timeout) => {
            error!("Shutdown timeout - forcing exit");
        }
    }

    info!("Shutdown complete");
    ui_result
}

/// Load configuration from file
fn load_config() -> Result<AppConfig> {
    let config = Config::builder()
        .add_source(config::File::with_name("config"))
        .add_source(config::File::with_name("config.local").required(false))
        .add_source(config::Environment::with_prefix("JDC"))
        .build()?;

    let app_config: AppConfig = config.try_deserialize()?;
    Ok(app_config)
}

/// Initialize tracing/logging
fn init_logging(config: &LoggingConfig) -> Result<()> {
    let log_level = config.level.parse::<tracing::Level>()
        .map_err(|e| JdcError::Config(
            config::ConfigError::Message(format!("Invalid log level: {}", e))
        ))?;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    format!("stratum_v2_jdc={}", log_level).into()
                })
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

/// Parse coinbase outputs from configuration
fn parse_coinbase_outputs(
    configs: &[CoinbaseOutputConfig],
) -> Result<Vec<CoinbaseOutput>> {
    configs
        .iter()
        .map(|c| {
            let script_pubkey = hex::decode(&c.script_pubkey)
                .map_err(|e| JdcError::Config(
                    config::ConfigError::Message(format!("Invalid script_pubkey hex: {}", e))
                ))?;
            
            Ok(CoinbaseOutput {
                value: c.value,
                script_pubkey,
            })
        })
        .collect()
}
