# TDD (测试驱动开发) 流程规范

## 1. 铁律

> **没有先写失败测试，就不允许编写生产代码。**

违反即删除代码，重头开始。

## 2. Red-Green-Refactor 标准循环

```
RED   → 编写一个最小化的失败测试，表达期望行为
GREEN → 编写刚好能让测试通过的最简代码
REFACTOR → 清理代码、消除重复、改善命名，保持测试全绿
```

### 2.1 每一步的具体要求

| 阶段 | 操作 | 验证要求 |
|------|------|---------|
| **RED** | 写一个测试，只测一个行为 | `cargo test <test_name>` 必须**因功能缺失**而失败 (不是编译错误) |
| **GREEN** | 写刚好使测试通过的代码 | `cargo test` 全部通过，无 warnings |
| **REFACTOR** | 重构，不改行为 | `cargo test` 保持全绿 |

### 2.2 确认 RED 失败的检查清单

- [ ] 测试名称清晰描述被测行为 (如 `test_create_znode_persistent`)
- [ ] 测试因**功能缺失**失败 (不是拼写错误或编译错误)
- [ ] 失败信息是预期的 (如 `assertion failed: expected X, got Y`)
- [ ] 一个测试只测一件事 (测试名里有 "and" 就应该拆分)

## 3. 测试命名规范

```
test_<被测函数>_<场景>_<预期行为>
```

示例:
- `test_process_start_sync_sends_started`
- `test_lamport_clock_increments_on_send`
- `test_transfer_atomicity_rollback_on_failure`
- `test_ephemeral_node_cleanup_on_session_timeout`

## 4. 测试目录结构

```
src/
├── communication/
│   ├── mod.rs
│   └── tests.rs          # 模块的单元测试
├── lamport_clock/
│   ├── mod.rs
│   └── tests.rs
└── ...
tests/
├── integration_test.rs    # 集成测试
└── common/
    └── mod.rs             # 测试辅助工具
```

## 5. 测试类型划分

| 类型 | 位置 | 用途 |
|------|------|------|
| **单元测试** | `src/<module>/tests.rs` (或 `#[cfg(test)] mod tests`) | 验证单个函数/结构体行为 |
| **集成测试** | `tests/` | 验证多模块协作，模拟分布式场景 |
| **文档测试** | `///` 注释中的代码块 | 确保示例代码可运行 |

## 6. 测试质量标准

### 好测试的特征
- **最小化**: 一个测试只验证一件事
- **可读**: 测试名 = 预期行为描述
- **真实**: 测试真实代码，避免过度 mock
- **隔离**: 测试之间不依赖执行顺序
- **快速**: 单元测试应在毫秒级完成

### 测试覆盖维度
每个功能模块的测试需覆盖:
- ✅ 正常路径 (Happy Path)
- ✅ 边界条件 (空值、最大值、最小值)
- ✅ 错误路径 (超时、网络断开、非法输入)
- ✅ 并发场景 (多进程/线程竞争)

## 7. 提交流程 (TDD + Git)

```
1. 创建功能分支: feat/<阶段>-<描述>
2. RED:  编写测试 → cargo test (确认失败) → git add <test> → git commit -m "test: ..."
3. GREEN: 编写实现 → cargo test (确认通过) → git add <impl> → git commit -m "feat: ..."
4. REFACTOR: 重构     → cargo test (确认通过) → git add <重构> → git commit -m "refactor: ..."
5. 推送 PR，CI 全绿后合并
```

**Commit 消息规范** (Conventional Commits):
- `test:` — 添加或修改测试
- `feat:` — 新功能实现
- `fix:` — Bug 修复
- `refactor:` — 重构
- `docs:` — 文档变更
- `chore:` — 构建/工具配置变更

## 8. CI 门禁

合并 PR 必须通过:
- `cargo fmt --check` — 代码格式
- `cargo clippy -- -D warnings` — 静态检查零警告
- `cargo build` — 编译通过
- `cargo test` — 所有测试通过

## 9. 反模式警告

| 反模式 | 正确做法 |
|--------|---------|
| 先写实现再补测试 | 删除代码，从头 TDD |
| 测试直接通过 (没看到失败) | 停止，确认测试确实在测缺失的功能 |
| Mock 一切 | 优先使用真实对象，仅在 I/O/网络边界使用 mock |
| 一个测试测多个场景 | 拆成多个独立测试 |
| 测试名 `test1` `test2` | 写可读的测试名 |
