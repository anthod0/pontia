# Control Plane Runtime Manager 设计

## 1. 文档目标

本文定义 MVP 阶段 Runtime Manager 在 Control Plane 中的职责边界和抽象能力。

MVP 的顶层目标是 backend-only 的可控型 Control Plane。Runtime Manager 支撑 session 的启动、停止、中断等操作，但不作为 External API 的直接状态源。对外可见的 session / turn 状态仍由领域事件和投影产生。

本文保持在设计层面，不规定具体命令、进程识别方式、启动包装方式、代码结构、数据库字段或进程管理实现。

---

## 2. Runtime Manager 职责

Runtime Manager 负责：

- 创建 runtime 容器或运行环境
- 启动 agent client
- 向 agent client 下发控制动作
- 停止或终止 agent client
- 尝试中断当前执行
- 观察 runtime 生命周期
- 将重要生命周期事实发布为领域事件
- 维护 runtime binding 等辅助状态

Runtime Manager 不负责：

- 定义 External API 契约
- 维护 session / turn 领域状态投影
- 解析 agent 输出语义
- 实现具体 client adapter 协议
- 执行 Approval / human-in-the-loop
- 作为对外状态读取来源

---

## 3. Runtime 抽象

MVP 不要求顶层绑定任何具体 runtime backend。底层可以是本地进程、终端复用器、容器、远端 worker 或测试替身；顶层只要求 Runtime Manager 提供统一能力：

```text
create_runtime(session)
start_runtime(session)
submit_input(session, turn, input)
interrupt(session, optional turn)
terminate(session)
restart(session)
observe_lifecycle(session)
```

具体如何执行由 runtime backend 决定。

---

## 4. RuntimeBinding

RuntimeBinding 是 session 与底层 runtime 实例之间的绑定记录。

它用于：

- 查找运行时实例
- 执行控制动作
- 诊断运行时状态
- 支持重启和清理

RuntimeBinding 是辅助状态，不参与 session / turn 领域事件重放承诺。

---

## 5. Runtime 事件生产

Runtime Manager 在观察到重要生命周期事实时，应发布领域事件：

| Runtime 事实 | 领域事件 |
|---|---|
| session runtime 开始启动 | `session.starting` |
| client 已启动 | `session.started` |
| client 可接收任务 | `session.ready` |
| client 正常退出 | `session.exited` |
| client 启动或运行异常 | `session.error` |
| interrupt 已生效 | `turn.interrupted` |
| interrupt 执行失败 | 诊断 warning 或后续失败事件 |

`turn.interrupt_requested` 由 External API command processor 在接受中断命令后发布；Runtime Manager 负责尝试执行中断，并在成功后发布 `turn.interrupted`。Runtime Manager 不应直接把 session projection 改成某个对外状态；它应通过事件进入 reducer。

---

## 6. 输入下发边界

External API 提交 turn 后，Turn Manager 通过 Runtime Manager 或 Adapter 的输入通道把任务下发给 agent client。

顶层抽象为：

```text
AgentInputSink.submit_turn(session_id, turn_id, input)
```

输入通道可以是标准输入、HTTP callback、消息队列、文件投递或其他方式。MVP 顶层设计不绑定具体方式。

---

## 7. 生命周期观察

Runtime Manager 应能够发现：

- runtime 启动成功或失败
- client 正常退出
- client 异常退出
- runtime 不可达
- interrupt / terminate 操作是否完成

这些观察结果进入 Control Plane 时应区分：

- 领域事实：影响 session / turn 状态，需要发布事件
- 辅助诊断：只更新 runtime binding 或 warning

---

## 8. 与 Client Adapter 的关系

Runtime Manager 管运行时，Client Adapter 管 client 协议。

例如：

- Runtime Manager 知道如何启动和停止进程
- Client Adapter 知道如何把一个 turn 输入转换为 client 可接受的形式
- Client Adapter 知道如何把 client 输出或 hook 转换为领域事件

在简单实现中二者可以由同一模块完成，但设计上应保持边界清晰。

---

## 9. 安全边界

MVP 需要在设计上承认以下安全边界：

- Runtime Manager 可能向 agent client 注入用户输入
- Runtime Manager 可能终止或中断进程
- Runtime metadata 可能包含敏感路径或 token 引用
- External API 的权限校验在 API 层完成

具体命令转义、环境变量注入、文件权限、进程组管理属于实现设计，不在本文展开。

---

## 10. MVP 范围

### 10.1 MVP 必做

- Runtime Manager 抽象职责
- RuntimeBinding 辅助状态定位
- session 启动 / 停止 / 重启 / 中断能力
- runtime 生命周期事实转领域事件
- 输入下发抽象

### 10.2 Post-MVP 或实现阶段处理

- 具体 runtime backend 命令
- 进程识别和进程组策略
- 启动包装方式选择
- 环境变量注入细节
- 终端输出捕获细节
- pre-warm pool
- runtime backend 插件化

---

## 11. 最终结论

Runtime Manager 是 Control Plane 的运行时执行层和生命周期事件生产者。它支撑 External API 的可控编排能力，但不替代领域事件和状态投影。MVP 文档只定义运行时抽象和边界，具体 runtime backend 和进程管理方案留到实现设计阶段确定。
