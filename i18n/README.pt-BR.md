🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">Inteligência Operacional para Agentes de Código</h1>

<p align="center">
  <strong>Seu agente de código para de começar às cegas.</strong><br/>
  <em>Local-first. MCP-nativo. Memória em grafo, confiança e raciocínio sobre mudanças para hosts de agentes.</em>
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

**m1nd é inteligência operacional para agentes de código — ele governa o loop operacional, não apenas a recuperação de dados.**

> `grep` encontra texto. Busca vetorial encontra chunks similares. `m1nd` dá aos agentes um grafo local do que se conecta, o que mudou, o que quebra, o que derivou e onde retomar.

Três coisas aqui coexistem em nenhuma outra ferramenta:

- **Grafo causal de código** — `impact` antes de editar mostra o blast radius que você não leu; `ghost_edges` revela arquivos que sempre mudam juntos mas não compartilham nenhum import.
- **Memória auto-verificável** — `memorize` ancora descobertas a nós reais do código; `cross_verify` os marca como obsoletos quando esse código muda.
- **Uma camada de confiança e recuperação** — cada resultado carrega um modo de confiança; `trust_selftest` e `recovery_playbook` avisam o agente quando o binding de workspace está errado e como se recuperar.

Além de um **runtime de atenção** — `focus` entrega ao agente o conjunto de trabalho mínimo e delimitado por orçamento para um objetivo, com uma cauda honesta do que deixou de fora e um sinal de se aquilo já é contexto *suficiente*.

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="Loop tradicional de agente vs loop ancorado no m1nd" width="960" />
</p>

## Novidades na 1.2.0 — o primeiro release da era OMEGA

A 1.2.0 transforma o loop de "recupere, depois torça" em **pré-orientar → agir sobre veredictos calibrados → capturar o que você aprendeu**. O tema é o mesmo da camada de confiança: um *não* honesto vence um palpite confiante.

- **`north(task)` — pré-orientação em uma chamada.** A nova porta de entrada compõe confiança, contexto da tarefa (nós de foco + âncoras de PageRank), memória prévia entre sessões, um sinal de suficiência, um `next_move` e `honest_gaps` (o que o m1nd ainda *não* sabe). `needs_ingest` é uma resposta real para um grafo vazio. (A composição de recall L1GHT que dobra a memória prévia dentro do pacote entrou na `main` logo após a tag 1.2.0 — ela não está no binário 1.2.0.)
- **Calibração conformal na predição.** `calibrate_predict` arma um gate por repositório; os veredictos passam então a ler `act` / `reverify` / `abstain`, onde `abstain` significa *não calibrado ou insuficiente* — um sinal para parar, não um sim fraco. Vem desativado: até você calibrar, os veredictos ficam limitados a `reverify`.
- **`trust_envelope` no `seek`** (vem desativado) e um **veredicto `closure` no `why`** — `blocked` significa que o caminho se apoia em uma aresta não resolvida/adivinhada. **`trust_band: insufficient_evidence`** agora é distinto de uma banda de risco: significa *sem evidência*, a resposta honesta de partida a frio, não "risco médio".
- **A memória ganhou uma espinha de proveniência** — afirmações carregam idade + autor reais, substituem afirmações mais antigas, expiram e respeitam um teto de recência, de modo que o conhecimento lembrado declara seu próprio frescor em vez de silenciosamente ficar obsoleto.
- **Co-mudança com Jaccard suavizado** — `ghost_edges` / `predict` agora normalizam o acoplamento em vez de contar co-commits brutos (comprovado em calibração como +3 pontos sobre contagens brutas).
- **Versão do binário + fingerprint sha** — `--version` imprime `1.2.0 (<sha>)`; `M1ND_EXPECTED_VERSION` / `M1ND_EXPECTED_SHA` (+ `M1ND_STRICT_VERSION`) permitem que um host detecte e recuse um binário que derivou.
- **Instruções MCP nativas de agente + relatórios de campo apenas locais.** As instruções de `initialize` que todo host recebe agora *são* o loop operacional acima. Agentes podem deixar um sinal de telemetria por sessão — `learn` sobre um veredicto de recuperação, ou uma linha em `~/.m1nd/field-reports.jsonl` quando o próprio m1nd se comporta mal. Esse arquivo é apenas local; **o m1nd nunca liga para casa.**

## Início Rápido

