//! 分布式通信模块 (阶段一 — 初识分布式通信)
//!
//! ## 核心模型
//!
//! 分布式系统中的各进程（节点）之间**不共享内存**，仅通过**消息传递**实现同步与协调。
//! 本模块便是实现这一通信框架的基石。
//!
//! 在阶段一中实现了：
//! - 消息的序列化 / 反序列化 (`message` 子模块)
//! - 基于 TCP 的消息收发原语 (`channel` 子模块)
//! - 父进程协调与子进程同步函数 (`process` 子模块)
//!
//! ## 通信流程
//!
//! ```text
//! 父进程 (parent_work)                    子进程 (child_work)
//!   │                                        │
//!   │  绑定 TCP 端口，监听连接                  │
//!   │                                        │
//!   ├── spawn ──────────────────────────────►│  启动
//!   │                                        │
//!   │◄────────── STARTED 消息 ──────────────│  发送 STARTED
//!   │                                        │
//!   ├─────────── DATA 消息 ─────────────────►│  接收并处理
//!   │                                        │
//!   │◄────────── DONE 消息 ─────────────────│  发送 DONE
//!   │                                        │
//!   ▼  等待所有子进程 DONE 后返回              ▼  退出
//! ```

pub mod channel;
pub mod message;
pub mod process;

pub use channel::{recv_message, send_message};
pub use message::Message;
pub use process::{child_work, parent_work};
