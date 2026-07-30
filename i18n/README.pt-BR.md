<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** dá ao seu agente de codificação um cérebro por repositório: um grafo local de código servido pelo MCP, memória ancorada ao código que cita, e um veredito de confiança para cada resposta. "Evidência insuficiente" é uma resposta válida aqui. Assim como "não confie nisso ainda, e aqui está como corrigir".

Nada sai da sua máquina. Um binário em Rust. MIT.

Pense nisso como um raio-X do seu repositório que seu agente pode ler: uma estrutura unificada que combina tudo e diz onde cada coisa está, para que serve aquele programa, no que estão trabalhando, o que foi concluído e o que ainda está pendente. Esse panorama é algo que nenhuma outra ferramenta entrega ao seu agente.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Quatro comandos para instalar: <a href="#sixty-seconds">Sessenta segundos</a>. Razões para fechar esta aba antes: <a href="#when-not-to-use-m1nd">Quando não usar m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Uma sessão real no m1nd: north fornece confiança, foco e lacunas honestas; seek responde com um veredito de reverificação; memorize ancora a descoberta no código." />
</p>

<p align="center"><em>Uma sessão real no grafo de 6.453 nós deste repositório (m1nd-mcp 1.4.0): <code>north</code> orienta, <code>seek</code> responde com um veredito de <code>reverify</code>, <code>memorize</code> ancora a descoberta no código.</em></p>

## A auditoria pela qual seu agente para de pagar

Você conhece o ritual. O agente abre um arquivo, executa um grep, abre outro arquivo, executa outro grep, queima a maior parte de seu contexto reconstruindo o que é o repositório, e só então começa a tarefa principal. Com m1nd, essa varredura se torna uma única pergunta. Em menos de um segundo, o agente obtém o mapa: o que chama o quê, o que quebra o quê, onde tudo está. Não é uma pilha de correspondências a interpretar. Uma estrutura conectada, já montada.

E ela lembra. Entre sessões, e entre agentes. O que um agente aprende à noite, outro agente herda no dia seguinte, com as evidências anexadas e uma bandeira indicando se o código foi alterado desde então. Cada conclusão deixa um rastro, para que você, ou qualquer agente que venha depois, possa sempre ver o que aconteceu com aquele código e por quê.

Então o l1ght leva isso mais longe: artigos, RFCs, rascunhos e anotações são conectados às partes do código que eles explicam, dentro da mesma estrutura. O agente obtém o CONTEXTO CERTO em vez do mais próximo que parece correto, e inventar código que não existe deixa de ser o caminho de menor resistência: a estrutura diz o que existe, e o veredito diz quanta confiança ter nisso.

Antes do m1nd, uma função era apenas uma função, perdida em algum manual. Agora, ela vive dentro da inteligência do agente, combinada com o código, sua história, seus documentos e seus riscos. Não encontrei nada como isso em nenhum outro lugar.

## grep responde a boas perguntas. m1nd responde às perguntas mais profundas.

Perguntas que seu agente agora pode fazer e obter uma resposta estrutural:

- O que quebra se eu alterar esta função?
- Onde ocorre a atualização do token neste repositório?
- Por que esses dois arquivos estão conectados, e esse caminho é sólido ou apenas uma suposição?
- O que a última sessão aprendeu sobre este código, e ainda é verdade?
- O que sempre muda junto aqui, mesmo sem uma importação entre eles?
- Esta edição cruza um limite de arquitetura que eu não deveria cruzar?
- Qual afirmação neste artigo esta função implementa?
- O bug que acabei de corrigir está escondido em outro lugar, como uma forma?
- O que está faltando aqui que esse padrão geralmente possui?
- Eu estou até mesmo no repositório certo?
- Eu devo agir com base nessa resposta ou verificá-la primeiro?

Cada uma é um verbo na superfície do MCP (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), não um truque de sugestão.

## E não para apenas em mostrar estruturas

Anticorpos: um bug corrigido torna-se um padrão estrutural nomeado, e todas as sessões futuras buscam essa forma em todo o repositório. Corrija uma vez, procure para sempre.

Conexões fantasma: arquivos que sempre mudam juntos, sem nenhuma importação entre eles, extraídos de seu histórico git. O acoplamento invisível que quebra os refatoramentos.

Lacunas estruturais: `missing` procura pelo código que não está lá. A proteção, o retry, o timeout que esse padrão geralmente inclui e que está ausente neste caso.

Hipóteses contra o grafo: você declara algo em linguagem simples ("configurações podem alcançar o boot sem validação") e o declara testado contra a estrutura viva.

Tremor: arquivos cuja velocidade de alteração está acelerando são sinalizados antes que alguém faça o relatório de bugs.

Um grafo aquecido: resultados confirmados fortalecem suas conexões, no estilo hebbiano, para que os caminhos que se provaram úteis tenham maior prioridade para o próximo agente.

Cada uma dessas funções sinaliza e sugere; seu compilador e testes ainda farão as comprovações.

## m1nd não apenas busca. Ele escreve.

Aqui está a parte em que as pessoas demoram alguns segundos para acreditar. O grafo que lê seu repositório também pode operar nele. Seu agente nomeia um símbolo e um destino, cerca de 48 tokens, e `transplant` calcula toda a movimentação a partir do grafo: a região ampliada (comentários de documentação e atributos viajam juntos), dependências classificadas por suas conexões de chamada (privadas viajam, compartilhadas permanecem e ganham uma importação de retorno), cada referência é requalificada em todos os arquivos que a nomeiam. Então ele grava as alterações de forma atômica, reanalisa e retorna um recibo honesto: o que se moveu, o que permaneceu e o que ele não conseguiu resolver. `refs_unresolved` nunca estará vazio silenciosamente quando algo deu errado.

Isso é feito em duas fases, `transplant_preview` antes de `transplant_commit`. A confirmação valida novamente o hash de cada arquivo que planejava tocar, para que nada seja aplicado a um repositório que mudou enquanto isso. Se algo essencial no repositório estiver protegido, a operação falha fechada. Uma recusa nunca toca um byte e ensina como corrigir: uma colisão nomeia o ocupante, um caminho de módulo inválido nomeia a si mesmo, e uma movimentação entre crates nomeia ambos os roots.

Medido em um caso real: a edição todo do arquivo custou 12.235 tokens de saída, o transplante custou 48 de entrada e gravou 3 arquivos em 1,3 segundos, com o crate compilando do outro lado. rust-analyzer tem uma issue aberta solicitando movimentações entre arquivos desde 2019.

Limitações da v1, declaradas claramente: Apenas em Rust, apenas `fn` no nível mais alto, apenas no mesmo crate, o arquivo de destino já deve existir, e referências originadas dentro de macros são invisíveis para o m1nd. Cada limitação é intencional e documentada em [docs/TRANSPLANT-PRD.md](docs/TRANSPLANT-PRD.md), ao lado de 13 arquivos de teste que validam o verbo.

Continua...
