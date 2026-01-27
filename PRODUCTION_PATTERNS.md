# Production Patterns & Best Practices

## Rust Idioms Used Throughout

### 1. Error Handling with thiserror

**Pattern:**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JdcError {
    #[error("Bitcoin RPC error: {0}")]
    BitcoinRpc(#[from] bitcoincore_rpc::Error),
    
    #[error("Pool connection error: {0}")]
    PoolConnection(String),
}
```

**Why:**
- Automatic `From` implementations via `#[from]`
- Display trait auto-derived from `#[error]` attribute
- Type-safe error propagation with `?` operator
- Zero runtime cost

**Anti-pattern to avoid:**
```rust
// ❌ String errors lose type information
fn bad() -> Result<(), String> {
    some_operation().map_err(|e| e.to_string())?
}

// ✅ Typed errors preserve context
fn good() -> Result<()> {
    some_operation().map_err(JdcError::BitcoinRpc)?
}
```

### 2. Actor Pattern with Tokio

**Structure:**
```rust
pub struct NodeActor {
    config: NodeConfig,
    tx: broadcast::Sender<AppMessage>,
    // ... state
}

impl NodeActor {
    pub async fn run(mut self) -> Result<()> {
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.handle_tick().await?;
                }
            }
        }
    }
}
```

**Benefits:**
- No shared mutable state
- Message passing ensures thread safety
- Each actor owns its data
- Graceful shutdown via message

**Why broadcast channel:**
```rust
let (tx, _) = broadcast::channel::<AppMessage>(100);

// Multiple subscribers
let rx1 = tx.subscribe();  // UI actor
let rx2 = tx.subscribe();  // Pool actor
let rx3 = tx.subscribe();  // Logging actor

// One sender, many receivers (fanout)
```

### 3. Zero-Unwrap Philosophy

**Every Result is handled:**

```rust
// ❌ Production code should NEVER do this
let client = Client::new(url, auth).unwrap();
let value = map.get("key").unwrap();
let parsed = str::parse().unwrap();

// ✅ Always handle errors explicitly
let client = Client::new(url, auth)
    .map_err(|e| JdcError::BitcoinRpc(e))?;

let value = map.get("key")
    .ok_or(JdcError::Config("Missing key".into()))?;

let parsed = str::parse()
    .map_err(|e| JdcError::Serialization(e.to_string()))?;
```

**Why it matters:**
- `unwrap()` panics crash the entire process
- In production, all failure paths must be handled
- Error types enable recovery strategies
- Monitoring systems can categorize errors

### 4. Trait-Based Abstractions

**Example: Extensible message handlers**

```rust
#[async_trait]
trait MessageHandler {
    async fn handle(&mut self, msg: AppMessage) -> Result<()>;
}

// Different implementations for different contexts
struct UiHandler { /* ... */ }
struct LogHandler { /* ... */ }
struct MetricsHandler { /* ... */ }

#[async_trait]
impl MessageHandler for UiHandler {
    async fn handle(&mut self, msg: AppMessage) -> Result<()> {
        // UI-specific handling
    }
}
```

**Benefits:**
- Polymorphism without inheritance
- Zero runtime cost (monomorphization)
- Easy to test with mock implementations

### 5. Ownership & Borrowing for Safety

**Actor ownership pattern:**

```rust
// Actor owns its state
pub struct NodeActor {
    client: Option<Client>,  // Owned
    config: NodeConfig,       // Owned
    // ...
}

impl NodeActor {
    pub async fn run(mut self) -> Result<()> {
        // Takes ownership, consumes self
        // Can't be used after .run() returns
    }
}

// Spawning moves ownership to the task
let actor = NodeActor::new(config, tx);
tokio::spawn(async move {
    actor.run().await  // 'actor' moved into task
});
```

**Why:**
- Compiler proves no data races at compile time
- No locks needed - actor exclusively owns data
- Can't accidentally access after shutdown

### 6. Builder Pattern for Complex Config

**Example:**
```rust
impl NodeActor {
    pub fn new(
        config: NodeConfig,
        tx: broadcast::Sender<AppMessage>,
        coinbase_outputs: Vec<CoinbaseOutput>,
    ) -> Self {
        Self {
            config,
            client: None,
            tx,
            coinbase_outputs,
            last_block_height: 0,
            template_id_counter: 0,
        }
    }
}
```

**Could be extended to:**
```rust
pub struct NodeActorBuilder {
    config: Option<NodeConfig>,
    tx: Option<broadcast::Sender<AppMessage>>,
    // ...
}

impl NodeActorBuilder {
    pub fn config(mut self, config: NodeConfig) -> Self {
        self.config = Some(config);
        self
    }
    
    pub fn build(self) -> Result<NodeActor> {
        Ok(NodeActor {
            config: self.config.ok_or(JdcError::Config("..."))?,
            // ...
        })
    }
}

// Usage
let actor = NodeActorBuilder::new()
    .config(config)
    .sender(tx)
    .build()?;
```

### 7. Type State Pattern

**Encode state in types:**

```rust
// Different types for different states
struct Disconnected;
struct Connected;
struct Authenticated;

struct PoolConnection<State> {
    addr: SocketAddr,
    _state: PhantomData<State>,
}

impl PoolConnection<Disconnected> {
    async fn connect(self) -> Result<PoolConnection<Connected>> {
        // Can only call connect on Disconnected
    }
}

impl PoolConnection<Connected> {
    async fn authenticate(self) -> Result<PoolConnection<Authenticated>> {
        // Can only authenticate after connected
    }
}

impl PoolConnection<Authenticated> {
    async fn send_job(&mut self) -> Result<()> {
        // Can only send jobs when authenticated
    }
}
```

**Benefit:** **Impossible states are unrepresentable**
- Can't call `send_job()` before authentication
- Compiler enforces correct state transitions

### 8. RAII (Resource Acquisition Is Initialization)

**Terminal cleanup example:**

```rust
pub async fn run(mut self) -> Result<()> {
    // Acquire resource
    crossterm::terminal::enable_raw_mode()?;
    
    let result = self.run_loop(&mut terminal).await;
    
    // Cleanup ALWAYS happens, even on error
    crossterm::terminal::disable_raw_mode()?;
    
    result  // Return original result
}
```

**Better with Drop:**
```rust
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

pub async fn run(mut self) -> Result<()> {
    let _guard = {
        crossterm::terminal::enable_raw_mode()?;
        TerminalGuard
    };
    
    // Guard automatically cleans up when scope exits
    self.run_loop(&mut terminal).await
}
```

### 9. Interior Mutability When Needed

**Example: Shared metrics:**
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone)]
struct Metrics {
    jobs_sent: Arc<AtomicU64>,
    jobs_accepted: Arc<AtomicU64>,
}

