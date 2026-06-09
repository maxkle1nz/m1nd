🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">コーディングエージェントのためのローカルミッションランタイム</h1>

<p align="center">
  <strong>あなたのコーディングエージェントは盲目でのスタートを卒業する。</strong><br/>
  <em>ローカルファースト。MCPネイティブ。エージェントホスト向けのグラフメモリ、トラスト、変更推論。</em>
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

**m1nd はコーディングエージェントのためのローカルミッションランタイムであり、単なる検索ではなく操作ループ全体を統括する。**

> `grep` はテキストを見つける。ベクトル検索は類似チャンクを見つける。`m1nd` はエージェントに、何が繋がっていて、何が変わって、何が壊れて、何がドリフトして、どこから再開すべきかを示すローカルグラフを与える。

以下の三つは他のどのツールにも同時には存在しない：

- **因果コードグラフ** — 編集前に `impact` を呼ぶと、見落としていた爆発半径が分かる；`ghost_edges` は import 関係がないのに常に一緒に変更されるファイルを浮かび上がらせる。
- **自己検証メモリ** — `memorize` は発見を実際のコードノードに固定する；コードが変更されると `cross_verify` がそれを古いものとして警告する。
- **トラスト／リカバリレイヤー** — すべての結果はトラストモードを持つ；`trust_selftest` と `recovery_playbook` は、ワークスペースバインディングが間違っているときとその回復方法をエージェントに伝える。

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="従来のエージェントループ vs m1ndグラウンドループ" width="960" />
</p>

## クイックスタート

最小限のハッピーパス——ソースからインストール（常に最新）、ヘルスチェック、ホストへの接続：

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
m1nd install-skills codex          # または: claude / gemini / antigravity / generic
m1nd mcp-config codex --project /your/project
```

npm ベータチャンネルからのインストール：`npm install -g @maxkle1nz/m1nd@beta`。

完全なインストールマップ、ホストパック、ネイティブランタイムビルド、更新フラグ：[docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · クライアント別セットアップ：[統合マトリクス](../docs/IDE-INTEGRATIONS.md)。

### エージェントエントリーポイント

エージェントはこの README をパースする。ホスト MCP セッションが古くなっているとき、間違ったリポジトリにバインドされているとき、またはまだロードされていないときは、ホスト中立 CLI を使用する——これは独立したランタイムを起動し、リポジトリにバインドし、機械可読なエンベロープを一つ返す：

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

`m1nd agent first-minute` は新しいリポジトリへの最も安全な最初の接触だ。リポジトリのスコープを定め、トラストを確立し、必要に応じてインジェストし、一回の有界な定向パスを実行し、候補アンカーを返し、そしてエージェントにソース、テスト、コンパイラ／ランタイム出力、ログ、またはプローブから直接証明するよう伝える。

MCP セッション内での教義はこのトラストループ——検索結果を信じる*前に*トラストを確立する：

```jsonc
// 0. 一回の呼び出しでバインディングを信頼（検索前に判定を得る）
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. 判定が full_trust でない場合、決定論的リカバリパスを要求する
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. グラフの真実を構築する
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. 構造的な質問をする——空の結果は「なぜ」を言う、決して「結果なし」とだけ言わない
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**初回セッションループ、四ステップで：** `trust_selftest` → `ingest` → `seek`/`audit` → `memorize` で永続的な発見を残し、次のセッションを先行スタートさせる。

## m1nd ではないもの

`m1nd` は単なる：

- より大きなインデックスを持つコード検索ツールではない
- ファイルやチャンクを取得するだけのリポジトリ RAG レイヤーではない
- ワークフローの決定をクライアントに任せるグラフデータベースではない
- コンパイラ、テスト、またはセキュリティツールの代替となる静的解析ツールではない
- 無関係なユーティリティの MCP バンドルではない

それらのサーフェスをエージェントが推論し行動できる運用システムに変えるレイヤーこそが m1nd だ。単一ファイルの検索、単純な grep、コンパイラの真実には向かない——そこでは普通のツールを使え。

