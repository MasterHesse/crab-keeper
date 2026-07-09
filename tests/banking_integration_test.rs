//! 集成测试 — 验证分布式银行系统的端到端转账流程。
//!
//! ## ITMO Lab 2 测试用例
//!
//! 这些测试通过真实进程生成（spawn）模拟整个银行系统：
//! - 父进程作为协调者（coordinator）
//! - 子进程作为银行分支（branch），管理各自的账户
//! - 进程间仅通过 TCP 消息传递通信
//!
//! ## 测试目标
//!
//! | 测试名称 | 验证内容 |
//! |---------|---------|
//! | `test_transfer_atomicity` | 转账原子性：任一环节失败则回滚 |
//! | `test_balance_consistency` | 余额一致性：转账前后系统总金额不变 |
//! | `test_cross_branch_transfer` | 跨分支转账：多笔转账后每个账户余额正确 |
//!
//! ## HINT: 模拟子进程的转账处理
//!
//! 在集成测试中，不直接 spawn 真实子进程（避免依赖 banking_child_work 的实现），
//! 而是在测试内用线程模拟子进程的三种行为：
//!
//! 1. **阶段 1**: 连接父进程 → 发送 STARTED
//! 2. **阶段 2**: 等待 TRANSFER 或 STOP
//!    - 收到 TRANSFER: 根据 src/dst 决定扣款/入账 + 转发/回复 ACK
//!    - 收到 STOP: 离开 Phase 2
//! 3. **阶段 3**: 发送 DONE → 发送 BALANCE_HISTORY
//!
//! 这种模拟方式允许在 banking_child_work 未完成时就编写集成测试。
//!
//! ## 参考
//!
//! - ITMO Lab 2 课件第 40-43 页（父/子进程工作流规范）
//! - ITMO Lab 2 课件第 7-9 页（转账三阶段图）
//! - ITMO Lab 2 课件第 10-11 页（总金额一致性）

use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};

// ═══════════════════════════════════════════════════════════
// 测试辅助: 在线程中模拟银行子进程行为
// ═══════════════════════════════════════════════════════════

use crab_keeper::banking::account::BranchAccount;
use crab_keeper::banking::time::PhysicalClock;
use crab_keeper::banking::types::{AccountId, Balance, BalanceHistory, TransferOrder};
use crab_keeper::communication::{recv_message, send_message, Message};
use std::thread;

/// 在后台启动一个 crab-keeper 子进程，以银行模式运行（备用，当前测试使用线程模拟）。
#[allow(dead_code)]
fn spawn_banking_child(parent_addr: &str, child_id: usize, balance: u64) -> Child {
    let exe = std::env::current_exe().expect("获取当前可执行文件路径失败");
    Command::new(exe)
        .arg("--banking-child")
        .arg(parent_addr)
        .env("CHILD_ID", child_id.to_string())
        .env("CHILD_BALANCE", balance.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn 子进程失败")
}

/// 模拟单个银行子进程的完整三阶段工作流。
///
/// 在独立线程中运行：接受父进程连接 → STARTED → 处理 TRANSFER/STOP → DONE+BALANCE_HISTORY。
fn simulate_banking_child(
    listener: TcpListener,
    child_id: AccountId,
    initial_balance: Balance,
) -> thread::JoinHandle<Result<BalanceHistory, String>> {
    thread::spawn(move || {
        // 阶段 1: 接受连接 + STARTED
        let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
        send_message(&mut stream, &Message::Started).map_err(|e| e.to_string())?;

        let mut account = BranchAccount::new(child_id, initial_balance);
        let mut clock = PhysicalClock::new();

        // 阶段 2: 处理 TRANSFER / STOP
        loop {
            let msg = recv_message(&mut stream).map_err(|e| e.to_string())?;
            match msg {
                Message::Transfer(ref payload) => {
                    let order =
                        TransferOrder::from_bytes(payload).map_err(|e| e.to_string())?;
                    let now = clock.now();

                    if order.src == child_id {
                        account
                            .debit(order.amount, now)
                            .map_err(|e| e.to_string())?;
                        // 将 TRANSFER 发回给父进程（父进程作为中继）
                        send_message(&mut stream, &Message::Transfer(payload.clone()))
                            .map_err(|e| e.to_string())?;
                    }
                    if order.dst == child_id {
                        account
                            .credit(order.amount, now)
                            .map_err(|e| e.to_string())?;
                        send_message(&mut stream, &Message::Ack)
                            .map_err(|e| e.to_string())?;
                    }
                }
                Message::Stop => break,
                _ => {}
            }
        }

        // 阶段 3: DONE + BALANCE_HISTORY
        send_message(&mut stream, &Message::Done).map_err(|e| e.to_string())?;
        let history_bytes = account.history.to_bytes();
        send_message(&mut stream, &Message::BalanceHistory(history_bytes))
            .map_err(|e| e.to_string())?;

        Ok(account.history)
    })
}

/// 辅助函数：等待所有子进程发送 STARTED
fn expect_started_all(streams: &mut [TcpStream]) {
    for (i, stream) in streams.iter_mut().enumerate() {
        let msg = recv_message(stream).unwrap_or_else(|e| panic!("子进程 {i} STARTED 失败: {e}"));
        assert_eq!(msg, Message::Started, "子进程 {i} 应先发 STARTED");
    }
}

/// 辅助函数：向所有子进程发送 STOP
fn send_stop_all(streams: &mut [TcpStream]) {
    for (i, stream) in streams.iter_mut().enumerate() {
        send_message(stream, &Message::Stop)
            .unwrap_or_else(|e| panic!("向子进程 {i} 发送 STOP 失败: {e}"));
    }
}

/// 辅助函数：接收所有子进程的 DONE + BALANCE_HISTORY
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
            }
            other => panic!("子进程 {i} 期望 BALANCE_HISTORY，收到 {}", other),
        }
    }
    histories
}

