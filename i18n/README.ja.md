```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd**は、リポジトリごとにコーディングエージェントに脳を提供します: MCP上で提供されるローカルコードグラフ、引用するコードに基づく記憶、そしてすべての回答に対する信頼判定。"証拠が不十分です"というのも適切な回答です。また"まだ信頼しないでください、これが修正方法です"も該当します。

情報はマシンを離れません。Rust製バイナリ1つ。MITライセンス。

m1ndは、エージェントが読めるリポジトリのX線図のようなものです: すべてを統合し、各要素がどこにあり、プログラムが何のために存在しているか、何が進行中で何が完了し、何が未完了なのかを示します。この全体像は、他のどのツールもエージェントには与えないものです。

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">インストールはたった4つのコマンドで完了します: <a href="#sixty-seconds">60秒で完了</a>。このタブを閉じてしまう理由: <a href="#when-not-to-use-m1nd">m1ndを使用しない場合</a>。</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="A real m1nd session: north returns trust, focus and honest gaps; seek answers with a reverify verdict; memorize anchors the finding to code" />
</p>

<p align="center"><em>このリポジトリの6,453ノードグラフ上での本物のセッション (m1nd-mcp 1.4.0): <code>north</code>が方向付け、<code>seek</code>が<code>reverify</code>判決をともなった回答を提供し、<code>memorize</code>が結果をコードに紐づけます。</em></p>

## エージェントがコストを支払っていた監査を解放

お馴染みの手順でしょう。エージェントはファイルを開き、grepを実行し、また別のファイルを開いて再びgrepを実行し、リポジトリが何であるかを再構築するのにリソースの大半を費やし、ようやく実際の作業に取り掛かります。m1ndを使えば、その動作が1つの質問に変わります。わずか1秒足らずで、エージェントは地図を手に入れます: 何が何を呼び出しているのか、何が何を壊しているのか、すべてがどこにあるのか。一致する結果の山を解読する必要はありません。すでに組み立てられた構造そのものです。

さらに記憶してくれます。セッション間、エージェント間で。あるエージェントが今夜学んだことを、別のエージェントが明日引き継ぎ、証拠付きで、コードが変わった場合はフラグが表示されます。すべての結論には記録が残されるため、あなたや後から続くエージェントは、コードがどう変更されなぜそうなったか常に確認することができます。

その後l1ghtがさらに進化させます: 論文、記事、RFC、ドラフト、メモが、解説しているコード部分にリンクされ、同じ構造内で接続されます。エージェントは最も適切な文脈を取得し、生み出されていないコードを想像することが最小化されます: 構造自体が存在の有無を示し、判断はその信頼性を伝えます。

m1ndの登場以前、関数は単なる関数であり、手作業で管理されていました。今やそれはエージェントの知能内に組み込まれ、コード、その履歴、ドキュメント、リスクと結合して存在します。このような他例を見つけたことがありません。

## grepは良い質問に答えます。m1ndはより深い質問に答えます。

エージェントが今や構造的な答えを得られる質問:

- この関数を変更すると何が壊れるのか？
- トークンリフレッシュはこのリポジトリ内でどこで実行されているのか？
- なぜこれら2つのファイルが接続されているのか、その接続はしっかりしているのか、それとも推測なのか？
- 最後のセッションでこのコードについて学んだことは何か、それはまだ真実か？
- ここで常に一緒に変化するものは何か、インポートのない場合でも？
- この編集は越えてはいけないアーキテクチャの境界を越えたのか？
- この関数が実装している論文の主張は何か？
- 今修正したバグが他の場所に同じ形で隠れていないか？
- このパターンが通常持っているのに欠けている要素は何か？
- 私は正しいリポジトリにいるのか？
- この回答に基づいて行動すべきか、それともまず検証すべきか？

これらの質問は全て、MCP表面上の動詞(`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`)であり、プロンプトの工夫ではありません。

## 構造を表示するだけではありません

抗体: 修正されたバグは命名された構造パターンとなり、後のすべてのセッションでその形状をリポジトリ全体でスキャンします。一度修正すれば、永遠に追跡できます。

ゴーストエッジ: インポートなしで常に一緒に変更されているファイル、git履歴から抽出。リファクターを壊す見えない結合。

構造的な穴: `missing`は存在しないコードを探します。このパターンが通常持っているガード、リトライ、タイムアウトを特定。

グラフに対する仮説: 平易な言語で主張を述べる ("設定が検証なしでブートに到達することができる") と、それがライブ構造と照合され検証されます。

加速警告: 変更速度が加速しているファイルがバグ報告がなされる前に通知されます。

温かいグラフ: 確認済みの結果がヘッブ型シナプス修飾でエッジを強化します。次回のエージェントに役立つパスがランクアップします。

これら全てがフラグを立て提案を行います。検証の役割はまだコンパイラーとテストの仕事です。
```
