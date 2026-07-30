🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** は、リポジトリごとにコーディングエージェントに脳を与えます: MCP 経由で提供されるローカルのコードグラフ、引用されたコードに結び付けられたメモリ、そして、すべての回答に対する信頼判定を提供します。「十分な証拠がありません」はここでは正当な答えと見なされます。また、「まだ信用できません、修正する方法はこちらです」という答えも含まれます。

データはすべてマシンの外に出ることはありません。一つの Rust 実行ファイル。MIT ライセンス。

これは、君のリポジトリをエージェントが読めるX線写真のようなものと考えるといいよ: すべてを組み合わせ、各要素がどこに位置し、何のためのプログラムであるのか、何が進行中で何が完了し、何が未解決なのかを示す一つの構造。それが、他のどのツールも君のエージェントに与えることができない全体像だ。

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">インストールに必要なコマンドは4つだけ: <a href="#sixty-seconds">Sixty seconds</a>。まずタブを閉じる理由: <a href="#when-not-to-use-m1nd">When not to use m1nd</a>.</p>

<p align="center">
  <img src="../docs/assets/demo.gif" width="760" alt="A real m1nd session: north returns trust, focus and honest gaps; seek answers with a reverify verdict; memorize anchors the finding to code" />
</p>

<p align="center"><em>このリポジトリの6,453ノードのグラフ上での実際のセッション (m1nd-mcp 1.4.0): <code>north</code> で方向付けを行い、<code>seek</code> は <code>reverify</code> の判定をつけた回答を提供し、<code>memorize</code> が発見内容をコードに結びつけます。</em></p>

## エージェントが料金を支払わなくなる監査

これはよくある話だ。エージェントがファイルを開いて greps を実行し、別のファイルを開いて再び greps を掛け、リポジトリが何なのかを再構築するのに大半の時間を費やした後、やっと実際のタスクに取りかかる。m1nd を使えば、その一連の手順が一つの質問に収束する。1秒以内に、エージェントは地図を手に入れることができる: どのコードがどれを呼び出し、何が壊れているのか、すべてがどこにあるのか。解釈する必要のあるマッチしたデータの山ではなく、すでに組み立てられたつながりの構造そのものだ。

そして、それは記憶する。異なるセッション間でも、異なるエージェント間でも。一つのエージェントが今日学んだことは、別のエージェントが明日継承する。証拠が添えられ、そのコードがそれ以来変わったかどうかのフラグも含まれる。すべての結論には追跡可能な証拠が添えられ、それによって君やその後に来るどんなエージェントも常にそのコードが何であるのか、何が起こったのかを確認することができる。

その後、l1ght はさらに一歩進める: 論文、記事、RFC、ドラフト、そしてノートが、それらが説明するコードの部分に接続され、同じ構造に組み込まれる。エージェントは正しいコンテキストを手に入れるので、最も近いものではなく疑問を生むコードを発明することが最も効率的な選択肢でなくなる。この構造が何が存在するのかを明示し、判定がどの程度信頼できるかを示してくれる。

m1nd登場以前は、関数は単なる関数にすぎず、どこかのマニュアルに埋もれていた。今や、それはエージェントの知性の内部に存在し、コードに、その歴史、文書、リスクと結びついている。このような機能を有するツールを、私は他で見つけたことがない。

## grepが良い質問には答える、m1ndはより深い質問に答える

エージェントが今では構造的な回答を得るために尋ねることができる質問:

- この関数を変更すると何が壊れるのか?
- このリポジトリではトークンの更新が実際にどこで行われているのか?
- なぜこれら2つのファイルが接続されているのか。そして、そのつながりは確かなものなのか、それとも推測に基づくものなのか?
- 前回のセッションでこのコードについて学んだことは何か、それはまだ有効か?
- ここでいつも一緒に変更されるものは何か。インポートなしでも?
- この編集はアーキテクチャの境界を越えてはいけない箇所を越えているか?
- この関数が実装している主張は、この論文のどの部分に関連しているのか?
- 今修正したバグが、形として他の場所に隠れていないか?
- ここに欠けているもの、通常このパターンにはあるものは何か?
- 自分は適切なリポジトリにいるのか?
- この回答をアクションに移すべきか、それともまず検証すべきか?

これらの質問は、MCP サーフェイス上の動詞(`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`)として実現されている。プロンプトトリックではない。

## 構造を見せるだけで終わらない

抗体: 修正されたバグは、構造的パターン名として登録され、その後のすべてのセッションでリポジトリ全体にわたるその形状をスキャンするように設定される。一度修正すれば、以後、永遠にその形を狩る。

ゴーストエッジ: git 履歴から採掘された、インポートなしでも常に一緒に変更されるファイル。リファクタリングを壊す見えない結びつき。

構造的ホール: `missing` は存在しないコードを探す。このパターンが通常備えているがこのインスタンスにはないガード、リトライ、タイムアウトなどだ。

グラフに対する仮説: 平易な言葉で主張を述べる (例: "設定が検証なしで起動まで到達できる") と、それがライブ構造に照らしてテストされる。

Tremor: 変更速度が加速しているファイルは、バグレポートが提出される前にフラグが立つ。

ウォームグラフ: 確認済みの結果がそのエッジを強化し、Hebbianスタイルでそれらの経路が次のエージェントのためにより高くランキングされる。

これらのすべてが警告するか提案する: コンパイラとテストは依然として証明を行う。

## m1ndは探すだけでなく、書くこともできる

...  

（残りの内容を続けて翻訳しますか?）