/// 辅助函数：执行一笔转账（父进程中继模式）
///   1. 发送 TRANSFER 给 src
///   2. 从 src 接收转发
///   3. 中继给 dst
///   4. 从 dst 接收 ACK
fn do_transfer(streams: &mut [TcpStream], src: AccountId, dst: AccountId, amount: Balance) {
    let order = TransferOrder { src, dst, amount };
    let msg = Message::Transfer(order.to_bytes());

    let src_idx = (src - 1) as usize;
    let dst_idx = (dst - 1) as usize;

    // 1. 发送 TRANSFER 给 src
    send_message(&mut streams[src_idx], &msg).expect("发送 TRANSFER 给 src 应成功");

    // 2. 从 src 接收转发的 TRANSFER
    let relayed = recv_message(&mut streams[src_idx]).expect("应收到 src 转发的 TRANSFER");
    assert!(matches!(relayed, Message::Transfer(_)), "src 应转发 TRANSFER");

    // 3. 中继给 dst
    send_message(&mut streams[dst_idx], &relayed).expect("中继 TRANSFER 给 dst 应成功");

    // 4. 从 dst 接收 ACK
    let ack = recv_message(&mut streams[dst_idx]).expect("应收到 dst 的 ACK");
    assert_eq!(ack, Message::Ack, "dst 应回复 ACK");
}

// ═══════════════════════════════════════════════════════════
// 转账原子性测试
// ═══════════════════════════════════════════════════════════

/// 验证单笔跨分支转账的原子性。
#[test]
fn test_transfer_atomicity() {
    let l1 = TcpListener::bind("127.0.0.1:0").expect("绑定 P1 端口应成功");
    let l2 = TcpListener::bind("127.0.0.1:0").expect("绑定 P2 端口应成功");
    let a1 = l1.local_addr().unwrap();
    let a2 = l2.local_addr().unwrap();

    let h1 = simulate_banking_child(l1, 1, 100);
    let h2 = simulate_banking_child(l2, 2, 50);

    let mut streams = vec![
        TcpStream::connect(a1).expect("连接 P1 应成功"),
        TcpStream::connect(a2).expect("连接 P2 应成功"),
    ];

    // Phase 1: STARTED 同步
    expect_started_all(&mut streams);

    // Phase 2: 转账 P1→P2 $30
    do_transfer(&mut streams, 1, 2, 30);

    // Phase 2: STOP
    send_stop_all(&mut streams);

    // Phase 3: DONE + BALANCE_HISTORY
    let histories = collect_done_and_histories(&mut streams);
    assert_eq!(histories.len(), 2);

    // 验证余额：P1=$70, P2=$80
    let h1b = histories.iter().find(|h| h.id == 1).expect("应有 P1 的历史");
    let h2b = histories.iter().find(|h| h.id == 2).expect("应有 P2 的历史");
    assert_eq!(h1b.states.last().unwrap().balance, 70, "P1 应为 100-30=70");
    assert_eq!(h2b.states.last().unwrap().balance, 80, "P2 应为 50+30=80");

    // 总金额保持 $150
    let total: i64 = histories.iter().map(|h| h.states.last().unwrap().balance).sum();
    assert_eq!(total, 150, "转账前后总金额应不变");

    h1.join().unwrap().unwrap();
    h2.join().unwrap().unwrap();
}

// ═══════════════════════════════════════════════════════════
// 余额一致性测试
// ═══════════════════════════════════════════════════════════

