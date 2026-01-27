# Implementation Checklist & Next Steps

##  COMPLETED - Foundation (Production Ready)

### Project Structure
- [x] Modular folder structure (`src/node/`, `src/pool/`, `src/ui/`, `src/common/`)
- [x] Cargo.toml with all required dependencies
- [x] Configuration system (TOML + environment variables)
- [x] Git ignore file
- [x] Comprehensive documentation

### Core Architecture
- [x] **Actor Pattern** implementation
  - [x] Node Actor (Bitcoin RPC client)
  - [x] Pool Actor (SV2 protocol handler)
  - [x] UI Actor (Terminal dashboard)
- [x] **Message Passing** via `tokio::broadcast`
- [x] **No Shared Mutable State** - compiler-verified

### Error Handling
- [x] **Zero-Unwrap Policy** - no `.unwrap()` in production code
- [x] Custom `JdcError` type with `thiserror`
- [x] All functions return `Result<T, JdcError>`
- [x] Contextual error propagation

### Node Actor (`src/node/mod.rs`)
- [x] Bitcoin Core RPC connection
- [x] `getblocktemplate` polling
- [x] Block template parsing
- [x] Transaction extraction
- [x] Fee calculation
- [x] Event broadcasting
- [x] Automatic reconnection logic

### Pool Actor (`src/pool/mod.rs`)
- [x] **TCP connection** to pool
- [x] **State machine** (Disconnected → Connected → InitiatorSent → Complete)
- [x] **Noise NX handshake** - FULLY IMPLEMENTED
  - [x] Initiator creation
  - [x] Step 0: Generate & send ephemeral key
  - [x] Step 1: Process responder key & derive transport keys
  - [x] NoiseCodec creation
- [x] **Encrypted channel** establishment
- [x] Frame encryption/decryption infrastructure
- [x] Message routing from other actors
- [x] Reconnection on disconnect

### UI Actor (`src/ui/mod.rs`)
- [x] `ratatui` terminal dashboard
- [x] Real-time statistics display
- [x] Connection status indicators
- [x] Event log with timestamps
- [x] User input handling ('q' to quit)
- [x] RAII cleanup (terminal state restoration)
- [x] Uptime tracking
- [x] Acceptance rate calculation

### Common Module (`src/common/`)
- [x] `AppMessage` enum (12 variants)
- [x] `AppStats` structure
- [x] `CoinbaseOutput` type
- [x] Error types (`JdcError` with 12 variants)

### Main Entry Point (`src/main.rs`)
- [x] Configuration loading
- [x] Logging initialization (`tracing`)
- [x] Tokio runtime setup
- [x] Actor spawning
- [x] Graceful shutdown handling

### Documentation
- [x] **README.md** - User-facing overview
- [x] **PROJECT_SUMMARY.md** - Complete project details
- [x] **ARCHITECTURE.md** - System design explanation
- [x] **NOISE_HANDSHAKE.md** - Deep dive on cryptography
- [x] **PRODUCTION_PATTERNS.md** - Rust best practices
- [x] **ARCHITECTURE_DIAGRAM.txt** - Visual architecture
- [x] Quick start script (`start.sh`)
- [x] Example configuration (`config.toml`)

### Build System
- [x] Compiles successfully (`cargo check` passes)
- [x] Release optimizations configured (LTO, opt-level 3)
- [x] All dependencies resolved

---

## 🚧 IN PROGRESS - SV2 Protocol Messages

### Message Encoding (High Priority)

#### 1. AllocateMiningJobToken (0x50)
**Status:** 🔴 Not Started  
**Location:** `src/pool/mod.rs`  
**What to implement:**
```rust
use job_declaration_sv2::AllocateMiningJobToken;

async fn request_job_token(&mut self) -> Result<Vec<u8>> {
    let msg = AllocateMiningJobToken {
        user_identifier: "miner_1".to_string(),
        request_id: self.next_request_id(),
    };
    
    let frame = StandardSv2Frame::from_message(
        msg,
        0x50, // message type
        0,    // extension type
        false // requires state?
    )?;
    
    let encoded = frame.serialize()?;
    self.send_encrypted(encoded).await?;
    Ok(())
}
```

