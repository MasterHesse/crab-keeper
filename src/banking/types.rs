//! 分布式银行系统的核心数据类型定义与序列化。
//!
//! ## 类型对照（ITMO Lab 2 对应 C 结构体）
//!
//! | Rust 类型 | C 类型 (banking.h) | 说明 |
//! |-----------|-------------------|------|
//! | `AccountId` | `local_id` (uint8_t) | 进程/账户标识，1-indexed |
//! | `Balance` | `balance_t` (int64_t) | 余额，支持负数检测透支 |
//! | `Timestamp` | `timestamp_t` (uint64_t) | 物理时间戳 |
//! | `TransferOrder` | `TransferOrder` | 转账指令 |
//! | `BalanceState` | `BalanceState` | 某时刻的余额快照 |
//! | `BalanceHistory` | `BalanceHistory` | 单进程的完整余额历史 |
//! | `AllHistory` | `AllHistory` | 所有进程的余额历史汇总 |
//!
//! ## 序列化格式
//!
//! ### TransferOrder (10 字节)
//! ```text
//! ┌─────────┬─────────┬──────────────────────┐
//! │ src(1B) │ dst(1B) │ amount(8B, i64 大端) │
//! └─────────┴─────────┴──────────────────────┘
//! ```
//!
//! ### BalanceState (24 字节)
//! ```text
//! ┌──────────────────────┬──────────────────────┬──────────────────────┐
//! │ balance(8B, 大端)   │ time(8B, u64 大端)   │ pending_in(8B, 大端) │
//! └──────────────────────┴──────────────────────┴──────────────────────┘
//! ```
//!
//! ### BalanceHistory (变长)
//! ```text
//! ┌────────┬───────────────────┐
//! │ id(1B) │ len(1B, 条目数)   │
//! └────────┴───────────────────┘
//! 后跟 len 个 BalanceState (各 24B)
//! ```
//!
//! ### AllHistory (变长)
//! ```text
//! ┌───────────────────┐
//! │ len(1B, 条目数)   │
//! └───────────────────┘
//! 后跟 len 个 BalanceHistory (各变长)
//! ```

use std::fmt;

/// 账户 / 进程标识。对应 ITMO 的 `local_id`，1-indexed。
pub type AccountId = u8;

/// 账户余额。对应 ITMO 的 `balance_t`。
pub type Balance = i64;

/// 物理时间戳。对应 ITMO 的 `timestamp_t`。
pub type Timestamp = u64;

/// 最大子进程数（与 ITMO 的 MAX_PROCESS_ID 对应）
pub const MAX_PROCESS_ID: usize = 11;

/// 物理时间最大值
pub const MAX_T: usize = 256;

/// 转账指令，父进程创建并嵌入 TRANSFER 消息发送给源子进程。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferOrder {
    pub src: AccountId,
    pub dst: AccountId,
    pub amount: Balance,
}

/// 某一时刻的余额快照。
///
/// `pending_in` 在 Lab 2 中始终为 0。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceState {
    pub balance: Balance,
    pub time: Timestamp,
    pub pending_in: Balance,
}

/// 单个子进程的完整余额历史。
///
/// `states[N]` 存储时刻 N 的余额状态，间隙会自动填充前值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceHistory {
    pub id: AccountId,
    pub states: Vec<BalanceState>,
}

/// 所有子进程余额历史的汇总。
///
/// `histories[0]` 对应进程 ID=1。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllHistory {
    pub histories: Vec<BalanceHistory>,
}

impl BalanceState {
    /// 创建一个新的余额快照，`pending_in` 始终为 0。
    #[must_use]
    pub fn new(balance: Balance, time: Timestamp) -> Self {
        BalanceState { balance, time, pending_in: 0 }
    }
}

impl TransferOrder {
    /// 序列化为 10 字节: src(1B) + dst(1B) + amount(8B, i64 大端)。
    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = vec![self.src, self.dst];
        buf.extend_from_slice(&self.amount.to_be_bytes());
        buf
    }

    /// 从 10 字节流解析 TransferOrder。
    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 10 {
            return Err("长度不匹配".to_string());
        }
        let src = bytes[0];
        let dst = bytes[1];
        let amount = i64::from_be_bytes(bytes[2..10].try_into().unwrap());
        Ok(TransferOrder { src, dst, amount })
    }
}

