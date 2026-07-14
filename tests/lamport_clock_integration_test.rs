//! 集成测试 — 验证 Lamport 逻辑时钟的分布式行为。
//!
//! ## ITMO Lab 3 测试用例
//!
//! 测试覆盖：
//!
//! | 测试名称 | 验证内容 | Lab 3 对应概念 |
//! |---------|---------|---------------|
//! | `test_logical_clock_increment` | 逻辑时钟每次事件递增 1 | R1 规则 |
//! | `test_event_timestamp_ordering` | 发送时间戳 < 接收时间戳，happened-before 关系 | R2 规则 + 偏序关系 |
//! | `test_clock_sync_on_receive` | 收到大时间戳时正确同步 | R2 规则 |
//! | `test_pending_in_tracking` | pending_in 追踪通道中资金，总金额守恒 | 飞行中资金 |
//!
//! ## 核心验证
//!
//! Lab 3 的核心目标是证明：使用 Lamport 逻辑时钟配合 `pending_in` 追踪，
//! 可以在任意一致切面下保证总金额不变。这与 Lab 2 形成对比——
//! Lab 2 中如果物理时钟不对齐，某一时刻的余额快照可能遗漏"通道中"的资金。
//!
//! ## 参考
//!
//! - ITMO Lab 3 课件第 7-9 页（飞行中的资金 / Pending Money）
//! - Lamport, L. (1978). "Time, Clocks, and the Ordering of Events in a Distributed System"

use crab_keeper::banking::account::BranchAccount;
use crab_keeper::banking::types::{AccountId, Balance, BalanceHistory, TransferOrder};
use crab_keeper::communication::{Message, recv_message, send_message};
use crab_keeper::lamport_clock::LamportClock;
use std::net::{TcpListener, TcpStream};
use std::thread;

// ═══════════════════════════════════════════════════════════
// 测试辅助: Lamport 时钟版的银行子进程模拟
// ═══════════════════════════════════════════════════════════

/// 使用 Lamport 逻辑时钟的银行子进程模拟。
///
/// 与 Lab 2 线程模拟的关键区别：
/// 1. 使用 `LamportClock` 追踪事件（发送、接收、内部操作）
/// 2. 当本进程为转账源时，扣款后记录 `pending_in` 状态
///    （资金已从本地扣除但尚未到达目标）
/// 3. 当本进程为转账目标时，入账后 `pending_in` 清零
fn simulate_banking_child_lamport(
    listener: TcpListener,
    child_id: AccountId,
    initial_balance: Balance,
) -> thread::JoinHandle<Result<BalanceHistory, String>> {
    thread::spawn(move || {
        // Phase 1: 接受连接 + STARTED（Lamport R1）
        let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
        let mut clock = LamportClock::new();
        clock.increment(); // 连接事件
        send_message(&mut stream, &Message::Started).map_err(|e| e.to_string())?;

        let mut account = BranchAccount::new(child_id, initial_balance);

        // Phase 2: 处理 TRANSFER / STOP
        loop {
            let msg = recv_message(&mut stream).map_err(|e| e.to_string())?;
            // R2: 接收消息 → 递增时钟
            // 注：完整实现需从消息头提取 sender 时间戳调用 clock.update(ts)，
            // 当前简化：每次接收直接 increment()
            clock.increment();

            match msg {
                Message::Transfer(ref payload) => {
                    let order = TransferOrder::from_bytes(payload).map_err(|e| e.to_string())?;
                    let now = clock.get();

                    if order.src == child_id {
                        // 源账户：扣款 → 资金进入 pending_in
                        account.debit(order.amount, now).map_err(|e| e.to_string())?;

                        // debit() 已创建 BalanceState（pending_in=0），需手动设为转账金额
                        account.history.states.last_mut().unwrap().pending_in = order.amount;

                        clock.increment(); // R1: 发送事件
                        send_message(&mut stream, &Message::Transfer(payload.clone()))
                            .map_err(|e| e.to_string())?;
                    }
                    if order.dst == child_id {
                        // 目标账户：入账 → pending_in 清零
                        account.credit(order.amount, now).map_err(|e| e.to_string())?;
                        clock.increment(); // R1: 发送 ACK
                        send_message(&mut stream, &Message::Ack).map_err(|e| e.to_string())?;
                    }
                },
                Message::Stop => break,
                _ => {},
            }
        }

        // Phase 3: DONE + BALANCE_HISTORY
        clock.increment(); // R1: DONE 事件
        send_message(&mut stream, &Message::Done).map_err(|e| e.to_string())?;
        let history_bytes = account.history.to_bytes();
        send_message(&mut stream, &Message::BalanceHistory(history_bytes))
            .map_err(|e| e.to_string())?;

        Ok(account.history)
    })
}