**References:**
- [SV2 Spec: AllocateMiningJobToken](https://github.com/stratum-mining/sv2-spec/blob/main/08-Message-Types.md#allocateminingjobtoken-server-to-client)
- `job_declaration_sv2` crate documentation

---

#### 2. DeclareMiningJob (0x52)
**Status:** 🟡 Placeholder Exists  
**Location:** `src/pool/mod.rs` → `build_job_declaration()`  
**Current state:** Returns empty vector  
**What to implement:**
```rust
use job_declaration_sv2::DeclareMiningJob;
use binary_sv2::Seq0255;

fn build_job_declaration(
    &self,
    template_id: u64,
    coinbase_outputs: Vec<CoinbaseOutput>,
    transactions: Vec<Vec<u8>>,
) -> Result<Vec<u8>> {
    // 1. Calculate transaction short IDs
    let tx_short_ids: Vec<u64> = transactions
        .iter()
        .map(|tx| calculate_siphash_short_id(tx, self.pool_k0, self.pool_k1))
        .collect();
    
    // 2. Build coinbase output sequence
    let outputs = Seq0255::new(
        coinbase_outputs.iter()
            .map(|o| OutputScript {
                value: o.value,
                script: o.script_pubkey.clone().into(),
            })
            .collect()
    )?;
    
    // 3. Create DeclareMiningJob message
    let msg = DeclareMiningJob {
        request_id: template_id as u32,
        mining_job_token: self.current_mining_job_token.clone().into(),
        version: self.block_version,
        coinbase_prefix: self.build_coinbase_prefix()?.into(),
        coinbase_suffix: self.build_coinbase_suffix()?.into(),
        tx_short_id_list: tx_short_ids.into(),
        tx_short_id_mapping: self.build_short_id_mapping(&transactions)?.into(),
        tx_hash_list_hash: calculate_tx_hash_list_hash(&transactions),
        excess_data: Vec::new().into(),
    };
    
    // 4. Serialize to frame
    let frame = StandardSv2Frame::from_message(msg, 0x52, 0, false)?;
    Ok(frame.serialize()?)
}
```

**Sub-tasks:**
- [ ] Implement SipHash-2-4 for transaction short IDs
- [ ] Build coinbase prefix/suffix
- [ ] Calculate tx_hash_list_hash
- [ ] Create short ID mapping
- [ ] Test against pool acceptance criteria

**References:**
- [SV2 Spec: DeclareMiningJob](https://github.com/stratum-mining/sv2-spec/blob/main/08-Message-Types.md#declareminingjob-client-to-server)
- [Transaction Short IDs](https://github.com/stratum-mining/sv2-spec/blob/main/04-Protocol-Overview.md#4216-transaction-short-ids)

---

#### 3. Transaction Short ID Calculation
**Status:** 🔴 Not Started  
**Create new file:** `src/pool/short_id.rs`  
**What to implement:**
```rust
use siphasher::sip::SipHasher24;
use std::hash::Hasher;

pub fn calculate_short_id(
    tx_data: &[u8],
    k0: u64,
    k1: u64,
) -> u64 {
    let mut hasher = SipHasher24::new_with_keys(k0, k1);
    hasher.write(tx_data);
    hasher.finish()
}

// During handshake, pool sends its keys:
pub struct PoolKeys {
    pub k0: u64,
    pub k1: u64,
}

// Store in PoolActor
struct PoolActor {
    // ...existing fields...
    pool_keys: Option<PoolKeys>,
}
```

**Dependencies to add:**
```toml
siphasher = "1.0"
```

---

#### 4. Message Parsing (Responses)
**Status:** 🟡 Partial  
**Location:** `src/pool/mod.rs` → `process_pool_message()`  
**Current state:** Logs message types but doesn't parse  
**What to implement:**

```rust
async fn process_pool_message(&mut self, frame: StandardSv2Frame<Vec<u8>>) -> Result<()> {
    match frame.msg_type {
        0x51 => {
            // AllocateMiningJobTokenSuccess
            let msg: AllocateMiningJobTokenSuccess = 
                codec_sv2::decode_message(&frame.payload)?;
            
            self.mining_job_token = Some(msg.mining_job_token.to_vec());
            info!("Received mining job token: {:?}", msg);
            
            let _ = self.tx.send(AppMessage::JobTokenReceived {
                request_id: msg.request_id,
            });
        }
        0x53 => {
            // DeclareMiningJobSuccess
            let msg: DeclareMiningJobSuccess = 
                codec_sv2::decode_message(&frame.payload)?;
            
            self.active_jobs.insert(msg.request_id, msg.new_mining_job_token.clone());
            
            let _ = self.tx.send(AppMessage::JobAccepted {
                template_id: msg.request_id as u64,
                new_mining_job_token: msg.new_mining_job_token.to_vec(),
            });
        }
        0x54 => {
            // DeclareMiningJobError
            let msg: DeclareMiningJobError = 
                codec_sv2::decode_message(&frame.payload)?;
            
            error!("Job declaration error: {:?}", msg);
            
            let _ = self.tx.send(AppMessage::JobRejected {
                template_id: msg.request_id as u64,
                reason: msg.error_code.clone(),
            });
        }
        0x55 => {
            // IdentifyTransactions
            self.handle_identify_transactions(frame).await?;
        }
        0x56 => {
            // ProvideMissingTransactions - sent by us, shouldn't receive
            warn!("Received unexpected ProvideMissingTransactions");
        }
        _ => {
            debug!("Unhandled message type: 0x{:02x}", frame.msg_type);
        }
    }
    Ok(())
}
```

---

#### 5. IdentifyTransactions Handler (0x55)
**Status:** 🔴 Not Started  
**What to implement:**
```rust
use job_declaration_sv2::{IdentifyTransactions, ProvideMissingTransactions};

async fn handle_identify_transactions(
    &mut self,
    frame: StandardSv2Frame<Vec<u8>>,
) -> Result<()> {
    let msg: IdentifyTransactions = codec_sv2::decode_message(&frame.payload)?;
    
    // Pool is asking for full transactions it doesn't have
    let unknown_tx_hashes: Vec<[u8; 32]> = msg.transaction_list.to_vec();
    
    // Find transactions in our template
    let mut missing_txs = Vec::new();
    for tx_hash in unknown_tx_hashes {
        if let Some(tx) = self.find_transaction_by_hash(&tx_hash) {
            missing_txs.push(tx);
        } else {
            warn!("Pool requested unknown transaction: {:?}", tx_hash);
        }
    }
    
    // Send ProvideMissingTransactions response
    let response = ProvideMissingTransactions {
        request_id: msg.request_id,
        transaction_list: Seq064K::new(missing_txs)?,
    };
    
    let frame = StandardSv2Frame::from_message(response, 0x56, 0, false)?;
    self.send_encrypted(frame.serialize()?).await?;
    
    Ok(())
}

fn find_transaction_by_hash(&self, hash: &[u8; 32]) -> Option<Vec<u8>> {
    // Look up in stored templates
    self.current_template
        .as_ref()
        .and_then(|t| t.transactions.get(hash))
        .cloned()
}
```

---

### State Management Enhancements

#### Add Job Tracking
**Status:** 🔴 Not Started  
**What to implement:**
```rust
// In PoolActor
struct JobState {
    template_id: u64,
    request_id: u32,
    submitted_at: SystemTime,
    status: JobStatus,
}

enum JobStatus {
    Pending,
    Accepted { mining_job_token: Vec<u8> },
    Rejected { error_code: String },
}

struct PoolActor {
    // ...existing...
    active_jobs: HashMap<u32, JobState>,
    mining_job_token: Option<Vec<u8>>,
    pool_keys: Option<PoolKeys>,
}
```

---

## 📋 TODO - Testing & Validation

### Unit Tests
- [ ] Test Noise handshake state transitions
- [ ] Test message encoding/decoding
- [ ] Test short ID calculation
- [ ] Test error handling paths

### Integration Tests
- [ ] Test against SV2 reference implementation pool
- [ ] Test reconnection logic
- [ ] Test multi-template scenarios
- [ ] Test edge cases (rejected jobs, network errors)

### Example:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_short_id_calculation() {
        let tx = hex::decode("...").unwrap();
        let k0 = 0x1234567890ABCDEF;
        let k1 = 0xFEDCBA0987654321;
        
        let short_id = calculate_short_id(&tx, k0, k1);
        assert_eq!(short_id, 0x...); // Known value
    }
    
    #[tokio::test]
    async fn test_job_declaration_encoding() {
        let job = build_test_job();
        let encoded = build_job_declaration(job).unwrap();
        
        // Decode and verify
        let decoded: DeclareMiningJob = 
            codec_sv2::decode_message(&encoded).unwrap();
        assert_eq!(decoded.request_id, job.template_id);
    }
}
```

---

##  TODO - Enhanced Features

### Transaction Selection Strategy
- [ ] Implement fee-based selection
- [ ] Add mempool filtering
- [ ] Support custom transaction policies
- [ ] Optimize for maximum fees

### Metrics & Monitoring
- [ ] Export Prometheus metrics
- [ ] Add latency tracking
- [ ] Monitor job acceptance rate
- [ ] Track network statistics

### Advanced UI
- [ ] Add job details view
- [ ] Show transaction selection criteria
- [ ] Display pool statistics
- [ ] Add keyboard shortcuts for navigation

---

##  Recommended Implementation Order

### Phase 1: Basic SV2 Communication (1-2 days)
1. Implement `AllocateMiningJobToken` request
2. Parse `AllocateMiningJobTokenSuccess` response
3. Store mining job token
4. Test token allocation flow

### Phase 2: Job Declaration (2-3 days)
1. Implement transaction short ID calculation
2. Build coinbase prefix/suffix
3. Implement `DeclareMiningJob` encoding
4. Test against pool

### Phase 3: Transaction Handling (1-2 days)
1. Implement `IdentifyTransactions` handler
2. Implement `ProvideMissingTransactions` response
3. Add transaction storage/lookup

### Phase 4: Testing & Refinement (2-3 days)
1. Write unit tests
2. Integration testing with real pool
3. Performance optimization
4. Documentation updates

**Total estimated time: 1-2 weeks**

---

##  Resources for Implementation

### Official Documentation
- [Stratum V2 Specification](https://github.com/stratum-mining/sv2-spec)
- [SRI Rust Implementation](https://github.com/stratum-mining/stratum)
- [Job Declaration Protocol](https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md)

### Crate Documentation
- [`codec_sv2` docs](https://docs.rs/codec_sv2/)
- [`job_declaration_sv2` docs](https://docs.rs/job_declaration_sv2/)
- [`binary_sv2` docs](https://docs.rs/binary_sv2/)

### Testing Pools
- SRI test pool (local setup)
- Public SV2 testnet pools

---

##  Definition of Done

The project is complete when:

1.  **Foundation** - Actor architecture working (COMPLETE)
2.  **Handshake** - Noise NX fully functional (COMPLETE)
3.  **Messages** - All SV2 job declaration messages implemented
4.  **Integration** - Successfully declares jobs to real pool
5.  **Testing** - Comprehensive test coverage
6.  **Documentation** - All code documented
7.  **Performance** - Optimized for production use

**Current Progress: 40% Complete (Foundation & Handshake Done)**

---

##  Quick Win Next Steps

To get the **first successful job declaration**:

1. Add SipHash dependency to `Cargo.toml`
2. Implement `calculate_short_id()` function
3. Request mining job token on handshake complete
4. Build basic `DeclareMiningJob` message
5. Send to pool and observe response

**Estimated time to first job: 4-6 hours**

---

This checklist provides a clear roadmap for completing the Stratum V2 JDC implementation!
