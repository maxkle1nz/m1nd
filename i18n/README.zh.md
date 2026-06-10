🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">面向代码智能体的本地任务运行时</h1>

<p align="center">
  <strong>你的代码智能体不再盲目启动。</strong><br/>
  <em>本地优先。原生 MCP。为智能体宿主提供图记忆、信任机制和变更推理。</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-core"><img src="https://img.shields.io/crates/v/m1nd-core.svg" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://docs.rs/m1nd-core"><img src="https://img.shields.io/docsrs/m1nd-core" alt="docs.rs" /></a>
</p>

<p align="center">
  <a href="https://github.com/openai/codex"><img src="https://img.shields.io/badge/OpenAI_Codex-412991?logo=openai&logoColor=fff" alt="OpenAI Codex" /></a>
  <a href="https://claude.ai/download"><img src="https://img.shields.io/badge/Claude_Code-f0ebe3?logo=claude&logoColor=d97706" alt="Claude Code" /></a>
  <a href="https://cursor.sh"><img src="https://img.shields.io/badge/Cursor-000?logo=cursor&logoColor=fff" alt="Cursor" /></a>
  <a href="https://codeium.com/windsurf"><img src="https://img.shields.io/badge/Windsurf-0d1117?logo=windsurf&logoColor=3ec9a7" alt="Windsurf" /></a>
  <a href="https://github.com/features/copilot"><img src="https://img.shields.io/badge/GitHub_Copilot-000?logo=githubcopilot&logoColor=fff" alt="GitHub Copilot" /></a>
  <a href="https://zed.dev"><img src="https://img.shields.io/badge/Zed-084ccf?logo=zedindustries&logoColor=fff" alt="Zed" /></a>
  <a href="https://github.com/cline/cline"><img src="https://img.shields.io/badge/Cline-000?logo=cline&logoColor=fff" alt="Cline" /></a>
  <a href="https://roocode.com"><img src="https://img.shields.io/badge/Roo_Code-6d28d9?logoColor=fff" alt="Roo Code" /></a>
  <a href="https://github.com/continuedev/continue"><img src="https://img.shields.io/badge/Continue-000?logoColor=fff" alt="Continue" /></a>
  <a href="https://opencode.ai"><img src="https://img.shields.io/badge/OpenCode-18181b?logoColor=fff" alt="OpenCode" /></a>
  <a href="https://aistudio.google.com"><img src="https://img.shields.io/badge/Gemini-4285F4?logo=google&logoColor=fff" alt="Gemini" /></a>
  <a href="https://aws.amazon.com/q/developer"><img src="https://img.shields.io/badge/Amazon_Q-232f3e?logo=amazonaws&logoColor=f90" alt="Amazon Q" /></a>
</p>

---

**m1nd 是面向代码智能体的本地任务运行时——它掌管的是操作循环，而不仅仅是检索。**

> `grep` 能找到文本。向量搜索能找到相似片段。`m1nd` 给智能体一张本地图，显示什么与什么相连、什么发生了变化、什么会崩溃、什么产生了漂移，以及从哪里恢复。

以下三点在任何其他工具中都不同时存在：

- **因果代码图** — 编辑前调用 `impact` 可看清你没读到的爆炸半径；`ghost_edges` 会找出那些总是一起变动但没有任何 import 关系的文件。
- **自我验证记忆** — `memorize` 将发现锚定到真实代码节点；当代码变更时，`cross_verify` 会将其标记为过期。
- **信任与恢复层** — 每个结果都附带信任模式；`trust_selftest` 和 `recovery_playbook` 会告知智能体工作区绑定何时出错以及如何恢复。

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="传统智能体循环 vs m1nd 接地循环" width="960" />
</p>

## 快速开始

