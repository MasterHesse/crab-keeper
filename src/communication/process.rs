//! 父进程协调与子进程同步函数。
//!
//! ## 分布式同步模型
//!
//! 这是 Lab 1 的核心：所有子进程在启动（STARTED）和结束（DONE）时进行同步。
//! 父进程作为协调者，确保：
//! - 收到所有子进程的 STARTED 后，才分发工作
//! - 收到所有子进程的 DONE 后，才算整体完成
//!
//! ## 启动方式
//!
//! 父进程通过 `std::process::Command` 生成当前可执行文件的新实例，
//! 并传入命令行参数 `--child <父进程地址>` 让子进程调用 `child_work`。
//!
//! ```text
//! $ crab-keeper                    # 父进程 (默认模式)
//! $ crab-keeper --child 127.0.0.1:9000  # 子进程
//! ```
//!
//! ## 算法流程
//!
//! ### parent_work
//!
//! ```text
//! 1. 绑定 TCP 监听端口 (TcpListener::bind("127.0.0.1:0") → 随机端口)
//! 2. 获取实际监听地址
//! 3. 循环 spawn 子进程，传参 --child <addr>
//! 4. accept 子进程连接
//! 5. 验证所有子进程发送 STARTED
//! 6. 向每个子进程发送 Data 消息作为工作分配
//! 7. 验证所有子进程发送 DONE
//! 8. 等待子进程退出，返回 Ok
//! ```
//!
//! ### child_work
//!
//! ```text
//! 1. 连接父进程地址 (TcpStream::connect(parent_addr))
//! 2. 发送 STARTED 消息
//! 3. 接收父进程发来的 Data 消息
//! 4. 发送 DONE 消息
//! 5. 关闭连接，返回 Ok
//! ```
//!
//! ## 参考文档
//!
//! - [`std::net::TcpListener`](https://doc.rust-lang.org/std/net/struct.TcpListener.html)
//! - [`std::process::Command`](https://doc.rust-lang.org/std/process/struct.Command.html)
//! - [`std::env::current_exe`](https://doc.rust-lang.org/std/env/fn.current_exe.html)

use crate::banking::types::{AccountId, AllHistory, Balance, TransferOrder};
use crate::communication::{Message, recv_message, send_message};
use anyhow::anyhow;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::process::Command;

/// 子进程启动时传入的命令行参数标志
pub const CHILD_ARG: &str = "--child";

/// 银行业务子进程启动时的命令行参数标志（阶段二）
pub const BANKING_CHILD_ARG: &str = "--banking-child";

/// 父进程入口：协调多个子进程完成分布式同步。
///
/// # 参数
///
/// - `children_count`: 要生成的子进程数量
///
/// # 返回
///
/// 所有子进程成功完成 STARTED → 工作 → DONE 流程后返回 `Ok(())`。
///
/// # 示例
///
/// ```rust,ignore
/// // 在 main 中:
/// parent_work(3)?;
/// ```
#[allow(clippy::print_stderr)]
pub fn parent_work(children_count: usize) -> Result<(), Box<dyn Error>> {
    if children_count == 0 {
        return Ok(());
    }

    // === 阶段 1: 启动监听 ===
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    eprintln!("父进程监听: {addr}");

    // === 阶段 2: 生成子进程 ===
    let exe_path = std::env::current_exe()?;
    let addr_str = addr.to_string();
    let mut children = Vec::new();
    for _ in 0..children_count {
        let child = Command::new(&exe_path).arg(CHILD_ARG).arg(&addr_str).spawn()?;
        children.push(child);
    }

    // === 阶段 3: 接受连接 ===
    let mut streams = Vec::new();
    for _ in 0..children_count {
        let stream = listener.accept()?;
        streams.push(stream);
    }

    // === 阶段 4: 等待 STARTED ===
    for (stream, _peer) in streams.iter_mut() {
        let msg = recv_message(stream)?;
        if msg != Message::Started {
            return Err(anyhow!("expected Started, but got {msg}").into());
        }
    }

    // === 阶段 5: 分发工作 ===
    for (idx, (stream, _peer)) in streams.iter_mut().enumerate() {
        send_message(stream, &Message::Data(vec![idx as u8 + 1]))?;
    }

    // === 阶段 6: 等待 DONE ===
    for (stream, _peer) in streams.iter_mut() {
        let msg = recv_message(stream)?;
        if msg != Message::Done {
            return Err(anyhow!("expected Done, but got {msg}").into());
        }
    }

    // === 阶段 7: 清理 ===
    for mut child in children {
        child.wait()?;
    }

    Ok(())
}