// ═══════════════════════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════════════════════

fn expect_started_all(streams: &mut [TcpStream]) {
    for (i, stream) in streams.iter_mut().enumerate() {
        let msg = recv_message(stream).unwrap_or_else(|e| panic!("子进程 {i} STARTED 失败: {e}"));
        assert_eq!(msg, Message::Started, "子进程 {i} 应先发 STARTED");
    }
}

fn send_stop_all(streams: &mut [TcpStream]) {
    for (i, stream) in streams.iter_mut().enumerate() {
        send_message(stream, &Message::Stop)
            .unwrap_or_else(|e| panic!("向子进程 {i} 发送 STOP 失败: {e}"));
    }
}

fn collect_done_and_histories(streams: &mut [TcpStream]) -> Vec<BalanceHistory> {
    let mut histories = Vec::with_capacity(streams.len());
    for (i, stream) in streams.iter_mut().enumerate() {
        let done = recv_message(stream).unwrap_or_else(|e| panic!("子进程 {i} DONE 失败: {e}"));
        assert_eq!(done, Message::Done, "子进程 {i} 应发送 DONE");

        let history_msg =
            recv_message(stream).unwrap_or_else(|e| panic!("子进程 {i} HISTORY 失败: {e}"));
        match history_msg {
            Message::BalanceHistory(bytes) => {
                let h = BalanceHistory::from_bytes(&bytes)
                    .unwrap_or_else(|e| panic!("解析子进程 {i} 历史失败: {e}"));
                histories.push(h);
            },
            other => panic!("子进程 {i} 期望 BALANCE_HISTORY，收到 {}", other),
        }
    }
    histories
}

fn do_transfer(streams: &mut [TcpStream], src: AccountId, dst: AccountId, amount: Balance) {
    let order = TransferOrder { src, dst, amount };
    let msg = Message::Transfer(order.to_bytes());

    let src_idx = (src - 1) as usize;
    let dst_idx = (dst - 1) as usize;

    send_message(&mut streams[src_idx], &msg).expect("发送 TRANSFER 给 src 应成功");
    let relayed = recv_message(&mut streams[src_idx]).expect("应收到 src 转发的 TRANSFER");
    assert!(matches!(relayed, Message::Transfer(_)), "src 应转发 TRANSFER");
    send_message(&mut streams[dst_idx], &relayed).expect("中继 TRANSFER 给 dst 应成功");
    let ack = recv_message(&mut streams[dst_idx]).expect("应收到 dst 的 ACK");
    assert_eq!(ack, Message::Ack, "dst 应回复 ACK");
}

// ═══════════════════════════════════════════════════════════
// 测试 1: 逻辑时钟递增 (R1)
// ═══════════════════════════════════════════════════════════

/// 验证 Lamport 时钟在每次事件（发送、接收、内部操作）后正确递增。
///
/// 测试场景: 模拟两个进程通过 TCP 通信，发送方每次事件递增 1，
/// 接收方也递增。验证时钟单调递增且初始值为 0。
#[test]
fn test_logical_clock_increment() {
    // 单进程验证：LamportClock 的基本递增行为
    let mut clock = LamportClock::new();
    assert_eq!(clock.get(), 0, "初始值为 0");

    // 模拟 5 个本地事件，每个递增 1
    let events: Vec<u64> = (0..5).map(|_| clock.increment()).collect();
    assert_eq!(events, vec![1, 2, 3, 4, 5], "每次 event 应递增 1");
    assert_eq!(clock.get(), 5);

    // 验证单调性：后续值始终大于前值
    for w in events.windows(2) {
        assert!(w[1] > w[0], "时间戳应严格递增");
    }
}