impl BalanceState {
    /// 序列化为 24 字节: balance(8B) + time(8B) + pending_in(8B)。
    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&self.balance.to_be_bytes());
        buf.extend_from_slice(&self.time.to_be_bytes());
        buf.extend_from_slice(&self.pending_in.to_be_bytes());
        buf
    }

    /// 从 24 字节流解析 BalanceState。
    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 24 {
            return Err("长度不匹配".to_string());
        }
        let balance = i64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let time = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        let pending_in = i64::from_be_bytes(bytes[16..24].try_into().unwrap());
        Ok(BalanceState { balance, time, pending_in })
    }
}

impl BalanceHistory {
    /// 新建余额历史，初始状态在 time=0。
    #[must_use]
    #[allow(dead_code)]
    pub fn new(id: AccountId, initial_balance: Balance) -> Self {
        BalanceHistory { id, states: vec![BalanceState::new(initial_balance, 0)] }
    }

    /// 序列化为: id(1B) + states_len(1B) + states[]。
    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = vec![self.id, self.states.len() as u8];
        for state in &self.states {
            buf.extend_from_slice(&state.to_bytes());
        }
        buf
    }

    /// 从字节流解析 BalanceHistory。
    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 2 {
            return Err("长度不匹配".to_string());
        }
        let id = bytes[0];
        let len = bytes[1] as usize;
        if bytes.len() != 24 * len + 2 {
            return Err("长度不匹配".to_string());
        }
        let mut states: Vec<BalanceState> = Vec::with_capacity(len);
        for i in 0..len {
            let start = 24 * i + 2;
            let end = 24 * (i + 1) + 2;
            states.push(BalanceState::from_bytes(&bytes[start..end])?);
        }
        Ok(BalanceHistory { id, states })
    }
}

impl AllHistory {
    /// 创建空的 AllHistory。
    #[must_use]
    #[allow(dead_code)]
    pub fn new() -> Self {
        AllHistory { histories: Vec::new() }
    }

    /// 添加一个进程的余额历史。
    #[allow(dead_code)]
    pub fn push(&mut self, history: BalanceHistory) {
        self.histories.push(history);
    }

    /// 序列化为: count(1B) + for each: len(8B u64) + data。
    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = vec![self.histories.len() as u8];
        for history in &self.histories {
            let history_bytes = history.to_bytes();
            buf.extend_from_slice(&(history_bytes.len() as u64).to_be_bytes());
            buf.extend_from_slice(&history_bytes);
        }
        buf
    }

    /// 从字节流解析 AllHistory。
    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 1 {
            return Err("长度不匹配".to_string());
        }
        let count = bytes[0] as usize;
        let mut histories: Vec<BalanceHistory> = Vec::with_capacity(count);
        let mut offset = 1usize;
        for _ in 0..count {
            if offset + 8 > bytes.len() {
                return Err("长度不匹配".to_string());
            }
            let history_len =
                u64::from_be_bytes(bytes[offset..offset + 8].try_into().unwrap()) as usize;
            offset += 8;
            if offset + history_len > bytes.len() {
                return Err("长度不匹配".to_string());
            }
            histories.push(BalanceHistory::from_bytes(&bytes[offset..offset + history_len])?);
            offset += history_len;
        }
        Ok(AllHistory { histories })
    }
}

impl fmt::Display for TransferOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TRANSFER {{ src={}, dst={}, amount={} }}", self.src, self.dst, self.amount)
    }
}

impl fmt::Display for BalanceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BalanceState {{ balance={}, time={}, pending_in={} }}", self.balance, self.time, self.pending_in)
    }
}

impl fmt::Display for BalanceHistory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BalanceHistory(id={}, {} states)", self.id, self.states.len())
    }
}

impl fmt::Display for AllHistory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AllHistory({} processes)", self.histories.len())
    }
}

#[cfg(test)]
mod types_tests {
    use super::*;

