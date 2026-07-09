//! 物理时钟模拟。
//!
//! ITMO Lab 2 中假设物理时钟是"完美的"——无漂移、无偏差。
//! 每次调用 `get_physical_time()` 返回单调递增的时间戳。
//!
//! 在 Lab 3 (Lamport 逻辑时钟) 中将用逻辑时钟替代物理时钟。

use std::sync::atomic::{AtomicU64, Ordering};

use crate::banking::types::Timestamp;

/// 全局物理时钟计数器，初始值为 0。
static GLOBAL_CLOCK: AtomicU64 = AtomicU64::new(0);

/// 获取当前物理时间并递增全局时钟。
///
/// 对应 ITMO banking.h 中的 `timestamp_t get_physical_time()`。
/// 第一次调用返回 0，之后每次 +1。
#[must_use]
#[allow(dead_code)]
pub fn get_physical_time() -> Timestamp {
    GLOBAL_CLOCK.fetch_add(1, Ordering::SeqCst) as Timestamp
}

/// 局部物理时钟，每次调用自增 1，用于单元测试避免全局状态耦合。
#[derive(Debug)]
pub struct PhysicalClock {
    current: Timestamp,
}

impl PhysicalClock {
    /// 创建从 0 开始的局部时钟。
    #[must_use]
    pub fn new() -> Self {
        Self { current: 0 }
    }

    /// 获取当前时间并递增。
    #[allow(dead_code)]
    pub fn now(&mut self) -> Timestamp {
        let old = self.current;
        self.current += 1;
        old
    }

    /// 重置时钟为 0。
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.current = 0
    }
}

impl Default for PhysicalClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod time_tests {
    use super::*;

    #[test]
    fn test_physical_clock_monotonic() {
        let mut clock = PhysicalClock::new();
        assert_eq!(clock.now(), 0);
        assert_eq!(clock.now(), 1);
        assert_eq!(clock.now(), 2);
        assert_eq!(clock.now(), 3);
        assert_eq!(clock.now(), 4);
    }

    #[test]
    fn test_physical_clock_reset() {
        let mut clock = PhysicalClock::new();
        clock.now();
        clock.now();
        clock.now();
        assert_eq!(clock.now(), 3);
        clock.reset();
        assert_eq!(clock.now(), 0);
        assert_eq!(clock.now(), 1);
    }

    #[test]
    fn test_physical_clock_default() {
        let mut clock = PhysicalClock::default();
        assert_eq!(clock.now(), 0);
    }
}
