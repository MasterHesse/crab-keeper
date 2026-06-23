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
//! ```
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
//! ```
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

use crate::communication::{recv_message, send_message, Message};
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::process::Command;

use anyhow::anyhow;

/// 子进程启动时传入的命令行参数标志
pub const CHILD_ARG: &str = "--child";

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
        eprintln!(
            "[子进程 #{id}] PID={pid}, 连接父进程: {parent_addr}",
        );
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;
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
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("绑定监听端口应成功");
        let addr = listener.local_addr().unwrap();

        // 子线程：模拟子进程行为
        let child_handle = thread::spawn(move || {
            run_child_worker(&addr.to_string())
        });

        // 父进程：接受连接，验证 STARTED 和 DONE
        let (mut stream, _) = listener.accept().expect("接受连接应成功");

        // 检查 STARTED
        let started_msg =
            recv_message(&mut stream).expect("接收 STARTED 应成功");
        assert_eq!(
            started_msg,
            Message::Started,
            "第一条消息应为 STARTED"
        );

        // 发送工作分配
        send_message(
            &mut stream,
            &Message::Data(b"do work".to_vec()),
        )
        .expect("发送工作分配应成功");

        // 检查 DONE
        let done_msg =
            recv_message(&mut stream).expect("接收 DONE 应成功");
        assert_eq!(done_msg, Message::Done, "最后一条消息应为 DONE");

        // 确认子线程正常完成
        child_handle.join().expect("子线程应正常结束").expect("子逻辑应成功");
    }

    /// 验证多子进程并发 STARTED 同步
    #[test]
    fn test_process_start_sync_multiple_children() {
        const N: usize = 3;
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("绑定监听端口应成功");
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
            let (stream, _) =
                listener.accept().expect("接受连接应成功");
            streams.push(stream);
        }

        // 检查所有 STARTED
        for (i, stream) in streams.iter_mut().enumerate() {
            let msg =
                recv_message(stream).expect(&format!("子进程 {i} STARTED 接收应成功"));
            assert_eq!(msg, Message::Started, "子进程 {i} 应先发 STARTED");
        }

        // 发送工作分配
        for (i, stream) in streams.iter_mut().enumerate() {
            send_message(
                stream,
                &Message::Data(format!("work for child {i}").into_bytes()),
            )
            .expect("发送工作应成功");
        }

        // 检查所有 DONE
        for (i, stream) in streams.iter_mut().enumerate() {
            let msg =
                recv_message(stream).expect(&format!("子进程 {i} DONE 接收应成功"));
            assert_eq!(msg, Message::Done, "子进程 {i} 最后应发 DONE");
        }

        // 确认所有子线程正常完成
        for handle in child_handles {
            handle.join().expect("子线程应正常结束").expect("子逻辑应成功");
        }
    }
}