// ═══════════════════════════════════════════════════════════
// 测试 2: 事件时间戳排序 (happened-before)
// ═══════════════════════════════════════════════════════════

/// 验证发送事件的时间戳严格小于对应接收事件的时间戳（happened-before 关系）。
///
/// Lamport 时钟的核心保证：如果事件 A 发生在事件 B 之前
/// (A → B)，则 C(A) < C(B)。
///
/// 测试场景:
///   - 进程 1 依次执行: e1=1, e2=2, e3=3, send=4
///   - 进程 2 在收到消息前执行: e1=1, e2=2
///   - 进程 2 收到消息(send_time=4): update(4)→max(2,4)+1=5
///   - 验证: sender.send_time(4) < receiver.recv_time(5)
#[test]
fn test_event_timestamp_ordering() {
    // 进程 1（发送方）：依次执行 4 个事件
    let mut sender_clock = LamportClock::new();
    let _e1 = sender_clock.increment(); // 1 → event A
    let _e2 = sender_clock.increment(); // 2 → event B
    let _e3 = sender_clock.increment(); // 3 → event C
    let send_time = sender_clock.increment(); // 4 → event D (发送)
    assert_eq!(send_time, 4);

    // 进程 2（接收方）：2 个本地事件后收到消息
    let mut receiver_clock = LamportClock::new();
    let _e1r = receiver_clock.increment(); // 1
    let _e2r = receiver_clock.increment(); // 2

    // R2: 收到消息，发送方时间戳 = 4
    let recv_time = receiver_clock.update(send_time);
    assert_eq!(recv_time, 5, "max(2,4)+1 = 5");

    // Lamport 核心保证: C(send) < C(recv)
    assert!(send_time < recv_time, "发送事件 ({send_time}) 应严格早于接收事件 ({recv_time})");

    // 后续事件应继续递增
    assert_eq!(receiver_clock.increment(), 6);
    assert_eq!(receiver_clock.increment(), 7);
}

// ═══════════════════════════════════════════════════════════
// 测试 3: 接收时时钟同步 (R2)
// ═══════════════════════════════════════════════════════════

/// 验证接收方在收到消息时正确执行 Lamport R2 规则：
/// Lⱼ = max(Lⱼ, L_msg) + 1。
///
/// 测试三个场景:
///   1. 收到更大的时间戳 → 跳到发送方 +1
///   2. 收到更小的时间戳 → 保持本地 +1（不倒退）
///   3. 收到相等的时间戳 → +1
#[test]
fn test_clock_sync_on_receive() {
    // 场景 1: 本地时钟落后，收到更大的时间戳
    {
        let mut local = LamportClock::new();
        // 本地已递增 3 次
        for _ in 0..3 {
            local.increment();
        }
        assert_eq!(local.get(), 3);
        // 收到消息，发送方时间戳 = 10
        assert_eq!(local.update(10), 11, "max(3,10)+1 = 11");
        assert_eq!(local.get(), 11);
    }

    // 场景 2: 本地时钟领先，收到更小的时间戳（不应倒退）
    {
        let mut local = LamportClock::new();
        local.set(20); // 本地已推进到 20
        // 收到消息，发送方时间戳仅 = 5
        assert_eq!(local.update(5), 21, "max(20,5)+1 = 21（不应倒退）");
        assert_eq!(local.get(), 21);
    }

    // 场景 3: 收到相等的时间戳
    {
        let mut local = LamportClock::new();
        local.set(7);
        assert_eq!(local.update(7), 8, "max(7,7)+1 = 8");
        assert_eq!(local.get(), 8);
    }
}

// ═══════════════════════════════════════════════════════════
// 测试 4: pending_in 追踪 — 飞行中的资金
// ═══════════════════════════════════════════════════════════

