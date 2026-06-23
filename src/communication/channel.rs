//! 基于 TCP 的消息发送 / 接收原语。
//!
//! ## 你需要完成的 TODO
//!
//! 1. **实现 `send_message`** — 将 `Message` 通过 `TcpStream` 发送
//! 2. **实现 `recv_message`** — 从 `TcpStream` 接收并解析 `Message`
//!
//! ## 设计思路
//!
//! TCP 是面向字节流的协议，不像 UDP 有消息边界。因此我们需要在应用层
//! 定义消息分帧（framing）。本模块采用 **长度前缀分帧**（length-prefixed framing）：
//!
//! ```text
//! ┌────────────────────┬───────────────────────┐
//! │  frame_len (8B)    │  message_bytes (变长)  │
//! │  u64 大端序        │  由 Message::to_bytes  │
//! │                    │  产生的字节流           │
//! └────────────────────┴───────────────────────┘
//! ```
//!
//! 其中 `frame_len` = `message_bytes.len()` 作为 u64 大端序。
//!
//! ## HINT: 从 TcpStream 精确读取 N 字节
//!
//! `TcpStream::read(&mut buf)` 可能一次返回少于请求的字节数（分片到达），
//! 因此你需要用 `read_exact` 或一个循环来确保读满指定字节：
//!
//! ```rust,ignore
//! use std::io::Read;
//!
//! fn read_exact_n(stream: &mut TcpStream, n: usize) -> io::Result<Vec<u8>> {
//!     let mut buf = vec![0u8; n];
//!     stream.read_exact(&mut buf)?;
//!     Ok(buf)
//! }
//! ```
//!
//! [`std::io::Read::read_exact`](https://doc.rust-lang.org/std/io/trait.Read.html#method.read_exact)
//!
//! ## HINT: 从 TcpStream 写入全部数据
//!
//! ```rust,ignore
//! use std::io::Write;
//! stream.write_all(&data)?;
//! ```
//!
//! [`std::io::Write::write_all`](https://doc.rust-lang.org/std/io/trait.Write.html#method.write_all)

use crate::communication::Message;
use std::io::{self, Read, Write};
use std::net::TcpStream;

/// 通过 TCP 流发送一条消息。
///
/// 先发送 8 字节帧长度（大端序 u64），再发送帧内容（由 `Message::to_bytes` 编码）。
///
/// # 参数
///
/// - `_stream`: 已建立的 TCP 连接（可写端）
/// - `_msg`: 待发送的消息
///
/// # 错误
///
/// 网络 I/O 错误时返回 `io::Error`。
///
/// # 示例
///
/// ```rust,ignore
/// use std::net::TcpStream;
/// use crab_keeper::communication::{send_message, Message};
///
/// let mut stream = TcpStream::connect("127.0.0.1:9000")?;
/// send_message(&mut stream, &Message::Started)?;
/// ```
#[allow(dead_code)]
pub fn send_message(stream: &mut TcpStream, msg: &Message) -> io::Result<()> {
    let msg_bytes = msg.to_bytes();
    let frame_len =  (msg_bytes.len() as u64).to_be_bytes();
    stream.write_all(&frame_len)?;
    stream.write_all(&msg_bytes)?;
    Ok(())
}

/// 从 TCP 流接收一条消息。
///
/// 先读取 8 字节帧长度，再读取对应长度的帧内容，最后调用 `Message::from_bytes` 解码。
///
/// # 参数
///
/// - `_stream`: 已建立的 TCP 连接（可读端）
///
/// # 错误
///
/// - 网络 I/O 错误时返回 `io::Error`
/// - 消息解码失败时返回 `io::Error::other(...)`
///
/// # 示例
///
/// ```rust,ignore
/// use std::net::TcpStream;
/// use crab_keeper::communication::recv_message;
///
/// let mut stream = TcpStream::connect("127.0.0.1:9000")?;
/// let msg = recv_message(&mut stream)?;
/// println!("收到: {msg}");
/// ```
#[allow(dead_code)]
pub fn recv_message(stream: &mut TcpStream) -> io::Result<Message> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;
    let frame_len = u64::from_be_bytes(header) as usize;
    let mut msg_body = vec![0u8; frame_len];
    stream.read_exact(&mut msg_body)?;
    match Message::from_bytes(&msg_body){
        Ok(msg) => Ok(msg),
        Err(err) => Err(io::Error::other(err))
    }  
}

#[cfg(test)]
mod channel_tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    /// 在本地回环上创建一个 TCP 连接对 (client, server)
    fn create_connection_pair() -> io::Result<(TcpStream, TcpStream)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let client = TcpStream::connect(addr)?;
        let (server, _) = listener.accept()?;
        Ok((client, server))
    }

    /// 验证发送单条 STARTED 消息能正确接收
    #[test]
    fn test_send_recv_single_message() {
        let (mut client, mut server) =
            create_connection_pair().expect("创建连接对应成功");

        // 在另一个线程接收，避免死锁
        let handle = thread::spawn(move || recv_message(&mut server));

        send_message(&mut client, &Message::Started)
            .expect("发送 STARTED 应成功");

        let received = handle.join().expect("线程应正常结束").expect("接收应成功");
        assert_eq!(received, Message::Started);
    }

    /// 验证连续发送多条消息都能正确接收
    #[test]
    fn test_send_recv_multiple_messages() {
        let (mut client, mut server) =
            create_connection_pair().expect("创建连接对应成功");

        let handle = thread::spawn(move || {
            let msg1 = recv_message(&mut server).unwrap();
            let msg2 = recv_message(&mut server).unwrap();
            let msg3 = recv_message(&mut server).unwrap();
            (msg1, msg2, msg3)
        });

        send_message(&mut client, &Message::Started).unwrap();
        send_message(&mut client, &Message::Data(b"hello".to_vec())).unwrap();
        send_message(&mut client, &Message::Done).unwrap();

        let (m1, m2, m3) = handle.join().unwrap();
        assert_eq!(m1, Message::Started);
        assert_eq!(m2, Message::Data(b"hello".to_vec()));
        assert_eq!(m3, Message::Done);
    }
}