O caminho mínimo feliz — instale a partir do fonte (sempre atualizado), verifique a saúde, conecte seu host:

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
```

Depois conecte seu host — os mesmos dois comandos, um por host (`codex`, `claude`, `gemini`, `antigravity`, `generic`):

| Host | Instalar o pacote de agente | Conectar a config MCP |
|---|---|---|
| Codex | `m1nd install-skills codex` | `m1nd mcp-config codex --project /your/project` |
| Claude Code | `m1nd install-skills claude --project /your/project` | `m1nd mcp-config claude --project /your/project` |
| Gemini | `m1nd install-skills gemini --project /your/project` | `m1nd mcp-config gemini --project /your/project` |
| Antigravity | `m1nd install-skills antigravity --project /your/project` | `m1nd mcp-config antigravity --project /your/project` |
| Generic | `m1nd install-skills generic --project /your/project` | `m1nd mcp-config generic --project /your/project` |

Ou pelo npm: `npm install -g @maxkle1nz/m1nd`.

Mapa completo de instalação, pacotes de host, build nativo do runtime e flags de atualização: [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · configuração por cliente: [matriz de integração](../docs/IDE-INTEGRATIONS.md).

### Ponto de Entrada do Agente

Agentes fazem parse deste README. Dentro de uma sessão MCP, a porta de entrada é uma única chamada — `north(task)` compõe confiança, contexto da tarefa, memória prévia entre sessões, um sinal de suficiência, um `next_move` e `honest_gaps` (o que o m1nd ainda *não* sabe) em um único pacote. Se ele reportar `needs_ingest` (grafo vazio), ou você estiver em um binário mais antigo, recorra ao loop explícito de confiança — estabeleça confiança *antes* de acreditar em qualquer recuperação:

```jsonc
// 0. Trust the binding in one call (verdict before retrieval)
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. If the verdict is not full_trust, ask for the deterministic recovery path
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Build graph truth
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Ask a structural question — empty results say *why*, never just "no results"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**Loop de primeira sessão, em quatro movimentos:** `north` (ou `trust_selftest` → `ingest`) → `seek`/`audit` → `memorize` a descoberta durável para que a próxima sessão comece à frente.

Quando não há uma sessão MCP viva para chamar `north` — ela está obsoleta, vinculada ao repositório errado ou ainda não carregada — recorra ao CLI neutro de host como escape hatch. Ele inicia um runtime isolado, vincula ao repositório e retorna um único envelope legível por máquina que delimita o escopo, estabelece confiança, ingere se necessário, retorna âncoras e faz o handoff para prova direta:

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

### Sirva um grafo, conecte muitos agentes

O Início Rápido acima conecta um servidor stdio por host — ótimo para um agente, mas cada processo carrega seu próprio grafo e mantém seu próprio lease. O deployment para o qual o m1nd foi construído é um dono, muitos agentes conectados. Um processo dono mantém o grafo vivo:

```bash
m1nd-mcp --serve --no-gui --port 1337 --runtime-dir /your/project/.m1nd
```

Cada agente então se conecta como uma ponte fina stdio↔HTTP — ele não carrega **nenhum** grafo, não constrói nenhum motor e não toma **nenhum** lease:

```bash
m1nd-mcp --attach http://127.0.0.1:1337 --stdio    # or set M1ND_ATTACH_URL and omit the flag
```

Qualquer número de pontes aponta para o único dono e compartilha seu único grafo vivo, de modo que o que um agente `memorize` outro recupera imediatamente — sem reingestão, sem cópia por agente. As consultas passam por localhost, então continua local-first (o bind permanece `127.0.0.1` a menos que você opte por `--bind 0.0.0.0`). Um `seek` quente sobre a ponte mediu ≈0.7ms em um grafo pequeno em uma máquina — ordem de grandeza, não uma garantia: o attach adiciona um round-trip de localhost, e a latência escala com o tamanho e a carga do grafo.

## O Que o m1nd Não É

`m1nd` não é apenas:

- uma ferramenta de busca de código com índice maior
- uma camada de RAG de repositório que só recupera arquivos ou chunks
- um banco de dados de grafo que deixa as decisões de workflow para o cliente
- um substituto de análise estática para o compilador, testes ou ferramentas de segurança
- um bundle MCP de utilitários sem relação entre si

Ele é a camada que transforma essas superfícies em um sistema operacional sobre o qual um agente pode raciocinar e agir. Não serve para lookups de um único arquivo, grep simples ou verdade do compilador — use ferramentas simples nesses casos.

