//! 消息类型定义与序列化。
//!
//! ## 序列化格式说明 (二进制协议)
//!
//! 每条消息采用 Tag-Length-Value (TLV) 格式：
//!
//! ```text
//! ┌──────────┬──────────────────┬──────────────────────┐
//! │ tag (1B) │ payload_len (8B) │ payload (变长, 大端)  │
//! └──────────┴──────────────────┴──────────────────────┘
//! ```
//!
//! ### Tag 分配表 (阶段一 + 阶段二)
//!
//! | tag | 消息类型 | 说明 |
//! |-----|---------|------|
//! | `0x00` | STARTED | 子进程就绪 (Lab 1) |
//! | `0x01` | DONE | 子进程完成 (Lab 1) |
//! | `0x02` | DATA | 通用数据负载 (Lab 1) |
//! | `0x03` | TRANSFER | 转账指令 (Lab 2 新增) |
//! | `0x04` | ACK | 转账确认 (Lab 2 新增) |
//! | `0x05` | STOP | 停止信号 (Lab 2 新增) |
//! | `0x06` | BALANCE_HISTORY | 余额历史 (Lab 2 新增) |
//!
//! ### 示例
//!
//! ```text
//! STARTED  → [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
//! DONE     → [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
//! DATA(b"hi") → [0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, b'h', b'i']
//! TRANSFER(order_10bytes) → [0x03, ...8B len=10..., order_bytes...]
//! ACK → [0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
//! STOP → [0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
//! ```
//! - [`Vec::extend_from_slice`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.extend_from_slice)

use std::fmt;

/// 分布式进程间通信的消息类型。
///
/// 覆盖阶段一和阶段二的消息需求：
/// - `Started` / `Done` — 进程生命周期同步 (Lab 1)
/// - `Data` — 承载任意业务数据 (Lab 1)
/// - `Transfer` / `Ack` / `Stop` / `BalanceHistory` — 银行系统协议 (Lab 2 新增)
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// 子进程已启动并就绪 (Child → Parent / Peer)
    Started,
    /// 子进程已完成工作 (Child → Parent / Peer)
    Done,
    /// 携带数据负载的消息 (双向，通用)
    Data(Vec<u8>),
    /// 转账指令 (Parent → Source Child; Source Child → Dest Child)
    /// payload = TransferOrder 序列化字节 (10B)
    Transfer(Vec<u8>),
    /// 转账确认 (Dest Child → Parent)
    /// payload 为空 (0B)
    Ack,
    /// 无更多转账，停止工作 (Parent → All Children)
    /// payload 为空 (0B)
    Stop,
    /// 余额历史上报 (Child → Parent)
    /// payload = BalanceHistory 序列化字节 (变长)
    BalanceHistory(Vec<u8>),
}

impl Message {
    /// 将消息序列化为字节流。
    ///
    /// # 序列化格式
    ///
    /// | 字段 | 字节数 | 说明 |
    /// |------|--------|------|
    /// | tag  | 1      | `0x00` = Started, `0x01` = Done, `0x02` = Data |
    /// | len  | 8      | 负载长度 (u64 大端序) |
    /// | data | 变长   | 负载内容 (仅 Data 类型) |
    ///
    /// # 示例
    ///
    /// ```
    /// use crab_keeper::communication::Message;
    ///
    /// let msg = Message::Started;
    /// let bytes = msg.to_bytes();
    /// assert_eq!(bytes.len(), 9); // 1B tag + 8B len
    /// ```
    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        match self {
            Message::Started => {
                buf.push(0x00);
                buf.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
            },
            Message::Done => {
                buf.push(0x01);
                buf.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
            },
            Message::Data(payload) => {
                buf.push(0x02);
                buf.extend_from_slice(&(payload.len() as u64).to_be_bytes());
                buf.extend_from_slice(payload);
            },
            Message::Transfer(payload) => {
                buf.push(0x03);
                buf.extend_from_slice(&(payload.len() as u64).to_be_bytes());
                buf.extend_from_slice(payload);
            },
            Message::Ack => {
                buf.push(0x04);
                buf.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
            },
            Message::Stop => {
                buf.push(0x05);
                buf.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
            },
            Message::BalanceHistory(payload) => {
                buf.push(0x06);
                buf.extend_from_slice(&(payload.len() as u64).to_be_bytes());
                buf.extend_from_slice(payload);
            },
        }

