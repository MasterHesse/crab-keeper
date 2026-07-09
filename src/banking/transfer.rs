//! 跨分支转账协议（链式，同步）。
//!
//! ```text
//! Parent                        Source Child                Dest Child
//!   │                              │                           │
//!   │── TRANSFER(order) ──────────►│                           │
//!   │                              │ 扣款 (debit)              │
//!   │                              │── TRANSFER(order) ───────►│
//!   │                              │                           │ 入账 (credit)
//!   │◄─────────────────────────────────────────── ACK ────────│
//! ```
//!
//! 父进程向源子进程发送转账指令 → 源扣款后转发给目标 → 目标入账后确认。
//! 链式设计保证了扣款+入账的原子性。

use std::net::TcpStream;

use crate::banking::types::{AccountId, Balance, TransferOrder};
use crate::communication::{recv_message, send_message, Message};

/// 银行系统错误类型。
#[derive(Debug)]
pub enum BankingError {
    AccountNotFound(AccountId),
    InsufficientFunds { account: AccountId, balance: Balance, required: Balance },
    ProtocolError(String),
    NetworkError(String),
    SerializationError(String),
}

impl std::fmt::Display for BankingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountNotFound(id) => write!(f, "账户未找到: {id}"),
            Self::InsufficientFunds { account, balance, required } => {
                write!(f, "账户 {account} 余额不足: 现有 {balance}，需要 {required}")
            }
            Self::ProtocolError(msg) => write!(f, "协议错误: {msg}"),
            Self::NetworkError(msg) => write!(f, "网络错误: {msg}"),
            Self::SerializationError(msg) => write!(f, "序列化错误: {msg}"),
        }
    }
}

impl std::error::Error for BankingError {}

/// 执行一笔跨分支转账，由父进程调用。
///
/// `child_streams[i-1]` 为进程 ID=i 的连接，AccountId 为 1-indexed。
#[allow(dead_code)]
pub fn transfer(
    child_streams: &mut [TcpStream],
    src: AccountId,
    dst: AccountId,
    amount: Balance,
) -> Result<(), BankingError> {
    let order = TransferOrder { src, dst, amount };
    let payload = order.to_bytes();

    let src_idx = (src - 1) as usize;
    send_message(&mut child_streams[src_idx], &Message::Transfer(payload))
        .map_err(|e| BankingError::NetworkError(e.to_string()))?;

    let dst_idx = (dst - 1) as usize;
    let msg = recv_message(&mut child_streams[dst_idx])
        .map_err(|e| BankingError::NetworkError(e.to_string()))?;

    match msg {
        Message::Ack => Ok(()),
        other => Err(BankingError::ProtocolError(format!("expected ACK, got {}", other))),
    }
}

#[cfg(test)]
mod transfer_tests {
    use super::*;
    use std::io;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn create_n_connection_pairs(n: usize) -> io::Result<(Vec<TcpStream>, Vec<TcpStream>)> {
        let mut parent_streams = Vec::with_capacity(n);
        let mut child_streams = Vec::with_capacity(n);
        for _ in 0..n {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let addr = listener.local_addr()?;
            let parent_stream = TcpStream::connect(addr)?;
            let (child_stream, _) = listener.accept()?;
            parent_streams.push(parent_stream);
            child_streams.push(child_stream);
        }
        Ok((parent_streams, child_streams))
    }

    #[test]
    fn test_transfer_same_account() {
        let (mut parent_streams, mut child_streams) =
            create_n_connection_pairs(1).expect("创建连接对应成功");

        let handle = thread::spawn(move || {
            let mut stream = child_streams.pop().unwrap();
            let msg = recv_message(&mut stream).expect("子进程应收到 TRANSFER");
            assert!(matches!(msg, Message::Transfer(_)));
            send_message(&mut stream, &Message::Ack).expect("发送 ACK 应成功");
        });

        transfer(&mut parent_streams, 1, 1, 100).expect("同账户转账应成功");
        handle.join().expect("子线程应正常结束");
    }

    #[test]
    fn test_transfer_cross_account() {
        let (mut parent_streams, mut child_streams) =
            create_n_connection_pairs(2).expect("创建连接对应成功");

        let dst_stream = child_streams.pop().unwrap();
        let src_stream = child_streams.pop().unwrap();

        let dst_handle = thread::spawn(move || {
            let mut stream = dst_stream;
            send_message(&mut stream, &Message::Ack).expect("dst 发送 ACK 应成功");
        });
        let src_handle = thread::spawn(move || {
            let mut stream = src_stream;
            let msg = recv_message(&mut stream).expect("src 应收到 TRANSFER");
            assert!(matches!(msg, Message::Transfer(_)));
        });

        transfer(&mut parent_streams, 1, 2, 100).expect("跨账户转账应成功");
        dst_handle.join().expect("dst 线程应正常结束");
        src_handle.join().expect("src 线程应正常结束");
    }

    #[test]
    fn test_transfer_unexpected_response() {
        let (mut parent_streams, mut child_streams) =
            create_n_connection_pairs(2).expect("创建连接对应成功");

        let dst_stream = child_streams.pop().unwrap();
        let _src_stream = child_streams.pop().unwrap();

        let _dst_handle = thread::spawn(move || {
            let mut stream = dst_stream;
            send_message(&mut stream, &Message::Done).expect("发送 DONE 应成功");
        });

        let result = transfer(&mut parent_streams, 1, 2, 100);
        assert!(result.is_err());
        match result {
            Err(BankingError::ProtocolError(msg)) => assert!(msg.contains("expected ACK")),
            _ => panic!("预期 ProtocolError"),
        }
    }
}
