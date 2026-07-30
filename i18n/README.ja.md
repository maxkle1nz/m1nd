```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** は、あなたのコードエージェントにリポジトリごとの脳を提供します: MCP経由で提供されるローカルコードグラフ、引用元のコードに固定されたメモリ、そして各回答に信頼の評価を付けます。"証拠が不十分"もここでは正当な回答です。また"まだ信頼できない、修復方法はこちら"もそうです。

何もあなたのマシンから離れません。一つのRustバイナリ。MITライセンス。

これを、エージェントが読めるリポジトリのX線写真のように考えてください: すべてを統合し、各要素がどこにあるか、このプログラムが何を目指しているのか、進行中のものは何で、完了したものや未解決のもの、それらを一つにまとめたパノラマ画像です。これは他のツールではエージェントが得られないものです。

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">インストールには4つのコマンドが必要です: <a href="#sixty-seconds">Sixty seconds</a>. 閲覧をやめる理由: <a href="#when-not-to-use-m1nd">When not to use m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="A real m1nd session: north returns trust, focus and honest gaps; seek answers with a reverify verdict; memorize anchors the finding to code" />
</p>

<p align="center"><em>A real session on this repo's 6,453-node graph (m1nd-mcp 1.4.0): <code>north</code> orients, <code>seek</code> answers wearing a <code>reverify</code> verdict, <code>memorize</code> anchors the finding to code.</em></p>

## エージェントが負担しなくなる監査

お馴染みの手順をご存知でしょう。エージェントがファイルを開き、grepを行い、別のファイルを開いて再びgrepを行い、リポジトリが何であるかを再構築するためにほとんどのコンテキストを消耗し、その後ようやく実際のタスクを開始します。m1ndを使えば、この作業は一つの質問で済みます。1秒以内にエージェントがマップを取得します: どの関数がどれを呼び出すのか、どれが何を壊しているのか、すべてがどこにあるのかが分かります。解釈が必要な検索結果の山ではなく、すでに構築済みの接続された構造が得られるのです。

しかも、それを記憶します。セッションをまたいで、エージェントをまたいで、一つのエージェントが今夜学んだことは別のエージェントが明日引き継ぎます。根拠を添えて、コードが移動した場合にはフラグが立ちます。すべての結論は痕跡を残すため、あなたや後から来るエージェントは、なぜそのコードに何が起こったのか常に見ることができます。

そして、l1ghtがそれをさらに推し進めます: 論文、記事、RFC、ドラフト、メモは、それらが説明するコード部分に接続され、同じ構造内に配置されます。エージェントは的確なコンテキストを得ることができ、存在しないコードを発明することが最も抵抗の少ない道ではなくなります: 構造が存在しているものを示し、評価はそれがどこまで信頼できるかを示します。

m1nd以前、関数はただの関数で、何らかの手動操作の中に埋もれていました。しかし今、それはエージェントの知能の内に存在し、コード、履歴、文書、リスクと結びついています。他にこれに似たものは見つけられませんでした。

## grepが良い質問に答える。m1ndはもっと深い質問に答える。

エージェントが今できる質問と、その構造的な回答例:

- この関数に触れると何が壊れるのか？
- トークンの更新はこのリポジトリのどこで実際に行われているのか？
- なぜこれら2つのファイルが接続されているのか、それは確実なのか推測なのか？
- 最後のセッションでこのコードについて何が学ばれ、それはまだ有効なのか？
- インポートされていないのに常に一緒に変更されるものは何？
- この編集は超えてはいけないアーキテクチャの境界を越えていないか？
- この関数はこの論文のどの主張を実装しているのか？
- 修正したバグは、どこか他に隠れていないだろうか？
- このパターンが通常持っているけど、ここには欠けているものは何？
- 自分がこのリポジトリにいること自体が正しいのか？
- この回答に基づいて行動すべきか、それともまず確認すべきか？

これらの質問は、MCP上での動詞 (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`) で回答します, プロンプトの工夫ではありません。

## 構造を示すだけではありません

抗体: 修正されたバグが名前の付いた構造的パターンとなり、その後のすべてのセッションでその形状をリポジトリ全体にわたってスキャンします。一度修正すれば、永遠に追跡できます。

ゴーストエッジ: Gitの履歴から発掘された、インポートされていないのに常に一緒に変更されるファイル。リファクタを壊す見えない結合です。

構造的欠陥: `missing` は存在しないコードを探します。このパターンが通常持つガード、リトライ、タイムアウト、そしてここには欠けているものです。

グラフに対する仮説: 普通の言葉での主張 ("設定がバリデーションなしでブートに到達できる") を示し、それをライブな構造に対してテストします。

震源地: 変更の速度が加速しているファイルは、誰かがバグ報告を提出する前にフラグが付けられます。

温かいグラフ: 確認された結果はそのエッジをヘッブ型の強化により強化していくので、有用であることが証明されたパスが次のエージェントにとってより高くランク付けされます。

これらすべてのフィーチャーはフラグを立て、提案を行います。証明はコンパイラとテストが引き受けます。
```

*(The translation continues in this format beyond the given preview – if you need the continuation or specific sections translated, please request.)*