最简快捷路径——从源码安装（始终最新）、检查健康状态、连接你的宿主：

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
m1nd install-skills codex          # 或: claude / gemini / antigravity / generic
m1nd mcp-config codex --project /your/project
```

或通过 npm 测试通道安装：`npm install -g @maxkle1nz/m1nd@beta`。

完整安装指南、宿主包、原生运行时构建和更新标志：[docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · 逐客户端配置：[集成矩阵](../docs/IDE-INTEGRATIONS.md)。

### 智能体入口点

智能体会解析此 README。当宿主 MCP 会话过期、绑定到错误仓库或尚未加载时，使用宿主中立的 CLI——它会启动一个隔离的运行时、将其绑定到仓库，并返回一个机器可读的信封：

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

`m1nd agent first-minute` 是接触新仓库最安全的第一步。它会界定仓库范围、建立信任、按需摄入、运行一次有界的定向扫描、返回候选锚点，然后告知智能体直接从源码、测试、编译器/运行时输出、日志或探针来验证。

在 MCP 会话内，教义是如下信任循环——在相信任何检索结果之前，先*建立信任*：

```jsonc
// 0. 一次调用即可信任绑定（检索前先得出裁决）
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. 若裁决不是 full_trust，请求确定性恢复路径
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. 构建图真相
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. 提问结构性问题——空结果会说明*原因*，而不只是"无结果"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**首次会话循环，四步完成：** `trust_selftest` → `ingest` → `seek`/`audit` → `memorize` 持久化发现，让下次会话提前起跑。

## m1nd 不是什么

`m1nd` 不只是：

- 一个索引更大的代码搜索工具
- 一个只检索文件或片段的仓库 RAG 层
- 一个把工作流决策留给客户端的图数据库
- 一个替代编译器、测试或安全工具的静态分析工具
- 一个无关工具的 MCP 捆绑包

它是将这些界面转化为智能体可以推理和行动的操作系统的那一层。不适用于单文件查找、简单 grep 或编译器真相——那些情况请用普通工具。

## 为什么智能体需要它

没有 m1nd，每次会话都从 grep 循环和手动重新定向开始；上周的发现已消失，空搜索结果与错误工作区绑定无从区分。有了 m1nd，会话以信任裁决开始，过去的发现已自动加载并锚定到支撑它们的代码，空结果会说明*原因*。

在真实代码库上工作的智能体失败，不是因为无法搜索，而是因为没有操作模型。它们每次会话都从头重建上下文，在不知道爆炸半径的情况下编辑，无法区分"什么都没有"和"仓库绑定错误"这两种空结果。

对于小型代码库，这还行得通。当项目有生成产物、规格文档、隐藏的共变更历史、多个智能体和漫长的交接时，就会崩溃。问题不仅在于智能体的推理——智能体根本没有代码库结构的持久模型。`m1nd` 给了它这个模型：一个跨结构、语义、时间和因果维度进行扩散激活的因果代码图，加上每个智能体跨会话复利的 Hebbian 可塑性。

## 复利记忆（L1GHT）

大多数工具给智能体更好的*检索*。`m1nd` 还让智能体能够**撰写持久的、机器可读的知识**，这些知识跨会话复利，并对代码保持诚实。L1GHT 将撰写的知识转化为图原生结构，当所引用代码发生变化时自动标记——高置信度的声明会传播更多激活。

端到端循环：

1. **得出结论** — 智能体得到持久性结论（一个决策、一个经过验证的发现、代码为何如此设计），并用结构化声明和 `evidence` 路径调用 `memorize`。

```jsonc
memorize({
  "agent_id": "dev",
  "node_label": "AuthTokenFlow",
  "claims": [
    { "label": "TokenValidator", "text": "validates JWTs via HMAC",
      "confidence": "high", "evidence": ["src/auth/token.rs"] }
  ]
})
```

