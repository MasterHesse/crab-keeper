//! Lamport 逻辑时钟的单元测试。
//!
//! 测试覆盖：
//!
//! | 测试名称 | 验证内容 |
//! |---------|---------|
//! | `test_clock_starts_at_zero` | 新创建的时钟初始值为 0 |
//! | `test_clock_increment_basic` | increment 每次 +1，返回新值 |
//! | `test_clock_increment_multiple` | 连续递增 10 次，验证单调性 |
//! | `test_clock_update_received_larger` | 收到更大时间戳时正确同步 |
//! | `test_clock_update_received_smaller` | 收到更小时间戳时不倒退 |
//! | `test_clock_update_received_equal` | 收到相等时间戳时仍递增 |
//! | `test_clock_get_does_not_modify` | get() 不改变时钟值 |
//! | `test_clock_reset` | reset() 将时钟归零 |
//! | `test_clock_set` | set() 设置指定时钟值 |
//! | `test_clock_default` | Default 实现创建值为 0 的时钟 |
//! | `test_is_transfer_complete` | 转账完成判断函数 |
//! | `test_send_before_receive_ordering` | 发送事件时间戳 < 接收事件时间戳 |
//! | `test_clock_display` | Display 格式化输出 |

use crate::lamport_clock::{self, LamportClock};

// ═══════════════════════════════════════════════════════════
// 基本操作测试
// ═══════════════════════════════════════════════════════════

/// 新创建的 LamportClock 初始值应为 0。
#[test]
fn test_clock_starts_at_zero() {
    let clock = LamportClock::new();
    assert_eq!(clock.get(), 0, "时钟初始值应为 0");
}

/// 每次 increment 应将时钟值 +1 并返回新值。
#[test]
fn test_clock_increment_basic() {
    let mut clock = LamportClock::new();
    assert_eq!(clock.increment(), 1, "第一次 increment 应返回 1");
    assert_eq!(clock.increment(), 2, "第二次 increment 应返回 2");
    assert_eq!(clock.increment(), 3, "第三次 increment 应返回 3");
    assert_eq!(clock.get(), 3, "get() 应反映当前值");
}

/// 连续 increment 10 次，验证单调递增。
#[test]
fn test_clock_increment_multiple() {
    let mut clock = LamportClock::new();
    for expected in 1..=10 {
        assert_eq!(clock.increment(), expected, "第 {expected} 次 increment 应返回 {expected}");
    }
    assert_eq!(clock.get(), 10);
}

// ═══════════════════════════════════════════════════════════
// 接收同步 (R2) 测试
// ═══════════════════════════════════════════════════════════

/// 收到更大的时间戳时，时钟应同步到 max(local, received) + 1。
///
/// 规则 R2: Lⱼ = max(Lⱼ, L_msg) + 1
#[test]
fn test_clock_update_received_larger() {
    let mut clock = LamportClock::new();
    // 本地时钟已递增到 3
    for _ in 0..3 {
        clock.increment();
    }
    assert_eq!(clock.get(), 3);

    // 收到消息，发送方时间戳为 7
    // max(3, 7) = 7, +1 = 8
    assert_eq!(clock.update(7), 8, "应同步到 max(3,7)+1 = 8");
    assert_eq!(clock.get(), 8);
}

/// 收到更小的时间戳时，时钟不应倒退，仅 +1。
///
/// 规则 R2: max(local, received) = local（因为 local > received）
/// 然后 +1
#[test]
fn test_clock_update_received_smaller() {
    let mut clock = LamportClock::new();
    // 本地时钟已推进到 10
    clock.set(10);

    // 收到消息，发送方时间戳仅为 2
    // max(10, 2) = 10, +1 = 11
    assert_eq!(clock.update(2), 11, "应保持本地进度 max(10,2)+1 = 11");
    assert_eq!(clock.get(), 11);
}

/// 收到相等时间戳时，时钟应 +1。
///
/// 规则 R2: max(local, received) = local = received
/// 仍执行 R1 递增
#[test]
fn test_clock_update_received_equal() {
    let mut clock = LamportClock::new();
    clock.set(5);

    // 收到消息，发送方时间戳也是 5
    // max(5, 5) = 5, +1 = 6
    assert_eq!(clock.update(5), 6, "应返回 max(5,5)+1 = 6");
    assert_eq!(clock.get(), 6);
}

// ═══════════════════════════════════════════════════════════
// 辅助方法测试
// ═══════════════════════════════════════════════════════════

/// get() 不应该修改时钟值。
#[test]
fn test_clock_get_does_not_modify() {
    let mut clock = LamportClock::new();
    clock.set(42);
    let val1 = clock.get();
    let val2 = clock.get();
    let val3 = clock.get();
    assert_eq!(val1, 42);
    assert_eq!(val2, 42);
    assert_eq!(val3, 42);
    assert_eq!(clock.get(), 42, "get() 多次调用应返回相同值");
}