## エージェントがそれを必要とする理由

m1nd がなければ、すべてのセッションは grep ループと手動の再定向から始まる；先週の発見は消え、空の検索結果は間違ったワークスペースバインドと区別できない。m1nd があれば、セッションはトラスト判定から始まり、過去の発見はそれを支えるコードに固定された状態で自動ロードされ、空の結果は*理由*を語る。

実際のコードベースで働くエージェントが失敗するのは、検索できないからではない。操作モデルを持っていないからだ。毎セッションゼロからコンテキストを再構築し、爆発半径を知らずに編集し、「何もない」という空の結果と「間違ったリポジトリ」という空の結果を区別できない。

小さなコードベースではこれでも通用する。プロジェクトに生成された成果物、仕様書、ドキュメント、隠れた共変更履歴、複数のエージェント、長い引き継ぎがあると崩壊する。問題はエージェントの推論だけではない——エージェントにはコードベースの構造に関する永続モデルがないのだ。`m1nd` がそれを与える：構造的、意味的、時間的、因果的な次元にわたって拡散活性化するとともに、セッションをまたいでエージェントごとに複利する Hebbian 可塑性を持つ因果コードグラフ。

## 複利メモリ（L1GHT）

ほとんどのツールはエージェントにより良い*検索*を与える。`m1nd` はエージェントが**永続的で機械可読な知識を著述**できるようにし、その知識はセッションをまたいで複利し、コードに対して誠実であり続ける。L1GHT は著述された知識をグラフネイティブな構造に変換し、引用しているコードが変更されると自動でフラグを立てる——高確信度の主張はより多くの活性化を伝播する。

エンドツーエンドのループ：

1. **結論を出す** — エージェントが永続的なもの（決定、検証済みの発見、コードがそうなっている理由）に到達し、構造化された主張と `evidence` パスを持って `memorize` を呼ぶ。

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

2. **固定する** — m1nd は `<runtime>/agent-memory/` 下にグラフネイティブな `.light.md` を書き、インジェストし（`adapter=light mode=merge`）、`grounded_in` エッジで各 `evidence` パスを実際のコードノードに解決する——知識がコードと同じ活性化空間に存在し、`seek` / `activate` / `impact` でサーフェスされるようになる。
3. **自動ロード** — すべての将来のセッション開始時に、`m1nd` は `agent-memory/` を自動的にインジェストし、`session_handshake.agent_memory` で報告する。過去の発見は `mode=replace` インジェストを生き延び、そこに*存在する*。
4. **古さの自己フラグ立て** — `cross_verify(check: ["evidence_freshness"])` はすべての引用ファイルを再ハッシュし、コードが変更されたために古くなった主張を名指しする——メモリが嘘をつくときに教えてくれる、誤解させるのではなく。

このループはエンドツーエンドで実証されている：`memorize` → `grounded_in` エッジ → 編集されたファイルのフレッシュネスフラグ → `mode=replace` を生き延びる → 起動時の自動ロード。有界ミッションを閉じるとき？`write_light_memory: true` を `mission_close` に渡すと、その検証済みの主張を同じ方法で永続化できる。この習慣は、すべての MCP クライアントが `initialize` 時に受け取るサーバーの `instructions` に文書化されている——ホスト非依存、クライアント固有のプラグイン不要。

## トラスト／誠実性レイヤー

これは m1nd が行う最も防御力の高いことであり、競合他社は誰も提供していない。教義：**信頼性は誠実さから来る、常に勝つことからではない。**