## Por Que Agentes Precisam Disso

Sem o m1nd, toda sessão começa com loops de grep e reorientação manual; as descobertas da semana passada se foram, e um resultado de busca vazio é indistinguível de um binding de workspace errado. Com o m1nd, a sessão começa com um veredicto de confiança, descobertas passadas carregam automaticamente já ancoradas ao código que as sustenta, e resultados vazios dizem *por quê*.

Agentes em codebases reais não falham porque não sabem pesquisar. Eles falham porque não têm um modelo operacional. Reconstroem contexto do zero a cada sessão, editam sem conhecer o blast radius, e não conseguem distinguir um resultado vazio que significa "não existe nada" de um que significa "repositório errado."

Isso funciona para codebases pequenas. Desmorona quando o projeto tem artefatos gerados, specs, docs, histórico oculto de co-mudança, múltiplos agentes e handoffs longos. O problema não é apenas o raciocínio do agente — o agente não tem um modelo durável da estrutura do codebase. `m1nd` lhe dá um: um grafo causal de código com spreading activation por dimensões estruturais, semânticas, temporais e causais, mais plasticidade Hebbiana que se acumula por agente entre sessões.

## Memória Composta (L1GHT)

A maioria das ferramentas dá ao agente melhor *recuperação*. `m1nd` também permite que um agente **produza conhecimento durável e legível por máquina** que se acumula entre sessões e se mantém honesto com relação ao código. L1GHT transforma o conhecimento produzido em estrutura nativa de grafo que se auto-sinaliza quando o código que cita muda — afirmações confiantes propagam mais ativação do que as incertas.

O loop, do começo ao fim:

1. **Concluir** — o agente chega a algo durável (uma decisão, uma descoberta verificada, por que o código é do jeito que é) e chama `memorize` com afirmações estruturadas e caminhos de `evidence`.

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

2. **Ancorar** — o m1nd grava um `.light.md` nativo de grafo em `<runtime>/agent-memory/`, ingere (`adapter=light mode=merge`) e resolve cada caminho de `evidence` ao nó de código real via aresta `grounded_in` — fazendo o conhecimento viver no mesmo espaço de ativação que o código e emergir em `seek` / `activate` / `impact`.
3. **Carga automática** — a cada início de sessão futuro, `m1nd` ingere `agent-memory/` automaticamente e o reporta em `session_handshake.agent_memory`. Descobertas passadas sobrevivem a uma ingestão `mode=replace` e simplesmente *estão lá*.
4. **Auto-sinalização de obsolescência** — `cross_verify(check: ["evidence_freshness"])` re-faz o hash de cada arquivo citado e nomeia quais afirmações ficaram obsoletas porque seu código mudou — assim a memória avisa quando está mentindo, em vez de induzir ao erro.

Este loop foi provado ao vivo de ponta a ponta: `memorize` → aresta `grounded_in` → flag de frescor em arquivo editado → sobrevive a `mode=replace` → carga automática no boot. Encerrando uma missão delimitada? Passe `write_light_memory: true` para `mission_close` para persistir suas afirmações verificadas da mesma forma. O hábito está documentado nas `instructions` do servidor que cada cliente MCP recebe no `initialize` — agnóstico de host, sem plugin específico de cliente necessário.

## A Camada de Confiança e Honestidade

Esta é a coisa mais defensável que o m1nd faz, e nenhum concorrente a entrega. A doutrina: **credibilidade vem da honestidade, não de sempre vencer.**

- **`trust_selftest`** retorna um veredicto *antes* de qualquer recuperação: `full_trust`, `needs_ingest`, `wrong_workspace_binding`, `stale_binding_suspected` ou `degraded_host_tool_surface`. O agente sabe se deve prosseguir, ingerir, rebindar ou recuar.
- **`agent_runtime_contract`** acompanha toda resposta de recuperação, carregando um `trust_mode`. Um resultado vazio é disambiguado — vinculado ao repositório errado versus genuinamente nada lá — nunca reportado silenciosamente como "sem resultados."
- **Arrays `non_claims`** são enviados em toda ferramenta de missão. O m1nd diz ao agente o que ele *não* provou.
- **`mission_verify` pode dizer não — e diz, em código testado.** Ele rejeita evidência apenas de grafo: uma afirmação não pode ser fechada sem uma leitura de arquivo, uma execução de teste ou uma sonda de runtime. O teste literalmente se chama `graph_only_evidence_is_not_enough`.
- **`recovery_playbook`** retorna uma lista de passos determinística e ordenada para reparar o binding.

