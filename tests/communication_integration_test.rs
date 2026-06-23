//! 集成测试 — 验证父进程/子进程通过真实进程生成的端到端同步。
//!
//! 这些测试通过 `cargo test` 在同一个机器上模拟分布式场景。
//! 父进程代码和子进程代码在不同线程/进程中执行，
//! 仅通过 TCP 消息传递进行通信。

use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};

/// 在后台启动一个 crab-keeper 子进程，连接到指定地址
fn spawn_child_process(parent_addr: &str, child_id: usize) -> Child {
    let exe = std::env::current_exe().expect("获取当前可执行文件路径失败");
    Command::new(exe)
        .arg("--child")
        .arg(parent_addr)
        .env("CHILD_ID", child_id.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn 子进程失败")
}

/// 验证父进程能正确协调单个子进程完成 STARTED → DONE 流程
///
/// 此测试需要 `channel` 和 `process` 的 TODO 全部实现完成后才可通过。
#[test]
fn test_integration_parent_child_sync() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .expect("绑定监听端口失败");
    let addr = listener.local_addr().unwrap();

    // 启动子进程
    let mut child = spawn_child_process(&addr.to_string(), 0);

    // 父进程：接受连接
    let (mut stream, _) = listener.accept().expect("接受子进程连接失败");

    // 接收 STARTED
    use crab_keeper::communication::{recv_message, send_message, Message};
    let msg = recv_message(&mut stream).expect("接收 STARTED 失败");
    assert_eq!(
        msg,
        Message::Started,
        "子进程应首先发送 STARTED"
    );

    // 发送工作分配
    send_message(&mut stream, &Message::Data(b"hello from parent".to_vec()))
        .expect("发送工作分配失败");

    // 接收 DONE
    let msg = recv_message(&mut stream).expect("接收 DONE 失败");
    assert_eq!(msg, Message::Done, "子进程最后应发送 DONE");

    // 等待子进程退出
    let status = child.wait().expect("等待子进程退出失败");
    assert!(status.success(), "子进程应以 0 退出");
}

/// 验证多个子进程能并发地完成 STARTED → DONE 同步
#[test]
fn test_integration_multiple_children() {
    use crab_keeper::communication::{recv_message, send_message, Message};

    const N: usize = 3;
    let listener = TcpListener::bind("127.0.0.1:0")
        .expect("绑定监听端口失败");
    let addr = listener.local_addr().unwrap();

    // 启动 N 个子进程
    let mut children: Vec<Child> = (0..N)
        .map(|i| spawn_child_process(&addr.to_string(), i))
        .collect();

    // 父进程：接受 N 个连接
    let mut streams: Vec<TcpStream> = Vec::with_capacity(N);
    for _ in 0..N {
        let (stream, _) = listener.accept().expect("接受连接失败");
        streams.push(stream);
    }

    // 检查所有 STARTED
    for (i, stream) in streams.iter_mut().enumerate() {
        let msg = recv_message(stream)
            .unwrap_or_else(|e| panic!("子进程 {i} STARTED 接收失败: {e}"));
        assert_eq!(msg, Message::Started, "子进程 {i} 应发送 STARTED");
    }

    // 分发工作
    for (i, stream) in streams.iter_mut().enumerate() {
        send_message(
            stream,
            &Message::Data(format!("work #{i}").into_bytes()),
        )
        .unwrap_or_else(|e| panic!("向子进程 {i} 发送工作失败: {e}"));
    }

    // 检查所有 DONE
    for (i, stream) in streams.iter_mut().enumerate() {
        let msg = recv_message(stream)
            .unwrap_or_else(|e| panic!("子进程 {i} DONE 接收失败: {e}"));
        assert_eq!(msg, Message::Done, "子进程 {i} 应发送 DONE");
    }

    // 等待所有子进程退出
    for (i, mut child) in children.drain(..).enumerate() {
        let status = child.wait().unwrap_or_else(|e| {
            panic!("等待子进程 {i} 退出失败: {e}")
        });
        assert!(status.success(), "子进程 {i} 应以 0 退出, 实际: {status}");
    }
}

/// 验证子进程在连接失败时能正常报错退出
#[test]
fn test_integration_child_connection_refused() {
    // 使用一个不存在的端口——没有监听者
    let mut child = spawn_child_process("127.0.0.1:1", 99);

    // 子进程应该以非 0 状态码退出（连接被拒绝）
    let status = child.wait().expect("等待子进程退出失败");
    assert!(
        !status.success(),
        "连接被拒绝时，子进程应以非 0 状态码退出"
    );
}