impl Metrics {
    fn increment_sent(&self) {
        self.jobs_sent.fetch_add(1, Ordering::Relaxed);
    }
}

// Can be cloned and shared across actors
let metrics = Metrics::new();
let m1 = metrics.clone();  // To actor 1
let m2 = metrics.clone();  // To actor 2
```

**When to use:**
- Shared read-only data: `Arc<T>`
- Shared counters: `Arc<AtomicU64>`
- Shared mutable state: `Arc<Mutex<T>>` or `Arc<RwLock<T>>`

**Prefer message passing over shared state!**

### 10. Testing Strategies

**Unit tests with mock implementations:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    struct MockRpcClient {
        responses: Vec<BlockTemplate>,
    }
    
    impl RpcClient for MockRpcClient {
        fn get_block_template(&self) -> Result<BlockTemplate> {
            Ok(self.responses[0].clone())
        }
    }
    
    #[tokio::test]
    async fn test_template_polling() {
        let mock = MockRpcClient { /* ... */ };
        let actor = NodeActor::with_client(mock);
        // Test without real Bitcoin node
    }
}
```

**Integration tests:**
```rust
// tests/integration_test.rs
#[tokio::test]
async fn test_full_handshake() {
    let pool = start_mock_pool().await;
    let actor = PoolActor::new(/* ... */);
    
    let result = actor.connect_and_handshake().await;
    assert!(result.is_ok());
}
```

## Performance Considerations

### 1. Async vs Sync