        buf
    }

    /// 从字节流反序列化为消息。
    ///
    /// # 错误
    ///
    /// - 字节流长度不足 9 (tag + len) 时返回错误
    /// - 字节流中声明的 payload 长度与实际剩余字节不符时返回错误
    /// - tag 值非法时返回错误
    ///
    /// # 示例
    ///
    /// ```
    /// use crab_keeper::communication::Message;
    ///
    /// let bytes = vec![0x01, 0,0,0,0,0,0,0,0]; // DONE
    /// let msg = Message::from_bytes(&bytes).unwrap();
    /// assert_eq!(msg, Message::Done);
    /// ```
    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 9 {
            return Err("消息太短".to_string());
        };

        let tag: u8 = bytes[0];

        let payload_len: u64 = u64::from_be_bytes(bytes[1..9].try_into().unwrap());

        if bytes[9..].len() as u64 != payload_len {
            return Err("消息长度不匹配".to_string());
        };

        match tag {
            0x00 => {
                if payload_len != 0 {
                    return Err("STARTED 消息长度必须为 0".to_string());
                }
                Ok(Message::Started)
            },
            0x01 => {
                if payload_len != 0 {
                    return Err("DONE 消息长度必须为 0".to_string());
                }
                Ok(Message::Done)
            },
            0x02 => Ok(Message::Data(bytes[9..].to_vec())),
            0x03 => Ok(Message::Transfer(bytes[9..].to_vec())),
            0x04 => {
                if payload_len == 0 {
                    Ok(Message::Ack)
                } else {
                    Err("ACK 消息长度必须为 0".to_string())
                }
            },
            0x05 => {
                if payload_len == 0 {
                    Ok(Message::Stop)
                } else {
                    Err("STOP 消息长度必须为 0".to_string())
                }
            },
            0x06 => Ok(Message::BalanceHistory(bytes[9..].to_vec())),
            _ => Err(format!("未知 tag: 0x{:02X}", tag)),
        }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Started => write!(f, "STARTED"),
            Self::Done => write!(f, "DONE"),
            Self::Data(payload) => {
                write!(f, "DATA({}B)", payload.len())
            },
            Self::Transfer(payload) => {
                write!(f, "TRANSFER({}B)", payload.len())
            },
            Self::Ack => write!(f, "ACK"),
            Self::Stop => write!(f, "STOP"),
            Self::BalanceHistory(payload) => {
                write!(f, "BALANCE_HISTORY({}B)", payload.len())
            },
        }
    }
}

#[cfg(test)]
mod message_tests {
    use super::*;

    /// 验证 STARTED 消息的序列化/反序列化往返正确
    #[test]
    fn test_message_serialization_roundtrip_started() {
        // 此测试在 RED 阶段会因为 todo!() 而 panic
        let msg = Message::Started;
        let bytes = msg.to_bytes();
        let decoded = Message::from_bytes(&bytes).expect("STARTED 反序列化应成功");
        assert_eq!(msg, decoded, "STARTED 往返序列化应保持不变");
        assert_eq!(bytes[0], 0x00, "STARTED 的 tag 应为 0x00");
    }

    /// 验证 DONE 消息的序列化/反序列化往返正确
    #[test]
    fn test_message_serialization_roundtrip_done() {
        let msg = Message::Done;
        let bytes = msg.to_bytes();
        let decoded = Message::from_bytes(&bytes).expect("DONE 反序列化应成功");
        assert_eq!(msg, decoded, "DONE 往返序列化应保持不变");
        assert_eq!(bytes[0], 0x01, "DONE 的 tag 应为 0x01");
    }

    /// 验证 DATA 消息的序列化/反序列化往返正确
    #[test]
    fn test_message_serialization_roundtrip_data() {
        let payload = b"hello, distributed world!".to_vec();
        let msg = Message::Data(payload.clone());
        let bytes = msg.to_bytes();
        let decoded = Message::from_bytes(&bytes).expect("DATA 反序列化应成功");
        assert_eq!(msg, decoded, "DATA 往返序列化应保持不变");
    }

    /// 验证空 DATA 消息也能正确序列化
    #[test]
    fn test_message_serialization_empty_data() {
        let msg = Message::Data(vec![]);
        let bytes = msg.to_bytes();
        let decoded = Message::from_bytes(&bytes).expect("空 DATA 反序列化应成功");
        assert_eq!(msg, decoded);
    }

    /// 验证非法 tag 会返回错误
    #[test]
    fn test_message_from_bytes_invalid_tag() {
        let bytes = vec![0xFF, 0, 0, 0, 0, 0, 0, 0, 0];
        let result = Message::from_bytes(&bytes);
        assert!(result.is_err(), "非法 tag 应返回错误");
    }

    /// 验证字节流长度不足时返回错误
    #[test]
    fn test_message_from_bytes_too_short() {
        let bytes = vec![0x00, 0, 0]; // 只有 3 字节，不足 9
        let result = Message::from_bytes(&bytes);
        assert!(result.is_err(), "字节流过短应返回错误");
    }

