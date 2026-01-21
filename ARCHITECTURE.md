# Stratum V2 Job Declarator Client (JDC)

A production-grade implementation of a Stratum V2 Job Declarator Client in Rust, enabling individual miners to select their own transactions from a local Bitcoin node and negotiate mining jobs with pools using the SV2 Job Declaration Protocol.

## Architecture Overview

### Modular Design

The project follows a clean, actor-based architecture with complete separation of concerns:

```
src/
├── main.rs           # Application entry point & orchestration
├── common/          # Shared types and utilities
│   ├── mod.rs
│   ├── error.rs     # Centralized error handling with thiserror
│   └── types.rs     # Message enum & shared data structures
├── node/            # Bitcoin Core RPC client actor
│   └── mod.rs
├── pool/            # Stratum V2 pool protocol actor
│   └── mod.rs
└── ui/              # Terminal UI dashboard actor
    └── mod.rs
```

### Actor Pattern Implementation

Each component runs as an independent actor with its own async task:

1. **Node Actor** - Polls Bitcoin Core for block templates
2. **Pool Actor** - Manages SV2 protocol connection & handshake
3. **UI Actor** - Renders terminal dashboard and handles user input

Communication happens exclusively via **message passing** using `tokio::sync::broadcast` channels - no shared mutable state.

### Message Flow

```
┌─────────────┐                    ┌──────────────┐
│ Node Actor  │──NewBlockTemplate─→│              │
│             │                    │   Broadcast  │
│ Bitcoin RPC │──SendJobDecl──────→│   Channel    │
└─────────────┘                    │              │
                                   │  (AppMessage)│
┌─────────────┐                    │              │
│ Pool Actor  │←──SendJobDecl──────│              │
│             │                    │              │
│ SV2 Protocol│──JobAccepted──────→│              │
└─────────────┘                    └──────┬───────┘
                                          │
┌─────────────┐                          │
│  UI Actor   │←─────All Messages─────────┘
│             │
│  ratatui    │
└─────────────┘
```

## Noise NX Handshake State Machine

The Pool Actor implements a rigorous state machine for the Noise NX handshake:

### State Transitions

```
┌──────────────┐
│ Disconnected │
└──────┬───────┘
       │ TCP connect()
       ▼
┌──────────────┐
│  Connected   │
└──────┬───────┘
       │ Send first message (-> e)
       ▼
┌──────────────┐
│InitiatorSent │
└──────┬───────┘
       │ Receive & verify (<- e, ee, s, es)
       ▼
┌──────────────┐
│   Complete   │ ─────→ Encrypted channel ready
└──────────────┘
```

### Implementation Details

**Step 0: Generate ephemeral key**
```rust
let mut initiator = Initiator::new(None)?;  // NX pattern - no static key
let first_message = initiator.step_0()?;     // Generate -> e
```

**Step 1: Send ephemeral public key**
- Transmits initiator's ephemeral public key
- Unencrypted, as channel not yet established

**Step 2: Receive responder's response**
- Contains: ephemeral key (e), DH operations (ee, es), static key (s)
- Pool authenticates itself via static key

**Step 3: Derive shared secrets**
```rust
let codec = initiator.step_1(&second_message)?;  // Transition to transport mode
```

From this point, all communication is **encrypted and authenticated** using the Noise Protocol's ChainingKey and derived cipher states.

### Zero-Unwrap Philosophy

Every operation uses proper error handling:

```rust
// ❌ NEVER do this
let client = Client::new(&url, auth).unwrap();

// ✅ ALWAYS do this
let client = Client::new(&url, auth)
    .map_err(|e| JdcError::BitcoinRpc(e))?;
```

All errors are typed using `thiserror`:
- `JdcError::NoiseHandshake` - Handshake failures
- `JdcError::Framing` - Encryption/decryption errors
- `JdcError::PoolConnection` - Network issues
- `JdcError::BitcoinRpc` - Bitcoin Core errors

## Key Features

### 1. Robust Error Handling
- Custom error types with `thiserror`
- Contextual error propagation
- No panics in production code

### 2. Zero-Copy Where Possible
- Uses `bytes::BytesMut` for buffer management
- Minimizes allocations in hot paths
- Efficient frame encoding/decoding

### 3. Async-First Design
- `tokio` multi-threaded runtime
- Non-blocking I/O throughout
- Graceful shutdown handling

### 4. Production-Ready Logging
- Structured logging with `tracing`
- Configurable log levels
- Event correlation across actors

## Configuration

Edit [`config.toml`](config.toml):

```toml
[bitcoin_node]
rpc_url = "http://127.0.0.1:8332"
rpc_user = "bitcoin"
rpc_password = "password"
poll_interval = 5

[pool]
address = "127.0.0.1:34254"

[jdc]
coinbase_outputs = [
    { value = 0, script_pubkey = "76a914..88ac" }
]
min_fee_rate = 1.0
max_template_size = 4000000

[logging]
level = "info"
```

## Building & Running

```bash
# Build
cargo build --release

# Run
cargo run --release

# With custom config
JDC_LOGGING__LEVEL=debug cargo run --release
```

## Protocol Implementation Status

### ✅ Implemented
- Noise NX handshake (full state machine)
- TCP connection management
- Encrypted channel establishment
- Bitcoin Core RPC integration
- Block template polling
- Terminal UI with real-time stats

### 🚧 In Progress
- Full SV2 message encoding (DeclareMiningJob)
- Transaction short ID calculation
- Merkle proof generation
- Job token management

### 📋 Planned
- Mining job tracking
- Fee optimization strategies
- Multi-pool support
- Advanced transaction selection

## Security Considerations

1. **No .unwrap()** - All errors handled explicitly
2. **Noise Protocol** - Forward-secret encryption
3. **Memory Safety** - Rust's ownership guarantees
4. **No Shared State** - Actor isolation prevents data races

## Dependencies

Core SV2 crates from [Stratum Reference Implementation](https://github.com/stratum-mining/stratum):
- `noise_sv2` - Noise Protocol implementation
- `framing_sv2` - SV2 frame encoding/decoding
- `codec_sv2` - Message serialization
- `binary_sv2` - Binary protocol primitives

## License

This is a reference implementation for educational and production use.

## Contributing

This codebase demonstrates production Rust patterns:
- Trait-based abstractions
- Zero-copy optimizations
- Idiomatic error handling
- Clear separation of concerns

Contributions should maintain these standards.