/// reset() 应将时钟重置为 0。
#[test]
fn test_clock_reset() {
    let mut clock = LamportClock::new();
    clock.set(99);
    assert_eq!(clock.get(), 99);
    clock.reset();
    assert_eq!(clock.get(), 0, "reset 后时钟应为 0");
    // reset 后 increment 应从 1 开始
    assert_eq!(clock.increment(), 1);
}

/// set() 应将时钟设置为指定值。
#[test]
fn test_clock_set() {
    let mut clock = LamportClock::new();
    clock.set(7);
    assert_eq!(clock.get(), 7);
    clock.set(0);
    assert_eq!(clock.get(), 0);
    clock.set(u64::MAX);
    assert_eq!(clock.get(), u64::MAX);
}

/// Default trait 应创建值为 0 的时钟。
#[test]
fn test_clock_default() {
    let clock: LamportClock = Default::default();
    assert_eq!(clock.get(), 0);
    assert_eq!(LamportClock::default(), LamportClock::new());
}

// ═══════════════════════════════════════════════════════════
// 转账完成判断测试
// ═══════════════════════════════════════════════════════════

/// 当 send_time > 0 且 recv_time > 0 时，转账完成。
#[test]
fn test_is_transfer_complete_both_positive() {
    assert!(lamport_clock::is_transfer_complete(1, 2), "双方均已操作 → 完成");
    assert!(lamport_clock::is_transfer_complete(5, 3), "双方均已操作 → 完成（recv 可早于 send");
}

/// 当 send_time 为 0 时，转账未完成（源尚未扣款）。
#[test]
fn test_is_transfer_complete_send_zero() {
    assert!(!lamport_clock::is_transfer_complete(0, 5), "send_time=0 → 未完成");
}

/// 当 recv_time 为 0 时，转账未完成（目标尚未入账）。
#[test]
fn test_is_transfer_complete_recv_zero() {
    assert!(!lamport_clock::is_transfer_complete(3, 0), "recv_time=0 → 未完成");
}

/// 当双方都为 0 时，转账未完成。
#[test]
fn test_is_transfer_complete_both_zero() {
    assert!(!lamport_clock::is_transfer_complete(0, 0), "双方均为 0 → 未完成");
}

// ═══════════════════════════════════════════════════════════
// 事件排序测试 — 验证 Lamport 时钟的核心语义
// ═══════════════════════════════════════════════════════════

/// 验证发送事件的时间戳必定小于对应接收事件的时间戳。
///
/// 这是 Lamport 时钟的核心保证：如果事件 A 发生在事件 B 之前
/// (happened-before)，则 clock(A) < clock(B)。
///
/// 模拟场景：
///   - 进程 P1 在 t=5 时发送消息
///   - 进程 P2 在 t=3 时收到消息
///   - P2 调用 update(5) → max(3, 5) + 1 = 6
///   - 结论: 发送时间戳 5 < 接收时间戳 6 ✓
#[test]
fn test_send_before_receive_ordering() {
    // 进程 P1（发送方）的时钟
    let mut sender_clock = LamportClock::new();
    // P1 做了一些本地工作
    sender_clock.increment();
    sender_clock.increment();
    sender_clock.increment();
    sender_clock.increment();
    let send_time = sender_clock.increment(); // 发送事件: 第 5 个事件
    assert_eq!(send_time, 5);

    // 进程 P2（接收方）的时钟 — 稍慢
    let mut receiver_clock = LamportClock::new();
    receiver_clock.increment();
    receiver_clock.increment();
    let recv_time_before = receiver_clock.increment(); // 收到消息前: t=3
    assert_eq!(recv_time_before, 3);

    // P2 收到消息，携带发送方时间戳 send_time=5
    // R2: max(3, 5) = 5, +1 = 6
    let recv_time = receiver_clock.update(send_time);
    assert_eq!(recv_time, 6, "接收事件时间戳应为 6");

    // Lamport 时钟核心保证: send_time < recv_time
    assert!(send_time < recv_time, "发送事件 ({send_time}) 应早于接收事件 ({recv_time})");
}

// ═══════════════════════════════════════════════════════════
// Display 格式化测试
// ═══════════════════════════════════════════════════════════

/// 验证 Display 正确输出。
#[test]
fn test_clock_display() {
    let mut clock = LamportClock::new();
    clock.increment();
    assert_eq!(format!("{}", clock), "LamportClock(1)");

    clock.set(42);
    assert_eq!(format!("{}", clock), "LamportClock(42)");
}
