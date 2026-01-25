# Stratum V2 Job Declarator Client

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A production-grade **Stratum V2 Job Declarator Client** that enables individual miners to select their own transactions from a local Bitcoin node and negotiate mining jobs with pools using the SV2 Job Declaration Protocol.

**Goal:** Give miners control over transaction selection - a major improvement over Stratum V1 where pools dictate all block contents.

##  What This Does

-  **Connects to your Bitcoin Core node** - Polls `getblocktemplate` for transaction selection
- **Establishes encrypted channel with pool** - Full Noise NX handshake implementation
- **Declares custom mining jobs** - Send your transaction selection to the pool
-  **Real-time dashboard** - Terminal UI showing connection status, stats, and logs

##  Architecture Highlights

### Actor-Based Design
Three independent actors communicate via Tokio broadcast channels:
- **Node Actor** - Bitcoin RPC client
- **Pool Actor** - SV2 protocol handler with Noise encryption
- **UI Actor** - Terminal dashboard



## Terminal UI

```
┌──────────────────────────────────────────────────────────┐
│ Stratum V2 Job Declarator Client                         │
├──────────────────────────────────────────────────────────┤
│ Status                                                   │
│ Bitcoin Node: Connected                                  │
│ Pool: Connected (Encrypted)                              │
│ Current Height: 850123                                   │
│ Uptime: 01:23:45                                         │
├──────────────────────────────────────────────────────────┤
│ Statistics                                               │
│ Templates Created: 15                                    │
│ Jobs Declared: 15                                        │
│ Jobs Accepted: 14                                        │
│ Jobs Rejected: 1                                         │
│ Total Fees Collected: 125000 sats                        │
│ Acceptance Rate: 93.3%                                   │
├──────────────────────────────────────────────────────────┤
│ Event Log                                                │
│ [12:34:56] ✓ Noise handshake complete                   │
│ [12:34:55] ✓ Pool TCP connection established            │
│ [12:34:50] → New template: height=850123, txs=2500        │
│ [12:34:45] ✓ Bitcoin node connected                      │
└──────────────────────────────────────────────────────────┘
Press 'q' or ESC to quit
```


    └── mod.rs           # Terminal UI (ratatui)
```

##  Implementation Status

### Complete
- Actor architecture with message passing
-  Bitcoin Core RPC integration
-  Block template polling
-  TCP pool connection
- **Noise NX handshake (fully working)**
-  Encrypted channel establishment
-  Terminal UI dashboard
-  Configuration system
-  Error handling (zero unwrap)
-  Structured logging

### In Progress
-  SV2 message encoding (`DeclareMiningJob`)
-  Transaction short ID calculation
-  Merkle proof generation
-  Mining job token management