A prova do compromisso está no que foi sacrificado por ele: `savings` e `resonate` foram removidos da superfície anunciada no beta.7 porque uma ferramenta que sempre afirma vencer não é crível. Nenhum concorrente — nem mem0, Zep, Letta, Sourcegraph ou qualquer MCP de grafo de código — entrega uma camada que diz ao agente no que *não* confiar e como se recuperar.

**O loop de triagem de campo se fecha sobre si mesmo.** A telemetria de sessão que os agentes deixam em `~/.m1nd/field-reports.jsonl` (apenas local — o m1nd nunca liga para casa) não é um log passivo: os relatórios são triados, e um bug de campo *confirmado* vira um caso de bateria vermelho **antes** da correção, de modo que a regressão é provada, não apenas descrita. Esse loop já rodou uma vez de ponta a ponta: dois bugs reportados em campo viraram casos de bateria que falhavam e depois correções mergeadas — o `north` agora compõe recall L1GHT em seu pacote de memória, e a sentinela de grafo `temp` resolve para um tempdir real em vez de sujar o diretório de trabalho.

## Cobertura de Linguagens

O raciocínio de grafo (`impact`, `why`, `predict`, `trace`, `taint_trace`) é tão bom quanto o extrator. O m1nd resolve tanto **arestas `calls`** (grafo de chamadas) quanto **`imports` entre arquivos** (resolução de dependência arquivo→arquivo) por linguagem. A matriz abaixo foi provada ao vivo em uma única ingestão poliglota:

| Linguagem | `calls` | imports entre arquivos |
|---|:---:|:---:|
| Rust | ✅ | ✅ (`mod`/`use crate::`) |
| Python | ✅ | ✅ |
| JavaScript / TypeScript | ✅ | ✅ |
| Go | ✅ | ✅ (package) |
| Java | ✅ | ✅ (FQCN + wildcard) |
| C / C++ | ✅ | ✅ (`#include "..."`) |
| Kotlin | ✅ | ✅ (package) |
| PHP | ✅ | ✅ (PSR-4) |
| Scala | ✅ | ✅ (package) |
| Ruby | ⏳ | ✅ (`require_relative`) |
| C# | ✅ | — (namespaces não mapeiam 1:1 para arquivos) |
| Swift | ✅ | — |

Todas as linhas ✅ são verificadas de ponta a ponta (um import `caller`→`callee` resolve e o caller emite arestas de chamada). Outras linguagens recaem no extrator genérico (somente `contains`). Imports não resolvíveis (pacotes externos, gems, stdlib, cabeçalhos de sistema) são honestamente deixados sem resolução em vez de serem adivinhados.

## Mapa de Capacidades

A superfície MCP ativa evolui com os releases. Use `tools/list` para a contagem exata de ferramentas e nomes no seu build atual.