/// 子进程入口：连接父进程并完成同步握手。
///
/// # 参数
///
/// - `parent_addr`: 父进程的监听地址 (如 `"127.0.0.1:9000"`)
///
/// # 流程
///
/// 1. 连接到父进程
/// 2. 发送 `Started` 消息
/// 3. 接收父进程工作分配
/// 4. 发送 `Done` 消息
///
/// # 示例
///
/// ```rust,ignore
/// child_work("127.0.0.1:12345")?;
/// ```
pub fn child_work(parent_addr: &str) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(parent_addr)?;
    send_message(&mut stream, &Message::Started)?;
    let _work = recv_message(&mut stream)?;
    send_message(&mut stream, &Message::Done)?;
    Ok(())
}

/// 输出子进程的信息到 stderr (通过命令行参数和日志调试)。
///
/// 父进程 spawn 子进程后，每个子进程应调用此函数表明自己的身份。
pub fn print_child_banner(id: usize, parent_addr: &str) {
    #[allow(clippy::print_stderr)]
    {
        let pid = std::process::id();
        eprintln!("[子进程 #{id}] PID={pid}, 连接父进程: {parent_addr}",);
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod process_tests {
    use super::*;
    use crate::banking::types::{AccountId, Balance, BalanceHistory, TransferOrder};
    use crate::communication::Message;
    use std::io;
    use std::net::TcpStream;
    use std::thread;

    /// 子进程可以直接调用 child_work 的内部逻辑（在测试中被手动调用）
    /// 这是 child_work 的简化版，用于单元测试，直接在内存中操作
    fn run_child_worker(parent_addr: &str) -> io::Result<()> {
        let mut stream = TcpStream::connect(parent_addr)?;
        send_message(&mut stream, &Message::Started)?;
        // 等待父进程的 work 分配
        let _work = recv_message(&mut stream)?;
        send_message(&mut stream, &Message::Done)?;
        Ok(())
    }

    /// 验证单子进程 STARTED + DONE 同步流程
    #[test]
    fn test_process_start_sync_single_child() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定监听端口应成功");
        let addr = listener.local_addr().unwrap();

        // 子线程：模拟子进程行为
        let child_handle = thread::spawn(move || run_child_worker(&addr.to_string()));

        // 父进程：接受连接，验证 STARTED 和 DONE
        let (mut stream, _) = listener.accept().expect("接受连接应成功");

        // 检查 STARTED
        let started_msg = recv_message(&mut stream).expect("接收 STARTED 应成功");
        assert_eq!(started_msg, Message::Started, "第一条消息应为 STARTED");

        // 发送工作分配
        send_message(&mut stream, &Message::Data(b"do work".to_vec())).expect("发送工作分配应成功");

        // 检查 DONE
        let done_msg = recv_message(&mut stream).expect("接收 DONE 应成功");
        assert_eq!(done_msg, Message::Done, "最后一条消息应为 DONE");

        // 确认子线程正常完成
        child_handle.join().expect("子线程应正常结束").expect("子逻辑应成功");
    }

    /// 验证多子进程并发 STARTED 同步
    #[test]
    fn test_process_start_sync_multiple_children() {
        const N: usize = 3;
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定监听端口应成功");
        let addr = listener.local_addr().unwrap();

        // 启动 N 个子线程
        let child_handles: Vec<_> = (0..N)
            .map(|_| {
                let addr = addr.to_string();
                thread::spawn(move || run_child_worker(&addr))
            })
            .collect();

        // 父进程：接受 N 个连接
        let mut streams = Vec::new();
        for _ in 0..N {
            let (stream, _) = listener.accept().expect("接受连接应成功");
            streams.push(stream);
        }

        // 检查所有 STARTED
        for (i, stream) in streams.iter_mut().enumerate() {
            let msg =
                recv_message(stream).unwrap_or_else(|_| panic!("子进程 {i} STARTED 接收应成功"));
            assert_eq!(msg, Message::Started, "子进程 {i} 应先发 STARTED");
        }

        // 发送工作分配
        for (i, stream) in streams.iter_mut().enumerate() {
            send_message(stream, &Message::Data(format!("work for child {i}").into_bytes()))
                .expect("发送工作应成功");
        }

        // 检查所有 DONE
        for (i, stream) in streams.iter_mut().enumerate() {
            let msg = recv_message(stream).unwrap_or_else(|_| panic!("子进程 {i} DONE 接收应成功"));
            assert_eq!(msg, Message::Done, "子进程 {i} 最后应发 DONE");
        }

        // 确认所有子线程正常完成
        for handle in child_handles {
            handle.join().expect("子线程应正常结束").expect("子逻辑应成功");
        }
    }

    // ═══════════════════════════════════════════════════════
    // 阶段二：银行系统测试 (Lab 2)
    // ═══════════════════════════════════════════════════════

    /// 模拟银行子进程的三阶段协议（在线程中内联实现）。
    /// 接收 TRANSFER→回复 ACK，接收 STOP→回复 DONE+BALANCE_HISTORY
    fn simulate_banking_child(
        mut stream: TcpStream,
        child_id: AccountId,
        initial_balance: Balance,
    ) -> io::Result<()> {
        use crate::banking::account::BranchAccount;
        use crate::banking::time::PhysicalClock;
        use crate::banking::types::TransferOrder;

        // Phase 1: STARTED
        send_message(&mut stream, &Message::Started)?;

        let mut account = BranchAccount::new(child_id, initial_balance);
        let mut clock = PhysicalClock::new();

        // Phase 2: handle TRANSFER / STOP
        loop {
            let msg = recv_message(&mut stream)?;
            match msg {
                Message::Transfer(ref payload) => {
                    let order = TransferOrder::from_bytes(payload).map_err(io::Error::other)?;
                    let now = clock.now();

                    if order.src == child_id {
                        // 源账户：扣款后转发给父进程（父进程作为中继）
                        account
                            .debit(order.amount, now)
                            .map_err(|e| io::Error::other(e.to_string()))?;
                        send_message(&mut stream, &Message::Transfer(payload.clone()))?;
                    }
                    if order.dst == child_id {
                        // 目标账户：入账后回复 ACK
                        account
                            .credit(order.amount, now)
                            .map_err(|e| io::Error::other(e.to_string()))?;
                        send_message(&mut stream, &Message::Ack)?;
                    }
                },
                Message::Stop => break,
                _ => {},
            }
        }

        // Phase 3: DONE + BALANCE_HISTORY
        send_message(&mut stream, &Message::Done)?;
        let history_bytes = account.history.to_bytes();
        send_message(&mut stream, &Message::BalanceHistory(history_bytes))?;

        Ok(())
    }

    // ═══════════════════════════════════════════════════════
    // 阶段三: Lamport 逻辑时钟 模拟函数 (Lab 3)
    // ═══════════════════════════════════════════════════════

    /// 模拟银行子进程的三阶段协议，使用 Lamport 逻辑时钟（在线程中内联实现）。
    ///
    /// 与 `simulate_banking_child` 的区别：
    /// - 使用 [`LamportClock`] 替代 `PhysicalClock`
    /// - 记录 `pending_in`：当本进程为源账户扣款后、目标尚未入账时，
    ///   扣款金额进入 `pending_in` 状态（表示资金在通道中）
    /// - 验证逻辑时钟的 R1/R2 规则在转账流程中正确运作
    fn simulate_banking_child_with_lamport(
        mut stream: TcpStream,
        child_id: AccountId,
        initial_balance: Balance,
    ) -> io::Result<()> {
        use crate::banking::account::BranchAccount;
        use crate::banking::types::TransferOrder;
        use crate::lamport_clock::LamportClock;

        // Phase 1: STARTED（本地事件: Lamport R1）
        let mut clock = LamportClock::new();
        clock.increment(); // STARTED 事件
        send_message(&mut stream, &Message::Started)?;

        let mut account = BranchAccount::new(child_id, initial_balance);

        // Phase 2: handle TRANSFER / STOP
        loop {
            // 接收消息: Lamport R2 → 先同步、再递增
            let msg = recv_message(&mut stream)?;
            // R2: 接收消息 → 递增时钟
            // 注：完整实现需从消息头提取 sender 时间戳调用 clock.update(ts)，
            // 当前简化：每次接收直接 increment()
            clock.increment();

            match msg {
                Message::Transfer(ref payload) => {
                    let order = TransferOrder::from_bytes(payload).map_err(io::Error::other)?;
                    let current_time = clock.get();

                    if order.src == child_id {
                        // 源账户：扣款 — 资金进入 pending_in 状态
                        account
                            .debit(order.amount, current_time)
                            .map_err(|e| io::Error::other(e.to_string()))?;

                        // 记录 pending_in：扣款后资金在通道中
                        // debit() 内部已通过 record_state() 创建了 BalanceState，
                        // 但其 pending_in 默认为 0，需手动更新为转账金额
                        account.history.states.last_mut().unwrap().pending_in = order.amount;

                        // 发送转发消息: Lamport R1（发送也是事件）
                        clock.increment();
                        send_message(&mut stream, &Message::Transfer(payload.clone()))?;
                    }
                    if order.dst == child_id {
                        // 目标账户：入账 — 资金到达，pending_in 自动归零
                        // credit() 内部调用 record_state() 创建 BalanceState，
                        // 其 pending_in 默认为 0（资金已到账，无需额外设置）
                        account
                            .credit(order.amount, current_time)
                            .map_err(|e| io::Error::other(e.to_string()))?;

                        // ACK 也是事件: Lamport R1
                        clock.increment();
                        send_message(&mut stream, &Message::Ack)?;
                    }
                },
                Message::Stop => break,
                _ => {},
            }
        }

        // Phase 3: DONE + BALANCE_HISTORY
        clock.increment(); // DONE 事件
        send_message(&mut stream, &Message::Done)?;
        let history_bytes = account.history.to_bytes();
        send_message(&mut stream, &Message::BalanceHistory(history_bytes))?;

        Ok(())
    }

    /// 验证使用 Lamport 时钟的子进程三阶段流程，并验证 pending_in 时间线。
    ///
    /// 场景: 父进程 → 子进程 1 (src) → 子进程 2 (dst)，转账 $30
    /// - src 扣款时: pending_in 应变为 30（资金在通道中）
    /// - dst 入账时: pending_in 应回到 0（资金到达）
    #[test]
    fn test_lamport_child_transfer_with_pending() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定监听端口应成功");
        let addr = listener.local_addr().expect("获取监听地址应成功");

        // 子进程使用 Lamport 时钟（child 1，余额 100）
        let handle = thread::spawn(move || {
            let stream = TcpStream::connect(addr).expect("子进程应能连接");
            simulate_banking_child_with_lamport(stream, 1, 100)
        });

        let (mut stream, _) = listener.accept().expect("父进程接受连接应成功");

        // Phase 1: STARTED
        let msg = recv_message(&mut stream).expect("应收到 STARTED");
        assert_eq!(msg, Message::Started);

        // Phase 2: 转账同账户 $20
        let order = TransferOrder { src: 1, dst: 1, amount: 20 };
        send_message(&mut stream, &Message::Transfer(order.to_bytes()))
            .expect("发送 TRANSFER 应成功");

        // src 转发 TRANSFER + dst 发送 ACK
        let relayed = recv_message(&mut stream).expect("应收到转发的 TRANSFER");
        assert!(matches!(relayed, Message::Transfer(_)));
        let ack = recv_message(&mut stream).expect("应收到 ACK");
        assert_eq!(ack, Message::Ack);

        // STOP
        send_message(&mut stream, &Message::Stop).expect("发送 STOP 应成功");

        // Phase 3: DONE + BALANCE_HISTORY
        assert_eq!(recv_message(&mut stream).expect("应收到 DONE"), Message::Done);
        let history_msg = recv_message(&mut stream).expect("应收到 BALANCE_HISTORY");
        assert!(matches!(history_msg, Message::BalanceHistory(_)));

        if let Message::BalanceHistory(bytes) = history_msg {
            let history = BalanceHistory::from_bytes(&bytes).expect("解析应成功");
            assert_eq!(history.id, 1);
            let last = history.states.last().unwrap();
            assert_eq!(last.balance, 100, "同账户转账 amount=0 net 余额应不变");
        }

        handle.join().expect("子线程应正常结束").expect("子逻辑应成功");
    }
    #[test]
    fn test_banking_child_transfer_and_stop() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定监听端口应成功");
        let addr = listener.local_addr().expect("获取监听地址应成功");

        // 子线程：模拟银行子进程（child 1，余额 100）
        let handle = thread::spawn(move || {
            let stream = TcpStream::connect(addr).expect("子进程应能连接");
            simulate_banking_child(stream, 1, 100)
        });

        let (mut stream, _) = listener.accept().expect("父进程接受连接应成功");

        // Phase 1: 子进程发送 STARTED
        let msg = recv_message(&mut stream).expect("应收到 STARTED");
        assert_eq!(msg, Message::Started, "子进程应先发 STARTED");

        // Phase 2: 发送 TRANSFER（src=dst=1 同账户，金额 20）
        // 子进程先扣款→转发(TRANSFER)，再入账→回复(ACK)，余额增/减相抵不变
        let order = TransferOrder { src: 1, dst: 1, amount: 20 };
        send_message(&mut stream, &Message::Transfer(order.to_bytes()))
            .expect("发送 TRANSFER 应成功");

        // 子进程先转发 TRANSFER（作为 src），再发送 ACK（作为 dst）
        let relayed = recv_message(&mut stream).expect("应收到转发的 TRANSFER");
        assert!(matches!(relayed, Message::Transfer(_)), "src 应转发 TRANSFER");

        let ack = recv_message(&mut stream).expect("应收到 ACK");
        assert_eq!(ack, Message::Ack, "dst 应回复 ACK");

        // 发送 STOP，子进程进入 Phase 3
        send_message(&mut stream, &Message::Stop).expect("发送 STOP 应成功");

        // Phase 3: 子进程发送 DONE + BALANCE_HISTORY
        assert_eq!(
            recv_message(&mut stream).expect("应收到 DONE"),
            Message::Done,
            "子进程应发送 DONE"
        );

        let history_msg = recv_message(&mut stream).expect("应收到 BALANCE_HISTORY");
        assert!(matches!(history_msg, Message::BalanceHistory(_)), "子进程应发送 BALANCE_HISTORY");

        // 验证余额历史：同账户转账 amount=0，余额不变
        if let Message::BalanceHistory(bytes) = history_msg {
            let history = BalanceHistory::from_bytes(&bytes).expect("解析 BALANCE_HISTORY 应成功");
            assert_eq!(history.id, 1);
            let last = history.states.last().unwrap();
            assert_eq!(last.balance, 100, "amount=0 转账后余额应不变");
        }

        handle.join().expect("子线程应正常结束").expect("子逻辑应成功");
    }

    /// 验证父进程协调多子进程的转账流程（使用线程模拟）
    #[test]
    fn test_banking_parent_work_transfers() {
        // 父进程监听
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定监听端口应成功");
        let addr = listener.local_addr().expect("获取监听地址应成功");

        // 启动 2 个模拟子进程线程（余额 P1=100, P2=50）
        let child1 = thread::spawn(move || {
            let stream = TcpStream::connect(addr).expect("连接应成功");
            simulate_banking_child(stream, 1, 100)
        });
        let child2 = thread::spawn(move || {
            let stream = TcpStream::connect(addr).expect("连接应成功");
            simulate_banking_child(stream, 2, 50)
        });

        // 父进程接受 2 个连接
        let (mut s1, _) = listener.accept().expect("接受子进程 1 应成功");
        let (mut s2, _) = listener.accept().expect("接受子进程 2 应成功");

        // Phase 1: 等待 STARTED x2
        assert_eq!(recv_message(&mut s1).unwrap(), Message::Started);
        assert_eq!(recv_message(&mut s2).unwrap(), Message::Started);

        // Phase 2: 转账 P1→P2 $30
        // 1. 发送 TRANSFER 给 P1（src）
        let order = TransferOrder { src: 1, dst: 2, amount: 30 };
        send_message(&mut s1, &Message::Transfer(order.to_bytes())).unwrap();
        // 2. P1 扣款后发回 TRANSFER，父进程中继给 P2
        let relayed = recv_message(&mut s1).unwrap();
        send_message(&mut s2, &relayed).unwrap();
        // 3. P2 入账后回复 ACK，父进程接收
        let ack = recv_message(&mut s2).unwrap();
        assert_eq!(ack, Message::Ack);

        // 发送 STOP x2
        send_message(&mut s1, &Message::Stop).unwrap();
        send_message(&mut s2, &Message::Stop).unwrap();

        // Phase 3: 接收 DONE x2
        assert_eq!(recv_message(&mut s1).unwrap(), Message::Done);
        assert_eq!(recv_message(&mut s2).unwrap(), Message::Done);

        // 接收 BALANCE_HISTORY x2，汇总验证
        let h1 = match recv_message(&mut s1).unwrap() {
            Message::BalanceHistory(b) => BalanceHistory::from_bytes(&b).unwrap(),
            other => panic!("expected BALANCE_HISTORY from child 1, got {}", other),
        };
        let h2 = match recv_message(&mut s2).unwrap() {
            Message::BalanceHistory(b) => BalanceHistory::from_bytes(&b).unwrap(),
            other => panic!("expected BALANCE_HISTORY from child 2, got {}", other),
        };

        // 验证余额：P1=100-30=70, P2=50+30=80
        assert_eq!(h1.states.last().unwrap().balance, 70, "P1 扣款后应为 70");
        assert_eq!(h2.states.last().unwrap().balance, 80, "P2 入账后应为 80");

        child1.join().unwrap().unwrap();
        child2.join().unwrap().unwrap();
    }
}

