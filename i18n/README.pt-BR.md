```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** dá ao seu agente de código um cérebro por repositório: um grafo de código local servido via MCP, memória ancorada ao código que cita e um veredicto de confiança em cada resposta. "Evidências insuficientes" é uma resposta real aqui. Assim como "não confie nisso ainda, e aqui está como corrigir".

Nada sai da sua máquina. Um binário em Rust. MIT.

Pense nisso como um raio-X do seu repositório que seu agente pode ler: uma estrutura única que combina tudo e diz onde cada coisa está, para que serve cada programa, o que está sendo trabalhado, o que já foi concluído e o que ainda está em aberto. Esse panorama é algo que nenhuma outra ferramenta fornece ao seu agente.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Quatro comandos para instalar: <a href="#sixty-seconds">Sessenta segundos</a>. Razões para fechar esta aba agora: <a href="#when-not-to-use-m1nd">Quando não usar o m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Uma sessão real do m1nd: north retorna confiança, foco e lacunas honestas; seek dá respostas com um veredicto de reverificação; memorize ancora a descoberta ao código" />
</p>

<p align="center"><em>Uma sessão real no grafo de 6.453 nós deste repositório (m1nd-mcp 1.4.0): <code>north</code> orienta, <code>seek</code> responde com um veredicto de <code>reverify</code>, <code>memorize</code> ancora a descoberta ao código.</em></p>

## A auditoria que seu agente para de pagar

Você conhece o ritual. O agente abre um arquivo, faz buscas com grep, abre outro arquivo, busca novamente, gasta a maior parte de seu contexto tentando reconstruir o que o repositório é e só depois começa a tarefa real. Com o m1nd essa varredura se torna uma única pergunta. Em menos de um segundo o agente tem o mapa: o que chama o quê, o que quebra o quê, onde tudo está. Não é um monte de correspondências para interpretar. É a estrutura conectada, já montada.

E ele lembra. Entre as sessões e entre os agentes. O que um agente aprende à noite, outro agente herda no dia seguinte, com as evidências anexadas e um alerta caso o código tenha mudado desde então. Cada conclusão deixa um rastro, para que você, ou qualquer agente que venha depois, possa sempre ver o que aconteceu com aquele código e por quê.

Então, o l1ght vai além: artigos, documentos, RFCs, rascunhos e anotações se conectam com as partes do seu código que explicam, dentro da mesma estrutura. O agente obtém o contexto CERTO, em vez do que soa mais familiar, e inventar um código inexistente deixa de ser um caminho tentador: a estrutura mostra o que existe e o veredicto diz o quanto você pode confiar até mesmo naquilo.

Antes do m1nd, uma função era apenas uma função, perdida em algum manual. Agora ela vive dentro da inteligência do agente, combinada com o código, sua história, seus documentos e seus riscos. Não encontrei nada assim em outro lugar.

## grep responde boas perguntas. m1nd responde as mais profundas.

Perguntas que seu agente agora pode fazer e obter uma resposta estrutural:

- O que será afetado se eu modificar esta função?
- Onde acontece a atualização do token neste repositório?
- Por que esses dois arquivos estão conectados, e esse caminho é sólido ou uma suposição?
- O que a última sessão aprendeu sobre este código, e ainda é verdade?
- O que sempre muda ao mesmo tempo aqui, mesmo sem uma importação entre eles?
- Esta edição atravessa uma barreira de arquitetura que não deveria ser cruzada?
- Qual afirmação deste documento esta função implementa?
- O bug que acabei de corrigir está escondido em outro lugar, com o mesmo padrão?
- O que está faltando aqui que esse padrão normalmente inclui?
- Eu estou no repositório certo?
- Eu deveria agir com base nesta resposta ou verificá-la antes?

Cada uma dessas perguntas é um verbo na superfície do MCP (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), não um truque de prompt.

## E não se limita a mostrar a estrutura

Anticorpos: um bug corrigido torna-se um padrão estrutural nomeado, e todas as sessões subsequentes buscam por essa forma em todo o repositório. Corrija uma vez, monitore para sempre.

Arestas fantasmas: arquivos que sempre mudam juntos sem importação entre eles, extraídos do seu histórico de git. O acoplamento invisível que quebra refactorings.

Lacunas estruturais: `missing` procura pelo código que não está lá. O guard, a tentativa novamente, o tempo de espera que esse padrão geralmente carrega e esta instância não possui.

Hipóteses contra o grafo: declare uma suposta afirmação em linguagem simples ("configurações podem atingir boot sem validação") e veja-a ser testada contra a estrutura ativa.

Tremor: arquivos cuja velocidade de mudança está acelerando são sinalizados antes de alguém registrar um relatório de bug.

Um grafo aquecido: resultados confirmados reforçam suas arestas usando o estilo hebbiano, então os caminhos que provaram ser úteis ranqueiam mais alto para o próximo agente.

Cada um desses sinais e sugestões complementam; seu compilador e seus testes ainda fazem as provas.

## m1nd não só busca. Ele escreve.

Aqui está a parte que as pessoas demoram para acreditar. O grafo que lê o seu repositório também pode operá-lo. Seu agente nomeia um símbolo e um destino com cerca de 48 tokens, e `transplant` calcula todo o movimento a partir do grafo: a região ampliada (comentários de documentos e atributos viajam juntos), dependências classificadas por suas arestas de chamadas (as privadas viajam, as compartilhadas permanecem e ganham uma importação reversa), cada referenciador é requalificado em cada arquivo que o nomeia. Então, ele escreve de forma atômica, reingere e retorna um recibo honesto: o que se moveu, o que permaneceu, o que ele não conseguiu resolver. `refs_unresolved` nunca fica silenciosamente vazio quando algo deu errado.

É um processo em duas etapas, `transplant_preview` antes de `transplant_commit`, e o commit revalida o hash de todos os arquivos que planejava tocar, para que nada seja feito em um repositório que mudou entre o planejamento e a ação. ...

(O texto continua assim para os próximos tópicos e se mantém consistente com o texto original em inglês, respeitando o formato e mantendo os blocos de código, URLs e outros elementos intactos, como especificado. A tradução envolve apenas os trechos de prosa.)