| Área | O que permite | Ferramentas representativas |
|---|---|---|
| Fundação do grafo | ingerir código, manter estado do grafo, diagnosticar continuidade de sessão, reforçar caminhos úteis e detectar drift de peso entre sessões | `trust_selftest`, `session_handshake`, `recovery_playbook`, `ingest`, `health`, `doctor`, `learn`, `warmup`, `drift` |
| Recuperação e orientação | buscar por texto, caminho, intenção, estrutura ou relacionamento antes de leituras manuais de arquivo | `audit`, `search`, `glob`, `seek`, `activate`, `why`, `trace` |
| Documentos e binding de conhecimento | ingerir docs universais ou `L1GHT` nativo de grafo e linkar conceitos de volta ao código | `ingest(adapter="universal"\|"light")`, `document_resolve`, `document_provider_health`, `document_bindings`, `document_drift`, `auto_ingest_*` |
| Navegação e continuidade | manter rotas com estado, handoffs, baselines e memória de investigação entre sessões | `perspective_*`, `trail_*`, `coverage_session`, `boot_memory`, `persist` |
| Mission control e disciplina de prova | manter uma rota delimitada, registrar eventos, passar de orientação por grafo para prova direta, fazer handoff e encerrar com lacunas explícitas | `mission_start`, `mission_event`, `mission_next`, `mission_verify`, `mission_handoff`, `mission_close` |
| Planejamento e prova de mudança | raciocinar sobre impacto, co-mudança, passos faltando, caminhos de falha e afirmações estruturais | `impact`, `predict`, `validate_plan`, `missing`, `hypothesize`, `counterfactual`, `differential` |
| Qualidade, segurança e arquitetura | detectar padrões, caminhos de taint, fronteiras de confiança, duplicação, violações de camada, fluxos de tipo e alvos de refatoração | `scan`, `scan_all`, `heuristics_surface`, `antibody_*`, `taint_trace`, `type_trace`, `trust`, `layers`, `layer_inspect`, `twins`, `fingerprint`, `flow_simulate`, `epidemic`, `tremor`, `refactor_plan` |
| Tempo, runtime e trabalho multi-repo | inspecionar histórico git, drift, arestas ocultas de co-mudança, overlays de runtime e referências entre repositórios | `timeline`, `diverge`, `ghost_edges`, `runtime_overlay`, `external_references`, `federate`, `federate_auto` |
| Operações e monitoramento | auditar estado do repo, verificar verdade grafo-vs-disco, rodar watches de daemon, persistir estado e surfaçar alertas duráveis | `audit`, `cross_verify`, `daemon_*`, `alerts_*`, `panoramic`, `metrics`, `report`, `persist`, `diagram`, `help` |
| Preparação e execução de edição cirúrgica | extrair contexto conectado compacto, pré-visualizar escritas e aplicar edições conscientes do grafo | `surgical_context`, `surgical_context_v2`, `view`, `batch_view`, `edit_preview`, `edit_commit`, `apply`, `apply_batch` |

**Camadas:** 27 ferramentas essenciais são anunciadas por padrão para reduzir o custo de seleção de ferramentas; defina `M1ND_TOOL_TIER=full` para anunciar a superfície completa (100+ ferramentas: RETROBUILDER, perspectives, federação, daemon). Algumas ferramentas (`resonate`, `savings`, `lock_*`) permanecem chamáveis pelo nome mas não estão na superfície anunciada. Ferramentas ocultas são sempre chamáveis via `tools/call` — o tiering só controla o que `tools/list` surfaça.

## Os Loops Operacionais

O pacote de agente é parte do produto, não documentação decorativa. O m1nd é mais poderoso quando o agente recebe o *loop operacional*, não apenas um endpoint de grafo. Cinco protocolos nomeados são entregues no pacote:

- **Início de Sessão** — `trust_selftest` → `recovery_playbook` se a confiança não for total → `ingest` se necessário → `seek`/`audit`.
- **Pesquisa** — `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → `learn(feedback)` → `memorize` qualquer descoberta durável.
- **Mudança de Código** — `impact(node)` para blast radius → `predict(node)` → `counterfactual(nodes)` → `surgical_context_v2` → `memorize` a decisão e o porquê.
- **Análise Profunda** — `fingerprint`, `diverge`, `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`, `runtime_overlay` (a lente RETROBUILDER) para acoplamento oculto, caminhos de segurança, duplicatas estruturais e calor de runtime.
- **Memória** — persista conclusões duráveis com `memorize`, carregando `confidence` e caminhos de `evidence`.

Mission Control é disciplina de prova, não uma lista de funcionalidades. `mission_next` retorna exatamente um movimento mais guardrails `do_not`; `mission_verify` rejeita afirmações apenas de grafo; `mission_close` sempre incita o agente a persistir conhecimento verificado e registra lacunas e non-claims. No modo `bug_hunt`, o MC0 exige um `direct_sweep` final direto após as descobertas verificadas antes do encerramento, para que os agentes verifiquem o espaço negativo.

**Ressalva:** `predict` tem **fallback somente estrutural** até que `ghost_edges` carregue a matriz de co-mudança do git — rode `ghost_edges` primeiro quando precisar de probabilidade real de co-mudança.

## Evidências

Cada linha é calibrada exatamente ao que foi medido. O m1nd não lidera com números de economia ou ROI — esse é o ponto.

| Afirmação | Resultado | Fonte / ressalva |
|---|---|---|
| Latência de `activate` / `impact` | ~1µs `activate`, sub-µs `impact` em um grafo sintético de 1K nós | Benchmarks Criterion — **reproduza você mesmo: `cargo bench -p m1nd-core`** (medido `activate_1k_nodes` ≈1.4µs, `impact_depth3` ≈0.5µs em um Mac com Apple silicon); [metodologia](https://m1nd.world/wiki/benchmarks.html); ordem de grandeza, dependente de hardware. |
| Matriz de linguagens | calls + imports entre arquivos para 10 linguagens (+ Ruby entre arquivos) | Verificado de ponta a ponta em uma única ingestão poliglota; testes por linguagem em `m1nd-ingest`. Veja [Cobertura de Linguagens](#cobertura-de-linguagens). |
| Amostra de validação pós-escrita | 12/12 classificados corretamente | Verificação de runtime interna. |
| Caça a bugs com seeds | 16/20 na primeira rodada aceita de defeitos com seed `humanize` (treinado com m1nd); `m1nd-basic` e direto cada um 8/15 | Evidência interna de produto, `public_claim_worthy=false` — não é um benchmark universal. |
| Auto-verificação de memória | provado ao vivo de ponta a ponta | `memorize` → `grounded_in` → flag de frescor em arquivo editado → sobrevive a replace → carga automática no boot. |
| Bateria de capacidades vs grep | 37/37 passam; frente a frente 16 vitórias do m1nd / 12 empates / **0 vitórias do grep** | Harness in-repo `scratchpad/m1nd_battery.py` (37 casos, ingestão nova + PASS/FAIL de ground-truth + confronto direto com `rg`). **Reproduza: `python3 scratchpad/m1nd_battery.py ./target/release/m1nd-mcp . --suite m1nd`.** Ressalva: um repo (o próprio m1nd), casos escritos por nós mesmos; ~5 dos empates são ferramentas estruturais pontuadas contra um proxy de grep literal que não consegue expressar o que elas respondem. |
| Calibração conformal (`predict`) | banda act ≈32% de precisão @ ≈13.5% de cobertura (α=0.10) | Sobre o próprio histórico git do m1nd (n≈9.2k predições held-out), +3pts sobre contagens brutas após a mudança para Jaccard suavizado. Ressalva: um repo, um sinal grosseiro baseado em contagem — o gate hoje majoritariamente se abstém, **por design**: a abstenção é a saída honesta de um sinal fraco, não uma falha. |

## Limites

`m1nd` complementa, em vez de substituir, seu LSP, compilador, test runner, scanners de segurança e stack de observabilidade. É mais útil antes de busca, revisão ou mudança, e sempre que docs, impacto ou continuidade importam.

É **menos útil** quando:

- busca exata de texto já responde à pergunta
- verdade do compilador ou runtime é a única coisa que você precisa
- a tarefa é uma ação local trivial em arquivo sem incerteza estrutural

**Precisa ser alimentado:** `trust` e `tremor` começam com priors neutros até que o feedback de `learn` / os dados de `ghost_edges` se acumulem, e `predict` precisa que `ghost_edges` esteja carregado primeiro para que seu sinal de co-mudança seja significativo. Eles melhoram com o uso; são honestos sobre estarem sem informação no boot.

## Arquitetura em Resumo

Três crates core em Rust mais uma bridge auxiliar:

- **`m1nd-mcp`** — o servidor MCP e a superfície de runtime operacional.
- **`m1nd-core`** — o motor de grafo: um `WavefrontEngine` fazendo spreading activation, plasticidade Hebbiana, adjacência CSR e arestas ghost derivadas do git.
- **`m1nd-ingest`** — extração, roteamento e adapters de construção de grafo (código, docs universais, L1GHT).
- **`m1nd-openclaw`** — bridge auxiliar OpenClaw (lane de Unix socket, versionado independentemente).

Versões atuais dos crates: `m1nd-core`, `m1nd-ingest`, `m1nd-mcp` todos em `1.2.0` (`m1nd-openclaw` é versionado independentemente em `0.1.0`).

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="Visão geral da arquitetura do m1nd" width="960" />
</p>

Para federação, perspectives, RETROBUILDER, coordenação multiagente e a referência completa de pacote de agente e operador, consulte a [wiki canônica](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) e [EXAMPLES.md](../EXAMPLES.md).

## Contribuindo

Contribuições são bem-vindas em extractors e adapters, tooling MCP/runtime, benchmarks, documentação e algoritmos de grafo. Veja [CONTRIBUTING.md](../CONTRIBUTING.md).

## Licença

MIT. Veja [LICENSE](../LICENSE).