    /// 验证 payload 长度与实际不匹配时返回错误
    #[test]
    fn test_message_from_bytes_payload_len_mismatch() {
        // tag=0x02(DATA), len=5, 但实际无 payload
        let len_bytes = 5u64.to_be_bytes();
        let mut bytes = vec![0x02];
        bytes.extend_from_slice(&len_bytes);
        // 不追加 payload — 长度不匹配
        let result = Message::from_bytes(&bytes);
        assert!(result.is_err(), "payload 长度不匹配应返回错误");
    }

    // ═══════════════════════════════════════════════════════
    // 阶段二新增消息类型测试 (Lab 2)
    // ═══════════════════════════════════════════════════════

    /// 验证 TRANSFER 消息序列化 / 反序列化往返
    #[test]
    fn test_message_transfer_roundtrip() {
        let payload = vec![3, 7, 0, 0, 0, 0, 0, 0, 1, 244]; // src=3, dst=7, amount=500
        let msg = Message::Transfer(payload);
        let bytes = msg.to_bytes();
        let decoded = Message::from_bytes(&bytes).expect("TRANSFER 反序列化应成功");
        assert_eq!(decoded, msg, "TRANSFER 往返应一致");
        assert_eq!(bytes[0], 0x03, "tag 应为 0x03");
    }

    /// 验证 ACK 消息往返（无 payload）
    #[test]
    fn test_message_ack_roundtrip() {
        let msg = Message::Ack;
        let bytes = msg.to_bytes();
        let decoded = Message::from_bytes(&bytes).expect("ACK 反序列化应成功");
        assert_eq!(decoded, msg, "ACK 往返应一致");
    }

    /// 验证 STOP 消息往返（无 payload）
    #[test]
    fn test_message_stop_roundtrip() {
        let msg = Message::Stop;
        let bytes = msg.to_bytes();
        let decoded = Message::from_bytes(&bytes).expect("STOP 反序列化应成功");
        assert_eq!(decoded, msg, "STOP 往返应一致");
    }

    /// 验证 BALANCE_HISTORY 消息往返
    #[test]
    fn test_message_balance_history_roundtrip() {
        let payload = b"dummy history data".to_vec();
        let msg = Message::BalanceHistory(payload.clone());
        let bytes = msg.to_bytes();
        let decoded = Message::from_bytes(&bytes).expect("BALANCE_HISTORY 反序列化应成功");
        assert_eq!(decoded, msg, "BALANCE_HISTORY 往返应一致");
        assert_eq!(bytes[0], 0x06, "tag 应为 0x06");
    }

    /// 验证 ACK 的 to_bytes 第一个字节（tag）为 0x04
    #[test]
    fn test_message_ack_tag() {
        let bytes = Message::Ack.to_bytes();
        assert_eq!(bytes[0], 0x04, "ACK 的 tag 应为 0x04");
        // payload_len 后 8 字节全为 0
        assert_eq!(&bytes[1..9], &[0u8; 8], "ACK payload_len 应为 0");
    }

    /// 验证 STOP 的 to_bytes 第一个字节（tag）为 0x05
    #[test]
    fn test_message_stop_tag() {
        let bytes = Message::Stop.to_bytes();
        assert_eq!(bytes[0], 0x05, "STOP 的 tag 应为 0x05");
        assert_eq!(&bytes[1..9], &[0u8; 8], "STOP payload_len 应为 0");
    }

    /// 验证 TRANSFER 的 to_bytes 第一个字节（tag）为 0x03
    #[test]
    fn test_message_transfer_tag() {
        let msg = Message::Transfer(vec![1, 2, 3]);
        let bytes = msg.to_bytes();
        assert_eq!(bytes[0], 0x03, "TRANSFER 的 tag 应为 0x03");
    }

    /// 验证 ACK 的 from_bytes 拒绝非空 payload
    #[test]
    fn test_message_ack_rejects_payload() {
        // 构造 tag=0x04 但 payload_len=5 的字节流
        let len_bytes = 5u64.to_be_bytes();
        let mut bytes = vec![0x04];
        bytes.extend_from_slice(&len_bytes);
        bytes.extend_from_slice(&[0u8; 5]); // 追加 5 字节 payload
        assert!(Message::from_bytes(&bytes).is_err(), "ACK 带 payload 应返回错误");
    }

    /// 验证 STOP 的 from_bytes 拒绝非空 payload
    #[test]
    fn test_message_stop_rejects_payload() {
        let len_bytes = 3u64.to_be_bytes();
        let mut bytes = vec![0x05];
        bytes.extend_from_slice(&len_bytes);
        bytes.extend_from_slice(&[1u8, 2, 3]);
        assert!(Message::from_bytes(&bytes).is_err(), "STOP 带 payload 应返回错误");
    }
}
