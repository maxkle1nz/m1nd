🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">面向代码智能体的操作智能</h1>

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

**m1nd 是面向代码智能体的操作智能——它掌管的是操作循环，而不仅仅是检索。**

> grep 能找到文本。向量搜索能找到相似片段。`m1nd` 给智能体一张本地图，显示什么与什么相连、什么发生了变化、什么会崩溃、什么产生了漂移，以及从哪里恢复。

以下三点在任何其他工具中都不同时存在：

- **因果代码图** — 编辑前调用 `impact` 可看清你没读到的爆炸半径；`ghost_edges` 会找出那些总是一起变动但没有任何 import 关系的文件。
- **自我验证记忆** — `memorize` 将发现锚定到真实代码节点；当代码变更时，`cross_verify` 会将其标记为过期。
- **信任 / 恢复层** — 每个结果都附带信任模式；`trust_selftest` 和 `recovery_playbook` 会告知智能体工作区绑定何时出错以及如何恢复。

此外还有一个**注意力运行时**——`focus` 为一个目标向智能体交付最小的、受预算约束的工作集，附带一条诚实的尾部列出它遗漏了什么，以及一个信号来判断上下文*是否已足够*。

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="传统智能体循环 vs m1nd 接地循环" width="960" />
</p>

## 1.2.0 新特性——首个 OMEGA 时代版本

1.2.0 将循环从"检索，然后寄希望"转变为**预定向 → 依据校准过的裁决行动 → 捕获所学**。主题与信任层一致：诚实的"不"胜过自信的猜测。

- **`north(task)` — 一次调用即可预定向。** 这个新的前门整合了信任、任务上下文（focus 节点 + PageRank 锚点）、先前的跨会话记忆、一个充分性信号、一个 `next_move`，以及 `honest_gaps`（m1nd *尚不*知道的内容）。对于空图，`needs_ingest` 是一个真实的答案。（将先前记忆折叠进数据包的 L1GHT-recall 整合是在 1.2.0 标签之后才落到 `main` 上的——它不在 1.2.0 二进制文件中。）
- **预测上的保形校准。** `calibrate_predict` 为每个仓库装配一道闸门；此后裁决读作 `act` / `reverify` / `abstain`，其中 `abstain` 意味着*未校准或不充分*——一个停止信号，而不是弱肯定。默认隐藏出厂：在你校准之前，裁决最高只到 `reverify`。
- **`seek` 上的 `trust_envelope`**（隐藏出厂）以及 **`why` 上的 `closure` 裁决**——`blocked` 意味着该路径依赖于一条未解析/猜测的边。**`trust_band: insufficient_evidence`** 现在与风险等级截然不同：它意味着*没有证据*，是诚实的冷启动答案，而非"中等风险"。
- **记忆长出了一条溯源脊柱**——声明携带真实的年龄 + 作者、取代更旧的声明、随时间老化，并遵守一个新近度上限，因此被记住的知识会陈述自己的新鲜度，而不是悄然过期。
- **平滑化 Jaccard 共变更**——`ghost_edges` / `predict` 现在对耦合进行归一化，而不是统计原始的共提交次数（经校准证明，比原始计数高出 +3 个点）。
- **二进制版本 + sha 指纹**——`--version` 打印 `1.2.0 (<sha>)`；`M1ND_EXPECTED_VERSION` / `M1ND_EXPECTED_SHA`（+ `M1ND_STRICT_VERSION`）让宿主能够检测并拒绝一个漂移的二进制文件。
- **智能体原生的 MCP 指令 + 仅本地的现场报告。** 每个宿主收到的 `initialize` 指令现在*就是*上述操作循环。智能体每个会话可以留下一个遥测信号——对某个检索裁决执行 `learn`，或在 m1nd 自身行为异常时在 `~/.m1nd/field-reports.jsonl` 中写一行。该文件仅在本地；**m1nd 从不回传数据。**

## 快速开始

