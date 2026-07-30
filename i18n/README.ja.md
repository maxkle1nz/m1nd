```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** は、あなたのコーディングエージェントにリポジトリごとの頭脳を提供します: MCP経由で提供されるローカルコードグラフ、参照したコードにアンカーされたメモリ、そしてすべての回答に対する信頼性の判断。"証拠不足" も立派な回答です。そして "まだ信頼すべきでない、修正方法はこちら" といった応答も同様です。

あなたのマシンから何も外に出ません。Rust製の単一のバイナリ。MITライセンス。

リポジトリをX線で撮影したようなものと考えてください。それをエージェントが読み取ります: すべてが統合され、それぞれがどこにあり、そのプログラムが何のためにあるのか、何が進行中で、何が完了し、何が未解決かを示す一つの構造。他のどのツールも、これをエージェントに提供しません。

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">インストールするのに必要なコマンドは4つだけ: <a href="#sixty-seconds">Sixty seconds</a>。まずタブを閉じるべき理由: <a href="#when-not-to-use-m1nd">When not to use m1nd</a>。</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="A real m1nd session: north returns trust, focus and honest gaps; seek answers with a reverify verdict; memorize anchors the finding to code" />
</p>

<p align="center"><em>このリポジトリの6,453ノードのグラフ上でのリアルなセッション (m1nd-mcp 1.4.0): <code>north</code> は方向付けを行い、<code>seek</code> は <code>reverify</code> 判定をつけた答えを返し、<code>memorize</code> は発見をコードにアンカーします。</em></p>

## エージェントが負う監査の不要化

この流れを知っていますね。エージェントはファイルを開き、grepを実行し、さらに別のファイルを開いてgrepをし、そして最終的にはリポジトリが何であるかを再構成するために多くの時間を燃やし、その後でようやく実際の作業を始める。m1ndを使えば、その流れが1つの質問になります。1秒以内にエージェントは地図を手に入れます: 何が何を呼び出し、何が何を壊し、すべてがどこにあるのか。その解釈が必要な一致の山ではありません。すでに組み立てられた関連性のある構造です。

さらに記憶します。セッションの間、そしてエージェント同士の間で。一つのエージェントが今夜学んだことを、別のエージェントが明日引き継ぎます。証拠を添付し、コードが進化してからのフラグをつけておきます。すべての結論は経緯を残します。だから、あなたや後のエージェントが常にそのコードが何をしていたのか、なぜそうであったのかを見ることができます。

そこからさらに進むのがl1ghtです: 論文、記事、RFC、ドラフト、ノートは、それらが説明しているコードと同じ構造内で結びつきます。エージェントは適切な文脈を得て、曖昧なコードを新たに発明しようとするのではなく、何が存在しているのかを示し、その信頼性まで明確にします。

m1ndを導入する前は、関数は単なる関数であり、手動でどこかに埋もれていました。今ではエージェントの知能の中にそのコードと結びついた状態で、履歴、ドキュメントやリスクと共に存在します。これに似たものは他には見つかりませんでした。

## grepは良い質問に答えます。m1ndはもっと深い質問に答えます。

エージェントが今や尋ね、構造的な回答を得られるようになった質問:

- この関数に手を加えると何が壊れる？
- このリポジトリ内でトークンの更新は実際どこで行われている？
- なぜこれら二つのファイルが繋がっているのか？その経路は確かなのか、それとも推測に過ぎないのか？
- 最後のセッションでこのコードについて何を学び、それはまだ有効か？
- ここで常に一緒に変更されるものは何か？インポートが無いとしても？
- この編集は越えてはいけないアーキテクチャ境界を跨いでいないか？
- この論文での主張のどれをこの関数が実装している？
- 修正したバグはどこか他の場所にも隠れていないか？
- 通常このパターンが備えているものでここに欠けているものは何か？
- 自分はそもそも正しいリポジトリにいるのか？
- この回答に行動すべきか、それとも先に検証すべきか？

これらは全てMCPサーフェス上の動詞です (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`)。プロンプトトリックではありません。

## 構造を示すだけでは終わりません

（以下翻訳対象外部分同様に続く）  
```