2. **锚定** — m1nd 在 `<runtime>/agent-memory/` 下写入图原生 `.light.md`，摄入它（`adapter=light mode=merge`），并通过 `grounded_in` 边将每条 `evidence` 路径解析到真实代码节点——让知识与代码处于同一激活空间，并在 `seek` / `activate` / `impact` 中浮现。
3. **自动加载** — 在每次未来会话开始时，`m1nd` 自动摄入 `agent-memory/` 并在 `session_handshake.agent_memory` 中报告。过去的发现在 `mode=replace` 摄入后依然存在，随时可用。
4. **自动标记过期** — `cross_verify(check: ["evidence_freshness"])` 对每个引用文件重新哈希，并指出哪些声明因代码变更而过期——让记忆在撒谎时告诉你，而不是误导你。

这个循环已被端到端验证：`memorize` → `grounded_in` 边 → 编辑文件上的新鲜度标志 → 在 `mode=replace` 后存活 → 启动自动加载。关闭一个有界任务？向 `mission_close` 传入 `write_light_memory: true` 以同样方式持久化其经过验证的声明。该习惯记录在每个 MCP 客户端在 `initialize` 时收到的服务器 `instructions` 中——与宿主无关，无需特定客户端插件。

## 信任与诚实层

这是 m1nd 最无可替代的事情，没有竞争对手提供它。教义：**可信度来自诚实，而非永远正确。**

- **`trust_selftest`** 在任何检索*之前*返回裁决：`full_trust`、`needs_ingest`、`wrong_workspace_binding`、`stale_binding_suspected` 或 `degraded_host_tool_surface`。智能体知道是继续、摄入、重新绑定还是降级。
- **`agent_runtime_contract`** 附在每个检索响应上，携带 `trust_mode`。空结果有明确区分——绑定到错误仓库还是真的什么都没有——而不是静默报告"无结果"。
- **`non_claims` 数组** 附在每个任务工具上。m1nd 告诉智能体它*没有*证明什么。
- **`mission_verify` 可以说不——并且在测试代码中确实如此。** 它拒绝纯图证据：声明在没有文件读取、测试运行或运行时探针的情况下无法关闭。该测试的名称字面上就是 `graph_only_evidence_is_not_enough`。
- **`recovery_playbook`** 返回修复绑定的确定性、有序步骤列表。

对这一承诺的证明是为此牺牲的东西：`savings` 和 `resonate` 在 beta.7 中从公告的界面中移除，因为一个总声称获胜的工具不可信。没有竞争对手——不是 mem0、Zep、Letta、Sourcegraph，也不是任何代码图 MCP——提供一个告诉智能体*不该信任什么*以及如何恢复的层。

## 语言覆盖

图推理（`impact`、`why`、`predict`、`trace`、`taint_trace`）取决于提取器的质量。m1nd 为每种语言解析 **`calls` 边**（调用图）和**跨文件 `imports`**（文件→文件依赖解析）。下表在单次多语言摄入中得到验证：

| 语言 | `calls` | 跨文件 imports |
|---|:---:|:---:|
| Rust | ✅ | ✅ (`mod`/`use crate::`) |
| Python | ✅ | ✅ |
| JavaScript / TypeScript | ✅ | ✅ |
| Go | ✅ | ✅（包） |
| Java | ✅ | ✅（FQCN + 通配符） |
| C / C++ | ✅ | ✅ (`#include "..."`) |
| Kotlin | ✅ | ✅（包） |
| PHP | ✅ | ✅（PSR-4） |
| Scala | ✅ | ✅（包） |
| Ruby | ⏳ | ✅ (`require_relative`) |
| C# | ✅ | —（命名空间不能 1:1 映射到文件） |
| Swift | ✅ | — |

所有 ✅ 行均经过端到端验证（`caller`→`callee` import 可解析且调用者生成 call 边）。其他语言回退到通用提取器（仅 `contains`）。无法解析的 import（外部包、gem、标准库、系统头文件）会如实留作未解析，而不是猜测。

## 能力图

Live MCP 界面随版本更新。使用 `tools/list` 获取当前构建中的确切工具数量和名称。

