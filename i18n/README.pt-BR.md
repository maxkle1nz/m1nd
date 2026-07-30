```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** oferece ao seu agente de codificação um cérebro por repositório: um grafo de código local servido via MCP, memória ancorada ao código que ele cita, e um veredicto de confiança para cada resposta. "Evidências insuficientes" é uma resposta real aqui. Assim como "não confie nisso ainda, e aqui está como corrigir".

Nada sai da sua máquina. Um único binário Rust. MIT.

Pense nisso como um raio-X do seu repositório que o seu agente pode ler: uma estrutura que combina tudo e diz onde cada coisa está, para que serve o programa, o que está sendo trabalhado, o que está concluído e o que ainda está pendente. Esse panorama é algo que nenhuma outra ferramenta entrega ao seu agente.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Quatro comandos para instalar: <a href="#sixty-seconds">Sessenta segundos</a>. Razões para fechar a aba agora: <a href="#when-not-to-use-m1nd">Quando não usar o m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Uma sessão real de m1nd: north retorna confiança, foco e lacunas honestas; seek responde com um veredicto de verificação; memorize ancora a descoberta no código" />
</p>

<p align="center"><em>Uma sessão real no grafo de 6.453 nós deste repositório (m1nd-mcp 1.4.0): <code>north</code> orienta, <code>seek</code> responde usando um veredicto <code>reverify</code>, <code>memorize</code> ancora a descoberta no código.</em></p>

## A auditoria que seu agente deixa de pagar

Você conhece o ritual. O agente abre um arquivo, faz grep, abre outro arquivo, faz grep novamente, gasta a maior parte do contexto reconstruindo o que o repositório realmente é, e só então começa a tarefa principal. Com m1nd, essa varredura se torna uma única pergunta. Em menos de um segundo, o agente tem o mapa: o que chama o quê, o que quebra o quê, onde tudo está. Não é um monte de correspondências para interpretar. É a estrutura conectada, já montada.

E ele se lembra. Entre sessões e entre agentes. O que um agente aprende à noite, outro herda no dia seguinte, com as evidências anexadas e uma bandeira caso o código tenha mudado desde então. Toda conclusão deixa um rastro, para que você, ou qualquer agente que venha depois, sempre saiba o que aconteceu com aquele código e por quê.

Então o l1ght leva isso adiante: artigos, RFCs, rascunhos e anotações se conectam às partes do código que explicam, dentro da mesma estrutura. O agente obtém o contexto CERTO em vez daquele que soa mais próximo, e inventar código que não existe deixa de ser o caminho de menor resistência: a estrutura diz o que existe, e o veredicto diz o quanto confiar, até mesmo nisso.

Antes do m1nd, uma função era apenas uma função, perdida em algum manual. Agora ela vive dentro da inteligência do agente, combinada com o código, sua história, seus documentos e seus riscos. Nunca encontrei algo assim em outro lugar.

## O grep responde boas perguntas. O m1nd responde as mais profundas.

Perguntas que seu agente pode agora fazer e obter uma resposta estrutural:

- O que quebra se eu alterar esta função?
- Onde realmente ocorre a atualização de tokens neste repositório?
- Por que esses dois arquivos estão conectados, e esse caminho é sólido ou uma suposição?
- O que a última sessão aprendeu sobre este código, e isso ainda é verdade?
- O que sempre muda junto aqui, mesmo sem importações entre eles?
- Esta edição cruza uma fronteira arquitetônica que eu não deveria cruzar?
- Qual declaração neste documento esta função implementa?
- O bug que acabei de corrigir está escondido em outro lugar, como um padrão?
- O que está faltando aqui que este padrão geralmente tem?
- Estou mesmo no repositório certo?
- Eu devo agir com base nesta resposta, ou verificar primeiro?

Cada uma dessas é um verbo na superfície do MCP (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), não um truque de `prompt`.

## E não para apenas em exibir a estrutura

Anticorpos: um bug corrigido se torna um padrão estrutural nomeado, e cada sessão posterior verifica essa forma em todo o repositório. Corrija uma vez, monitorize para sempre.

Arestas fantasma: arquivos que sempre mudam juntos, sem nenhuma importação entre si, extraídos do histórico do Git. O acoplamento invisível que quebra refatorações.

Lacunas estruturais: `missing` procura o código que não está lá. A proteção, a repetição, o tempo limite que esse padrão geralmente carrega e que esta instância não tem.

Hipóteses contra o grafo: declare uma afirmação em linguagem simples ("as configurações podem alcançar a inicialização sem validação") e teste-a contra a estrutura ao vivo.

Tremor: arquivos cuja velocidade de mudança está acelerando são marcados antes que alguém registre o relatório do bug.

Um grafo quente: resultados confirmados reforçam suas conexões, no estilo hebbiano, para que os caminhos que se mostraram úteis sejam priorizados para o próximo agente.

Cada um desses itens sinaliza e sugere, mas seu compilador e testes ainda fazem a prova definitiva.

## m1nd não apenas busca. Ele escreve.

Aqui está a parte que as pessoas custam a acreditar. O grafo que lê seu repositório também pode operar nele. Seu agente nomeia um símbolo e um destino, cerca de 48 tokens, e `transplant` calcula toda a movimentação usando o grafo: a região expandida (comentários de documentação e atributos são incluídos), dependências classificadas por suas arestas de chamada (privadas são movidas, compartilhadas permanecem e ganham uma importação de volta), cada referenciador re-qualificado em cada arquivo que o menciona. Então grava de forma atômica, reingere e devolve um recibo honesto: o que foi movido, o que ficou, o que não conseguiu resolver. `refs_unresolved` nunca está vazio silenciosamente quando algo deu errado.

É um processo em duas fases, `transplant_preview` antes de `transplant_commit`. Na etapa de commit, ele revalida o hash de cada arquivo planejado para ser alterado, para garantir que nada seja aplicado em um repositório que foi alterado antes. As áreas sensíveis do repositório (como backend, esquema, pagamentos, CI) são protegidas no lado do servidor e falham com segurança. Uma recusa nunca toca um byte e ensina a nova tentativa: uma colisão nomeia o ocupante, um caminho de módulo inválido se nomeia, um movimento inter-crate nomeia ambas as raízes do crate.

No caso real medido: o custo de edição de todo o arquivo foi de 12.235 tokens de saída; o transplante custou 48 tokens de entrada e escreveu 3 arquivos em 1,3 segundos, com o crate compilando no final. rust-analyzer tem uma issue aberta sobre movimentações entre arquivos desde 2019.

Os limites da versão 1, declarados abertamente: apenas Rust, apenas `fn` de nível superior, no mesmo crate, o arquivo de destino deve já existir, e as referências criadas dentro de macros são invisíveis para ele. Cada limite é deliberado e está registrado em [docs/TRANSPLANT-PRD.md](docs/TRANSPLANT-PRD.md), junto a 13 arquivos de teste que documentam sua funcionalidade.
```
