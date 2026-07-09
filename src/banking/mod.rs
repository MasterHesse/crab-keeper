//! 分布式银行系统模块（阶段二 — 分布式银行系统）。
//!
//! ## ITMO Lab 2 对应关系
//!
//! 本模块严格参照 ITMO 分布式系统课程 Laboratory Work #2 设计。
//!
//! ### 核心模型
//!
//! ```text
//! 父进程 (parent)                         子进程 (child branches)
//!   │                                          │
//!   │  1. 等待所有子进程发送 STARTED             │
//!   │                                          │
//!   │  2. 调用 transfer() 发起转账               │
//!   │── TRANSFER(src, dst, amount) ───────────►│  src: 扣款 → 转发给 dst
//!   │                                          │  dst: 入账 → 发送 ACK 给 parent
//!   │◄── ACK ─────────────────────────────────│
//!   │                                          │
//!   │  3. 发送 STOP 给所有子进程                  │
//!   │── STOP ─────────────────────────────────►│  进入 Phase 3
//!   │                                          │
//!   │  4. 等待所有子进程发送 DONE                 │
//!   │◄── DONE ────────────────────────────────│
//!   │                                          │
//!   │  5. 等待并汇总 BALANCE_HISTORY              │
//!   │◄── BALANCE_HISTORY ──────────────────────│
//!   │                                          │
//!   ▼  调用 print_history()                     ▼  结束
//! ```
//!
//! ### 转账协议（链式，同步）
//!
//! ```text
//! Parent                Source Child           Dest Child
//!   │                      │                      │
//!   │── TRANSFER ─────────►│                      │
//!   │                      │ debit balance         │
//!   │                      │── TRANSFER ──────────►│
//!   │                      │                      │ credit balance
//!   │◄───────────────────────────────── ACK ──────│
//!   │ transfer complete     │                      │
//! ```
//!
//! ### 关键学习点
//!
//! 1. **转账原子性**：链式协议保证一次转账的原子性（扣款+入账要么都发生要么都不发生）
//! 2. **余额历史**：BalanceHistory 记录每个时间点的余额，必须填补时间间隙
//! 3. **物理时钟局限**：各进程有独立物理时钟——将在 Lab 3 用 Lamport 时钟修正

pub mod account;
pub mod time;
pub mod transfer;
pub mod types;

pub use account::BranchAccount;
pub use time::{PhysicalClock, get_physical_time};
pub use transfer::transfer;
pub use types::{
    AccountId, AllHistory, Balance, BalanceHistory, BalanceState, MAX_PROCESS_ID, MAX_T, Timestamp,
    TransferOrder,
};