| 领域 | 功能 | 代表性工具 |
|---|---|---|
| 图基础 | 摄入代码、维护图状态、诊断会话连续性、强化有用路径，以及检测跨会话权重漂移 | `trust_selftest`、`session_handshake`、`recovery_playbook`、`ingest`、`health`、`doctor`、`learn`、`warmup`、`drift` |
| 检索与定向 | 在手动读取文件之前按文本、路径、意图、结构或关系搜索 | `audit`、`search`、`glob`、`seek`、`activate`、`why`、`trace` |
| 文档与知识绑定 | 摄入通用文档或图原生 `L1GHT`，然后将概念链接回代码 | `ingest(adapter="universal"\|"light")`、`document_resolve`、`document_provider_health`、`document_bindings`、`document_drift`、`auto_ingest_*` |
| 导航与连续性 | 跨会话维护有状态路由、交接、基线和调查记忆 | `perspective_*`、`trail_*`、`coverage_session`、`boot_memory`、`persist` |
| Mission Control 与证明纪律 | 维护有界路由、记录事件、从图定向切换到直接证明、交接并带明确缺口关闭 | `mission_start`、`mission_event`、`mission_next`、`mission_verify`、`mission_handoff`、`mission_close` |
| 变更规划与证明 | 推理影响、共变更、缺失步骤、失败路径和结构声明 | `impact`、`predict`、`validate_plan`、`missing`、`hypothesize`、`counterfactual`、`differential` |
| 质量、安全与架构 | 检测模式、污点路径、信任边界、重复、层违规、类型流和重构目标 | `scan`、`scan_all`、`heuristics_surface`、`antibody_*`、`taint_trace`、`type_trace`、`trust`、`layers`、`layer_inspect`、`twins`、`fingerprint`、`flow_simulate`、`epidemic`、`tremor`、`refactor_plan` |
| 时间、运行时与多仓库工作 | 检查 git 历史、漂移、隐藏共变更边、运行时叠加和跨仓库引用 | `timeline`、`diverge`、`ghost_edges`、`runtime_overlay`、`external_references`、`federate`、`federate_auto` |
| 运维与监控 | 审计仓库状态、验证图与磁盘真相、运行守护进程监控、持久化状态和浮现持久告警 | `audit`、`cross_verify`、`daemon_*`、`alerts_*`、`panoramic`、`metrics`、`report`、`persist`、`diagram`、`help` |
| 外科编辑准备与执行 | 拉取紧凑的关联上下文、预览写入并应用图感知编辑 | `surgical_context`、`surgical_context_v2`、`view`、`batch_view`、`edit_preview`、`edit_commit`、`apply`、`apply_batch` |

**分层：** 默认公告 27 个基础工具以降低工具选择成本；设置 `M1ND_TOOL_TIER=full` 可公告完整界面（100+ 工具：RETROBUILDER、perspectives、federation、daemon）。少数工具（`resonate`、`savings`、`lock_*`）仍可按名称调用，但不在公告界面上。隐藏工具始终可通过 `tools/call` 调用——分层仅控制 `tools/list` 显示什么。

## 操作循环

智能体包是产品的一部分，而不是装饰性文档。当智能体收到的是*操作循环*而不仅仅是图端点时，m1nd 最为强大。包中附带五个命名协议：

