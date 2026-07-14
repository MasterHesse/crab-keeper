use crate::communication::Message;
use std::io::{self, Read, Write};
use std::net::TcpStream;

#[allow(dead_code)]
pub fn send_message(stream: &mut TcpStream, msg: &Message) -> io::Result<()> {
    let msg_bytes = msg.to_bytes();
    let frame_len = (msg_bytes.len() as u64).to_be_bytes();
    stream.write_all(&frame_len)?;
    stream.write_all(&msg_bytes)?;
    Ok(())
}

#[allow(dead_code)]
pub fn recv_message(stream: &mut TcpStream) -> io::Result<Message> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;
    let frame_len = u64::from_be_bytes(header) as usize;
    let mut msg_body = vec![0u8; frame_len];
    stream.read_exact(&mut msg_body)?;
    match Message::from_bytes(&msg_body) {
        Ok(msg) => Ok(msg),
        Err(err) => Err(io::Error::other(err)),
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
        let (mut client, mut server) = create_connection_pair().expect("创建连接对应成功");

        // 在另一个线程接收，避免死锁
        let handle = thread::spawn(move || recv_message(&mut server));

        send_message(&mut client, &Message::Started).expect("发送 STARTED 应成功");

        let received = handle.join().expect("线程应正常结束").expect("接收应成功");
        assert_eq!(received, Message::Started);
    }

    /// 验证连续发送多条消息都能正确接收
    #[test]
    fn test_send_recv_multiple_messages() {
        let (mut client, mut server) = create_connection_pair().expect("创建连接对应成功");

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
