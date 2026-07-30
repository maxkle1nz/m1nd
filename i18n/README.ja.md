```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** は、リポジトリごとにコーディングエージェントの脳を提供します: MCP 上で提供されるローカルなコードグラフ、引用されたコードに基づく記憶、そしてすべての回答に対する信頼の判定を行います。「十分な証拠がない」はここでは正当な答えです。「まだ信頼できない。そしてそれを修正する方法はこちら」という答えも同様です。

データはマシンから出ません。Rust バイナリひとつ。MIT。

リポジトリを読み取る X 線のようなものと考えてください。すべてを統合し、どこに何があり、そのプログラムが何のためで、何が進行中で、何が完了していて、何が未解決なのかを示すひとつの構造。それが、他のツールではエージェントに提供できない全貌です。

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">インストールに必要なコマンドは4つ: <a href="#sixty-seconds">Sixty seconds</a>。まずタブを閉じるべき理由: <a href="#when-not-to-use-m1nd">When not to use m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="A real m1nd session: north returns trust, focus and honest gaps; seek answers with a reverify verdict; memorize anchors the finding to code" />
</p>

<p align="center"><em>A real session on this repo's 6,453-node graph (m1nd-mcp 1.4.0): <code>north</code> orients, <code>seek</code> answers wearing a <code>reverify</code> verdict, <code>memorize</code> anchors the finding to code.</em></p>

## あなたのエージェントがこれ以上無駄にしない監査

この儀式をご存知でしょう。エージェントはファイルを開き、grep し、それからまた別のファイルを開き、またgrep。そしてリポジトリが何なのかを再構築するためにほとんどのコンテキストを消費し、ようやく実際のタスクを開始します。m1nd を使えば、このプロセスはひとつの質問に変わります。1秒以内にエージェントは何が何を呼び出し、何が何を壊し、どこにすべてがあるのかという地図を手に入れます。解釈する必要がある一致の山ではなく、すでに組み立てられた接続構造です。

そして覚えています。セッション間でも、エージェント間でも。一つのエージェントが今夜学んだことを、別のエージェントが明日引き継ぎます。それには証拠が添付されており、コードがその後変更された場合には警告フラグが付きます。すべての結論には履歴があり、あなた、またはそれ以降のどのエージェントも、コードに何が起こったのか、なぜそうなったのかを常に確認できます。

そして l1ght はこれをさらに進めます: 論文、記事、RFC、ドラフトやノートが説明するコードの部分に接続され、同じ構造内に統合されます。エージェントは正しい文脈を手に入れ、中途半端なサウンドの文脈を避けます。そして、実在しないコードを作り出す行為が最も手っ取り早い道ではなくなります。構造が何が存在するのかを示し、判定がその信頼性の度合いを示します。

m1nd 以前は、関数は単なる関数であり、何らかのマニュアルで見失われていました。今では、コード、履歴、文書、リスクと組み合わさり、エージェントの知性の中に生きています。他のどこにもこれほどのものを見つけたことはありません。