- **会话开始** — `trust_selftest` → 若信任不完整则 `recovery_playbook` → 若需要则 `ingest` → `seek`/`audit`。
- **研究** — `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → `learn(feedback)` → `memorize` 任何持久性发现。
- **代码变更** — `impact(node)` 获取爆炸半径 → `predict(node)` → `counterfactual(nodes)` → `surgical_context_v2` → `memorize` 决策及其原因。
- **深度分析** — `fingerprint`、`diverge`、`ghost_edges`、`taint_trace`、`twins`、`refactor_plan`、`runtime_overlay`（RETROBUILDER 镜头），用于隐藏耦合、安全路径、结构重复和运行时热点。
- **记忆** — 用 `memorize` 持久化持久性结论，携带 `confidence` 和 `evidence` 路径。

Mission Control 是证明纪律，而不是功能列表。`mission_next` 恰好返回一个动作加 `do_not` 护栏；`mission_verify` 拒绝纯图声明；`mission_close` 始终推动智能体持久化经过验证的知识并记录缺口和非声明。在 `bug_hunt` 模式下，MC0 在关闭前要求在验证发现之后进行最终的直接 `direct_sweep`，以便智能体检查负空间。

**注意：** `predict` 在 `ghost_edges` 加载 git 共变更矩阵之前**仅有结构回退**——当你需要真实的共变更可能性时，请先运行 `ghost_edges`。

## 证据

每行的对冲范围恰好是所测量的内容。m1nd 不以节省或 ROI 数字作为引导——这正是要点所在。

| 声明 | 结果 | 来源与对冲 |
|---|---|---|
| `activate` / `impact` 延迟 | `activate` 亚微秒，`impact` 亚毫秒 | `m1nd-core/benches/` 中基于 1K 节点合成图的 Criterion 基准——[方法论](https://m1nd.world/wiki/benchmarks.html)；视为数量级参考。 |
| 语言矩阵 | 10 种语言的 calls + 跨文件 imports（+ Ruby 跨文件） | 在单次多语言摄入中端到端验证；每种语言的测试在 `m1nd-ingest` 中。见[语言覆盖](#语言覆盖)。 |
| 写后验证样本 | 12/12 分类正确 | 内部运行时检查。 |
| 种子 bug 搜寻 | 在第一轮被接受的 `humanize` 种子缺陷测试中 16/20（m1nd 训练）；`m1nd-basic` 和直接模式各 8/15 | 内部产品证据，`public_claim_worthy=false`——非通用基准。 |
| 记忆自我验证 | 端到端验证 | `memorize` → `grounded_in` → 编辑文件上的新鲜度标志 → 在 replace 后存活 → 启动自动加载。 |

## 局限性

`m1nd` 是对 LSP、编译器、测试运行器、安全扫描器和可观测性栈的补充，而非替代。在搜索、审查或变更之前，以及在文档、影响或连续性重要时，它最为有用。

在以下情况**用处较小**：

- 精确文本搜索已能回答问题
- 编译器或运行时真相是唯一需要的
- 任务是无结构不确定性的简单本地文件操作

**需要喂数据：** `trust` 和 `tremor` 在 `learn` 反馈 / `ghost_edges` 数据积累之前从中性先验开始，`predict` 在 `ghost_edges` 加载之前需要其共变更信号才有意义。这些会随使用而改善；它们对启动时的无信息状态保持诚实。

## 架构概览

三个核心 Rust crate 加一个辅助桥接：

- **`m1nd-mcp`** — MCP 服务器和操作运行时界面。
- **`m1nd-core`** — 图引擎：执行扩散激活、Hebbian 可塑性、CSR 邻接和 git 派生幽灵边的 `WavefrontEngine`。
- **`m1nd-ingest`** — 提取、路由和图构建适配器（代码、通用文档、L1GHT）。
- **`m1nd-openclaw`** — 辅助 OpenClaw 桥接（Unix socket 通道，独立版本）。

当前 crate 版本：`m1nd-core`、`m1nd-ingest`、`m1nd-mcp` 均为 `0.9.0-beta.8`。

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="m1nd 架构概览" width="960" />
</p>

关于联邦、perspectives、RETROBUILDER、多智能体协调以及完整的智能体包和运维参考，请参阅[官方 wiki](https://m1nd.world/wiki/)、[docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) 和 [EXAMPLES.md](../EXAMPLES.md)。

## 贡献

欢迎在提取器与适配器、MCP/运行时工具、基准、文档和图算法方面做出贡献。详见 [CONTRIBUTING.md](../CONTRIBUTING.md)。

## 许可证

MIT。详见 [LICENSE](../LICENSE)。
