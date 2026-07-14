use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LamportClock {
    /// 当前逻辑时间。初始值为 0。
    time: u64,
}

impl LamportClock {
    #[must_use]
    pub fn new() -> Self {
        Self { time: 0 }
    }

    #[allow(dead_code)]
    pub fn increment(&mut self) -> u64 {
        self.time += 1;
        self.time
    }

    /// 在收到消息时同步时钟（对应规则 R2）。
    ///
    /// 接收方用消息携带的时间戳 `received_time` 更新本地时钟：
    /// ```text
    /// Lⱼ = max(Lⱼ, L_msg) + 1
    /// ```
    ///
    /// 此方法同时完成了规则 R2 的两个步骤：
    /// 1. 取 `max(self.time, received_time)`  — 同步到较大值
    /// 2. 执行 R1 递增（+1）                    — 接收本身是事件
    ///
    /// # 参数
    ///
    /// - `received_time`: 发送方的逻辑时间戳（从收到的消息中提取）
    ///
    /// # 实现提示
    ///
    /// 步骤一：使用 [`u64::max`](https://doc.rust-lang.org/std/primitive.u64.html#method.max)
    /// 将本地时钟更新为两者中较大值：
    /// ```text
    /// self.time = self.time.max(received_time);
    /// ```
    ///
    /// 步骤二：完成 R1 递增（因为接收本身也是一个事件）：
    /// ```text
    /// self.time += 1;
    /// ```
    ///
    /// 最后返回新的 `self.time`。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let mut clock = LamportClock::new();
    /// // 本地时钟已推进到 3
    /// clock.time = 3;
    ///
    /// // 收到消息，发送方时间戳为 7
    /// // max(3, 7) = 7, 然后 +1 = 8
    /// assert_eq!(clock.update(7), 8);
    /// ```
    #[allow(dead_code)]
    pub fn update(&mut self, received_time: u64) -> u64 {
        // TODO: 实现 Lamport R2 规则
        // 1. self.time = self.time.max(received_time);  // 同步到较大值
        // 2. self.time += 1;                             // R1 递增（接收事件）
        // 3. self.time                                   // 返回新值
        //
        // 提示: `u64::max` 返回两个值中的较大者。
        // 参考: https://doc.rust-lang.org/std/primitive.u64.html#method.max
        self.time = self.time.max(received_time);
        self.time += 1;
        self.time
    }

    /// 获取当前时钟值（不修改状态）。
    ///
    /// # 实现提示
    ///
    /// 直接返回 `self.time`。
    #[must_use]
    #[allow(dead_code)]
    pub fn get(&self) -> u64 {
        // TODO: 你已经完成了这个函数！
        self.time
    }

    /// 重置时钟为 0（仅供测试使用）。
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        // TODO: 你已经完成了这个函数！
        self.time = 0;
    }

    /// 直接设置时钟值（仅供测试使用）。
    #[allow(dead_code)]
    pub fn set(&mut self, time: u64) {
        // TODO: 你已经完成了这个函数！
        self.time = time;
    }
}

impl Default for LamportClock {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for LamportClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LamportClock({})", self.time)
    }
}

// ═══════════════════════════════════════════════════════════
// Stage 3 扩展: 银行系统集成函数 (TODO — 教学风格)
// ═══════════════════════════════════════════════════════════

/// 判断一次转账在当前 Lamport 时间线上是否"已完成"。
///
/// 在 Lab 3 中，判断转账是否完成不再依赖物理时间，而是通过以下条件：
/// - 源账户已在时间 t_send 完成扣款
/// - 目标账户已在时间 t_recv 完成入账
///
/// 当这些条件都满足时，该笔转账对应的 `pending_in` 可以被清零。
///
/// # 参数
///
/// - `send_time`: 源账户扣款时的 Lamport 时间戳（0 表示尚未扣款）
/// - `recv_time`: 目标账户入账时的 Lamport 时间戳（0 表示尚未入账）
///
/// # 返回值
///
/// 仅当双方均已操作（`send_time > 0 && recv_time > 0`）时返回 `true`。
///
/// # 实现提示
///
/// 逻辑非常简单——两个时间戳都必须大于 0：
/// ```text
/// send_time > 0 && recv_time > 0
/// ```
///
/// 这里的关键不是代码复杂度，而是**概念转换**：
/// Lab 2 中我们用物理时钟判断"是否到时间了"，
/// Lab 3 中我们用逻辑时钟判断"事件是否已经发生"。
///
/// # 参考
///
/// - ITMO Lab 3 课件第 7-9 页（飞行中的资金 / Pending Money）
#[must_use]
#[allow(dead_code)]
pub fn is_transfer_complete(send_time: u64, recv_time: u64) -> bool {

    if send_time > 0 && recv_time > 0 {
        return true;
    }
    false
}

#[cfg(test)]
mod tests;