**Use async for I/O:**
```rust
// ✅ Network, disk, timers
async fn poll_template() -> Result<BlockTemplate> {
    let response = client.get_block_template().await?;
    Ok(response)
}
```

**Use sync for CPU-bound:**
```rust
// ✅ Heavy computation
fn calculate_merkle_root(txs: &[Transaction]) -> Hash {
    // Synchronous computation
}

// Or spawn blocking
let hash = tokio::task::spawn_blocking(|| {
    expensive_computation()
}).await?;
```

### 2. Zero-Copy Optimizations

**Use `bytes::BytesMut` for buffers:**
```rust
use bytes::BytesMut;

let mut buf = BytesMut::with_capacity(4096);
reader.read_buf(&mut buf).await?;

// Split without copying
let header = buf.split_to(6);
let payload = buf;  // Remaining bytes
```

### 3. Allocation Reduction

**Reuse buffers:**
```rust
struct PoolActor {
    read_buffer: BytesMut,  // Reused across reads
}

async fn read_frame(&mut self) -> Result<Frame> {
    self.read_buffer.clear();  // Don't allocate new
    self.stream.read_buf(&mut self.read_buffer).await?;
    // ...
}
```

## Security Best Practices

### 1. Input Validation

```rust
fn parse_coinbase_outputs(configs: &[Config]) -> Result<Vec<Output>> {
    configs.iter().map(|c| {
        // Validate before using
        if c.value > MAX_COINBASE_VALUE {
            return Err(JdcError::Config("Invalid value".into()));
        }
        
        let script = hex::decode(&c.script_pubkey)
            .map_err(|e| JdcError::Config(format!("Invalid hex: {}", e)))?;
            
        if script.len() > MAX_SCRIPT_SIZE {
            return Err(JdcError::Config("Script too large".into()));
        }
        
        Ok(Output { value: c.value, script })
    }).collect()
}
```

### 2. Secrets Management

```rust
// ❌ Don't log secrets
info!("Connecting with password: {}", password);

// ✅ Redact sensitive data
info!("Connecting with password: [REDACTED]");

// ✅ Use secure string types
use secrecy::{Secret, ExposeSecret};

struct Config {
    rpc_password: Secret<String>,
}

// Only expose when absolutely necessary
let auth = Auth::UserPass(user, config.rpc_password.expose_secret());
```

### 3. Constant-Time Operations

```rust
// For comparing authentication tokens
use subtle::ConstantTimeEq;

fn verify_token(expected: &[u8], received: &[u8]) -> bool {
    expected.ct_eq(received).into()
}
```

## Monitoring & Observability

### Structured Logging

```rust
use tracing::{info, warn, error, instrument};

#[instrument(skip(self), fields(template_id))]
async fn process_template(&mut self, template_id: u64) -> Result<()> {
    info!(tx_count = template.transactions.len(), "Processing template");
    // Automatically includes template_id in log context
    
    if let Err(e) = self.send_job().await {
        error!(error = %e, "Failed to send job");
        return Err(e);
    }
    
    Ok(())
}
```

**Logs will show:**
```
INFO process_template{template_id=123} tx_count=2500 Processing template
ERROR process_template{template_id=123} error="Connection closed" Failed to send job
```

## Documentation Standards

```rust
/// Performs Noise NX handshake with the pool.
///
/// # State Transitions
/// 
/// `Connected` -> `InitiatorSent` -> `Complete`
///
/// # Errors
///
/// Returns `JdcError::NoiseHandshake` if:
/// - DH operation fails
/// - AEAD verification fails
/// - Invalid message format
///
/// # Example
///
/// ```no_run
/// let stream = TcpStream::connect(addr).await?;
/// let codec = perform_handshake(stream).await?;
/// ```
async fn perform_handshake(&mut self, stream: TcpStream) 
    -> Result<(TcpStream, NoiseCodec)>
{
    // Implementation
}
```

---

These patterns ensure the codebase is:
- **Maintainable**: Clear ownership and error handling
- **Safe**: Compile-time guarantees prevent data races
- **Performant**: Zero-copy, minimal allocations
- **Testable**: Trait-based abstractions enable mocking
- **Observable**: Structured logging aids debugging
