# Noise NX Handshake - Deep Dive

## Overview

The Noise Protocol Framework provides cryptographic handshake patterns for building secure channels. The **NX pattern** is specifically designed for scenarios where:
- The **initiator** (client/JDC) has no static key
- The **responder** (pool) has a static key known to initiator
- Forward secrecy is required

## Pattern: NX

```
NX:
  <- s
  ...
  -> e
  <- e, ee, s, es
```

### Notation
- `->` : Initiator sends
- `<-` : Responder sends
- `e` : Ephemeral key
- `s` : Static key
- `ee` : Diffie-Hellman between ephemeral keys
- `es` : Diffie-Hellman between initiator ephemeral and responder static

## Implementation in Pool Actor

### State Machine

```rust
enum HandshakeState {
    Disconnected,    // No connection
    Connected,       // TCP established, ready for Noise
    InitiatorSent,   // First message sent, awaiting response
    Complete,        // Encrypted channel active
}
```

### Step-by-Step Execution

#### **Step 0: Initialize Initiator**

```rust
let mut initiator = Initiator::new(None)
    .map_err(|e| JdcError::NoiseHandshake(
        format!("Failed to create initiator: {:?}", e)
    ))?;
```

**What happens:**
1. Generate ephemeral keypair (25519 curve)
2. Initialize Noise protocol state
3. No static key required for NX pattern

**Internal state:**
- `symmetricState.ck` ← HASHLEN bytes of zeros
- `symmetricState.h` ← Hash(protocol_name)
- Generate `e` (ephemeral keypair)

#### **Step 1: Generate First Message (-> e)**

```rust
let first_message = initiator.step_0()
    .map_err(|e| JdcError::NoiseHandshake(
        format!("Step 0 failed: {:?}", e)
    ))?;
```

**Message contents:**
- 32 bytes: Initiator's ephemeral public key

**Operations:**
1. `MixHash(e.pub)` - Mix ephemeral key into handshake hash
2. Return ephemeral public key as message payload

**Cryptographic state update:**
- `h = Hash(h || e.pub)`

#### **Step 2: Send First Message**

```rust
stream.write_all(&first_message).await
    .map_err(|e| JdcError::NoiseHandshake(
        format!("Failed to send first message: {}", e)
    ))?;
```

**Security note:** This message is **not encrypted** because:
- No shared secret exists yet
- Ephemeral key is public by design
- Authenticity comes from subsequent DH operations

#### **Step 3: Receive Second Message (<- e, ee, s, es)**

```rust
let mut second_message = vec![0u8; 1024];
let n = stream.read(&mut second_message).await
    .map_err(|e| JdcError::NoiseHandshake(
        format!("Failed to receive second message: {}", e)
    ))?;

second_message.truncate(n);
```

**Expected message structure:**
1. **32 bytes**: Responder's ephemeral public key (`re`)
2. **DH(e, re)**: Perform `ee` operation
3. **48 bytes**: Encrypted responder static key (`rs`) + auth tag
4. **DH(e, rs)**: Perform `es` operation

**Why these DH operations?**
- `ee` (ephemeral-ephemeral): Provides forward secrecy
- `es` (ephemeral-static): Authenticates responder using static key

#### **Step 4: Process Response & Derive Keys**

```rust
let codec = initiator.step_1(&second_message)
    .map_err(|e| JdcError::NoiseHandshake(
        format!("Step 1 failed: {:?}", e)
    ))?;
```

**Internal operations:**

1. **Extract responder ephemeral key:**
   ```
   re ← first 32 bytes of second_message
   MixHash(re)
   ```

2. **Perform ee DH:**
   ```
   shared_ee ← DH(e.priv, re.pub)
   MixKey(shared_ee)
   ```

3. **Decrypt responder static key:**
   ```
   rs ← DecryptAndHash(remaining_bytes)
   ```
   This uses AEAD with key derived from current chaining key.

4. **Perform es DH:**
   ```
   shared_es ← DH(e.priv, rs.pub)
   MixKey(shared_es)
   ```