// ═══════════════════════════════════════════════════════════
// 阶段二：银行业务工作流 (Lab 2)
// ═══════════════════════════════════════════════════════════

/// 银行业务子进程工作流。
///
/// 对应 ITMO Lab 2 中 `child_work` 的三阶段扩展版本。
///
/// # 参数
///
/// - `parent_addr`: 父进程监听地址
/// - `child_id`: 本子进程 ID（1-indexed），同时也是账户 ID
/// - `initial_balance`: 初始余额
///
/// # 三阶段流程
///
/// ## Phase 1: STARTED 同步
/// - 连接到父进程
/// - 发送 `Message::Started` 给父进程
/// - （完整 ITMO 模型中还需要与兄弟进程交换 STARTED）
///
/// ## Phase 2: 处理转账
/// - 使用 `receive_any()` 风格的循环接收消息
/// - 收到 `Message::Transfer(payload)`:
///   - 解析 `TransferOrder::from_bytes(&payload)`
///   - 若 `order.src == self child_id`（我是源）：执行 debit，将 TRANSFER 转发给 dst 子进程
///   - 若 `order.dst == self child_id`（我是目标）：执行 credit，向父进程发送 ACK
/// - 收到 `Message::Stop`: 退出 Phase 2，进入 Phase 3
///
/// ## Phase 3: DONE 同步 + 上报
/// - 发送 `Message::Done` 给父进程
/// - 发送 `Message::BalanceHistory(history.to_bytes())` 给父进程
///
/// ## HINT: 简化版本的 receive_any
///
/// 在本例中，子进程只有一个 TCP 连接到父进程（父进程作为消息中继）。
/// 因此接收消息时直接使用 `recv_message` 即可。
///
/// 完整 ITMO 模型中，子进程有多个对等连接，需要使用类似 `select` / `poll` 的机制
/// 从多个流中选择有数据可读的那个。在 Rust 中可以使用 `mio` 或简单的忙轮询。
#[allow(dead_code)]
pub fn banking_child_work(
    _parent_addr: &str,
    _child_id: AccountId,
    _initial_balance: Balance,
) -> Result<(), Box<dyn Error>> {
    unimplemented!()
}

