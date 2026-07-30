```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** 为你的编码代理每个代码库提供一个大脑：通过 MCP 提供的本地代码图，锚定于它引用的代码的记忆，以及对每个答案的可信度判断。这里的一个真实答案可以是“证据不足”，也可以是“暂时不要相信这个结果，但可以通过以下方法修复”。

一切都不会离开你的机器。一个 Rust 二进制文件。MIT 许可。

把它想象成你代码库的 X 光片，你的代理可以通过它来阅读：一种将一切结合起来的结构，指示每个内容的位置，该程序的用途，正在处理的内容，已经完成的内容以及仍然开放的问题。这个全景是其他工具无法提供给代理的东西。

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">安装只需四步：<a href="#sixty-seconds">60 秒上手</a>。想先关闭页面的理由：<a href="#when-not-to-use-m1nd">何时不应使用 m1nd</a>。</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="A real m1nd session: north returns trust, focus and honest gaps; seek answers with a reverify verdict; memorize anchors the finding to code" />
</p>

<p align="center"><em>在本代码库的 6,453 节点图上进行真实会话（m1nd-mcp 1.4.0）：<code>north</code> 给出方向，<code>seek</code> 返回带有 <code>reverify</code> 判断的答案，<code>memorize</code> 将发现结果锚定到代码中。</em></p>

## 让你的代理停下重复劳动的审计工具

熟悉这样的流程吗？代理打开一个文件，然后 grep，接着再打开另一个文件，再次 grep，耗费它几乎所有的记忆重建代码库的结构，然后才开始真正的任务。而借助 m1nd，这种反复搜寻将简化成一个问题。不到一秒，它就可以描绘出整幅地图：哪个调用了哪个，哪个破坏了哪个，每件东西的位置在哪。它不再只是呈现一堆待解释的结果，而是一个已组装完好的连接结构。

并且它会记住。在会话之间以及代理之间。一位代理在今晚学到的东西会被另一位代理在明天继承，并附加相应的证据，同时标记代码是否已经有所变化。每个结论都会留下痕迹，所以你，或任何之后的代理，总能清楚地看到发生了什么以及为什么。

随后，l1ght 将这一切带入更深远的层次：将论文、文章、RFC、草稿和笔记与代码中解释它们的部分联系起来，形成一个整体的结构。代理获取的是正确的背景，而不是最接近、听起来相似的信息。胡乱臆测不存在的代码不再是最简单的方式：结构指出了现有内容，判断也表明了甚至对这些内容的信任程度。

在 m1nd 之前，一个函数只不过是手册中迷失的一个函数。有了它，它就能成为代理智慧的一部分，与代码、历史、文档和风险结合在一起。我还没有在别的地方找到类似的工具。

## grep 能回答好问题，m1nd 则回答更深刻的问题

通过 m1nd，现在你的代理可以询问并获得结构性答案的问题：

- 如果我改动这个函数，会破坏什么？
- 这个代码库中 Token 刷新到底在哪里实现的？
- 为什么这两个文件是连接的？这种路径扎实还是臆测出来的？
- 上一次会话从这个代码中学到了什么？它是否仍然有效？
- 这里总会同时变化的部分是什么，即使它们之间没有调用关系？
- 这一编辑是否越过了我不应该跨越的架构边界？
- 这篇论文中的哪个主张被这个函数实现了？
- 我刚刚修复的 bug，其它地方还有相似的潜在问题吗？
- 这个模式通常会带有的内容在这里是否缺失了？
- 我甚至够资格处理这个代码库问题吗？
- 我应该采取行动还是先验证答案？

这些问题都可以以 MCP 接口暴露的动词来回答（`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`），而不是依赖提示词技巧。

## 它不止于展示结构

抗体：一个修复过的 bug 变成一个已命名的结构化模式，之后的每个会话都会在代码库中扫描这种形式。修复一次，永久捕捉。

幽灵边：文件在没有引用关系的情况下总是一同改变，这些从你的 git 历史中挖掘得来的文件反映了对重构的潜在风险。

结构洞：`missing` 寻找缺失的代码。比如这模式通常会带有的保护、重试或超时机制，但此实例中没有。

对图的假设：用自然语言陈述一个主张（例如“设置可以不验证就触发系统启动”），然后在活跃的结构中进行验证。

实时波动：变更速率加速增长的文件在出现故障报告前会被标记。

激活的图：被确认的结果加强其边界，就像 Hebbian 风格一样，所以对代理确实有用的路径会在下次优先排序。

以上每一个都会做出标记和建议，而你的编译器和测试工具仍然是最终的证明者。

## m1nd 不只是搜索，它能写。

这里是常令人难以置信的部分。阅读代码库的图也可以对其进行操作。你的代理只需提供一个符号名称和目标位置，大约 48 个 token，`transplant` 凭借图计算整个操作: 扩展的区域（文档注释和属性随之移动）, 依赖关系根据调用关系分类（私有的随之移动，公共的则保留并新增一个后向导入）, 每个引用者都在每个文件中被重新限定。完成后，它会原子化地写入数据，重新加载，并返回一份真实的操作结果：什么移动了，什么留了下来，哪些问题未能解决。如果出了问题，`refs_unresolved` 绝不会悄无声息地空着。

两阶段操作: 先 `transplant_preview` 再 `transplant_commit`, 提交阶段会重新验证它计划触及的所有文件的哈希值, 所以不会在发生变更期间提交错误代码。代码库的关键区域（比如后端、模式、支付、CI）受到服务端保护，如果操作失败系统将关闭，而不会触动任何数据。不成功时文件系统不会改变并指导调整: 冲突会命名冲突部分, 无效模块路径会明确说明, 跨模块移动会同时命名两个模块根。

具体的功能限制（v1 版本支持情况）: 仅支持 Rust, 仅处理顶级 `fn`, 仅支持同一个 crate, 目标文件必须已存在且宏内的引用暂无法识别。每一个限制都是经过深思熟虑的并记录在 [docs/TRANSPLANT-PRD.md](docs/TRANSPLANT-PRD.md) 中，同时有13个测试文件来验证相关功能。

... 
``` [Translation truncated at midpoint due to length]

---

Please upload the rest of your README if you'd like the continuation!
