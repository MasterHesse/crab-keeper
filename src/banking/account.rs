//! 分支账户操作：借/贷记、余额历史追踪。
//!
//! 每次借贷操作调用 `record_state(time)` 更新 `BalanceHistory`，
//! 自动填补时间间隙——如果上次记录在 t=2、本次在 t=5，
//! 则 t=3、t=4 的余额与前一条保持一致。

use crate::banking::types::{AccountId, Balance, BalanceHistory, BalanceState, Timestamp};

/// 银行账户操作错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountError {
    /// 余额不足
    InsufficientFunds { account_id: AccountId, balance: Balance, required: Balance },
    /// 金额无效（如非正数）
    InvalidAmount(Balance),
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientFunds { account_id, balance, required } => {
                write!(f, "账户 {account_id} 余额不足: 现有 {}，需要 {}", balance, required)
            }
            Self::InvalidAmount(amount) => write!(f, "无效金额: {}", amount),
        }
    }
}

/// 分支账户，维护余额和余额历史。
#[derive(Debug, Clone)]
pub struct BranchAccount {
    pub id: AccountId,
    pub history: BalanceHistory,
}

impl BranchAccount {
    /// 创建新账户，自动在 time=0 记录初始余额快照。
    #[must_use]
    #[allow(dead_code)]
    pub fn new(id: AccountId, initial_balance: Balance) -> Self {
        let history = BalanceHistory::new(id, initial_balance);
        Self { id, history }
    }

    /// 获取当前余额（history 中最后一条记录的 balance）。
    #[allow(dead_code)]
    pub fn current_balance(&self) -> Balance {
        self.history.states.last().unwrap().balance
    }

    /// 记录当前时间点的余额快照，自动填补时间间隙。
    #[allow(dead_code)]
    fn record_state(&mut self, time: Timestamp, new_balance: Balance) {
        let last_state = self.history.states.last().unwrap();
        let last_time = last_state.time;
        let last_balance = last_state.balance;
        if time > last_time {
            for t in (last_time + 1)..time {
                self.history.states.push(BalanceState::new(last_balance, t));
            }
        }
        self.history.states.push(BalanceState::new(new_balance, time));
    }

    /// 扣款操作。amount 必须为正，余额不足时返回 `InsufficientFunds`。
    #[allow(dead_code)]
    pub fn debit(&mut self, amount: Balance, time: Timestamp) -> Result<(), AccountError> {
        if amount <= 0 {
            return Err(AccountError::InvalidAmount(amount));
        }
        let current = self.current_balance();
        if current < amount {
            return Err(AccountError::InsufficientFunds {
                account_id: self.id,
                balance: current,
                required: amount,
            });
        }
        self.record_state(time, current - amount);
        Ok(())
    }

    /// 入账操作。amount 必须为正。
    #[allow(dead_code)]
    pub fn credit(&mut self, amount: Balance, time: Timestamp) -> Result<(), AccountError> {
        if amount <= 0 {
            return Err(AccountError::InvalidAmount(amount));
        }
        let new_balance = self.current_balance() + amount;
        self.record_state(time, new_balance);
        Ok(())
    }
}

#[cfg(test)]
mod account_tests {
    use super::*;

    #[test]
    fn test_account_new() {
        let acc = BranchAccount::new(1, 200);
        assert_eq!(acc.id, 1);
        assert_eq!(acc.current_balance(), 200);
        assert_eq!(acc.history.states.len(), 1);
        assert_eq!(acc.history.states[0].balance, 200);
        assert_eq!(acc.history.states[0].time, 0);
    }

    #[test]
    fn test_debit_success() {
        let mut acc = BranchAccount::new(2, 100);
        assert!(acc.debit(30, 1).is_ok());
        assert_eq!(acc.current_balance(), 70);
        assert_eq!(acc.history.states.len(), 2);
        assert_eq!(acc.history.states[1].balance, 70);
        assert_eq!(acc.history.states[1].time, 1);
    }

    #[test]
    fn test_debit_insufficient_funds() {
        let mut acc = BranchAccount::new(3, 50);
        let result = acc.debit(100, 1);
        assert!(result.is_err());
        match result {
            Err(AccountError::InsufficientFunds { account_id, balance, required }) => {
                assert_eq!(account_id, 3);
                assert_eq!(balance, 50);
                assert_eq!(required, 100);
            }
            _ => panic!("预期 InsufficientFunds 错误"),
        }
        assert_eq!(acc.current_balance(), 50);
        assert_eq!(acc.history.states.len(), 1, "失败不应产生新记录");
    }

    #[test]
    fn test_debit_invalid_amount() {
        let mut acc = BranchAccount::new(7, 100);
        assert_eq!(acc.debit(0, 1).unwrap_err(), AccountError::InvalidAmount(0));
        assert_eq!(acc.debit(-10, 1).unwrap_err(), AccountError::InvalidAmount(-10));
        assert_eq!(acc.current_balance(), 100);
    }

    #[test]
    fn test_credit_success() {
        let mut acc = BranchAccount::new(4, 100);
        assert!(acc.credit(50, 2).is_ok());
        assert_eq!(acc.current_balance(), 150);
        assert_eq!(acc.history.states.len(), 3);
        assert_eq!(acc.history.states[2].balance, 150);
        assert_eq!(acc.history.states[2].time, 2);
    }

    #[test]
    fn test_credit_invalid_amount() {
        let mut acc = BranchAccount::new(8, 100);
        assert_eq!(acc.credit(0, 1).unwrap_err(), AccountError::InvalidAmount(0));
        assert_eq!(acc.credit(-5, 1).unwrap_err(), AccountError::InvalidAmount(-5));
        assert_eq!(acc.current_balance(), 100);
    }

    #[test]
    fn test_balance_history_gap_filling() {
        let mut acc = BranchAccount::new(5, 100);
        acc.debit(20, 4).unwrap();
        assert_eq!(acc.history.states.len(), 5);
        let expected: Vec<(u64, i64)> = vec![(0, 100), (1, 100), (2, 100), (3, 100), (4, 80)];
        for (i, (t, b)) in expected.iter().enumerate() {
            assert_eq!(acc.history.states[i].time, *t);
            assert_eq!(acc.history.states[i].balance, *b);
        }
    }

    #[test]
    fn test_balance_history_multiple_operations() {
        let mut acc = BranchAccount::new(6, 200);
        acc.debit(50, 2).unwrap();
        acc.credit(30, 5).unwrap();
        assert_eq!(acc.history.states.len(), 6);
        assert_eq!(acc.current_balance(), 180);
        assert_eq!(acc.history.states[2].balance, 150);
        assert_eq!(acc.history.states[5].balance, 180);
        assert_eq!(acc.history.states[5].time, 5);
    }
}
