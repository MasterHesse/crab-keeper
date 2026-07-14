//! 集成测试 — 验证父进程/子进程通过真实进程生成的端到端同步。
//!
//! 这些测试通过 `cargo test` 在同一个机器上模拟分布式场景。
//! 父进程代码和子进程代码在不同线程/进程中执行，
//! 仅通过 TCP 消息传递进行通信。

use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// 查找 crab-keeper 主二进制文件路径。
///
/// 在 `cargo test` 环境中，`current_exe()` 返回的是测试二进制
/// （如 `target/debug/deps/communication_integration_test-xxx`），
/// 而我们需要的是主二进制 `target/debug/crab-keeper`。
///
/// 查找策略（按优先级）：
/// 1. `CARGO_BIN_EXE_crab-keeper` 编译期环境变量
/// 2. 从测试二进制路径推算（`deps/` 的父目录 + `crab-keeper`）
fn find_main_binary() -> Option<PathBuf> {
    // 策略 1: Cargo 编译期注入的路径
    if let Some(path) = option_env!("CARGO_BIN_EXE_crab-keeper") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // 策略 2: 从测试二进制路径推算
    // 测试二进制: target/debug/deps/communication_integration_test-<hash>
    // 主二进制:   target/debug/crab-keeper
    let test_exe = std::env::current_exe().ok()?;
    let build_dir = test_exe.parent()?.parent()?; // deps/ → debug/
    let main_bin = build_dir.join("crab-keeper");
    if main_bin.exists() {
        return Some(main_bin);
    }

    None
}

/// 在后台启动一个 crab-keeper 子进程，连接到指定地址。
///
/// 如果找不到主二进制文件（例如只执行了 `cargo test` 而未 `cargo build`），
/// 子进程无法启动，调用方应据此跳过测试。
fn spawn_child_process(parent_addr: &str, child_id: usize) -> io::Result<Child> {
    let exe = find_main_binary()
        .ok_or_else(|| io::Error::other("找不到 crab-keeper 主二进制，请先执行 cargo build"))?;

    Ok(Command::new(exe)
        .arg("--child")
        .arg(parent_addr)
        .env("CHILD_ID", child_id.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?)
}

/// 验证父进程能正确协调单个子进程完成 STARTED → DONE 流程。
///
/// 此测试需要先通过 `cargo build` 编译主二进制。
#[test]
fn test_integration_parent_child_sync() {
    let main_bin = find_main_binary();
    if main_bin.is_none() {
        eprintln!("SKIP: 找不到 crab-keeper 主二进制，跳过进程集成测试");
        return;
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定监听端口失败");
    let addr = listener.local_addr().unwrap();

    // 设置 accept 超时，防止子进程异常时永久阻塞
    listener.set_nonblocking(true).expect("设置非阻塞模式失败");

    // 启动子进程
    let mut child = spawn_child_process(&addr.to_string(), 0).expect("spawn 子进程失败");

    // 带超时的 accept：轮询最多 5 秒
    let mut stream = {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() > deadline {
                        panic!("等待子进程连接超时 (5s)");
                    }
                    std::thread::sleep(Duration::from_millis(100));
                },
                Err(e) => panic!("accept 错误: {e}"),
            }
        }
    };
    // 恢复阻塞模式用于后续读写
    stream.set_nonblocking(false).ok();

    // 接收 STARTED
    use crab_keeper::communication::{Message, recv_message, send_message};
    let msg = recv_message(&mut stream).expect("接收 STARTED 失败");
    assert_eq!(msg, Message::Started, "子进程应首先发送 STARTED");

    // 发送工作分配
    send_message(&mut stream, &Message::Data(b"hello from parent".to_vec()))
        .expect("发送工作分配失败");

    // 接收 DONE
    let msg = recv_message(&mut stream).expect("接收 DONE 失败");
    assert_eq!(msg, Message::Done, "子进程最后应发送 DONE");

    // 等待子进程退出
    let status = child.wait().expect("等待子进程退出失败");
    assert!(status.success(), "子进程应以 0 退出，实际: {status}");
}

/// 验证多个子进程能并发地完成 STARTED → DONE 同步。
///
/// 此测试需要先通过 `cargo build` 编译主二进制。
#[test]
fn test_integration_multiple_children() {
    let main_bin = find_main_binary();
    if main_bin.is_none() {
        eprintln!("SKIP: 找不到 crab-keeper 主二进制，跳过进程集成测试");
        return;
    }

    use crab_keeper::communication::{Message, recv_message, send_message};

    const N: usize = 3;
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定监听端口失败");
    let addr = listener.local_addr().unwrap();

    // 设置 accept 超时
    listener.set_nonblocking(true).expect("设置非阻塞模式失败");

    // 启动 N 个子进程
    let mut children: Vec<Child> = (0..N)
        .map(|i| spawn_child_process(&addr.to_string(), i).expect("spawn 子进程失败"))
        .collect();

    // 带超时的多连接 accept
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut streams: Vec<TcpStream> = Vec::with_capacity(N);
    while streams.len() < N {
        if std::time::Instant::now() > deadline {
            panic!("等待 {} 个子进程连接超时 (仅收到 {})", N, streams.len());
        }
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).ok();
                streams.push(stream);
            },
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            },
            Err(e) => panic!("accept 错误: {e}"),
        }
    }

    // 检查所有 STARTED
    for (i, stream) in streams.iter_mut().enumerate() {
        let msg =
            recv_message(stream).unwrap_or_else(|e| panic!("子进程 {i} STARTED 接收失败: {e}"));
        assert_eq!(msg, Message::Started, "子进程 {i} 应发送 STARTED");
    }

    // 分发工作
    for (i, stream) in streams.iter_mut().enumerate() {
        send_message(stream, &Message::Data(format!("work #{i}").into_bytes()))
            .unwrap_or_else(|e| panic!("向子进程 {i} 发送工作失败: {e}"));
    }

    // 检查所有 DONE
    for (i, stream) in streams.iter_mut().enumerate() {
        let msg = recv_message(stream).unwrap_or_else(|e| panic!("子进程 {i} DONE 接收失败: {e}"));
        assert_eq!(msg, Message::Done, "子进程 {i} 应发送 DONE");
    }

    // 等待所有子进程退出
    for (i, mut child) in children.drain(..).enumerate() {
        let status = child.wait().unwrap_or_else(|e| panic!("等待子进程 {i} 退出失败: {e}"));
        assert!(status.success(), "子进程 {i} 应以 0 退出, 实际: {status}");
    }
}

/// 验证子进程在连接失败时能正常报错退出。
///
/// 此测试需要先通过 `cargo build` 编译主二进制。
#[test]
fn test_integration_child_connection_refused() {
    let main_bin = find_main_binary();
    if main_bin.is_none() {
        eprintln!("SKIP: 找不到 crab-keeper 主二进制，跳过进程集成测试");
        return;
    }

    // 使用一个不可达的地址（端口 1 需要 root 权限，普通用户无法绑定）
    let mut child = match spawn_child_process("127.0.0.1:1", 99) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("SKIP: 无法启动子进程");
            return;
        },
    };

    // 子进程应该以非 0 状态码退出（连接被拒绝）
    let status = child.wait().expect("等待子进程退出失败");
    assert!(!status.success(), "连接被拒绝时，子进程应以非 0 状态码退出");
}
