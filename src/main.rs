//! Crab-Keeper 可执行文件入口。
//!
//! ## 启动模式
//!
//! - **父进程** (默认): `crab-keeper` — 启动协调器
//! - **子进程**: `crab-keeper --child <父进程地址>` — 启动工作节点

use crab_keeper::communication::process::{child_work, parent_work, print_child_banner, CHILD_ARG};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    // 解析命令行参数：如果包含 --child，则以子进程模式运行
    if args.len() >= 2 && args[1] == CHILD_ARG {
        // 子进程模式
        let parent_addr = if args.len() >= 3 {
            args[2].as_str()
        } else {
            print_usage_and_exit();
             unreachable!()
        };

        let child_id: usize = env::var("CHILD_ID")
            .unwrap_or_else(|_| "0".to_string())
            .parse()
            .unwrap_or(0);

        print_child_banner(child_id, parent_addr);

        #[allow(clippy::print_stderr)]
        match child_work(parent_addr) {
            Ok(()) => {
                eprintln!("[子进程 #{child_id}] 完成");
                Ok(())
            }
            Err(e) => {
                eprintln!("[子进程 #{child_id}] 错误: {e}");
                Err(e)
            }
        }
    } else {
        // 父进程模式
        #[allow(clippy::print_stdout)]
        {
            println!("Crab-Keeper 父进程启动...");
        }

        let children_count: usize = std::env::var("CHILDREN_COUNT")
            .unwrap_or_else(|_| "3".to_string())
            .parse()
            .unwrap_or(3);

        #[allow(clippy::print_stdout)]
        {
            println!("预计启动 {children_count} 个子进程");
        }

        parent_work(children_count)?;

        #[allow(clippy::print_stdout)]
        {
            println!("所有子进程同步完成!");
        }

        Ok(())
    }
}

fn print_usage_and_exit() {
    #[allow(clippy::print_stderr)]
    {
        eprintln!("用法:");
        eprintln!("  父进程: crab-keeper");
        eprintln!("  子进程: crab-keeper --child <父进程地址>");
        eprintln!("环境变量:");
        eprintln!("  CHILDREN_COUNT=3   子进程数量 (仅父进程)");
        eprintln!("  CHILD_ID=0         子进程编号 (仅子进程)");
    }
    std::process::exit(1);
}