- **`trust_selftest`** は検索*前*に判定を返す：`full_trust`、`needs_ingest`、`wrong_workspace_binding`、`stale_binding_suspected`、または `degraded_host_tool_surface`。エージェントは進むべきか、インジェストすべきか、再バインドすべきか、フォールバックすべきかを知る。
- **`agent_runtime_contract`** はすべての検索レスポンスに乗り、`trust_mode` を運ぶ。空の結果は明確に区別される——間違ったリポジトリにバインドされているのか、本当に何もないのか——「結果なし」として静かに報告されることはない。
- **`non_claims` 配列** はすべてのミッションツールに付く。m1nd はエージェントに何を*証明していない*かを伝える。
- **`mission_verify` は「ノー」と言える——そして、テストされたコードで実際にそうする。** グラフのみの証拠を拒否する：ファイル読み取り、テスト実行、またはランタイムプローブなしには主張をクローズできない。テストは文字通り `graph_only_evidence_is_not_enough` という名前だ。
- **`recovery_playbook`** はバインディングを修復するための決定論的な順序付きステップリストを返す。

この約束の証明は、そのために犠牲にしたもの：`savings` と `resonate` は beta.7 で公告サーフェスから削除された、なぜなら常に勝つと主張するツールは信頼できないからだ。競合他社——mem0、Zep、Letta、Sourcegraph、またはいかなるコードグラフ MCP も——エージェントに何を*信頼しないべきか*そして回復方法を伝えるレイヤーを提供していない。

## 言語カバレッジ

グラフ推論（`impact`、`why`、`predict`、`trace`、`taint_trace`）はエクストラクターの品質に依存する。m1nd は言語ごとに **`calls` エッジ**（コールグラフ）と**クロスファイル `imports`**（ファイル→ファイルの依存解決）を解決する。以下のマトリクスは単一のポリグロットインジェストで実証された：

| 言語 | `calls` | クロスファイル imports |
|---|:---:|:---:|
| Rust | ✅ | ✅ (`mod`/`use crate::`) |
| Python | ✅ | ✅ |
| JavaScript / TypeScript | ✅ | ✅ |
| Go | ✅ | ✅（パッケージ） |
| Java | ✅ | ✅（FQCN + ワイルドカード） |
| C / C++ | ✅ | ✅ (`#include "..."`) |
| Kotlin | ✅ | ✅（パッケージ） |
| PHP | ✅ | ✅（PSR-4） |
| Scala | ✅ | ✅（パッケージ） |
| Ruby | ⏳ | ✅ (`require_relative`) |
| C# | ✅ | —（名前空間はファイルに 1:1 でマップされない） |
| Swift | ✅ | — |

すべての ✅ 行はエンドツーエンドで検証されている（`caller`→`callee` の import が解決され、呼び出し元がコールエッジを発する）。他の言語はジェネリックエクストラクター（`contains` のみ）にフォールバックする。解決できない import（外部パッケージ、gem、stdlib、システムヘッダー）は、推測せず誠実に未解決のままにされる。

## ケイパビリティマップ

ライブ MCP サーフェスはリリースとともに進化する。現在のビルドにおける正確なツール数と名前は `tools/list` を使って確認すること。