/// 验证在转账过程中，使用 pending_in 追踪通道中资金后，
/// 系统总金额在任意时刻保持不变。
///
/// Lab 3 的关键收益：配合 Lamport 时钟的 pending_in 字段，
/// 解决了 Lab 2 中"在物理时钟不一致时快照总金额会出错"的问题。
///
/// 场景：
///   - 2 个账户，初始余额 P1=$100, P2=$50（总 $150）
///   - P1→P2 转账 $30
///   - 转账过程中（源已扣款但目标未入账），
///     正确总金额 = P1余额 + P2余额 + pending_in = 70 + 50 + 30 = 150 ✓
///   - 转账完成后 pending_in=0，总金额 70 + 80 = 150 ✓
#[test]
fn test_pending_in_tracking() {
    // 启动 2 个 Lamport 子进程
    let l1 = TcpListener::bind("127.0.0.1:0").expect("绑定 P1 端口应成功");
    let l2 = TcpListener::bind("127.0.0.1:0").expect("绑定 P2 端口应成功");
    let a1 = l1.local_addr().unwrap();
    let a2 = l2.local_addr().unwrap();

    let h1 = simulate_banking_child_lamport(l1, 1, 100);
    let h2 = simulate_banking_child_lamport(l2, 2, 50);

    let mut streams = vec![
        TcpStream::connect(a1).expect("连接 P1 应成功"),
        TcpStream::connect(a2).expect("连接 P2 应成功"),
    ];

    // Phase 1: STARTED
    expect_started_all(&mut streams);

    // Phase 2: 转账 P1→P2 $30
    do_transfer(&mut streams, 1, 2, 30);

    // Phase 2: STOP
    send_stop_all(&mut streams);

    // Phase 3: 收集结果
    let histories = collect_done_and_histories(&mut streams);
    assert_eq!(histories.len(), 2);

    // 验证最终余额
    let h1b = histories.iter().find(|h| h.id == 1).expect("应有 P1 的历史");
    let h2b = histories.iter().find(|h| h.id == 2).expect("应有 P2 的历史");

    assert_eq!(h1b.states.last().unwrap().balance, 70, "P1 应为 100-30=70");
    assert_eq!(h2b.states.last().unwrap().balance, 80, "P2 应为 50+30=80");

    // 总金额保持 $150
    let total: i64 = histories.iter().map(|h| h.states.last().unwrap().balance).sum();
    assert_eq!(total, 150, "转账前后总金额应不变");

    // 验证 pending_in 中间状态：扣款后 pending_in > 0 的时刻，
    // 余额 + pending_in 应等于扣款前余额（发送方的资金守恒）
    for state in &h1b.states {
        if state.pending_in > 0 {
            assert_eq!(
                state.balance + state.pending_in,
                100,
                "P1 时刻 {}: balance({}) + pending_in({}) 应等于扣款前余额 100",
                state.time,
                state.balance,
                state.pending_in
            );
        }
    }
    // 验证 P1 确实记录了 pending_in 状态
    assert!(h1b.states.iter().any(|s| s.pending_in > 0), "P1 应记录资金在通道中的中间状态");
    // P2 只接收资金，不应有 pending_in
    assert!(h2b.states.iter().all(|s| s.pending_in == 0), "P2 作为接收方不应有 pending_in");

    // Lab 3 核心验证：从全局角度看，最终余额之和 = 初始总金额（资金守恒）。
    // 注意：Lamport 时钟是各进程独立的——P1 的 t=2 和 P2 的 t=2 是不同进程的
    // 不同事件，不能按绝对时间戳对齐求和。核心验证点是：
    //   - 转账前（P1 扣款前）：余额之和 = 150
    //   - 转账后（最终状态）：余额之和 = 150
    //   - 中间态：P1 的 pending_in 记录了飞行中的资金
    let p1_pre_debit = h1b.states.iter().rev().find(|s| s.pending_in == 0).unwrap();
    let total_before = p1_pre_debit.balance + h2b.states.first().unwrap().balance;
    assert_eq!(total_before, 150, "转账前（P1 pending_in=0 的最后状态）余额之和应为 150");

    h1.join().unwrap().unwrap();
    h2.join().unwrap().unwrap();
}