5. **Split for transport mode:**
   ```
   (cipher_send, cipher_recv) ← Split()
   ```

**Result:** `NoiseCodec` containing:
- `cipher_send`: For encrypting outbound messages
- `cipher_recv`: For decrypting inbound messages
- Both use ChaCha20-Poly1305 AEAD

### Cryptographic Guarantees

After successful handshake:

1. **Confidentiality**: All subsequent messages encrypted with derived keys
2. **Integrity**: AEAD authentication tags prevent tampering
3. **Forward Secrecy**: Ephemeral keys ensure past sessions can't be decrypted
4. **Mutual Authentication**: 
   - Responder authenticated via static key
   - Initiator implicitly authenticated by ability to derive correct keys

### Error Handling Strategy

Every step can fail - we handle explicitly:

```rust
// ❌ WRONG: Silent failures or panics
let codec = initiator.step_1(&msg).expect("handshake failed");

// ✅ CORRECT: Explicit error propagation
let codec = initiator.step_1(&msg)
    .map_err(|e| JdcError::NoiseHandshake(
        format!("Step 1 failed: {:?}", e)
    ))?;
```

**Common failure modes:**
- Invalid message length → `JdcError::NoiseHandshake`
- AEAD verification failed → Potential MITM attack
- Connection dropped → `JdcError::Io`

### Post-Handshake: Transport Mode

Once `Complete`, all messages use the codec:

```rust
// Encryption
let encrypted = codec.encrypt(plaintext)
    .map_err(|e| JdcError::Framing(format!("Encrypt failed: {:?}", e)))?;

// Decryption
let plaintext = codec.decrypt(ciphertext)
    .map_err(|e| JdcError::Framing(format!("Decrypt failed: {:?}", e)))?;
```

**Frame structure:**
```
[6-byte header][N-byte payload]
     ↓              ↓
  Encrypted    Encrypted
```

Both header and payload are encrypted separately with incrementing nonces.

## Security Considerations

### Why NX Pattern?

1. **No client identity required**: Miner doesn't need a long-term key
2. **Pool authentication**: Pool proves identity via static key
3. **DoS resistance**: No state allocated until valid first message
4. **Forward secrecy**: Ephemeral keys rotated per session

### Attack Resistance

- **Replay attacks**: Prevented by nonce incrementing
- **MITM**: Detected via static key verification
- **Downgrade attacks**: Protocol version in handshake hash
- **Denial of service**: Minimal state before authentication

### Implementation Pitfalls to Avoid

```rust
// ❌ DANGEROUS: Reusing codec across connections
static CODEC: NoiseCodec = ...;

// ✅ SAFE: New codec per connection
let codec = perform_handshake().await?;

// ❌ DANGEROUS: Ignoring nonce overflow
codec.encrypt(data); // Call #2^64 wraps nonce!

// ✅ SAFE: Detect and rekey
if message_count > MAX_SAFE_MESSAGES {
    reconnect_and_rehandshake().await?;
}
```

## Debugging Handshake Issues

### Enable detailed logging:

```toml
[logging]
level = "debug"
```

### Look for these log patterns:

```
DEBUG Starting Noise NX handshake
DEBUG Sending first handshake message (32 bytes)
DEBUG Received second handshake message (80 bytes)
INFO  Noise handshake completed
```

### Common issues:

| Error | Cause | Solution |
|-------|-------|----------|
| "Step 0 failed" | RNG failure | Check system entropy |
| "Step 1 failed: decrypt error" | Wrong responder key | Verify pool static key |
| "Connection closed" | Pool rejected | Check pool allowlist |
| "Invalid message length" | Protocol mismatch | Verify SV2 version |

## References

- [Noise Protocol Framework](https://noiseprotocol.org/)
- [Stratum V2 Specification](https://github.com/stratum-mining/sv2-spec)
- [noise_sv2 crate documentation](https://docs.rs/noise_sv2/)