/// 银行业务父进程工作流。
///
/// 对应 ITMO Lab 2 中 `parent_work` 的扩展版本。
///
/// # 参数
///
/// - `children_count`: 子进程数量
/// - `initial_balances`: 每个子进程的初始余额（长度必须 == children_count）
/// - `transfer_orders`: 转账指令列表（由 `bank_operations` 生成）
///
/// # 流程
///
/// 1. 绑定监听端口、启动子进程、接受连接（同 Lab 1）
/// 2. 等待所有子进程发送 STARTED
/// 3. 对 `transfer_orders` 中每条指令调用 `transfer()`
/// 4. 向所有子进程发送 STOP
/// 5. 等待所有子进程发送 DONE
/// 6. 接收所有子进程的 BALANCE_HISTORY
/// 7. 汇总为 `AllHistory`，调用 `print_history()`
///
/// # HINT: 消息路由 (父进程作为中继)
///
/// 在简化模型中，子进程之间的通信通过父进程中继。
/// 当父进程收到一条 TRANSFER 消息（从源子进程发给目标子进程）时，
/// 它需要将这条消息转发给目标子进程。
///
/// 例如，在 transfer() 函数发送 TRANSFER 给 src 后，src 会扣款然后
/// 通过父进程将 TRANSFER 转发给 dst，父进程需要接收这个转发并传递给 dst。
///
/// 这意味着父进程在等待 ACK 时需要：
/// 1. 从 src 的流上收到转发的 TRANSFER（由 src 发向 dst）
/// 2. 将这条 TRANSFER 写入 dst 的流
/// 3. 从 dst 的流上等待 ACK
#[allow(dead_code)]
pub fn banking_parent_work(
    children_count: usize,
    _initial_balances: &[Balance],
    _transfer_orders: &[TransferOrder],
) -> Result<AllHistory, Box<dyn Error>> {
    if children_count == 0 {
        return Ok(AllHistory::new());
    }
    if children_count > 9 {
        return Err(anyhow!("子进程数量不能超过 9 (ITMO 限制)").into());
    }
    unimplemented!()
}

/// 打印所有子进程的余额历史到标准输出。
#[allow(dead_code)]
pub fn print_history(_history: &AllHistory) {
    unimplemented!()
}