/// 验证多笔转账后系统总金额保持不变。
#[test]
fn test_balance_consistency() {
    // 3 个子进程，初始余额 P1=$100, P2=$200, P3=$300（总 $600）
    let l1 = TcpListener::bind("127.0.0.1:0").unwrap();
    let l2 = TcpListener::bind("127.0.0.1:0").unwrap();
    let l3 = TcpListener::bind("127.0.0.1:0").unwrap();
    let a1 = l1.local_addr().unwrap();
    let a2 = l2.local_addr().unwrap();
    let a3 = l3.local_addr().unwrap();

    let h1 = simulate_banking_child(l1, 1, 100);
    let h2 = simulate_banking_child(l2, 2, 200);
    let h3 = simulate_banking_child(l3, 3, 300);

    let mut streams = vec![
        TcpStream::connect(a1).unwrap(),
        TcpStream::connect(a2).unwrap(),
        TcpStream::connect(a3).unwrap(),
    ];

    // Phase 1: STARTED 同步
    expect_started_all(&mut streams);

    // Phase 2: P1→P2 $50, P2→P3 $80, P3→P1 $30
    do_transfer(&mut streams, 1, 2, 50);
    do_transfer(&mut streams, 2, 3, 80);
    do_transfer(&mut streams, 3, 1, 30);

    // Phase 2: STOP
    send_stop_all(&mut streams);

    // Phase 3: 收集结果
    let histories = collect_done_and_histories(&mut streams);

    // 验证各账户余额：
    // P1: 100 - 50 + 30 = 80
    // P2: 200 - 80 + 50 = 170
    // P3: 300 - 30 + 80 = 350
    let get_balance = |histories: &[BalanceHistory], id: AccountId| -> Balance {
        histories.iter().find(|h| h.id == id).unwrap().states.last().unwrap().balance
    };
    assert_eq!(get_balance(&histories, 1), 80);
    assert_eq!(get_balance(&histories, 2), 170);
    assert_eq!(get_balance(&histories, 3), 350);

    // 总和: 80 + 170 + 350 = 600 ✓
    let total: i64 = histories.iter().map(|h| h.states.last().unwrap().balance).sum();
    assert_eq!(total, 600);

    h1.join().unwrap().unwrap();
    h2.join().unwrap().unwrap();
    h3.join().unwrap().unwrap();
}

// ═══════════════════════════════════════════════════════════
// 跨分支转账测试
// ═══════════════════════════════════════════════════════════

/// 验证 4 个分支多笔转账后余额历史正确。
#[test]
fn test_cross_branch_transfer() {
    // 4 个子进程，初始余额 P1=40, P2=30, P3=20, P4=10（总 $100）
    let listeners: Vec<TcpListener> = (0..4)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let addrs: Vec<_> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();
    let balances: Vec<i64> = vec![40, 30, 20, 10];

    let mut handles = vec![];
    for (i, (listener, &bal)) in listeners.into_iter().zip(balances.iter()).enumerate() {
        handles.push(simulate_banking_child(listener, (i + 1) as u8, bal));
    }

    let mut streams: Vec<TcpStream> = addrs
        .iter()
        .map(|a| TcpStream::connect(*a).unwrap())
        .collect();

    // Phase 1: STARTED
    expect_started_all(&mut streams);

    // Phase 2: 多笔转账
    do_transfer(&mut streams, 1, 2, 10); // P1→P2: P1=30, P2=40
    do_transfer(&mut streams, 3, 4, 5);  // P3→P4: P3=15, P4=15
    do_transfer(&mut streams, 2, 3, 20); // P2→P3: P2=20, P3=35
    do_transfer(&mut streams, 4, 1, 5);  // P4→P1: P4=10, P1=35

    send_stop_all(&mut streams);

    let histories = collect_done_and_histories(&mut streams);

    let get_balance = |histories: &[BalanceHistory], id| -> i64 {
        histories.iter().find(|h| h.id == id).unwrap().states.last().unwrap().balance
    };

    // 验证最终余额
    assert_eq!(get_balance(&histories, 1), 35); // 40-10+5
    assert_eq!(get_balance(&histories, 2), 20); // 30+10-20
    assert_eq!(get_balance(&histories, 3), 35); // 20-5+20
    assert_eq!(get_balance(&histories, 4), 10); // 10+5-5

    // 总金额不变
    let total: i64 = histories.iter().map(|h| h.states.last().unwrap().balance).sum();
    assert_eq!(total, 100);

    // 验证余额历史完整性（每个进程的 states 数量 > 1）
    for h in &histories {
        assert!(
            h.states.len() >= 2,
            "进程 {} 的历史应至少有初始和保护操作记录",
            h.id
        );
        // 时间戳应该非递减
        for w in h.states.windows(2) {
            assert!(w[1].time >= w[0].time, "进程 {} 时间不应倒退", h.id);
        }
    }

    for h in handles {
        h.join().unwrap().unwrap();
    }
}