    #[test]
    fn test_transfer_order_roundtrip() {
        let order = TransferOrder { src: 3, dst: 7, amount: 500 };
        let bytes = order.to_bytes();
        let decoded = TransferOrder::from_bytes(&bytes).expect("往返反序列化应成功");
        assert_eq!(decoded, order);
    }

    #[test]
    fn test_transfer_order_byte_length() {
        let bytes = TransferOrder { src: 1, dst: 2, amount: 0 }.to_bytes();
        assert_eq!(bytes.len(), 10);
    }

    #[test]
    fn test_transfer_order_amount_encoding() {
        let bytes = TransferOrder { src: 1, dst: 1, amount: 1 }.to_bytes();
        assert_eq!(bytes[2..10], [0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn test_transfer_order_from_bytes_invalid_length() {
        assert!(TransferOrder::from_bytes(&[0u8; 9]).is_err());
        assert!(TransferOrder::from_bytes(&[0u8; 11]).is_err());
    }

    #[test]
    fn test_balance_state_roundtrip() {
        let state = BalanceState { balance: 100, time: 3, pending_in: 0 };
        let bytes = state.to_bytes();
        assert_eq!(bytes.len(), 24);
        let decoded = BalanceState::from_bytes(&bytes).expect("往返反序列化应成功");
        assert_eq!(decoded, state);
    }

    #[test]
    fn test_balance_state_new() {
        let state = BalanceState::new(200, 5);
        assert_eq!(state.balance, 200);
        assert_eq!(state.time, 5);
        assert_eq!(state.pending_in, 0);
    }

    #[test]
    fn test_balance_state_pending_in_zero() {
        let state = BalanceState::new(42, 7);
        let bytes = state.to_bytes();
        assert_eq!(&bytes[16..24], &[0u8; 8]);
    }

    #[test]
    fn test_balance_history_new() {
        let history = BalanceHistory::new(2, 50);
        assert_eq!(history.id, 2);
        assert_eq!(history.states.len(), 1);
        assert_eq!(history.states[0].balance, 50);
        assert_eq!(history.states[0].time, 0);
    }

    #[test]
    fn test_balance_history_roundtrip_single_state() {
        let history = BalanceHistory::new(1, 100);
        let bytes = history.to_bytes();
        let decoded = BalanceHistory::from_bytes(&bytes).expect("单状态往返应成功");
        assert_eq!(decoded, history);
    }

    #[test]
    fn test_balance_history_roundtrip_multiple_states() {
        let mut history = BalanceHistory::new(3, 50);
        history.states.push(BalanceState::new(40, 1));
        history.states.push(BalanceState::new(30, 2));
        history.states.push(BalanceState::new(100, 3));
        let bytes = history.to_bytes();
        let decoded = BalanceHistory::from_bytes(&bytes).expect("多状态往返应成功");
        assert_eq!(decoded, history);
    }

    #[test]
    fn test_balance_history_from_bytes_empty() {
        assert!(BalanceHistory::from_bytes(&[]).is_err());
    }

    #[test]
    fn test_balance_history_from_bytes_truncated() {
        let mut bytes = vec![1u8, 3u8];
        bytes.extend_from_slice(&BalanceState::new(100, 0).to_bytes());
        assert!(BalanceHistory::from_bytes(&bytes).is_err());
    }

    #[test]
    fn test_all_history_new_empty() {
        assert_eq!(AllHistory::new().histories.len(), 0);
    }

    #[test]
    fn test_all_history_push() {
        let mut all = AllHistory::new();
        all.push(BalanceHistory::new(1, 42));
        assert_eq!(all.histories.len(), 1);
        assert_eq!(all.histories[0].id, 1);
    }

    #[test]
    fn test_all_history_roundtrip() {
        let mut all = AllHistory::new();
        all.push(BalanceHistory::new(1, 100));
        all.push(BalanceHistory::new(2, 200));
        let bytes = all.to_bytes();
        let decoded = AllHistory::from_bytes(&bytes).expect("AllHistory 往返应成功");
        assert_eq!(decoded, all);
    }

    #[test]
    fn test_all_history_from_bytes_empty() {
        assert!(AllHistory::from_bytes(&[]).is_err());
    }
}