最简快捷路径——从源码安装（始终最新）、检查健康状态、连接你的宿主：

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
```

然后连接你的宿主——每个宿主使用相同的两条命令（`codex`、`claude`、`gemini`、`antigravity`、`generic`）：

| 宿主 | 安装智能体包 | 连接 MCP 配置 |
|---|---|---|
| Codex | `m1nd install-skills codex` | `m1nd mcp-config codex --project /your/project` |
| Claude Code | `m1nd install-skills claude --project /your/project` | `m1nd mcp-config claude --project /your/project` |
| Gemini | `m1nd install-skills gemini --project /your/project` | `m1nd mcp-config gemini --project /your/project` |
| Antigravity | `m1nd install-skills antigravity --project /your/project` | `m1nd mcp-config antigravity --project /your/project` |
| Generic | `m1nd install-skills generic --project /your/project` | `m1nd mcp-config generic --project /your/project` |

或通过 npm：`npm install -g @maxkle1nz/m1nd`。

完整安装指南、宿主包、原生运行时构建和更新标志：[docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · 逐客户端配置：[集成矩阵](../docs/IDE-INTEGRATIONS.md)。

### 智能体入口点

智能体会解析此 README。在 MCP 会话内，前门是一次调用——`north(task)` 将信任、任务上下文、先前的跨会话记忆、一个充分性信号、一个 `next_move` 以及 `honest_gaps`（m1nd *尚不*知道的内容）整合成一个数据包。如果它报告 `needs_ingest`（空图），或者你使用的是较旧的二进制文件，则退回到显式的信任循环——在相信任何检索结果*之前*先建立信任：

```jsonc
// 0. Trust the binding in one call (verdict before retrieval)
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. If the verdict is not full_trust, ask for the deterministic recovery path
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Build graph truth
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Ask a structural question — empty results say *why*, never just "no results"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**首次会话循环，四步完成：** `north`（或 `trust_selftest` → `ingest`）→ `seek`/`audit` → `memorize` 持久化发现，让下次会话提前起跑。

当没有可供调用 `north` 的活跃 MCP 会话时——它已过期、绑定到错误仓库，或尚未加载——请转而使用宿主中立的 CLI 作为应急出口。它会启动一个隔离的运行时、将其绑定到仓库，并返回一个机器可读的信封，完成界定范围、建立信任、按需摄入、返回锚点，并交接给直接证明：

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

### 服务一张图，附着多个智能体

上面的快速开始为每个宿主连接了一个 stdio 服务器——对单个智能体来说没问题，但每个进程都加载自己的图并持有自己的租约。m1nd 为之而生的部署形态是一个所有者、多个附着的智能体。一个所有者进程持有活图：

```bash
m1nd-mcp --serve --no-gui --port 1337 --runtime-dir /your/project/.m1nd
```

随后每个智能体都作为一个轻量的 stdio↔HTTP 桥接附着——它**不**加载图、不构建引擎、也**不**持有租约：

```bash
m1nd-mcp --attach http://127.0.0.1:1337 --stdio    # or set M1ND_ATTACH_URL and omit the flag
```

任意数量的桥接都指向那一个所有者并共享其单一的活图，因此一个智能体 `memorize` 的内容，另一个能立即调回——无需重新摄入，无需每个智能体各存一份。查询走 localhost，因此它保持本地优先（除非你选择加入 `--bind 0.0.0.0`，否则 bind 保持 `127.0.0.1`）。在单台机器上的一张小图上，经桥接的热 `seek` 测得 ≈0.7ms——这是数量级参考，而非保证：附着会增加一次 localhost 往返，延迟随图规模和负载而变化。

## m1nd 不是什么

`m1nd` 不只是：

- 一个索引更大的代码搜索工具
- 一个只检索文件或片段的仓库 RAG 层
- 一个把工作流决策留给客户端的图数据库
- 一个替代编译器、测试或安全工具的静态分析工具
- 一个无关工具的 MCP 捆绑包