| エリア | 有効にすること | 代表的なツール |
|---|---|---|
| グラフ基盤 | コードのインジェスト、グラフ状態の維持、セッション継続性の診断、有用なパスの強化、クロスセッションの重みドリフト検出 | `trust_selftest`、`session_handshake`、`recovery_playbook`、`ingest`、`health`、`doctor`、`learn`、`warmup`、`drift` |
| 検索と定向 | 手動ファイル読み取りの前にテキスト、パス、意図、構造、または関係で検索 | `audit`、`search`、`glob`、`seek`、`activate`、`why`、`trace` |
| ドキュメントと知識バインディング | ユニバーサルドキュメントまたはグラフネイティブ `L1GHT` をインジェストし、コンセプトをコードにリンクする | `ingest(adapter="universal"\|"light")`、`document_resolve`、`document_provider_health`、`document_bindings`、`document_drift`、`auto_ingest_*` |
| ナビゲーションと継続性 | セッションをまたいでステートフルなルート、引き継ぎ、ベースライン、調査メモリを維持する | `perspective_*`、`trail_*`、`coverage_session`、`boot_memory`、`persist` |
| Mission Control と証明の規律 | 有界ルートを維持し、イベントを記録し、グラフ定向から直接証明に切り替え、明示的なギャップでクローズする | `mission_start`、`mission_event`、`mission_next`、`mission_verify`、`mission_handoff`、`mission_close` |
| 変更計画と証明 | 影響、共変更、欠落ステップ、失敗パス、構造的主張について推論する | `impact`、`predict`、`validate_plan`、`missing`、`hypothesize`、`counterfactual`、`differential` |
| 品質、セキュリティ、アーキテクチャ | パターン、汚染パス、トラスト境界、重複、レイヤー違反、型フロー、リファクタリング対象を検出する | `scan`、`scan_all`、`heuristics_surface`、`antibody_*`、`taint_trace`、`type_trace`、`trust`、`layers`、`layer_inspect`、`twins`、`fingerprint`、`flow_simulate`、`epidemic`、`tremor`、`refactor_plan` |
| 時間、ランタイム、マルチリポジトリ作業 | git 履歴、ドリフト、隠れた共変更エッジ、ランタイムオーバーレイ、クロスリポジトリ参照を検査する | `timeline`、`diverge`、`ghost_edges`、`runtime_overlay`、`external_references`、`federate`、`federate_auto` |
| 運用と監視 | リポジトリの状態を監査し、グラフとディスクの真実を検証し、デーモン監視を実行し、状態を永続化し、永続アラートを浮かび上がらせる | `audit`、`cross_verify`、`daemon_*`、`alerts_*`、`panoramic`、`metrics`、`report`、`persist`、`diagram`、`help` |
| サージカル編集の準備と実行 | コンパクトな接続済みコンテキストを取得し、書き込みをプレビューし、グラフ対応編集を適用する | `surgical_context`、`surgical_context_v2`、`view`、`batch_view`、`edit_preview`、`edit_commit`、`apply`、`apply_batch` |

**ティアリング：** デフォルトでは 27 のエッセンシャルツールが公告され、ツール選択コストを削減する；`M1ND_TOOL_TIER=full` を設定するとフルサーフェス（100 以上のツール：RETROBUILDER、perspectives、federation、daemon）が公告される。いくつかのツール（`resonate`、`savings`、`lock_*`）は名前で呼び出せるが公告サーフェスには載っていない。隠されたツールは常に `tools/call` で呼び出せる——ティアリングは `tools/list` がサーフェスするものだけを制御する。

## 操作ループ

エージェントパックは製品の一部であり、装飾的なドキュメントではない。エージェントがグラフエンドポイントだけでなく*操作ループ*を受け取るとき、m1nd は最も強力になる。パックには五つの命名されたプロトコルが含まれる：

