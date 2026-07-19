# M1nd 10 — PRD de convergência do organismo

> **Status:** RATIFIED — baseline vinculante para implementação
> **Versão do documento:** 1.0
> **Data do snapshot:** 2026-07-17
> **Ratificação humana:** `APPROVE`, recebida do root governor em 2026-07-18
> **Modo ratificado:** bootstrap `HUMAN_GATED`; target `FULL_AUTONOMY` somente após `G9` e `AutonomyActivationReceiptV1`
> **Escopo:** plataforma agentiva local-first M1nd e suas integrações operacionais
> **Fora de escopo:** mecânicas, conteúdo, renderização e pipelines específicos de jogos
> **Documento irmão:** [M1ND-10-UML.md](./M1ND-10-UML.md)
> **Source adotado do código:** `b59a1c2a1454a83164dfb4d5640c6b005154d1ee` (`v1.4.0-321-gb59a1c2`)
> **Estado desta draft:** `DIRTY` — estes dois documentos ainda estão untracked e não pertencem ao source adotado
> **Snapshot operacional:** 2026-07-17T21:35:38Z; `m1nd-mcp 1.4.0 (f1c025b)`, graph generation 17, 10.606 nós, 37.110 arestas
> **Ratificação askGOD de fundação:** `NOT_PROVEN` — Fable recusou a execução por saldo; duas tentativas Fugu inspecionaram o ground, mas não devolveram um veredito admissível e foram interrompidas. Este PRD não herda aprovação do oracle.

---

## 1. Decisão executiva

O M1nd não precisa de um novo domínio de produto antes de chegar a 10/10. O núcleo de graph intelligence, ingestão, memória, arquitetura ratificada, missões, runners, presença e Human View já existe. Ele precisa, sim, de primitives de control plane que ainda não existem — identidade, policy, transação, checkpoint e release evidence. O salto de qualidade depende de transformar as peças atuais em **um organismo coerente**, com uma única espinha de:

1. identidade causal;
2. autoridade explícita;
3. evidência verificável;
4. estado transacional;
5. disponibilidade e durabilidade;
6. verdade de build e distribuição.

A estratégia deste PRD é, portanto, **conectar e endurecer antes de expandir**. Novas funcionalidades só entram quando fecham um requisito mensurável que as peças atuais não conseguem satisfazer.

O alvo não é uma média 10. O release é 10/10 apenas quando **todos os dez requisitos** deste documento estiverem em `PASS`, sem um P0 aberto, sem prova emprestada de outro ambiente e com a ratificação final exigida pelo modo ativo: humana em `HUMAN_GATED`/`POLICY_AUTONOMOUS`, constitutional quorum em `FULL_AUTONOMY`.

---

## 2. Visão do produto

O M1nd é o sistema nervoso de um repositório: observa sua estrutura real, forma e atualiza conhecimento, localiza contexto para agentes, representa arquitetura ratificada, coordena trabalho sob prova e oferece ao humano uma visão operacional honesta do organismo.

Em sua forma 10/10, qualquer repositório suportado deve conseguir percorrer este ciclo:

```text
instalar → reconhecer o repo → ingerir → orientar → propor arquitetura
→ ratificar → abrir missão → executar isoladamente → provar
→ obter a AuthorityDecision do modo → transacionar receipt + estado
→ refletir a nova verdade em todas as superfícies
```

Esse ciclo deve sobreviver a concorrência, timeout, crash, restart, múltiplos clientes, múltiplos repos e versões diferentes do produto sem produzir duas verdades incompatíveis.

### 2.1 Usuários primários

- **Root governor humano:** estabelece/ratifica a constituição nos modos human-gated e policy-autonomous, pode operar break-glass; não fica no loop cotidiano.
- **Agente investigador:** localiza, orienta, consulta, compara e abstém quando o ground é insuficiente.
- **Agente executor:** recebe missão e capability limitadas, altera um escopo isolado e produz evidência.
- **Agente reviewer/oracle:** julga proposta ou diff em modo read-only e emite veredito auditável.
- **Constitutional agent council:** em modos autônomos, produz decisões independentes de propose/review/sentinel sob quorum explícito.
- **Operador da plataforma:** instala, atualiza, observa saúde, recupera, reverte e diagnostica hosts.

### 2.2 Princípios invioláveis

1. **Authority never impersonates origin.** Nenhum agente pode alegar origem humana. Ratificação é `HUMAN`, `POLICY` ou `AGENT_QUORUM` conforme o modo ativo e sempre declara qual autoridade decidiu.
2. **State is not evidence.** Uma carta diz o estado operacional; um receipt prova uma condição; nenhuma substitui a outra.
3. **Observed is not ratified.** O graph observa estrutura; SystemBlocks representam arquitetura ratificada.
4. **Delivery is not execution.** Entregar um packet não prova que ele foi executado.
5. **Green is not landed.** Gate verde sem receipt validado permanece `merge_wait`.
6. **Timeout is not cancellation.** Retornar ao cliente não pode deixar trabalho invisível segurando o organismo.
7. **Same machine is not trusted identity.** Strings como `human-ui` ou `human-touchid` não são prova de presença humana.
8. **Every engine may say NONE.** Abstention é um resultado válido e medido.
9. **One fact, one authority.** Projeções podem cachear; não podem se tornar um segundo dono da verdade.
10. **Proof travels with scope.** Evidência é vinculada à identidade, versão, boundary, digest e tempo em que foi obtida.

---

## 3. Linguagem de verdade

Todo PR, dashboard, release note, documento e resposta de agente deve usar estes estados:

| Estado | Significado |
|---|---|
| `EXISTS` | Código ou artefato foi localizado. |
| `CONNECTED` | Produtor, contrato e consumidor operam juntos no caminho nominal. |
| `IMPLEMENTED` | O comportamento existe no source adotado. |
| `MECHANICALLY_PROVEN` | Um gate reproduzível passou no escopo declarado. |
| `LIVE` | O comportamento foi observado no runtime indicado. |
| `HUMAN_RATIFIED` | O owner aceitou explicitamente o contrato ou resultado. |
| `POLICY_RATIFIED` | Uma action policy pré-ratificada autorizou a decisão dentro do envelope vigente. |
| `QUORUM_RATIFIED` | Um quorum agentivo independente satisfez a constituição e emitiu receipt. |
| `NOT_IMPLEMENTED` | O comportamento/contrato ainda não existe no source adotado. |
| `NOT_LIVE` | O comportamento não foi observado no runtime declarado. |
| `NOT_RATIFIED` | Nenhuma authority válida para o modo ativou o contrato/resultado. |
| `NOT_PROVEN` | Existe alegação ou implementação, mas falta a prova correspondente. |
| `NOT_RUN` | O gate não foi executado. |
| `FAIL` | O gate executou e falhou. |

Existência, implementação, prova mecânica, runtime e ratificação são dimensões independentes. Nenhuma implica automaticamente a outra.

---

## 4. Ground atual

### 4.1 `GroundSnapshotReceipt`

Snapshot observado em `2026-07-17T21:35:38Z`:

- `HEAD == origin/main == b59a1c2a1454a83164dfb4d5640c6b005154d1ee` para o source adotado;
- working tree `DIRTY` apenas pelos dois drafts untracked deste pacote;
- owner `status=ok`, generation 17, 10.606 nós, 37.110 arestas e 133/133 tools anunciadas;
- binário em execução `1.4.0 (f1c025b)` em `/Users/kle1nz/.m1nd/bin/m1nd-mcp`;
- `m1nd-system-block-store-v0`, store version 61, 12/12 blocos com `state=ratified` e 48 receipts dentro de `blocks[].receipts`;
- CI run [29606661859](https://github.com/maxkle1nz/m1nd/actions/runs/29606661859), workflow `CI`, concluiu `success` em 2026-07-17T19:47:56Z para esse HEAD.

Comandos de reprodução:

```bash
git status --short
git rev-parse HEAD
git rev-parse origin/main
curl -fsS http://127.0.0.1:1338/api/health
jq '{store_version,blocks:(.blocks|length),ratified_blocks:([.blocks[]|select(.state=="ratified")]|length),receipts:([.blocks[].receipts[]?]|length)}' \
  /Users/kle1nz/.m1nd/runtimes/claude/system_blocks.json
gh run view 29606661859 --json headSha,status,conclusion,createdAt,updatedAt,url
```

Os números LIVE são verdade de snapshot, não constantes. Depois de construído, `OrganismManifestV1` será a projeção canônica para lê-los com freshness por autoridade.

- A CI do HEAD passou Rust check, test, clippy e format em Ubuntu, macOS e Windows; isso não prova UI, packages ou o golden path.
- O h4nd está em outro repositório, com runtime e componentes reais ativos, mas com source local dirty e sem remote configurado.
- O reviewer tem 1 run e 15 vereditos; o gate publicado de pelo menos 3 runs e 30 vereditos ainda não foi satisfeito.

### 4.2 Inventário de sistemas

Legenda de ação:

- `REUSE`: preservar o núcleo e seu contrato.
- `CONNECT`: unir autoridades existentes por um contrato explícito.
- `HARDEN`: corrigir segurança, disponibilidade, integridade ou semântica.
- `BUILD`: criar a peça ausente.
- `RETIRE`: remover uma duplicidade de autoridade após migração.
- `PROVE`: adicionar prova reproduzível antes de alegar conclusão.

| Sistema | Estado atual | Autoridade correta | Ação para 10/10 |
|---|---|---|---|
| Graph Intelligence | Maduro e LIVE | Estrutura observada e sinais derivados | `REUSE`, `PROVE` qualidade, escala e abstention |
| Ingestão universal | LIVE para código e documentos | Conversão de fontes em graph | `REUSE`, `HARDEN` degradação de providers, `PROVE` matriz de formatos |
| Served owner MCP/REST/attach | LIVE | Porta operacional do brain | `REUSE`, `HARDEN` concorrência, cancelamento, auth e backpressure |
| ProjectBrainRegistry | LIVE, arquitetura intermediária | Roteamento de brains no owner atual | `REUSE` agora; decidir runtime por repo em ADR; não `RETIRE` antes de parity e rollback |
| L1GHT memorize/promote | LIVE | Conhecimento autorado, proveniência e supersession | `REUSE`, `CONNECT` identidade causal |
| Boot KV | LIVE, sobreposição semântica | Configuração efêmera de boot, não memória geral | `HARDEN` para config/boot; `RETIRE` usos semânticos após migração `MECHANICALLY_PROVEN` |
| SystemBlockStore | LIVE e hoje human-ratified | Arquitetura ratificada pela authority do modo e receipts | `REUSE`, `CONNECT` graph/X-RAY/proof, `HARDEN` soberania |
| Mission Letters/mailbox | LIVE, parcial | Estado operacional append-only | `HARDEN` state machine, autoria e transação de landing |
| Mission Control | LIVE | Trilha de raciocínio e handoff | `REUSE`, `CONNECT` por IDs; nunca torná-lo segundo estado operacional |
| Delegation/debrief | LIVE | Packet, outcome e calibração de delegação | `CONNECT` mission/block/receipt; `PROVE` calibração |
| runnerd | LIVE, parcial | Único spawner pinado e executor isolado | `REUSE`, `HARDEN` capability e fail-closed, `PROVE` golden path |
| Presence/instances/gardener | LIVE, parcial | Atividade recente, leases, inventário e alerts | `CONNECT` control plane; `PROVE` restart/churn/collision |
| X-RAY/cirurgical writes | LIVE, parcial | Projeção estrutural e aplicação física controlada | `REUSE`, `CONNECT`; incluir todo write no proof policy |
| Human View | LIVE e amplo | Projeção e gesto humano | `REUSE`, `CONNECT` manifest/authority; `HARDEN` identidade humana |
| h4nd | Parcialmente LIVE | Cockpit externo e device-owner authentication | `CONNECT` ao owner por approval/capability; corrigir landing; adotar build limpo |
| h4nd pool/god-runner | LIVE, mas política e cold lane parciais | Execução externa e run artifacts | `HARDEN` auth/policy; decidir drain humano vs autônomo; `PROVE` spawn real |
| Host adapters/skills/npm | Compostos, prova incompleta | Instalação e acesso por cliente | `REUSE`, `PROVE` paridade por host e upgrade |
| Reviewer | Piloto real, abaixo do gate | Julgamento read-only calibrado | `PROVE`, depois promover por estágio |
| SafetyKernel/constitution/grants/quorum/sentinel/actuator | Ausente | Autonomy mode, scoped authority decisions, epoch fencing e safety | `BUILD`, `PROVE` |
| CI/release | Rust CI verde; produto completo não coberto | Promoção de artefatos já testados | `BUILD` matriz completa, supply chain e rollback |
| Document Truth/Soul/auto-ingest | LIVE, parcial | Cache, binding, drift e freshness; nunca code authority | `REUSE`, `CONNECT`, `PROVE` |
| Field reports/project mailbox | LIVE | Telemetry local e fate derivado; não mission state/evidence | `REUSE`, `CONNECT`, `PROVE` |
| MCP transport sessions | LIVE | Protocolo, caller-root e sticky binding | `REUSE`, `HARDEN` identity e recovery |
| Perspectives/trails/coordination locks | LIVE | Navegação e continuidade agentiva | `REUSE`, `HARDEN`, `PROVE` |
| Runtime Job Registry | Ausente | Job id, deadline, cancel e terminal state | `BUILD` |
| Schema Migration Registry | Ausente | Compatibilidade e migração de stores/schemas | `BUILD`, `PROVE` |

### 4.3 P0 e P1 que impedem nota 10

#### P0 — integridade e soberania

1. `landed` valida um anchor declarativo, mas não prova que o receipt correspondente existe no store.
2. A state machine permite quase qualquer transição; somente `archived` possui regra de origem.
3. O CAS da cadeia protege head e sequência, mas não a autoridade de quem publica o próximo elo.
4. `force:true` em seed/delete pode substituir ou eliminar verdade soberana sem uma capability humana real.
5. O h4nd envia `receipt_candidate` cru para `receipt_import`, embora o owner exija um `Receipt` completo; depois também não publica o elo `landed`.
6. Duas missões `merge_wait` LIVE são hoje não-landáveis: uma está stale por boundary e outra é sintética sem bloco no store.
7. Runner/child/client ainda podem ser desenhados como autores diretos de Mission Letters; o target correto exige um `MissionService` owner-side como único appender.

#### P0 — disponibilidade e durabilidade

8. REST e MCP mantêm o mutex de `SessionState` durante dispatch longo; timeout não cancela o `spawn_blocking` e `/health` compete pelo mesmo lock.
9. Uma project brain dirty pode ser retirada do registry antes de um persist que falha; o erro é logado quando o estado já pode ser descartado.

#### P1 — segurança e prova

10. `human-ui`/`human-touchid` são labels forjáveis por outro processo do mesmo usuário.
11. `--allow-remote` expõe leitura e mutação sem autenticação; o cockpit h4nd também escuta fora de loopback e proxya `mission_post` sem auth/CSRF.
12. Perspective Peek com allow-list vazia permite tudo; o default correto para nota 10 é deny. Alterar ingest roots também amplia esse boundary e hoje não é uma ação administrativa soberana.
13. Proof gating é opt-in, cobre poucos nomes de tools e não invalida marks por digest/geração/TTL de forma completa; `xray_apply` escapa.
14. Persistência grava graph e sidecars sem um checkpoint coerente e recuperável.
15. Migrações são multi-write e não transacionais; faltam plan digest, fencing, journal, conservation proof e recovery por failpoint.
16. `CoChangeMatrix` não persiste; campos chamados `sha256` usam um hash de 64 bits.
17. A cold lane do pool está automaticamente armada, mas o runner simbólico configurado não corresponde aos runners LIVE; política humana versus autônoma não foi ratificada.
18. UI, Python, npm/attach, instalação, golden path, recuperação e packages não são required checks da CI principal.
19. O pipeline não possui uma identidade imutável de release candidate nem gate receipts; seria possível testar um artefato e promover outro.
20. Reviewer é piloto manual sem scheduler e sem conexão ao JURIS/Mission Control: 1 run, 15 itens, 0 approve, 5 bounce, 3 stale e 7 insufficient evidence. O gate atual seria vacuamente “zero reverted approves”.
21. O h4nd LIVE carrega source dirty via Vite/dev e mocka todas as APIs no Playwright; shell instalado idêntico não prova bundle, owner/pool integration nem landing.

---

## 5. Os dez requisitos de 10/10

Os thresholds abaixo são **PROPOSTOS** e precisam de ratificação humana antes de se tornarem gates de release. Depois de ratificados, só mudam por amendment versionado.

### R1 — Verdade coerente

O source adotado, binário, graph, SystemBlocks, runtime, UI, h4nd e package managers devem declarar a mesma identidade do organismo.

**10/10 quando:**

- 100% das superfícies de status consomem `OrganismManifestV1` ou uma projeção derivada dele;
- source commit, versão, binary digest, UI bundle digest, graph generation e store version são verificáveis;
- cada subfato declara authority, revision, digest, `observed_at`, freshness e status; indisponibilidade vira `UNKNOWN`/`DEGRADED`;
- qualquer divergência aparece como `DRIFT`, nunca como “healthy” genérico;
- PATHOS e documentos operacionais contêm claims precificados por prova ou marcados `NOT_PROVEN`;
- não existe estado arquitetural concorrente a SystemBlocks.

### R2 — Inteligência estrutural e recuperação

O M1nd deve localizar contexto útil melhor que uma busca textual ingênua e deve saber quando não possui ground suficiente.

**10/10 quando:**

- benchmark held-out versionado cobre no mínimo 200 tarefas, múltiplos tamanhos e linguagens;
- top-5 contém o anchor correto em pelo menos 90% das tarefas localizáveis;
- recall de abstention em tarefas não-localizáveis é pelo menos 95%;
- taxa de orientação que autoriza ação com ground errado é no máximo 1%;
- nenhuma regressão estatisticamente relevante contra o baseline ratificado;
- p95 de `north` e `seek` permanece dentro do SLO ratificado no corpus de referência.

### R3 — Ingestão e conhecimento universal

Código, Markdown, PDF, Office, XML, JSON e fontes L1GHT devem ingressar pelo mesmo contrato de proveniência, sem sucesso silencioso quando um provider não processou conteúdo.

**10/10 quando:**

- matriz de formatos possui fixtures positivas, negativas, corrompidas e encrypted/unsupported;
- cada arquivo recebe estado `INGESTED`, `DEGRADED`, `UNSUPPORTED` ou `FAILED`, com provider e motivo;
- `Ok(None)` nunca vira contagem zero silenciosa;
- supersession, origem, scope e evidence de memorizações sobrevivem a restart;
- Boot KV não é usado como memória semântica geral;
- ingest incremental e full rebuild produzem equivalência dentro do contrato definido.

### R4 — Missões, identidade e autoridade

Cada mudança de estado deve provar quem a iniciou, qual capability possuía, que head estendeu e qual escopo estava autorizado.

**10/10 quando:**

- a state machine completa é server-owned e todos os estados terminais são realmente terminais;
- somente o `MissionService` owner-side pode persistir uma Mission Letter; runner e reviewer devolvem resultados assinados;
- somente a capability esperada estende a cadeia;
- dois `seq+1` concorrentes resultam em um vencedor e um `stale_head`, sem fork invisível;
- replay, autor errado, brain errado, payload alterado e capability expirada falham;
- Mission Letters, Mission Control e delegation são correlacionados, mas mantêm autoridades separadas;
- todas as fixtures negativas de G3 passam.

### R5 — Evidência, proof e landing

Um estado `landed` deve ser uma consequência verificável de evidência importada, não uma alegação bem formada.

**10/10 quando:**

- a facade de landing do `MissionService` valida candidate, receipt, scope, boundary, contract, resolution digest, mission head e store version antes de invocar `LandTransactionV1` internamente;
- receipt import + append de `landed` usam `AuthorityWALV1` com records invisíveis antes do commit;
- crash em qualquer etapa converge para antes ou depois, nunca para metade;
- nonce, head CAS, store CAS e idempotency result pertencem à mesma decisão transacional;
- toda ação mutante, em qualquer ingress, declara seu conjunto de efeitos e passa o middleware de policy;
- proof marks vinculam agent, target, graph generation, disk digest e TTL;
- alteração do target, re-ingest ou generation bump invalida proof;
- h4nd completa uma landing real contra o owner e a prova é repetível.

### R6 — Disponibilidade, isolamento e durabilidade

Um trabalho pesado em um brain não pode derrubar health, outro brain ou a capacidade de recuperar estado.

**10/10 quando:**

- `/health` p99 abaixo de 100 ms durante uma operação de 30 s;
- nenhum lock de sessão atravessa filesystem, rede, subprocesso ou análise longa;
- cada brain possui actor/queue e backpressure explícito;
- timeout cancela cooperativamente ou isola o trabalho até conclusão observável;
- persist failure mantém brain viva e marca `degraded_persistence`;
- fault injection em toda fase de checkpoint prova recovery old-or-new, nunca misto;
- 10.000 operações concorrentes de teste não perdem writes nem cruzam brains.

Os números são avaliados por `MetricSpecV1`, incluindo hardware, workload, duração, seeds, revisions e intervalo de confiança; “10.000” significa total de operações no workload versionado, com a concorrência declarada pelo spec.

### R7 — Segurança e privacidade local-first

Loopback reduz exposição, mas não é identidade. O threat model de nota 10 considera outro processo do mesmo usuário potencialmente hostil.

**10/10 quando:**

- toda mutação ordinária autentica uma client/session identity matriculada; ações soberanas positivas acrescentam `AuthorityDecisionV1` e Human/Autonomy capability criptográfica one-shot válidas para o modo, authority variant, risk e scope ativos; o único variant sem decisão positiva é `SAFETY_KERNEL`, autenticado pela identity pinada do actuator e autorizado por RED + SafetyActionIntent + SafetyCapability negative-only; variants autônomos positivos também vinculam grant e tier;
- MCP, REST, CLI, hooks, background jobs, recovery e migrations passam pelo mesmo middleware de action policy;
- remote bind permanece proibido sem TLS, autenticação e autorização scoped;
- Perspective Peek deriva allow-list dos ingest roots e vazio significa deny;
- traversal, symlink escape, token replay, wrong-origin, wrong-brain e CSRF são fixtures required;
- secrets e paths host-local não entram em packets públicos, letters ou telemetry;
- delete/overwrite usam tombstone/backup e challenge explícito;
- threat model e resposta a incidentes estão versionados.

### R8 — Produto humano e h4nd

O humano deve enxergar a verdade operacional e executar gestos soberanos curtos sem que a interface componha uma segunda lógica de domínio.

**10/10 quando:**

- Universe, Hall, Tree, Build Map, Tray e h4nd mostram o mesmo manifest e authority state;
- nenhuma superfície de produção depende de fixture, mock ou source dirty/dev;
- em `HUMAN_GATED`, landing exige no máximo dois gestos conscientes e mostra scope/digest; em modos autônomos, não exige gesto e mostra authority/quorum receipt;
- quando há gesto humano, h4nd autentica intenção; em todo modo o owner compõe e transaciona o receipt;
- `WRITE`, health, pool, reviewer e run state são derivados, nunca constantes cosméticas;
- build instalado é byte-identificável com o artefato promovido;
- runtime prova que o shell instalado carregou exatamente o promoted bundle digest; mismatch recusa ou mostra `DRIFT`;
- acessibilidade, falha, stale scope e recovery possuem testes de UI e prova browser LIVE.

### R9 — Produto agentivo, hosts e distribuição

Codex, Claude e outros hosts suportados devem receber a mesma lei, contratos essenciais e comportamento de attach, sem copiar o cérebro para cada cliente.

**10/10 quando:**

- um catálogo canônico gera ou valida todas as superfícies de tools/skills;
- first-minute success é pelo menos 95% por host Tier A;
- mismatch de caller root nunca autoriza write e oferece recovery determinístico;
- install, attach, update e rollback passam em macOS, Linux e Windows para o core suportado;
- diferenças de host são declaradas em capability matrix;
- packages npm/crates/binários compartilham a mesma versão e manifest;
- não há tool mutante disponível sem policy, auth e prova correspondentes.

### R10 — Autonomia calibrada, operação e release

Autonomia é promoção auditada, não uma flag. Releases promovem artefatos já provados e podem ser revertidos.

**10/10 quando:**

- manifest separa modos suportados/provados do `active_mode`, grants/tiers scoped e activation receipt; nenhuma surface confunde capability mecânica com autoridade ativada;
- `POLICY_AUTONOMOUS` opera sem ratificação humana por ação dentro da ConstitutionStore ratificada;
- `FULL_AUTONOMY`, quando habilitado, governa arquitetura, landing, release e amendments abaixo do safety kernel sem humano no caminho obrigatório;
- proposer, executor, verifier e sentinel são independentes; nenhum agente ratifica o próprio trabalho ou promove seu próprio tier;
- reviewer completa o gate exploratório atual de pelo menos 3 runs e 30 vereditos, zero approve revertido, antes de S2; isso não autoriza autonomia por si só;
- qualquer approve autônomo exige `MetricSpecV1` com amostra mínima, decisões `APPROVE` reais e casos high-risk não vazios, shadow period e limite estatístico ratificado para false/reverted approve;
- cada promoção de autonomia tem dataset, precisão, cobertura, guardrails e rollback;
- required CI cobre Rust, UI, browser, Python, npm/attach, recovery, security, golden path e package smoke;
- release produz checksums, SBOM, assinatura e provenance;
- install/update/rollback são ensaiados a partir dos artefatos publicados;
- `ReleaseCandidateManifestV1` identifica imutavelmente todos os artefatos e cada gate emite `GateReceiptV1` ligado ao mesmo digest;
- um release candidate atravessa G0–G10 sem waiver P0/P1;
- a authority prevista pelo modo ratifica o relatório final: humano em `HUMAN_GATED`/`POLICY_AUTONOMOUS`, constitutional quorum em `FULL_AUTONOMY`; a origem fica explícita.

---

## 6. Arquitetura-alvo

### 6.1 Espinha de verdade: `OrganismManifestV1`

`OrganismManifestV1` é a leitura canônica de identidade e coerência. Ele não substitui os stores; referencia suas versões e digests.

```text
schema
organism_id
repo_id
brain_id
project_root_fingerprint
source: { commit, dirty, version }
runtime: { owner_id, binary_version, binary_sha256, started_at }
graph: { generation, snapshot_sha256, node_count, edge_count }
architecture: { store_version, skeleton_digest, ratification_state }
ui: { bundle_version, bundle_sha256, mode }
capabilities: { policy_version, enabled_effects }
autonomy: {
  supported_modes, mechanically_proven_modes, active_mode,
  activation_receipt_id, constitution_digest, constitution_epoch,
  safety_kernel_digest, autonomy_epoch, grants_digest,
  quorum_policy_digest, max_effective_tier_projection,
  issuance_frozen, sentinel_safety_state
}
schemas: { mission, receipt, checkpoint, light, system_blocks }
authorities: {
  authority_id: { revision, digest, observed_at, freshness, status }
}
release_provenance: { release_candidate_digest, signature }
generated_at
manifest_sha256
```

**Leis:**

- O manifest é montado a partir das autoridades; não é editado à mão.
- `dirty`, digest ausente ou versão divergente produz estado explícito de drift.
- Cada subfato informa autoridade, revision, digest, idade e freshness; store indisponível vira `UNKNOWN` ou `DEGRADED`, nunca um valor velho com timestamp novo.
- `manifest_sha256` é SHA-256 sobre serialização canônica com esse próprio campo omitido; a provenance de release assina o digest resultante.
- Um consumidor não pode sobrescrever fatos do manifest.
- O manifest é projeção de coerência, não autoridade de release nem de domínio.

### 6.2 Espinha causal: `CausalEnvelopeV1`

Todo evento que cruza um subsistema carrega um envelope correlacionável:

```text
schema
event_id
organism_id
brain_id
actor_id
actor_kind
issuer
key_id?
algorithm?
capability_id?
mission_id?
mission_head_id?
delegation_id?
block_id?
receipt_id?
presence_id?
graph_generation
store_version?
target_digest?
causation_id?
correlation_id
issued_at
expires_at?
payload_digest
signature?
```

O schema wire define serialização canônica, algoritmo permitido, trust domain, key lifecycle, replay domain e clock skew. Campos opcionais são exigidos por uma matriz de event class; evento que requer autorização e chega sem capability/signature é inválido. Eventos internos podem obter integridade do journal/checkpoint em vez de fingir uma assinatura humana.

O envelope conecta gramáticas; não as funde. O graph continua observação, SystemBlocks continuam arquitetura, receipts continuam prova e letters continuam estado.

### 6.3 Matriz de autoridades

| Fato | Único dono | Projeções/consumidores |
|---|---|---|
| Bytes e VCS atuais | Filesystem + VCS | ingest, audit, source viewer |
| Estrutura observada na generation N | Graph snapshot + engines | north, seek, X-RAY, Human View |
| Arquitetura ratificada | SystemBlockStore | Build Map, packets, reviewer |
| Evidência validada | `SystemBlockStore.blocks[].receipts` (`ReceiptV1`) | block rollup, landing, release gates |
| Artefato bruto | Bytes/URI endereçados por SHA-256 | `ReceiptV1`, runs, audit |
| Estado operacional de missão | Mission Letter chain | Tray, h4nd, cockpit |
| Trilha de raciocínio | Mission Control | auditoria, handoff, debrief |
| Conhecimento autorado | L1GHT | graph, orientação, promoção |
| Execução e gate | runnerd/run artifact | receipt candidate, runs view |
| Atividade recente visível | Presence sidecars | Universe, gardener |
| Lease/ownership do runtime | InstanceRegistry | owner discovery, recovery |
| Sessão e binding MCP | McpSessionRegistry | transport, caller-root routing |
| Frescor de runner | RunnerdRegistry | spawn eligibility |
| Intenção humana | Human approval chain | landing, ratify, archive, delete |
| Constituição e modos permitidos | ConstitutionStoreV1 | policy engine, quorum, manifest |
| Modo ativo, activation receipt, grants e safety fence | AutonomyEpochV1 sob root anti-rollback | policy engine, AuthorityTransaction, recovery, manifest |
| Decisão autônoma | PolicyDecision ou AgentQuorumDecision + AuthorityJournal/receipts | sovereign transactions e promotion/demotion |
| Identidade de release | Provenance assinada do artefato adotado | manifest, packages, UI |
| Projeção de coerência | OrganismManifestV1 | todas as superfícies; nunca autoridade |

### 6.4 `ClientIdentityV1` e `ActionPolicyRegistryV1`

Outro processo do mesmo UID está dentro do threat model para impersonation de API, replay e uso de credenciais não protegidas. Toda mutação ordinária precisa de uma sessão autenticada; ações soberanas positivas acrescentam `AuthorityDecisionV1` e Human/Autonomy capability válidas para o modo ativo. O safety variant autentica a identity pinada do actuator e sua SafetyCapability, nunca uma sessão/agente rotulado como autoridade positiva.

`ClientIdentityV1` registra `subject_id`, `key_id`, public key, app/host identity, scopes, enrollment evidence, created/revoked timestamps e status. O handshake assina nonce e session context. UDS peer credentials, code-signing/audit token e keystore são sinais adicionais; nenhum deles sozinho vira autorização universal. TCP loopback também exige identidade. Remote exige TLS/mTLS ou mecanismo ratificado equivalente.

`SameUidThreatModelV1` também cobre bypass pelo filesystem. `OwnerIdentityV1` usa trust anchor pinado e chave não exportável para assinar WAL entries, checkpoint roots e store epochs. O sentinel faz o mesmo para seu append-only outbox: non-exportable key, signed root, monotonic `outbox_epoch`, previous-root chain e latest accepted root/epoch em storage protegido; o owner guarda o último watermark aceito no protected journal. Se uma plataforma não oferecer anti-rollback forte, G2 exige owner e sentinel/actuator sob UIDs/sandboxes separados; sem uma dessas estratégias, same-UID fica `NOT_PROVEN`. Edição de bytes é tampering detectado; deleção total é DoS detectável e recuperável por backup assinado, não uma disponibilidade magicamente evitada.

Cada `ingress + action + mode + subject + authority variant + applicable grant/tier + risk class` declara um conjunto não vazio de efeitos. No variant `HUMAN`, grant/tier são ausentes por schema e a root authority, action, risk e scope vêm do challenge/decision; nos variants autônomos, o lookup exige o `AutonomyGrantV1` aplicável. Tier não é global. A policy efetiva é a união dos requisitos:

| Efeito | Exemplos | Requisitos mínimos |
|---|---|---|
| `READ` | health, north, seek, dry-run | scope, privacy e freshness |
| `GRAPH_MUTATION` | ingest, memorize, promote | client identity, brain binding, checkpoint |
| `RUNTIME_STORE_WRITE` | memorize, persist save, debrief | identity, OCC/journal, durability |
| `SOURCE_FILESYSTEM_WRITE` | apply, edit, `xray_apply(commit)` | target digest, proof mark, OCC, rollback |
| `COORDINATION_RECORD` | delegate, debrief, Mission Control event | identity, ownership, audit |
| `MISSION_STATE_WRITE` | MissionService transition | role result, head CAS, state machine |
| `SOVEREIGN_MUTATION` | ratify, land, archive, replace/delete | mode-valid AuthorityDecision, transaction, tombstone |
| `PROCESS_SPAWN` | mission_spawn | pinned runner, isolation, limits |
| `NETWORK_EXPOSE` | bind não-loopback | TLS, authn, authz, audit, explicit policy |

Exemplos multi-efeito: `memorize = RUNTIME_STORE_WRITE + GRAPH_MUTATION`; `delegate = READ + COORDINATION_RECORD`; `debrief = COORDINATION_RECORD + RUNTIME_STORE_WRITE + GRAPH_MUTATION`; `xray_apply(dry_run) = READ`; `xray_apply(commit) = SOURCE_FILESYSTEM_WRITE`.

O middleware owner-side cobre MCP, REST, CLI, hooks, background jobs, recovery e migrations. Um teste falha quando qualquer combinação alcançável de action/mode/authority-variant/applicable-grant/tier/risk não está registrada. Alterar ingest roots é ação administrativa porque amplia o boundary de leitura.

### 6.5 Aprovação humana e authority journal

Esta é a authority provider de `HUMAN_GATED` e a root-governance provider de `POLICY_AUTONOMOUS`; `FULL_AUTONOMY` usa o quorum definido em 6.16.

O protocolo separa quatro contratos:

1. `HumanKeyRegistryV1`: `key_id`, subject, platform, public key, attestation class, created/revoked timestamps e status; o client também pina `OwnerIdentityV1`.
2. `OwnerChallengeV1`: challenge assinado pelo owner com `intent_digest` + immutable intent ref, `required_authority_variant=HUMAN`, action-policy/classifier digests, issuer/decision/caller/proposer/executor identities, delegation grant opcional, organism/repo/brain, audience, action, active mode, constitution/autonomy epochs, `mission_id`, `mission_head_id`, block, candidate digest, expected store/boundary/contract versions e store epoch, idempotency key, payload digest, canonical human-readable summary, nonce e expiry.
3. `HumanApprovalV1`: challenge id, digest dos bytes canônicos completos do challenge, key id, user-presence flags/counter e assinatura da chave matriculada desbloqueada por biometria/keystore.
4. `HumanDecisionV1`: depois de verificar `HumanApprovalV1`, o owner cria a decisão humana que referencia o mesmo intent; `AuthorityDecisionV1` a envolve como o único variant ativo.
5. `HumanCapabilityV1`: somente depois da `AuthorityDecisionV1` final, o owner minta uma capability curta e one-shot reservada para a transação exata.

`HumanCapabilityV1` herda todos esses bindings, inclusive intent ref/digest, required authority variant e a cadeia distinta de issuer/decision/caller/proposer/executor, mais authority-decision digest, `key_id`, `issued_at` e `owner_signature` distinta da assinatura humana; trocar qualquer campo ou mudar mode/epoch invalida a capability. Não existe referência reversa da HumanDecision para a capability.

`AuthorityJournalV1` persiste challenges, nonces reservados/consumidos, idempotency keys, terminal outcomes, pending RED latches, key rotation/revocation e audit. Para uma operação soberana, AuthorityJournal e AuthorityWAL são duas projeções do mesmo log transacional, não arquivos com prepares independentes. Restart nunca torna um nonce consumido reutilizável. A idempotency key é scoped por `(organism, brain, subject, action, key)`, nunca é reutilizada e seu terminal result permanece até a policy de GC provar que nenhum checkpoint, mission ou release ainda o referencia.

**Ações soberanas mínimas:** `ratify`, `land`, `archive`, `replace_store`, `permanent_delete`, `change_ingest_roots`, `promote_autonomy`, `release_promote`.

Biometria autentica uso da chave; não prova sozinha que o humano entendeu o payload. Por isso o challenge assinado inclui o resumo canônico mostrado antes do gesto. A garantia contra spoofing visual por processo local precisa de decisão explícita no threat-model ADR; este PRD não a declara resolvida.

### 6.6 Schemas de execution e evidence

`ExecutionResultV1` e `ReviewResultV1` são resultados assinados que runner/reviewer devolvem ao `MissionService`; eles não persistem letters diretamente.

`ExecutionDispatchV1` fecha a janela owner→runnerd: `execution_id`, mission/head/iteration, packet digest, exact announced runner id, idempotency key, deadline e state `INTENT|ACKED|COMPLETED|FAILED`. O owner grava intent em outbox durável; runnerd possui inbox dedup por `execution_id` e devolve acceptance ACK. `MissionService` só transiciona `dispatching → executing` depois do ACK. Restart reconcilia intent sem ACK, ACK sem transition e completed result sem letter; retry nunca cria segundo processo para o mesmo execution id.

`EvidenceRefV1` contém kind, URI/path não absoluto quando público, SHA-256 real, producer identity, command, started/ended timestamps e retention status.

`ReceiptCandidateV1` contém `candidate_id`, mission/head/iteration IDs, block/scope versions, execution result digest, evidence refs, issuer/key e candidate digest.

`ReceiptV1` estende o contrato atual com `receipt_id`, `receipt_digest`, `transaction_id`, `mission_id`, `mission_head_id`, `iteration_id`, `candidate_digest`, complete scope, resolution hash, evidence, validity, emitter e import audit. `ReceiptCoreV1` exclui `receipt_id`, `receipt_digest` e signature; `receipt_digest = SHA-256("m1nd-receipt-v1" || canonical(ReceiptCoreV1))` e `receipt_id = "rcp:" || receipt_digest`. O registro permanece em `SystemBlockStore.blocks[].receipts`; um store físico separado exige ADR e migração explícitos.

### 6.7 `AuthorityTransactionV1`, `AuthorityWALV1` e `LandTransactionV1`

Toda mutação soberana usa `AuthorityTransactionV1`: um PREPARE/COMMIT idempotente que consome capability, nonce e epochs no owner. A transação é uma união discriminada:

- `PositiveAuthorityTransactionV1` exige `AuthorityDecisionV1` positiva e Human/Autonomy capability; ratify, land, archive, replace/delete, autonomy promotion, release promotion e constitution amendment usam esse variant;
- `SafetyKernelTransactionV1` exige `RedLatchReceiptV1=PENDING`, um versioned `SafetyActionIntentV1`, `SentinelVerdictV1=RED` e `SafetyCapabilityV1` da identity de actuator pinada; ele proíbe AuthorityDecision positiva e só aceita freeze, epoch fence/bump, revoke, demote e rollback previamente vinculados.

`LandTransactionV1` é especialização do variant positivo e commita receipt + Mission Letter. Ambos os variants usam a mesma semântica do `AuthorityWALV1`; o safety path não contorna o kernel transacional e nenhuma ação é write direto.

O owner, e somente ele, compõe a operação de landing. h4nd e Human View enviam intenção autenticada e candidate, não um receipt inventado pelo cliente.

**Input:**

- `brain_id`, `mission_id`, `expected_head_id`;
- `candidate_id` e `expected_candidate_digest` somente para equality check;
- `expected_store_version`;
- mode-valid `HumanCapabilityV1` ou `AutonomyCapabilityV1 land`;
- idempotency key.

**Validações:**

- missão existe, está em `merge_wait` e não é synthetic;
- owner relê o candidate canônico do head atual; os bytes enviados pelo client nunca são authority;
- candidate id/digest esperado pertence ao head e ao bloco atual;
- boundary/contract versions não mudaram;
- evidence, execution identity, time window e artifact hash são válidos;
- resolution hash é resolvido pelo owner;
- authority decision/capability, nonce, payload digest e expected versions são exatos.

**Semântica normativa para os stores atuais:**

1. um único append durável `PREPARE(transaction_id, transaction_variant, source_intent_ref/digest/version, capability_kind/id, nonce, idempotency, active mode/activation receipt, authority/autonomy epochs, required sentinel verdict, canonical digests, variant_payload)` valida e consome o variant exato, fsynca o snapshot de autorização e cria todas as reservas no AuthorityJournal/AuthorityWAL. O payload positivo contém AuthorityDecision, identity/role bindings, required authority variant, action-policy/classifier e head/store/boundary/contract reservations; o payload safety contém latch/mandate digest, versioned attempt id + safety intent ref/digest, RED digest, pinned actuator identity, current expected epoch, affected scope, allowed-negative-actions e rollback plan;
2. `MissionService` chama seu subcomponente interno `LandTransactionV1`;
3. o subcomponente escreve `ReceiptV1` provisional e devolve o resultado ao `MissionService`, que escreve a Mission Letter provisional; ambos carregam o mesmo `transaction_id`;
4. manter ambos invisíveis aos readers até commit;
5. fsync dos records e do journal;
6. publicar commit marker atômico e assinado com `committed_at`, protected monotonic/time evidence, authority snapshot digest e os epochs/expiries que eram válidos naquele instante;
7. readers passam a enxergar receipt + `landed` juntos;
8. registrar terminal outcome idempotente e atualizar projeções.

O safety variant aplica a mesma barreira old-or-new: PREPARE reserva o expected old epoch e escreve next epoch/revocations/demotion/rollback como records provisionais invisíveis; fsynca WAL + records; então um único append atômico faz CAS do latch `PENDING → COMMITTING(transaction_id)` e publica o signed COMMIT marker com old/new epoch e authorization snapshot. Só a tentativa que vence esse CAS recebe marker; perdedoras abortam e descartam provisional. Depois do marker, a vencedora troca o authoritative epoch pointer e torna os efeitos visíveis, então finaliza `COMMITTING(transaction_id) → TERMINAL` idempotentemente. Crash antes do CAS/marker preserva old epoch + latch PENDING; crash depois encontra o único `COMMITTING(transaction_id)` + marker e forward-completa somente esse txid. Nunca existe effect visível em PREPARED nem dois markers para o mesmo latch.

Antes do COMMIT, o owner recalcula os bytes canônicos do intent e revalida o variant. No positivo, revalida policy/classifier, identidade/papéis/delegation, OCC, capability, sentinel, authority/autonomy epochs, expiries atuais e ausência de `pending_red` aplicável. No safety, revalida RED, pins do kernel/actuator, safety intent/capability, negative-action allow-list, affected scope e expected epoch; qualquer verbo positivo é impossível por schema. Positive COMMIT e pending-RED latch linearizam no mesmo AuthorityJournal/WAL: se o latch vence, o positivo aborta; se o signed commit marker já venceu, o commit é durável e o safety transaction posterior o compensa/rollbacka. Demotion, freeze, revocation, substitution ou expiry fenceiam PREPARE positivo; mismatch do safety variant também o torna `ABORTED`. Recovery consulta o WAL e separa duas leis:

- `PREPARED` sem commit marker continua sendo autorização pendente: recarrega os bytes canônicos pelo `intent_ref`, recalcula o digest e revalida todo o estado corrente. No positivo, qualquer mismatch/expiry/epoch bump aborta. No safety, o `RedLatchReceiptV1` PENDING preserva o mandato negativo imutável, não a identidade de uma tentativa: nonce consumido, capability expirada ou expected epoch stale abortam aquele PREPARE, e o actuator deriva novo `SafetyActionIntentV1` versionado com fresh attempt id/nonce/idempotency e current expected epoch. Source RED/latch, affected scope, verbs e rollback permanecem byte-identical. Positive authority continua fenced até terminal safety outcome;
- `COMMITTED` com marker assinado não pede nova autorização: verifica bytes e signatures contra o authorization snapshot histórico e prova `issued_at <= committed_at < expires_at` sob os epochs registrados; expiry ou epoch bump posteriores não desfazem o commit, e recovery completa os efeitos idempotentemente. Para safety, `COMMITTING(txid)` no latch e o marker precisam nomear o mesmo único txid; qualquer outro attempt aborta. Um evento de safety posterior produz uma nova transação compensatória/rollback, nunca uma meia reversão do commit anterior.

Rollback também é idempotente, journaled e ligado ao transaction/candidate digest. `LandTransactionV1` não é endpoint nem autor de letter: é subcomponente do `MissionService`, o único writer de Mission Letters. Races `land/archive/reconcile`, journal corrompido e crash após cada append/write/fsync/rename possuem fixtures. Uma futura store transacional pode substituir o WAL sem mudar a semântica externa.

### 6.8 Runtime por brain

O target imediato substitui o mutex global por:

- immutable read snapshots para health/status;
- actor/queue serial por brain para mutações;
- worker pools limitados operando sobre snapshot versionado e devolvendo proposal;
- commit curto no actor com OCC contra a revision atual;
- cancellation token e status observável;
- `RuntimeJobRegistryV1` com job id, deadline, cancellation/terminal state e `running_after_timeout`;
- backpressure, fairness, queue SLO, overload response e read-your-writes token explícitos;
- project brain dirty somente é evictada após checkpoint ACK;
- nenhuma transação cross-brain implícita; promoção à medulla preserva a origem e usa seu próprio audit protocol.

Process-per-repo é uma opção de ADR, não conclusão deste draft. O actor per brain no owner atual pode satisfazer isolamento; a decisão compara memória, discovery, port collision, worktrees, upgrade e rollback.

### 6.9 `CheckpointManifestV1`

O checkpoint cobre graph e sidecars derivados do brain. SystemBlockStore e Mission Letters mantêm WAL/log próprio e são referenciados por revisions imutáveis; não se finge uma transação física única entre raízes diferentes.

```text
checkpoint_id
brain_id
epoch
schema_versions
graph_snapshot_digest
sidecar_digests: map<name,digest>
ingest_roots_digest
external_authority_refs: {
  system_block_store_version,
  mission_heads_index_digest,
  authority_wal_root_digest,
  intent_core_store_root_digest,
  sentinel_outbox_watermark_digest,
  autonomy_epoch_record_digest
}
created_at
previous_checkpoint_id
```

Implementação: escrever diretório imutável temporário, fsync de cada arquivo e do diretório, escrever manifest final, rename atômico do diretório, fsync de seu parent, troca atômica do pointer `CURRENT` e fsync do parent de `CURRENT`. ACK só existe depois dos dois parent-directory fsyncs. Boot valida todos os digests, revalida AuthorityWAL/IntentCoreStore/AutonomyEpoch contra o protected root, compara o sentinel outbox root/epoch ao protected watermark e recua para o checkpoint anterior; checkpoint nunca escolhe active mode. GC nunca apaga o último fallback válido nem intent bytes ainda referenciados por WAL, terminal outcome, mission, checkpoint ou release. Batteries cobrem a primitive Windows equivalente e seu recovery, disk-full, corrupção, power loss em cada fase e GC concorrente. Persist parcial nunca atualiza `last_persist_time` como sucesso.

### 6.10 h4nd como boundary, não segundo owner

O h4nd deve:

- ler manifest, missions, pools, runs e jurisprudence por contratos públicos;
- apresentar `OwnerChallengeV1` e solicitar autenticação de device owner;
- usar uma chave não exportável matriculada para produzir `HumanApprovalV1`, se o ADR escolher assinatura client-side;
- mostrar o resultado transacional do owner.

O h4nd não deve:

- compor localmente um Receipt completo;
- declarar `landed` por conta própria;
- manter uma cópia autoritativa de mission state;
- exibir `WRITE=disarmed` por constante quando o pool possui write capability;
- servir source dirty/dev como release.

O mecanismo atual usa `LocalAuthentication` e retorna apenas boolean, com possível password fallback; isso prova autenticação do device owner, não biometria nem assinatura do payload. O binário instalado ser idêntico ao release local prova presença do código, não o fluxo de landing nem a UI carregada, que hoje vem de servidor mutável.

Target de rede: bind `127.0.0.1` por default; autenticação para toda a superfície, não só writes; Host/Origin/CSRF, rate e body limits; CSP; zero secrets no browser. Remote somente sob `NETWORK_EXPOSE`. Fixtures incluem LAN direct, DNS rebinding/Host, wrong origin, unauth read/write e CSRF.

### 6.11 `MetricSpecV1`

Nenhum threshold numérico é executável sem um spec versionado:

```text
metric_id
question
corpus_or_cohort_digest
ground_truth_protocol
unit
numerator
denominator
minimum_n
strata
environment
workload_and_seeds
confidence_interval
pass_threshold
non_inferiority_margin
command
artifact_retention
```

R2, R6, R8, R9 e R10 só recebem `PASS` a partir de `MetricSpecV1` ratificado. First-minute success define clean/warm state; gesto define ação consciente; retrieval define equivalência de anchor e adjudication; workload concorrente define hardware, mix e duration.

O gate `3 runs / 30 verdicts` é somente saída do piloto S1. Antes de qualquer approve autônomo, a proposta é: pelo menos 200 casos humanos adjudicados e deduplicados por caso+revision, 60 decisões `APPROVE` realmente emitidas pelo reviewer, 30 casos high-risk, denominator e reversal window ratificados, shadow period completo e limite superior unilateral de 95% para false/reverted approve abaixo de 5%. Esses números continuam `PROPOSTOS` até ratificação.

### 6.12 `ReleaseCandidateManifestV1` e `GateReceiptV1`

`ReleaseCandidateManifestV1` fixa source commits de cada repo, schemas/policies, tool catalog, binary e UI bundle digests, h4nd shell/bundle, runner/reviewer/pool versions, compatibility-manifest e rollback-plan digests, test-harness/fixture/threat-matrix digests, build environment e provenance. Também fixa `SafetyKernelV1`, previous governance runtime, constitution/autonomy epochs, grants/independence/quorum policy e intended active-mode digests, para que uma release de governance não possa trocar a lei usada para aprovar a si própria. `candidate_digest` é SHA-256 da serialização canônica com digest/signature omitidos.

Cada gate emite um `GateReceiptV1` content-addressed e assinado com receipt id/digest, candidate digest, gate/spec e harness/fixture versions, environment, provider/key, inputs, command, timestamps, exit/verdict, findings e artifact digests. Mudança em qualquer componente invalida os receipts dependentes.

`IndependentAdversarialReviewReceiptV1` usa a mesma disciplina e acrescenta threat-matrix digest, provider/model/version, reviewed inputs, findings, binding changes e verdict. G10 exige binding exato ao candidate e à matriz atuais; receipt antigo ou de outra candidate é recusado.

O pipeline faz **build once antes dos E2E**, instala esses artefatos, executa browser/golden/recovery/security/upgrade/rollback contra eles e promove exatamente os mesmos digests. G10 não admite waiver P0/P1. Reclassificação de risco só antes do candidate freeze, por amendment com evidência independente.

### 6.13 `RepoOwnerDirectoryV1` e ADR de topologia

Se process-per-repo for ratificado, um directory local mínimo resolve `canonical_root_fingerprint → owner endpoint, lease, runtime version, status`. Ele não contém graph nem domain state. Aplica guards de overlap/worktree, detecta owner stale, evita dois owners e só permite nascimento pela cerimônia consentida `m1nd init`; bridge nunca cria brain por heurística silenciosa.

Até o ADR comparar owner multi-brain com process-per-repo, o ProjectBrainRegistry atual permanece. Só há `RETIRE` depois de discovery, parity, migration e rollback provados.

### 6.14 `SchemaMigrationRegistryV1`

Cada schema/store possui current version, compatible readers, migration plan digest, fencing lease, journal phases e rollback. Apply recusa input divergente. Migrações de medulla provam conservação por claim e byte digest; falha em qualquer copy/stamp/remove/manifest/root update recupera sem perda. Upgrade não mistura versões incompatíveis silenciosamente.

### 6.15 Cross-repo contract e independent review

`m1nd` e o repo lógico `h4nd` mantêm source ownership separado. Um compatibility manifest versiona os contratos compartilhados, commits/artifact digests, security owner, cross-repo CI attestation e rollback order. Paths absolutos pertencem somente ao snapshot local.

O gate final exige `IndependentAdversarialReviewReceipt`; askGOD é o provider preferido atual, não substitui gates determinísticos nem a ratificação exigida pelo modo ativo. Indisponibilidade permanece `NOT_RUN/NOT_PROVEN` conforme a policy ratificada; nunca vira aprovação implícita.

### 6.16 Autonomia constitucional

O M1nd deve suportar três regimes, declarados no manifest e nunca misturados silenciosamente:

O ground atual ainda não possui ConstitutionStore, quorum service nem sentinel: autonomia soberana permanece `BUILD` + `PROVE`, não `LIVE`. A adoção recomendada é fazer o bootstrap em `HUMAN_GATED`, promover para `POLICY_AUTONOMOUS` quando A0–A3 em shadow/canary e rollback estiverem mecanicamente provados e tratar `FULL_AUTONOMY` como opt-in somente depois de G9. Um conjunto de agentes concordar não antecipa essa promoção.

| Modo | Ratificação operacional | Alterar constituição/release authority | Humano no loop diário |
|---|---|---|---|
| `HUMAN_GATED` | HumanApproval por ação soberana | Humano | Sim, para soberania |
| `POLICY_AUTONOMOUS` | Policy engine ou agent quorum dentro do envelope | Humano ratifica constitution/amendment e promotion | Não |
| `FULL_AUTONOMY` | Agent quorum para operações, arquitetura, landing e release | Constitutional quorum sob safety kernel | Não; break-glass humano é opcional |

`SafetyKernelV1` fica fora do ConstitutionStore e fora da authority A5. Sua identidade, canonicalization/verifier code, audit/WAL, epoch/freeze, rollback e old-runtime approval são pinadas no `OwnerIdentityV1` e verificadas no boot e no upgrade. O kernel fixa pisos normativos, não apenas um digest opaco: quatro verifier seats, quorum mínimo 3-de-4, três failure domains, proposer/executor não votantes, sentinel independente não votante com `RED` absoluto, fail-closed quando sentinel é obrigatório/indisponível e identidade/key/binary/policy do `SafetyActuatorV1`, que nunca autoriza positivamente. Constitution/amendment/release não podem reduzir esses pisos ou trocar esse TCB. Agentes não podem emendá-lo; qualquer mudança exige authority externa offline ou bootstrap explícito de novo organism epoch.

`ConstitutionStoreV1` é uma cadeia append-only, content-addressed e assinada, com `constitution_epoch`, previous digest, effective/expiry times, allowed autonomy modes, objetivos, non-goals, resources, budgets, risk classes, allowed actions, quorum/independence rules, MetricSpecs, canary/rollback requirements e amendment rules. Ela define a lei; não armazena o modo ativo. Cada decisão carrega digest + epoch. Um amendment é julgado pelo kernel, runtime, quorum membership e constituição anteriores, passa por PREPARE, delay e canary e só então ativa novo epoch. Um candidate que muda governance não vota na própria adoção, e nenhuma mudança beneficia a decisão que a propôs.

`AutonomyEpochV1` é a única authority persistida do modo operacional. É monotônico, assinado e protegido pelo mesmo anti-rollback root do owner; guarda `active_mode`, `activation_receipt_id`, active constitution digest/epoch, autonomy epoch, grants digest, `issuance_frozen` e safety state. Boot/restart/recovery recusam soberania se esse registro estiver ausente, inválido, rolled back ou incoerente. `AutonomyGrantV1` pertence a um subject/role e define `mode`, tier máximo, action classes, risk domain, resource/environment scope, budget, expiry e promotion receipt. Tier é por grant, não uma propriedade global do organismo ou do nome de um agente; o manifest apenas projeta o registro autoritativo.

`AutonomyActivationReceiptV1` é content-addressed e assinado pela authority do modo/epoch anterior. Ele vincula previous mode + constitution/autonomy epochs, exact release candidate, target mode/grants, G9/canary receipts, activation time e rollback plan. Uma única `AuthorityTransactionV1` instala o receipt e atualiza `AutonomyEpochV1.active_mode`, activation receipt, grants e epoch; recovery converge old-or-new. Suporte ou prova mecânica sem esse commit permanece inativo.

`SovereignActionIntentV1` é criado antes de challenge, sentinel, policy decision ou quorum. Seu core canônico vincula action/payload; `issuer_subject_id`, `decision_subject_id`, `caller_subject_id`, audience, proposer, executor opcional e promotion target opcional; delegation grant opcional; `required_authority_variant`; ActionPolicyRegistry e classifier-decision digests; applicable grant id/digest; organism/repo/brain/mission/head/block/candidate/promotion subject; active mode, grant/tier/risk/resource/environment/budget scopes; constitution/autonomy epochs; expected store epoch/version, boundary version e contract version; MetricSpec/evidence/rollout/rollback digests; nonce, issued/expiry. O core exclui `intent_ref`, verdict, votes, decision, capability, transaction e o próprio digest; `intent_digest = SHA-256("m1nd-sovereign-intent-v1" || canonicalization_version || canonical(IntentCoreV1))`, e `intent_ref` é derivado desse digest. RED pode, portanto, vetar um intent sem criar decisão positiva, e não existe ciclo digest.

Antes de qualquer assinatura, `IntentCoreStoreV1` persiste e fsynca os bytes canônicos em storage content-addressed dentro do mesmo domínio de durabilidade do AuthorityJournal, com `intent_ref`, digest e canonicalization version. Challenge, sentinel, verifiers, decisions, capabilities e WAL referenciam exatamente esse record; ele fica retido até terminal outcome e até nenhum checkpoint/mission/release o referenciar. Digest sem bytes recuperáveis é inválido. Alterar state, policy, authority variant ou qualquer papel exige novo nonce + novo intent e repete sentinel/quorum; escalation nunca reaproveita o verdict anterior.

Os bindings de identidade são igualdade normativa, não labels livres: `ClientIdentity.subject_id == caller_subject_id`; `AutonomyGrant.subject_id == decision_subject_id`; issuer é uma principal registrada para o `required_authority_variant`; final decision, capability e transaction copiam os mesmos IDs e variant. Caller pode diferir do decision subject somente com `delegation_grant_digest` vigente e mais estreito que o grant. Em quorum, proposer/executor não pertencem às quatro verifier seats. Se a action promove grant/tier/budget/scope de `promotion_target_subject_id`, esse target não pode ser issuer, decision subject, proposer, executor ou verifier da própria promoção. Mint, PREPARE e COMMIT recusam qualquer desigualdade; recovery aplica a lei corrente a `PREPARED` e o snapshot histórico a `COMMITTED`.

`AuthorityDecisionV1` é uma união explícita:

- `HumanDecisionV1` → intent digest + HumanApproval; a capability só nasce depois da AuthorityDecision final;
- `PolicyDecisionV1` → intent digest + policy id, matched clauses, risk, budget, proof receipts e required sentinel verdict;
- `AgentQuorumDecisionV1` → intent digest + proposer/executor/verifiers, votes, dissents, independence evidence, required sentinel verdict, rollout e rollback plan.

`AuthorityDecisionV1` contém exatamente um variant, e seu `authority_kind` precisa ser igual ao `required_authority_variant` congelado no intent; policy não pode virar quorum, nem quorum virar policy, sem novo intent. `PolicyDecisionV1` só autoriza uma action inteiramente coberta por policy determinística e grant vigente. A decisão final vincula o `intent_digest` já observado pelo sentinel e, quando a risk class exige sentinel, também carrega o exact `sentinel_verdict_digest`; o sentinel nunca assina essa decisão final. Quando sentinel não é exigido, o campo é explicitamente nulo e a matched policy clause prova a dispensa. Para A4/A5, `IndependenceSpecV1` congela por constitution epoch quatro verifier seats votantes, com principals/keys únicos, proposer/executor não votantes, parent/session/context separados, blind packets sem shared mutable memory antes do voto e diversidade mínima de três failure domains de provider/model/runtime/control. Todos assinam o mesmo `intent_digest` e os mesmos digests canônicos de constitution, candidate, evidence, rollout e rollback. O quorum proposto é 3-de-4; um sentinel separado não vota e seu `RED` é veto independentemente do placar. Se identidade, membership ou independência não puderem ser provadas, a action abstém.

`AutonomyCapabilityV1` é mintada pelo owner policy engine, nunca pelo quorum/client, a partir de uma AuthorityDecision válida. Ela vincula intent ref/digest/version, capability/decision/policy/classifier e required-sentinel-verdict digests; required authority variant; issuer, decision subject, caller, audience, proposer/executor, delegation e promotion target; organism/repo/brain; mission/head/block/candidate/promotion subject quando aplicáveis; mode, grant, tier, risk e resource/environment/budget scopes; constitution/autonomy epochs; expected store epoch/version e boundary/contract versions; payload digest, nonce, idempotency key, issue/expiry e owner signature. Mint, PREPARE e COMMIT recalculam o intent canônico e revalidam todos os bindings e a validade corrente de `SentinelVerdictV1` quando requerido; digest ausente/stale é fail-closed. Recovery segue a separação `PREPARED` corrente versus `COMMITTED` histórico de 6.7. `PositiveAuthorityTransactionV1` carrega os mesmos bindings e também revalida a authority física `AutonomyEpochV1`, inclusive active mode/activation receipt. SystemBlock ratify/archive, land, release, positive promotion/demotion e amendment usam esse positive path; RED-triggered demotion/rollback usa exclusivamente o safety variant. Nenhum deles aceita um agente rotulado como humano.

Escada de promoção:

| Tier | Poder máximo |
|---|---|
| `A0 OBSERVE` | Read/diagnose; zero mutation. |
| `A1 PROPOSE` | Produzir candidates/packets; zero apply/landing. |
| `A2 EXECUTE` | Mutação não soberana em scope/budget limitado; humano ou quorum ainda landa. |
| `A3 AUTONOMOUS_LAND` | Executar e landar ações low-risk dentro da constitution. |
| `A4 AUTONOMOUS_GOVERN` | Ratificar arquitetura e promover releases dentro da constitution. |
| `A5 FULL_AUTONOMY` | Quorum pode emendar policies/constitution abaixo do immutable safety kernel; nenhum humano é requisito operacional. |

Promotion exige MetricSpec e shadow/canary receipts para o action class, risk domain, resource scope, environment e budget exatos; prova em um domínio não promove outro. Nunca é autoatribuída pelo subject promovido. Constitution expiry congela toda emissão soberana e volta ao último modo/tier seguro; grant expiry revoga somente o scope afetado e o reduz a A1.

`SentinelVerdictV1` é content-addressed e assinado pela identity pinada no kernel. Ele vincula intent ref/digest/canonicalization version, sentinel key/binary/policy, constitution/autonomy epochs, nonce, issued/expiry e verdict `GREEN|RED`; nunca inclui decision digest. Seu verdict core exclui `verdict_digest` e signature; o digest é calculado antes da assinatura. Os bytes canônicos resolvidos do IntentCoreStore vão diretamente ao sentinel. `GREEN` é apenas telemetry/non-veto no journal. Para `RED`, o sentinel primeiro persiste `SentinelRedOutboxV1` sob signed monotonic outbox root e entrega idempotentemente, sem passar pelo quorum, ao SafetyActuator e ao AuthorityJournal. O owner compara `outbox_epoch/root` com seu protected watermark; rollback, gap inexplicado ou root inválido congela o sistema. Ao receber um RED ainda válido, o journal emite `RedLatchReceiptV1` assinado com source intent, RED digest, protected `latched_at`, constitution/autonomy epochs, exact affected scope, allowed negative verbs, rollback candidate/plan, monotonic latch epoch, immutable mandate digest e state `PENDING`; esse latch é o fence de mint/PREPARE/COMMIT positivo. A autoridade RED lineariza nesse append owner-side, não no outbox remoto: um positive commit marker anterior continua válido e será compensado; qualquer marker posterior ao latch é recusado. O outbox é a garantia de entrega eventual, não uma alegação impossível de revogação retroativa distribuída. Ele só termina após ACK do latch + terminal receipt do SafetyKernelTransaction. Crash/restart causa retry, e boot com RED pendente, outbox/journal divergente ou sentinel obrigatório indisponível fica frozen/fail-closed. Em risk classes que exigem sentinel, ausência, invalid signature/binding, expiry ou indisponibilidade produzem zero positive authorization.

O sentinel apenas emite um verdict assinado. Um `SafetyActuatorV1` owner-side, independente, roda em control lane/process isolado do brain/policy queue, com key/binary/policy pinados. `RedLatchReceiptV1` é o mandato negativo imutável. Para cada tentativa, o actuator deriva um `SafetyActionIntentV1` versionado do source intent + RED latch + current expected epoch + fresh attempt id/nonce/idempotency + affected grants/scope + exact negative verbs + rollback candidate/plan. O safety core exclui ref/digest/capability/transaction, usa domain separator próprio e é fsynced no `IntentCoreStoreV1` antes do PREPARE. O actuator então assina `SafetyCapabilityV1` one-shot para aquela tentativa. `SafetyKernelV1` não confia no scope proposto pelo actuator: resolve os bytes, recalcula independentemente a derivação a partir do source intent + RED/latch e valida pins, binding, nonce, expiry, affected scope exato e a allow-list imutável. Em retry, somente attempt id/sequence, nonce, idempotency e current expected epoch podem mudar; todos os campos de autoridade permanecem iguais ao mandate digest. O AuthorityJournal lineariza `PENDING → COMMITTING(txid)` junto com o marker: várias tentativas podem abortar, mas somente a vencedora pode ficar COMMITTED; o CAS `COMMITTING(txid) → TERMINAL` posterior apenas finaliza o mesmo claim. Se RED ou capability expirarem depois do latch, o latch PENDING não vira positive authority nem desaparece; ele permite novas tentativas negativas até terminal outcome.

Essa capability só instancia `SafetyKernelTransactionV1`. Ela nunca exige nem permite uma AuthorityDecision positiva e não pode landar, ratificar, promover, lançar release, emendar constitution ou escrever payload arbitrário. Seus únicos efeitos são freeze de emissão, epoch fence/bump, revoke, abort de PREPAREs não committed, demotion e rollback para candidate previamente assinado. Assim, `RED` é veto irrevogável dentro da decisão/epoch sem bypass transacional: em um único safety transaction, o actuator congela emissão, incrementa `autonomy_epoch`, revoga capabilities, fenceia/aborta PREPAREs não committed, registra rollback idempotente e demove os grants afetados. Identity, trust-root ou tamper failure levam qualquer tier a `FROZEN/A0`; recovery exige uma nova decisão sob a última authority válida. Break-glass humano pode pausar/revogar, mas `FULL_AUTONOMY` não depende dele para progresso normal.

O immutable safety kernel impede que agentes:

- removam separação entre proposer/executor/verifier;
- reduzam quatro seats, quorum 3-de-4, três failure domains ou tornem proposer/executor votantes;
- removam/bypassem o sentinel, seu veto `RED`, fail-closed obrigatório ou o SafetyActuator pinado;
- aumentem o próprio budget ou scope sem o quorum superior;
- desativem audit, receipts, rollback ou tamper detection;
- promovam o próprio tier;
- escondam dissent, abstention ou incident;
- reescrevam history ou se declarem humanos.

Quando estes componentes estiverem implementados, G9 estiver mecanicamente provado e uma activation receipt tiver sido emitida pela authority anterior, isso permite operação 100% autônoma no sentido prático. Hoje `FULL_AUTONOMY` permanece `NOT_IMPLEMENTED`, `NOT_PROVEN`, `NOT_RATIFIED` e `NOT_LIVE`. Mesmo depois da ativação, isso não prova que a constituição continua representando preferências humanas futuras nem elimina dependências externas como energia, contas, credenciais e lei; prova apenas consistência com a constituição e seu processo de amendment.

---

## 7. Requisitos funcionais por domínio

### F1 — Bootstrap e recepção

- Resolver caller root para exatamente um brain ou abster-se com recovery.
- Nunca gravar em medulla/brain errado por fallback.
- Retornar manifest e capability surface do brain recebido.
- Permitir warm rebind sem “renascer” o brain.
- Se o ADR process-per-repo vencer, resolver endpoint pelo `RepoOwnerDirectoryV1` e nunca iniciar owner silenciosamente.

### F2 — Orientação e busca

- `north`, `seek`, `impact`, `context`, `validate_plan` e proof routing compartilham o mesmo graph generation.
- Respostas incluem anchors, confidence/calibration, limits e abstention reason.
- Benchmarks guardam regressão contra busca textual e contra a versão anterior.

### F3 — Ingestão e memória

- Router declara provider escolhido e resultado por documento.
- L1GHT preserva provenance e supersession.
- Boot KV é migrado para configuração ou L1GHT conforme o tipo.
- Temporal/co-change state entra no checkpoint.

### F4 — Arquitetura e receipts

- Candidate, edit, reconcile e ratify preservam OCC e provenance.
- Graph observado nunca ratifica SystemBlocks automaticamente.
- Receipts carregam scope, validity e artifact identity verificáveis.
- X-RAY lê SystemBlocks; não cria um segundo mapa.

### F5 — Missões e delegação

- Mission Packet abre identidade causal única.
- Mission Control registra reasoning e handoff.
- `MissionService` é o único appender de Mission Letters e registra estado operacional a partir de resultados validados.
- Delegation herda scope/capability e debrief devolve outcome mensurável.
- Esses três registros se correlacionam por IDs, sem duplicar autoridade.
- Subagente devolve milestones/proof ao agente que possui o charter; não escreve no Mission Control alheio.

### F6 — Execução e pool

- runnerd é o único spawner de M1nd mission packets.
- Runner identity e capability são pinadas no owner.
- Owner→runnerd usa `ExecutionDispatchV1` com durable outbox/inbox dedup e acceptance ACK antes de `executing`.
- Worktree, limites, command, timestamps e full-log digest são evidência.
- Fallback de versão/boundary inventado é recusado, não preenchido com `(1,1)`.
- Runner/reviewer devolvem `ExecutionResultV1`/`ReviewResultV1`; somente o owner transiciona estado.
- poold é ator autônomo autenticado com scopes revogáveis, audit e backpressure. Warm/cold liveness não conta como execução; precisa de spawn real.
- A policy escolhe explicitamente drain humano ou autônomo e usa runner ID anunciado real, nunca `cold-runner` simbólico.

### F7 — Landing sob a authority do modo

- Human View/h4nd, policy engine ou constitutional council enviam `LandRequestV1` ao `MissionService` conforme o modo ativo; somente o owner invoca a transaction interna.
- O owner relê o head e candidate canônicos; payload do cliente é apenas digest esperado, nunca fonte de verdade.
- Stale scope mostra “re-run gate”; nada é re-datado para forçar import.
- Success retorna receipt e letter IDs verificáveis.
- Replay é idempotente e não duplica receipt/letter.
- `receipt_import` mission-bound e `mission_post` raw para qualquer fase tornam-se primitives privadas; toda chamada externa direta é recusada independentemente da capability.

### F8 — Operação

- Health independe de locks de execução.
- Presence e instance state têm TTL, collision e restart semantics.
- Gardener detecta, alerta e nunca repara soberania silenciosamente.
- Degraded persistence e version drift são visíveis no primeiro nível.
- Job registry, migration state e authority journal possuem health/read snapshots próprios.

### F9 — Hosts

- Catálogo de tools e skills é canônico.
- Adapters declaram limitações do host.
- Attach transport é fino e não cria cérebro paralelo.
- Host mismatch segue o mesmo recovery playbook.

### F10 — Release

- CI constrói uma vez e testa o produto inteiro a partir dos artefatos instalados.
- Release promove o mesmo `ReleaseCandidateManifestV1` testado.
- Packages possuem provenance, checksum e SBOM.
- Upgrade e rollback preservam ou migram checkpoint de forma provada.
- Cross-repo contracts do h4nd/pool/reviewer/god-runner entram na candidate e no rollback order.

---

## 8. State machine de missão proposta

Esta tabela substitui a regra quase aberta atual. É **PROPOSTA** para ratificação e acrescenta o wire state `revising`:

| Origem → destino | Autor/result permitido | Payload obrigatório | Lei |
|---|---|---|---|
| `∅ → judging` | `MissionService` com policy de oracle | packet + iteration 1 + expected reviewer | Abre chain. |
| `∅ → dispatching` | `MissionService` com policy direct | packet + iteration 1 + durable ExecutionDispatch intent | Fluxo sem oracle. |
| `judging → dispatching` | `ReviewResultV1 APPROVE` | verdict digest + mesmo packet digest + dispatch intent | CHANGE não executa direto. |
| `judging → revising` | `ReviewResultV1 CHANGE` | binding changes | Invalida artifacts da iteration. |
| `judging → failed` | `ReviewResultV1 REJECT` | verdict digest | Terminal. |
| `revising → judging` | owner/author proposal | novo packet digest + `iteration_id+1` | Policy exige novo julgamento. |
| `revising → dispatching` | owner/author proposal | novo packet digest + `iteration_id+1` + dispatch intent | Somente policy direct. |
| `dispatching → executing` | runnerd acceptance ACK | exact `execution_id` + runner id | Só depois de durable ACK. |
| `dispatching → failed` | MissionService | terminal dispatch failure artifact | Retry transitório reconcilia, não duplica. |
| `executing → gate` | `ExecutionResultV1` | command, exit, timestamps, log digest | Nenhum verdict. |
| `executing → failed` | `ExecutionResultV1` | failure artifact | Terminal. |
| `gate → review` | `MissionService` | gate digest + expected reviewer | Review obrigatório. |
| `gate → merge_wait` | `MissionService` | gate green + candidate digest | Review dispensado por policy. |
| `gate → failed` | `MissionService` | gate red | Terminal. |
| `review → revising` | `ReviewResultV1 CHANGE` | binding changes | Invalida gate/candidate/review. |
| `review → merge_wait` | `ReviewResultV1 APPROVE` | verdict + gate + candidate digests | Ainda não landed. |
| `review → failed` | `ReviewResultV1 REJECT` | verdict digest | Terminal. |
| `merge_wait → landed` | `MissionService` via `LandTransactionV1` interno | mode-valid AuthorityDecision + committed receipt | Único caminho de landing. |
| `merge_wait → archived` | `MissionService` | mode-valid AuthorityDecision archive | Candidate set aside. |

`landed`, `failed` e `archived` são terminais. Retry após terminal cria nova missão com `causation_id`. Toda transição valida role, client identity, capability, head CAS, iteration, packet/payload digest e idempotency key. Somente `MissionService` persiste a letter; runner/reviewer são produtores de resultados, não autores do store.

---

## 9. Gates cumulativos G0–G10

Nenhum gate posterior compensa a falha de um anterior.

### G0 — Baseline honesto

- Freeze do snapshot source/runtime/docs/h4nd.
- Manifest provisório gerado e divergências listadas.
- P0/P1 aceitos pelo owner como backlog vinculante.
- Golden fixtures atuais preservadas antes de refactor.

### G1 — Truth & Identity Spine

- `OrganismManifestV1` e `CausalEnvelopeV1` ratificados.
- Todas as autoridades mapeadas.
- Primeiro manifest servido por owner e consumido por Human View/h4nd.
- Drift source/binary/bundle possui fixture negativa.

### G2 — Authority Kernel & Security

- `ClientIdentityV1`, HumanKeyRegistry, challenge/approval/capability e AuthorityJournal ratificados.
- `SafetyKernelV1`, `ConstitutionStoreV1`, `AutonomyEpochV1`, `AutonomyGrantV1`, `SovereignActionIntentV1`, `SentinelRedOutboxV1`, `RedLatchReceiptV1`, `SafetyActionIntentV1`, `IntentCoreStoreV1`, `AuthorityDecisionV1` e Human/Autonomy/Safety capability schemas ratificados; o bootstrap fail-closed persiste `active_mode=HUMAN_GATED` no único record autoritativo.
- `ActionPolicyRegistryV1` cobre 100% de `ingress + action + mode + authority variant + applicable grant/tier + risk`, inclusive jobs, hooks, recovery e migrations.
- Toda mutação positiva exige client identity; ratify, land, archive, replace, delete e ingest-root change exigem `AuthorityDecisionV1` válida para o modo ativo. Safety effects exigem actuator identity pinada + RED + SafetyActionIntent/Capability e nunca compartilham esse positive path.
- Nonce/key lifecycle, rotation, revocation, restart e replay batteries verdes.
- Same-UID tampering/rollback é detectado por signed roots + protected epoch, ou o owner roda sob UID/sandbox separado; deletion recovery é provado.
- Remote sem TLS/auth é impossível.
- Peek default-deny e path escape battery verde.
- h4nd loopback/auth/origin/CSRF fechados.

### G3 — Mission State & Landing

- State machine completa e `MissionService` server-owned.
- `ExecutionResultV1`, `ReviewResultV1`, `ReceiptCandidateV1`, `ReceiptV1` e evidence refs versionados.
- `ExecutionDispatchV1` outbox/inbox fecha crash e retry owner→runnerd sem ghost executing ou double-spawn.
- A união `PositiveAuthorityTransactionV1 | SafetyKernelTransactionV1` + `AuthorityWALV1`, incluindo `LandTransactionV1`, fecha visibility fencing, RED sem decisão positiva, intent bytes duráveis e recovery distinto para PREPARED-current versus COMMITTED-historical.
- LandTransaction é subcomponente interno; somente MissionService persiste qualquer letter.
- Calls externas diretas aos legacy `receipt_import` e raw `mission_post` em qualquer fase recusam, independentemente da capability.
- Human View/h4nd ou autonomous council chama o novo contrato conforme o modo; owner relê candidate canônico; stale/synthetic é filtrado.
- Invented receipt, wrong mission/block/boundary, illegal transition, wrong author, concurrent seq, archive/land/reconcile race e crash fixtures passam.

### G4 — Runtime Isolation & Durability

- Actor/queue por brain e read snapshots.
- Health SLO sob carga.
- RuntimeJobRegistry, cancellation/backpressure/overload observáveis.
- Worker proposal + actor OCC curto; stale result não commita.
- Checkpoint directory + atomic `CURRENT`; evict somente após ACK.
- Disk-full, corruption, Windows, GC, fault injection e multi-brain isolation passam.
- Schema migrations usam plan digest, fencing, journal, conservation e rollback.

### G5 — Evidence & Proof Spine

- Proof pela união de efeitos, default-on para agentes mutantes.
- Marks generation/digest/TTL-aware.
- X-RAY e futuros physical writes entram automaticamente.
- Receipts, letters, delegation e Mission Control correlacionados.
- Golden mission completa termina em `landed` real.

### G6 — Knowledge Quality

- Benchmark held-out e thresholds de R2 ratificados.
- Matriz universal de ingestão e provider status.
- Temporal/co-change persistido.
- Boot KV migration concluída.
- Calibration e abstention gates verdes.

### G7 — Human Product Coherence

- Human View e h4nd consomem manifest/authority state.
- h4nd source adotado, build production e runtime attestation de que o shell instalado carregou o exact promoted bundle digest; mismatch recusa/DRIFT.
- UI unit, accessibility, browser fixture e browser LIVE real sem interceptar APIs são provas separadas.
- poold policy é explícita; cada lane ratificada warm/cold passa E2E não-sintético em bloco ratificado: claim → handoff → authenticated spawn → exact runner ACK → result → MissionService transition. Lane sem prova fica policy-disabled.
- Landing, stale, degraded, drift e recovery são visíveis.

### G8 — Agent/Host Interoperability

- Tool catalog parity por host Tier A.
- First-minute benchmark por host.
- macOS/Linux/Windows install/attach/update/rollback do core.
- Caller-root mismatch e reconnect batteries.
- Capability matrix publicada.
- ADR de topologia e, se necessário, RepoOwnerDirectory discovery/lease/worktree gates.

### G9 — Calibrated Autonomy & Release

- Reviewer satisfaz S1 e qualquer promoção posterior pelo `MetricSpecV1`, sem passagem vacuosa por zero approves.
- SafetyKernel/old-runtime approval, immutable quorum/sentinel/actuator floors, ConstitutionStore amendment chain, authoritative active-mode recovery, scoped grants, `SovereignActionIntentV1` durável, identity/role equality chain, `SentinelVerdictV1` + durable RED outbox/latch, SafetyActionIntent/SafetyCapability negative-only transaction, epoch fencing, auto-demotion e break-glass são mecanicamente provados.
- A0→A5 é provado em shadow/canary sem ativar A5 em produção; qualquer promotion/demotion emite receipts e agent self-promotion/self-ratification são fixtures negativas.
- Ativação de `POLICY_AUTONOMOUS` ou `FULL_AUTONOMY` exige `AutonomyActivationReceiptV1` autorizado pelo modo/epoch anterior e ligado ao exact release candidate; suporte mecânico nunca se autoativa.
- CI required cobre todas as lanes definidas em R10.
- `ReleaseCandidateManifestV1` é construído uma vez; todos os gates emitem `GateReceiptV1` para o mesmo digest.
- Packages, binaries, h4nd shell/bundle e runners são promovidos do mesmo build provado.
- Checksums, SBOM, assinatura e provenance.
- Cross-repo compatibility e rollback rehearsal concluídos.

### G10 — Convergência

- R1–R10 em `PASS` para o mesmo candidate digest.
- Zero P0/P1; G10 não aceita waiver nem reclassificação após freeze.
- Adversarial review executa a matriz de ameaças/gaps ratificada e deixa zero finding P0/P1 aberto; não se alega provar uma negativa universal.
- `IndependentAdversarialReviewReceipt` final existe; askGOD é o provider preferido enquanto disponível e mudanças obrigatórias são incorporadas e reavaliadas.
- A authority final do modo ratifica: humano em `HUMAN_GATED`/`POLICY_AUTONOMOUS`; constitutional quorum em `FULL_AUTONOMY`.
- Somente então a release recebe a marca “M1nd 10”.

---

## 10. Fixtures negativas obrigatórias

| Família | Fixture mínima | Resultado esperado |
|---|---|---|
| Mission | `landed` com anchor inventado | Refuse; zero append |
| Mission | `landed → executing`, `failed → landed`, `archived → *` | Refuse |
| Mission | agente B estende cadeia de A | Refuse `unauthorized_author` |
| Mission | dois `seq+1` concorrentes | Um commit; um `stale_head` |
| Mission | runner chama letter store diretamente | Refuse; somente MissionService escreve |
| Dispatch | crash antes/depois do runner ACK e retry concorrente | Um `execution_id`, no máximo um processo, estado reconciliado |
| Landing | receipt de outro block/mission | Refuse; zero import |
| Landing | boundary/contract stale | Refuse `stale_scope`; pedir re-run |
| Landing | candidate do client difere do head canônico | Refuse; zero prepare |
| Landing | external legacy `receipt_import` ou raw `mission_post` em qualquer fase | Refuse sempre; private MissionService path only |
| Landing | reader antes do WAL commit | Zero receipt e zero letter visíveis |
| Landing | crash/corrupção em cada WAL phase | Recovery old-or-new |
| Landing | corrida land/archive/reconcile | Um vencedor coerente; demais stale |
| Capability | replay de nonce | Refuse |
| Capability | payload/head/brain/version alterado | Refuse |
| Capability | key revoked/rotated, wrong audience ou restart replay | Refuse |
| Capability | autonomy capability muda subject/caller/mode/grant/tier/risk/scope/epoch | Refuse; zero PREPARE |
| Capability | capability é mintada antes da AuthorityDecision final ou decision variant difere do required authority variant | Refuse por schema/order; novo intent obrigatório para outro variant |
| Capability | demotion/freeze entre PREPARE e COMMIT | Epoch fence; abort idempotente |
| Autonomy | executor também tenta verificar/ratificar | Refuse por independence rule |
| Autonomy | agente tenta aumentar seu tier/budget/scope | Refuse; superior quorum required |
| Autonomy | metric/anomaly threshold vermelho | Sentinel veto; SafetyActuator faz freeze + epoch bump + revoke/fence + rollback journaled + demotion |
| Autonomy | RED tenta usar transaction positivo, omite SafetyCapability, ou safety variant pede land/ratify/promote/release/amend/arbitrary write | Refuse por discriminated union/kernel allow-list; zero positive effect; safety veto continua pending/fail-closed |
| Autonomy | replay concorrente do mesmo RED/SafetyCapability ou actuator amplia affected scope/rollback plan | Retries podem abortar, mas commit-claim CAS atômico com marker permite um COMMITTED no máximo; kernel recalcula/refusa divergência |
| Autonomy | crash entre RED, outbox, journal latch, actuator ACK, safety PREPARE ou terminal receipt | Outbox retenta; depois do latch toda autorização positiva fica fenced; marker anterior é compensado; boot permanece frozen até recovery |
| Autonomy | rollback/delete/gap no sentinel outbox root/epoch ou owner watermark | Detectar por signed chain + protected monotonic epoch; boot/reconnect frozen, ou isolamento UID/sandbox obrigatório |
| Autonomy | RED/capability expira, nonce foi consumido ou expected epoch mudou após RedLatchReceipt PENDING | Tentativa aborta; novo versioned SafetyActionIntent usa fresh attempt nonce/idempotency/current epoch e o mesmo immutable mandate; no máximo um COMMITTED |
| Autonomy | corrida pending-RED latch contra positive COMMIT marker | Uma ordem linear no mesmo WAL: latch-first aborta positivo; commit-first forward-completa e safety compensa/rollbacka |
| Autonomy | quorum dissent ou independence insuficiente | Abstain/escalate; zero sovereign commit |
| Autonomy | aliases/keys ou failure domains não independentes satisfazem 3-de-4 | Refuse; membership congelado no epoch |
| Autonomy | caller/decision/grant subject diverge sem delegation, ou proposer/executor/promotion target ocupa verifier/issuer proibido | Refuse em decision/mint/PREPARE/COMMIT; zero self-authorization |
| Autonomy | policy/quorum variant, classifier/policy digest ou expected store/boundary/contract snapshot muda após sentinel/votos | Refuse; novo intent + novo verdict/quorum |
| Autonomy | Constitution/A5 tenta reduzir seats/quorum/failure domains, remover RED veto ou trocar SafetyActuator | Refuse pelo SafetyKernel; external root/new organism required |
| Autonomy | SentinelVerdict ausente, expirado, replayed ou com action/candidate/MetricSpec/epoch divergente | Zero positive authorization; audit/freeze conforme risk policy |
| Autonomy | intent muda depois do verdict, ou canonicalização tenta incluir verdict/decision no próprio intent | Digest diverge/refuse; zero decision, capability ou PREPARE; fixture prova ausência de ciclo |
| Autonomy | restart perde os bytes canônicos do intent ou intent ref não resolve para o digest | Fail-closed; nenhum PREPARE/COMMIT/recovery trusted |
| Autonomy | PolicyDecision/capability/PREPARE omite o required sentinel digest ou muda após restart | Refuse no mint/PREPARE/COMMIT; zero sovereign commit |
| Autonomy | quorum tenta suprimir ou atrasar `RED` válido | SafetyActuator recebe direto; freeze/fence independe do placar |
| Autonomy | active-mode record ausente, corrompido, rolled back ou conflitante com activation receipt | Boot/recovery fail-closed; zero sovereign commit |
| Autonomy | constitution/grant expirado | Freeze fail-closed e demotion scoped; zero sovereign commit |
| Autonomy | novo governance runtime/kernel/quorum root tenta aprovar a própria adoção | Refuse; somente runtime/constitution anterior pode autorizar |
| Autonomy | release tenta alterar/ignorar SafetyKernel, audit, WAL ou tamper code | Refuse no verified boot/upgrade gate |
| Recovery | PREPARED positivo reinicia depois de expiry, revocation ou epoch bump | Recalcula intent, revalida estado corrente e aborta/libera reservas |
| Recovery | safety crash antes/depois do latch COMMITTING + signed marker ou epoch-pointer publish | Antes: descarta provisional e preserva latch PENDING; depois: somente claimed txid forward-completa e finaliza TERMINAL; nunca PREPARED com effect visível |
| Recovery | COMMITTED reinicia depois de expiry ou epoch bump posterior | Valida snapshot no committed_at e forward-completa idempotentemente; safety posterior usa nova transação |
| Same UID | editar/rollback/deletar store ou journal pelo filesystem | Detect tampering/rollback; recover deletion; zero trusted commit |
| Store | force replace/delete sem capability | Verb indisponível/refuse |
| Runtime | operação 30 s + health | health p99 <100 ms |
| Runtime | timeout | trabalho cancelado ou status rastreável; nenhum lock órfão |
| Runtime | worker retorna proposal contra revision velha | OCC refuse; zero commit |
| Durability | falha em cada write/rename/fsync/CURRENT | Restart old-or-new |
| Durability | disk-full, corrupt latest ou GC concorrente | Fallback íntegro preservado |
| Registry | evict com persist failure | brain permanece viva/degraded |
| Policy | action/mode/authority-variant/applicable-grant/tier/risk em qualquer ingress sem effects | CI falha |
| Policy | tool multi-efeito omite um requisito | CI falha |
| Proof | target alterado após mark | mark invalidado |
| Security | empty Peek allow-list | deny |
| Security | symlink/traversal/remote unauth | deny e audit |
| Security | LAN direct, DNS rebinding, wrong Host/Origin e CSRF no h4nd | deny e audit |
| Pool | symbolic/unannounced runner ou unauthorized drain | refuse; zero spawn |
| Ingest | provider retorna `None` | `DEGRADED/UNSUPPORTED`, nunca sucesso vazio |
| Ingest | mudança de roots sem mode-valid AuthorityDecision | refuse |
| Migration | input muda depois do plan ou crash por fase | refuse/recover com conservação |
| Identity | source/binary/bundle divergentes | Manifest `DRIFT` |
| Host | caller root ambíguo | abstain; zero write |
| Release | gate receipt pertence a outro candidate digest | promotion bloqueada |
| Release | gate/review receipt sem signature, current harness ou threat-matrix digest | promotion bloqueada |
| Release | package/shell/bundle difere do artifact instalado no gate | promotion bloqueada |

---

## 11. Plano de entrega

### Fase A — Fechar verdade e identidade (G0–G1)

1. Ratificar os contratos deste PRD e o UML.
2. Congelar `GroundSnapshotReceipt`, authorities e ADR backlog.
3. Construir manifest, causal envelope e release candidate identity.

**Saída:** cada fato e artifact possui identidade, authority, revision e freshness.

### Fase B — Fechar autoridade e landing (G2–G3)

1. Client identity, Human Approval, Authority Journal, Action Policy Registry e ConstitutionStore/AuthorityDecision.
2. Fechar remote, Peek e h4nd network boundary.
3. Construir MissionService, schemas de result/evidence e state machine.
4. Construir `AuthorityTransactionV1 + AuthorityWALV1` e a especialização `LandTransactionV1`; tornar os legacy bypasses internos.
5. Migrar Human View/h4nd e provar todas as fixtures de autoridade/landing.

**Saída:** é impossível declarar landing sem receipt real, candidate canônico e autoridade correta.

### Fase C — Sobreviver e unificar prova/conhecimento (G4–G6)

1. Actor per brain, RuntimeJobRegistry, cancellation, backpressure e health snapshot.
2. Checkpoint directory/CURRENT, evict ACK, migration registry e failpoints.
3. Proof marks completos usando effects do Action Policy Registry.
4. Identity/evidence linkage em receipts, letters, delegation e Mission Control.
5. Universal ingest status, temporal persistence, Boot KV migration e benchmark de retrieval/calibration/abstention.

**Saída:** o owner resiste a concorrência/crash e cada ação importante possui ground, scope, evidência e validade mensuráveis.

### Fase D — Fechar os produtos (G7–G8)

1. Human View e h4nd sobre manifest/authority contracts.
2. Adotar e buildar h4nd production; prova browser real.
3. Ratificar a policy poold/cold lane e provar um spawn production real.
4. Catálogo canônico de host surfaces e ADR de topologia/discovery.
5. Matrix install/attach/update/rollback.

**Saída:** humano e agentes enxergam o mesmo organismo em qualquer host suportado.

### Fase E — Autonomia e release (G9–G10)

1. Completar reviewer S1; provar scoped A0→A5, quorum/sentinel/SafetyActuator em shadow/canary sem autoativar e promover somente sob métricas.
2. Required CI do produto completo.
3. Build once, GateReceipts, supply-chain, packages, upgrade e rollback rehearsal cross-repo.
4. Independent adversarial review, askGOD quando disponível e ratificação pela authority do modo ativo.

**Saída:** release M1nd 10 promovida, reproduzível e reversível; qualquer mudança de active mode possui activation receipt emitido pela authority anterior.

---

## 12. Dependências críticas

```text
Manifest/Identity
  ├──> Constitution + mode ───────> Human/Policy/Quorum authority
  │                                  └──> MissionService ──> Landing
  ├──> Action policy ─────────────> Proof gate ──────> X-RAY/apply
  ├──> Runtime actor ──────> Checkpoint ──────────> multi-brain/recovery
  └──> Product projections ─> host parity ────────> release

Landing + Runtime + Proof
  └──> Golden path
       └──> Reviewer/autonomy
            └──> Release promotion
```

Começar por UI polish, mais runners ou mais ferramentas antes de G2–G4 aumenta a superfície de inconsistência e não melhora a nota real.

---

## 13. Ownership de implementação

| Área | Repo/dono de source | Responsabilidade |
|---|---|---|
| Graph, ingest, owner, stores, missions | `m1nd` | Contratos canônicos e enforcement |
| Human View | `m1nd/m1nd-ui` | Projeção e gestos sem duplicar domínio |
| runnerd | `m1nd/m1nd-runnerd` | Spawn isolado e evidence capture |
| h4nd | Repo lógico `h4nd` — checkout local do snapshot: `/Users/kle1nz/god-hud` | Cockpit e autenticação de intenção humana |
| Host adapters/skills | `m1nd` + instalações geradas | Paridade e recovery por host |
| SafetyKernel/constitution/grants/quorum/sentinel/actuator | `m1nd` | Autonomy mode, authority decisions, scoped promotion/demotion, epoch fencing e safety |
| Reviewer/pool/god-runner | Repo lógico `h4nd` até decisão de boundary | Operação externa, calibration e runs |
| CI/release | Owners de `m1nd` + `h4nd` e cross-repo attestation | Required gates e promotion |
| Ratificação | Authority do modo | Humano, policy ou constitutional quorum, sempre com origem explícita |

O h4nd permanece um boundary separado neste PRD. Mover seu source ou seus serviços para o monorepo requer decisão arquitetural própria; não é assumido aqui.

---

## 14. Decisões humanas pendentes

1. Ratificar os thresholds de R2, R6, R8, R9 e R10.
2. Ratificar o threat model same-UID e decidir explicitamente o limite contra UI spoofing/social deception.
3. Ratificar a state machine com `revising`, iteration invalidation e retry por nova missão.
4. Escolher client enrollment e HumanApproval primitives no macOS e fallbacks nos demais SOs.
5. Ratificar `AuthorityWALV1` como primeiro backend ou exigir store transacional unificado antes de G3.
6. Definir a fronteira permanente de h4nd/pool/reviewer/god-runner.
7. Ratificar por ADR owner multi-brain versus owner/runtime por repo e, no segundo caso, RepoOwnerDirectory/migração.
8. Ratificar hosts Tier A e sistemas operacionais oficialmente suportados.
9. Definir SLOs finais de latency/scale após benchmark de baseline.
10. Decidir se `failed` é terminal ou se haverá um estado explícito `retrying`; este PRD propõe missão nova.
11. Ratificar drain humano versus autônomo no poold e scopes de spawn/revogação.
12. Ratificar `MetricSpecV1` de autonomia, incluindo amostra approve/high-risk e reversal window.
13. Ratificar se indisponibilidade de askGOD bloqueia G10 ou se outro provider pode emitir o mesmo `IndependentAdversarialReviewReceipt`.
14. Ratificar `HUMAN_GATED` como bootstrap fail-closed, escolher o target steady-state (`POLICY_AUTONOMOUS` recomendado ou `FULL_AUTONOMY`) e ratificar SafetyKernel, ConstitutionStore, grants, quorum e promotion gates; ativação posterior exige G9 + receipt da authority anterior.

---

## 15. Riscos e mitigação

| Risco | Impacto | Mitigação |
|---|---|---|
| Big-bang refactor | Regressão e impossibilidade de atribuir falhas | Slices verticais por gate, fixtures antes da migração |
| Manifest virar novo dono | Mais uma verdade concorrente | Manifest somente deriva e referencia authorities |
| Capability aumentar fricção | Owner contorna o sistema | Gestos curtos, challenge legível, sessão limitada e auditada |
| Transaction atravessar stores legados | Half-commit | Journal + idempotency + recovery battery |
| Actor per brain reduzir throughput | Backlog e timeouts | Métricas, workers bounded, reads por snapshot |
| Benchmark otimizado para fixtures | Falsa sensação de qualidade | Held-out, versionado, múltiplos repos e revisão humana amostral |
| h4nd local dirty ser tratado como release | Drift silencioso | Adopt/commit, production build, digest no manifest |
| Autonomia promovida cedo | Approve incorreto com blast radius | Estágios, zero reverted approves, rollback e human sampling |
| Quorum com common-mode failure | Vários agentes repetem o mesmo erro | IndependenceSpec, membership congelado, blind isolation, failure-domain diversity e sentinel veto |
| Constitution drift em A5 | Sistema permanece consistente com objetivo já indesejado | SafetyKernel externo, amendments old-runtime em duas fases, expiry/review e break-glass opcional |
| CI muito lenta | Gates desativados | Pirâmide fast/required/nightly sem remover cobertura obrigatória |
| Documentação divergir do runtime | PATHOS vira mito | Claims machine-priced e manifest links em checkpoints |

---

## 16. Definition of Done do M1nd 10

O programa termina somente quando:

- G0–G10 estão `PASS` para o mesmo `release_candidate_digest`;
- R1–R10 possuem `MetricSpecV1`/gate spec, comando, artifacts e `GateReceiptV1`;
- todos os P0/P1 estão corrigidos; nenhum waiver ou reclassificação é aceito depois do candidate freeze;
- source, binaries, UI, h4nd instalado e packages possuem identidade coerente;
- golden path passou em ambiente limpo e após upgrade;
- restart, crash, timeout, concurrency e rollback foram exercitados;
- reviewer/autonomy está no estágio que os dados permitem, com evidência approve não vacuosa antes de qualquer autoridade de approve;
- supported/proven modes e a projeção do único `AutonomyEpochV1` autoritativo — active mode, activation receipt, scoped grants/tiers, constitution/kernel/autonomy epochs, quorum policy e sentinel/safety state — aparecem no manifest e passaram seus receipts;
- `IndependentAdversarialReviewReceipt` final foi emitido pelo provider ratificado; se a policy exigir askGOD, seu veredito é admissível e incorporado;
- a authority prevista pelo modo assinou a ratificação final; `FULL_AUTONOMY` não exige assinatura humana.

Até lá, a frase correta é: **“M1nd 10 em construção; estado atual conforme o manifest e os gates.”**

---

## 17. Protocolo de ratificação deste PRD

O owner respondeu `APPROVE` em 2026-07-18. Este PRD e o UML irmão são, desde então, a baseline vinculante da implementação.

A decisão registrada foi:

```text
APPROVE — bootstrap HUMAN_GATED; target FULL_AUTONOMY after G9
```

Essa ratificação autoriza a implementação cumulativa `G0–G10`; ela não declara nenhum gate concluído, não promove uma release e não permite autoativação. Qualquer mudança posterior de active mode continua exigindo `AutonomyActivationReceiptV1` emitido pela authority do modo/epoch anterior.

As decisões possíveis para uma futura revisão desta baseline permanecem:

- `APPROVE` — o PRD e o UML tornam-se baseline vinculante;
- `CHANGE` — lista de mudanças obrigatórias antes da baseline;
- `REJECT` — a direção é abandonada e nenhuma implementação deriva deste documento.

Uma revisão `CHANGE` cria nova versão da baseline; `REJECT` exige plano explícito de descontinuação ou substituição. Na ausência de nova decisão, a versão 1.0 ratificada permanece vigente.