它是将这些界面转化为智能体可以推理并据以行动的操作系统的那一层。不适用于单文件查找、简单 grep 或编译器真相——那些情况请用普通工具。

## 为什么智能体需要它

没有 m1nd，每次会话都从 grep 循环和手动重新定向开始；上周的发现已消失，空搜索结果与错误工作区绑定无从区分。有了 m1nd，会话以信任裁决开始，过去的发现已自动加载并锚定到支撑它们的代码，空结果会说明*原因*。

在真实代码库上工作的智能体失败，不是因为无法搜索，而是因为没有操作模型。它们每次会话都从头重建上下文，在不知道爆炸半径的情况下编辑，无法区分意味着"什么都不存在"的空结果和意味着"仓库错误"的空结果。

对于小型代码库，这还行得通。当项目有生成产物、规格、文档、隐藏的共变更历史、多个智能体和漫长的交接时，就会崩溃。问题不仅在于智能体的推理——智能体根本没有代码库结构的持久模型。`m1nd` 给了它这个模型：一个跨结构、语义、时间和因果维度进行扩散激活的因果代码图，加上每个智能体跨会话复利的 Hebbian 可塑性。

## 复利记忆（L1GHT）

大多数工具给智能体更好的*检索*。`m1nd` 还让智能体能够**撰写持久的、机器可读的知识**，这些知识跨会话复利，并对代码保持诚实。L1GHT 将撰写的知识转化为图原生结构，当其所引用的代码发生变化时自动标记——高置信度的声明比不确定的声明传播更多激活。

端到端的完整循环：

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
3. **自动加载** — 在每次未来会话开始时，`m1nd` 自动摄入 `agent-memory/` 并在 `session_handshake.agent_memory` 中报告。过去的发现在 `mode=replace` 摄入后依然存活，随时*就在那里*。
4. **自动标记过期** — `cross_verify(check: ["evidence_freshness"])` 对每个引用文件重新哈希，并指出哪些声明因其代码变更而过期——这样记忆会在它撒谎时告诉你，而不是误导你。

这个循环已被端到端实时验证：`memorize` → `grounded_in` 边 → 编辑文件上的新鲜度标志 → 在 `mode=replace` 后存活 → 启动自动加载。关闭一个有界任务？向 `mission_close` 传入 `write_light_memory: true`，以同样方式持久化其经过验证的声明。该习惯记录在每个 MCP 客户端在 `initialize` 时收到的服务器 `instructions` 中——与宿主无关，无需特定客户端插件。

## 信任 / 诚实层

这是 m1nd 最无可替代的事情，没有竞争对手提供它。教义：**可信度来自诚实，而非永远获胜。**

- **`trust_selftest`** 在任何检索*之前*返回裁决：`full_trust`、`needs_ingest`、`wrong_workspace_binding`、`stale_binding_suspected` 或 `degraded_host_tool_surface`。智能体由此知道是继续、摄入、重新绑定还是降级。
- **`agent_runtime_contract`** 附在每个检索响应上，携带一个 `trust_mode`。空结果得到明确区分——绑定到错误仓库还是真的什么都没有——绝不会静默报告为"无结果"。
- **`non_claims` 数组** 附在每个任务工具上。m1nd 告诉智能体它*没有*证明什么。
- **`mission_verify` 可以说不——并且在经过测试的代码中确实如此。** 它拒绝纯图证据：一个声明在没有文件读取、测试运行或运行时探针的情况下无法关闭。该测试的名称字面上就是 `graph_only_evidence_is_not_enough`。
- **`recovery_playbook`** 返回一份修复绑定的确定性、有序步骤列表。

对这一承诺的证明是为此牺牲的东西：`savings` 和 `resonate` 在 beta.7 中从公告的界面中撤下，因为一个总声称获胜的工具不可信。没有竞争对手——不是 mem0、Zep、Letta、Sourcegraph，也不是任何代码图 MCP——提供一个告诉智能体*不该信任什么*以及如何恢复的层。