- **セッション開始** — `trust_selftest` → トラストが完全でない場合 `recovery_playbook` → 必要に応じて `ingest` → `seek`/`audit`。
- **リサーチ** — `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → `learn(feedback)` → 永続的な発見を `memorize`。
- **コード変更** — 爆発半径のために `impact(node)` → `predict(node)` → `counterfactual(nodes)` → `surgical_context_v2` → 決定と理由を `memorize`。
- **深い分析** — `fingerprint`、`diverge`、`ghost_edges`、`taint_trace`、`twins`、`refactor_plan`、`runtime_overlay`（RETROBUILDER レンズ）、隠れた結合、セキュリティパス、構造的重複、ランタイムヒートのために。
- **メモリ** — `confidence` と `evidence` パスを持って `memorize` で永続的な結論を残す。

Mission Control は証明の規律であり、機能リストではない。`mission_next` はちょうど一つのアクションと `do_not` ガードレールを返す；`mission_verify` はグラフのみの主張を拒否する；`mission_close` は常にエージェントが検証済みの知識を永続化し、ギャップと非主張を記録するよう促す。`bug_hunt` モードでは、MC0 は検証済みの発見の後にクローズ前の最終 `direct_sweep` を要求し、エージェントが負の空間を確認するようにする。

**注意：** `predict` は `ghost_edges` が git 共変更マトリクスをロードするまで**構造的フォールバックのみ**——本当の共変更可能性が必要なときは先に `ghost_edges` を実行すること。

## 証拠

各行は正確に測定されたものにヘッジされている。m1nd は節約や ROI の数値を先頭に置かない——それこそが要点だ。

| 主張 | 結果 | ソース／ヘッジ |
|---|---|---|
| `activate` / `impact` レイテンシ | `activate` サブマイクロ秒、`impact` サブミリ秒 | `m1nd-core/benches/` の 1K ノード合成グラフでの Criterion ベンチマーク——[方法論](https://m1nd.world/wiki/benchmarks.html)；桁の参考として扱うこと。 |
| 言語マトリクス | 10 言語の calls + クロスファイル imports（+ Ruby クロスファイル） | 単一のポリグロットインジェストでエンドツーエンド検証；言語ごとのテストは `m1nd-ingest` にある。[言語カバレッジ](#言語カバレッジ)参照。 |
| 書き込み後検証サンプル | 12/12 を正しく分類 | 内部ランタイムチェック。 |
| シードされたバグハント | 最初に受け入れられた `humanize` シード欠陥ラウンドで 16/20（m1nd 訓練済み）；`m1nd-basic` と直接はそれぞれ 8/15 | 内部製品証拠、`public_claim_worthy=false`——ユニバーサルベンチマークではない。 |
| メモリの自己検証 | エンドツーエンドで実証済み | `memorize` → `grounded_in` → 編集されたファイルのフレッシュネスフラグ → replace を生き延びる → 起動時の自動ロード。 |

## 制限事項

`m1nd` は LSP、コンパイラ、テストランナー、セキュリティスキャナー、オブザーバビリティスタックを補完するものであり、置き換えるものではない。検索、レビュー、変更の前、そしてドキュメント、影響、または継続性が重要なときに最も有用だ。

以下の場合は**有用性が低い**：

- 正確なテキスト検索で既に質問に答えられる場合
- コンパイラまたはランタイムの真実だけが必要な場合
- 構造的な不確実性のない単純なローカルファイル操作の場合

**フィーディングが必要：** `trust` と `tremor` は `learn` フィードバック / `ghost_edges` データが蓄積されるまで中立的な事前分布から始まり、`predict` は共変更シグナルが意味を持つ前に `ghost_edges` のロードが必要だ。これらは使うほど改善する；起動時に情報がないことについて誠実だ。

## アーキテクチャ概観

三つのコア Rust クレートと一つの補助ブリッジ：

- **`m1nd-mcp`** — MCP サーバーと運用ランタイムサーフェス。
- **`m1nd-core`** — グラフエンジン：拡散活性化、Hebbian 可塑性、CSR 隣接、git 由来のゴーストエッジを処理する `WavefrontEngine`。
- **`m1nd-ingest`** — 抽出、ルーティング、グラフ構築アダプター（コード、ユニバーサルドキュメント、L1GHT）。
- **`m1nd-openclaw`** — 補助 OpenClaw ブリッジ（Unix ソケットレーン、独立バージョニング）。

現在のクレートバージョン：`m1nd-core`、`m1nd-ingest`、`m1nd-mcp` はすべて `0.9.0-beta.7`。

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="m1nd アーキテクチャ概観" width="960" />
</p>

フェデレーション、perspectives、RETROBUILDER、マルチエージェント協調、および完全なエージェントパックとオペレーターリファレンスについては、[公式 wiki](https://m1nd.world/wiki/)、[docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md)、[EXAMPLES.md](../EXAMPLES.md) を参照。

## コントリビューション

エクストラクターとアダプター、MCP/ランタイムツール、ベンチマーク、ドキュメント、グラフアルゴリズムにわたるコントリビューションを歓迎する。[CONTRIBUTING.md](../CONTRIBUTING.md) を参照。

## ライセンス

MIT。[LICENSE](../LICENSE) 参照。