**现场分诊循环闭合于自身。** 智能体留在 `~/.m1nd/field-reports.jsonl` 中的会话遥测（仅本地——m1nd 从不回传数据）并不是被动日志：报告会被分诊，一个*已确认*的现场 bug 会在修复**之前**变成一个红色的电池用例，因此该回归是被证明的，而非仅仅被描述。那个循环已经端到端运行过一次：两个现场上报的 bug 变成了失败的电池用例，随后是合并的修复——`north` 现在将 L1GHT recall 整合进其记忆数据包，而 `temp` 图哨兵会解析到一个真实的临时目录，而不是把工作目录弄乱。

## 语言覆盖

图推理（`impact`、`why`、`predict`、`trace`、`taint_trace`）的好坏取决于提取器。m1nd 为每种语言解析 **`calls` 边**（调用图）和**跨文件 `imports`**（文件→文件依赖解析）。下表在单次多语言摄入中得到实时验证：

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

所有 ✅ 行均经过端到端验证（一条 `caller`→`callee` import 可解析且调用者生成 call 边）。其他语言回退到通用提取器（仅 `contains`）。无法解析的 import（外部包、gem、标准库、系统头文件）会如实留作未解析，而不是猜测。

## 能力图

Live MCP 界面随版本更新。使用 `tools/list` 获取当前构建中确切的工具数量和名称。

| 领域 | 功能 | 代表性工具 |
|---|---|---|
| 图基础 | 摄入代码、维护图状态、诊断会话连续性、强化有用路径，以及检测跨会话权重漂移 | `trust_selftest`、`session_handshake`、`recovery_playbook`、`ingest`、`health`、`doctor`、`learn`、`warmup`、`drift` |
| 检索与定向 | 在手动读取文件之前按文本、路径、意图、结构或关系搜索 | `audit`、`search`、`glob`、`seek`、`activate`、`why`、`trace` |
| 文档与知识绑定 | 摄入通用文档或图原生 `L1GHT`，然后将概念链接回代码 | `ingest(adapter="universal"\|"light")`、`document_resolve`、`document_provider_health`、`document_bindings`、`document_drift`、`auto_ingest_*` |
| 导航与连续性 | 跨会话维护有状态路由、交接、基线和调查记忆 | `perspective_*`、`trail_*`、`coverage_session`、`boot_memory`、`persist` |
| Mission Control 与证明纪律 | 维护有界路由、记录事件、从图定向切换到直接证明、交接，并带明确缺口关闭 | `mission_start`、`mission_event`、`mission_next`、`mission_verify`、`mission_handoff`、`mission_close` |
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

Mission Control 是证明纪律，而不是功能列表。`mission_next` 恰好返回一个动作加 `do_not` 护栏；`mission_verify` 拒绝纯图声明；`mission_close` 始终推动智能体持久化经过验证的知识并记录缺口和非声明。在 `bug_hunt` 模式下，MC0 要求在关闭前、在验证发现之后进行一次最终的直接 `direct_sweep`，以便智能体检查负空间。

**注意：** 在 `ghost_edges` 加载 git 共变更矩阵之前，`predict` **仅有结构回退**——当你需要真实的共变更可能性时，请先运行 `ghost_edges`。

## 证据

每一行的对冲范围恰好是所测量的内容。m1nd 不以节省或 ROI 数字作为引导——这正是要点所在。

| 声明 | 结果 | 来源 / 对冲 |
|---|---|---|
| `activate` / `impact` 延迟 | 在 1K 节点合成图上 `activate` ~1µs，`impact` 亚微秒 | Criterion 基准——**自己复现它：`cargo bench -p m1nd-core`**（在一台 Apple 芯片 Mac 上测得 `activate_1k_nodes` ≈1.4µs，`impact_depth3` ≈0.5µs）；[方法论](https://m1nd.world/wiki/benchmarks.html)；数量级参考，取决于硬件。 |
| 语言矩阵 | 10 种语言的 calls + 跨文件 imports（+ Ruby 跨文件） | 在单次多语言摄入中端到端验证；每种语言的测试在 `m1nd-ingest` 中。见[语言覆盖](#语言覆盖)。 |
| 写后验证样本 | 12/12 分类正确 | 内部运行时检查。 |
| 种子 bug 搜寻 | 在第一轮被接受的 `humanize` 种子缺陷测试中 16/20（m1nd 训练）；`m1nd-basic` 和直接模式各 8/15 | 内部产品证据，`public_claim_worthy=false`——非通用基准。 |
| 记忆自我验证 | 端到端实时验证 | `memorize` → `grounded_in` → 编辑文件上的新鲜度标志 → 在 replace 后存活 → 启动自动加载。 |
| 能力电池 vs grep | 37/37 通过；正面交锋 16 个 m1nd 胜 / 12 个平局 / **0 个 grep 胜** | 仓库内测试框架 `scratchpad/m1nd_battery.py`（37 个用例，全新摄入 + 真值 PASS/FAIL + 与 `rg` 正面交锋）。**复现：`python3 scratchpad/m1nd_battery.py ./target/release/m1nd-mcp . --suite m1nd`。** 对冲：单个仓库（m1nd 自身）、自撰用例；约 5 个平局是结构性工具，被拿来与一个无法表达它们所回答内容的字面 grep 代理评分。 |
| 保形校准（`predict`） | act 等级 ≈32% 精确率 @ ≈13.5% 覆盖率（α=0.10） | 在 m1nd 自己的 git 历史上（n≈9.2k 个留出预测），在平滑化 Jaccard 变更后比原始计数高出 +3pts。对冲：单个仓库、一个粗糙的基于计数的信号——这道闸门如今大多选择弃权，**这是有意为之**：弃权是弱信号的诚实输出，而非失败。 |

## 局限性

`m1nd` 是对你的 LSP、编译器、测试运行器、安全扫描器和可观测性栈的补充，而非替代。它在搜索、审查或变更之前，以及在文档、影响或连续性重要时，最为有用。

在以下情况**用处较小**：

- 精确文本搜索已能回答问题
- 编译器或运行时真相是你唯一需要的
- 任务是无结构不确定性的简单本地文件操作

**需要喂数据：** `trust` 和 `tremor` 从中性先验开始，直到 `learn` 反馈 / `ghost_edges` 数据积累起来，而 `predict` 需要先加载 `ghost_edges`，其共变更信号才有意义。这些会随使用而改善；它们对启动时的无信息状态保持诚实。

## 架构概览

三个核心 Rust crate 加一个辅助桥接：

- **`m1nd-mcp`** — MCP 服务器和操作运行时界面。
- **`m1nd-core`** — 图引擎：一个执行扩散激活、Hebbian 可塑性、CSR 邻接和 git 派生幽灵边的 `WavefrontEngine`。
- **`m1nd-ingest`** — 提取、路由和图构建适配器（代码、通用文档、L1GHT）。
- **`m1nd-openclaw`** — 辅助 OpenClaw 桥接（Unix socket 通道，独立版本）。

当前 crate 版本：`m1nd-core`、`m1nd-ingest`、`m1nd-mcp` 均为 `1.2.0`（`m1nd-openclaw` 独立版本化，为 `0.1.0`）。

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="m1nd 架构概览" width="960" />
</p>

关于联邦、perspectives、RETROBUILDER、多智能体协调以及完整的智能体包和运维参考，请参阅[官方 wiki](https://m1nd.world/wiki/)、[docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) 和 [EXAMPLES.md](../EXAMPLES.md)。

## 贡献

欢迎在提取器与适配器、MCP/运行时工具、基准、文档和图算法方面做出贡献。详见 [CONTRIBUTING.md](../CONTRIBUTING.md)。

## 许可证

MIT。详见 [LICENSE](../LICENSE)。
