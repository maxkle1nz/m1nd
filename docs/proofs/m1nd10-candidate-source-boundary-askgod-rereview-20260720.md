# M1ND-10 candidate source boundary — askGOD re-review — 2026-07-20

## Binding and outcome

| Field | Value |
|---|---|
| Review mode | askGOD `review`, read-only, full gear |
| Voice | Fable seat (isolated read-only Explore agent) |
| Voice downgrade note | The cross-vendor Fugu seat was unavailable: the codex CLI OAuth refresh token is revoked (engine looped on auth, zero tokens). The Sakana API itself was verified alive (HTTP 200) before declaring the route down. The Fable seat is the same vendor as the implementing agent; the review prompt demanded reinforced adversariality and the oracle independently re-ran the decisive gates itself. |
| Repository base | working tree over `b59a1c2a1454a83164dfb4d5640c6b005154d1ee` |
| Reviewed range | local safeguard snapshots `cbf99ee0` → `07698f86` (275 files, +992/−34950); disk↔snapshot identity confirmed by the oracle via `git hash-object` on the key files |
| Verdict | `APPROVE` |
| Confidence | `alta` |
| Required changes | `NONE` |
| Owner/port contact | none; port 1338 and served owner untouched |
| Private benchmark access | none; no `operator-only`/`runner-results` content opened |
| Repository mutation by reviewer | none |
| Pre/post status-shape SHA-256 | `cf76f03fc9ee40c410038ae61b0704c9550d1b4f79997995b8de0c86dcc24c30` (identical before and after) |

Machine-local absolute paths in the oracle's verbatim evidence below are replaced with
`<repo-root>`/`<name>` placeholders; the redaction changes no finding (checkpoint-26 precedent).

## Verdict contract (verbatim, redacted)

```text
VERDICT: APPROVE
CONFIDENCE: alta
EVIDENCE:
- <repo-root>/scripts/m1nd10_candidate_source_guard.py:101-103 — casefold real de parts/name/suffix; :36-73 sets já armazenados casefolded (`.ds_store` em minúsculas confirma); RC1 cumprido conforme a letra do veredito anterior.
- scripts/m1nd10_candidate_source_guard.py:110-119 — `credential_file` (.env/.env.*, .npmrc/.pypirc/.netrc/.git-credentials, credentials[.toml] restrito a .cargo/.aws) e `private_key_material` (.ssh em parts, sufixos *_rsa/_dsa/_ecdsa/_ed25519, .p8/.der/.jks/.keystore + os quatro originais); RC2 cumprido, inclusive mais amplo que o exigido (_dsa/_ecdsa além de *_rsa/*_ed25519).
- scripts/m1nd10_candidate_source_guard.py:57-71,122-123 — `opaque_archive` com exatamente as 11 extensões do RC3 (.tar.gz coberto via sufixo .gz — testado em tests:204).
- scripts/m1nd10_candidate_source_guard.py:80-84 — PERSONAL_PATH_PATTERN em bytes, genérico por classe; verifiquei que `<` fora das classes de caracteres torna os placeholders imunes por construção, e que o regex NÃO casa o próprio fonte do guard (auto-consistência do content-gate).
- scripts/m1nd10_candidate_source_guard.py:323-335,350-361 — survivors = entries menos rejeições de path/metadata ANTES do scan de conteúdo: confirmo mecanicamente que operator-only nunca é aberto pelo content-gate; :288-302,305-320 falha de leitura → `unreadable_candidate_content` fail-closed.
- scripts/m1nd10_candidate_source_guard.py:270-282 — exceção C6 exige path exato E SHA-256 exato; rodei `shasum -a 256 docs/M1ND-10-PRD.md` = 00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38 (bate com a constante) e o PRD contém exatamente 3 ocorrências — a exceção é load-bearing e mínima.
- tests/test_m1nd10_candidate_source_guard.py:168-215 (tabela adversarial com reason exato por classe e variantes de caixa), :217-278 (content-gate em git real, worktree E exact-commit, incluindo binário e placeholder — payloads montados por concatenação para o próprio teste não se autoincriminar), :280-365 (C6 dos dois lados, autocrlf pinado), :149-166 (force-add de variante de caixa em exact-commit). RC5 cumprido.
- Diff cbf99ee→07698f8 de tests/test_m1nd10_ci_security_contract.py — tripwire semântico novo (casefold/credential_file/opaque_archive/personal_path_content/FROZEN_PRD_SHA256 + digest literal); exec() em :248 é pré-existente, roda código extraído do próprio workflow do repo sob mocks — risco LOW aceito, concordo.
- .github/workflows/ci.yml:143-150 e release.yml:80-89 — guard invocado contra `${GITHUB_SHA}` inalterado; o content-gate viaja dentro de `inspect_candidate`, logo os workflows herdam o gate sem edição. Gitleaks pinado (ff98106e…/8.30.1) confirmado.
- docs/proofs/m1nd10-candidate-source-boundary-askgod-review-20260719.md — li o veredito integral e mapeei os 5 required changes um a um contra o código: todos implementados.
- docs/M1ND-10-PUBLIC-PATH-MIGRATION-PLAN-20260719.md + docs/proofs/m1nd10-public-path-migration-ratification-20260720.md — cadeia plano→ratificação nomeia exatamente as duas decisões reservadas ao dono (retirada C1 e exceção C6); nada além disso foi autorizado.
- Diffs de scrub lidos: docs/proofs/m1nd10-g2-askgod-preflight-20260718.md (`<repo-root>` + nota de redação datada, nenhum resultado alterado), m1nd-mcp/src/session.rs e internal_tests/hall_brains_listing.rs (placeholders preservam a semântica dos asserts de basename), npm/test/cli.test.js (nota: o antigo `/Users/<name>/` CASARIA no regex novo — o scrub era necessário, não cosmético), scripts/benchmark/bug_hunt_round.py.
- tests/test_m1nd10_g6_held_out_v2_corpus.py:325-355 — a emenda de contrato (item 7); arquivo está gitignored (.gitignore:92) e por isso AUSENTE dos dois snapshots — a emenda NÃO está no diff range e não tem histórico git.
- VALIDAÇÃO PRÓPRIA (rodada por mim, sem mutação do workspace): guard --worktree-projection = PASS/PROVEN, 1169 paths, 0 violações (bate com o dossiê); 22 testes guard+ci-contract OK; o teste G6 emendado OK isolado; sha do UML 8a8a5fe9… confirmado; varredura própria da projeção: única ocorrência literal de path pessoal é o PRD (3, ratificadas), zero variantes de caixa, ocorrências de `/private/tmp/<...>` são fixtures neutras sem identidade.
- Identidade árvore↔snapshot: hash-object do disco == blob de 07698f8 para os 8 arquivos-chave; range 275 files +992/−34950 confirmado.

RATIONALE: Os 5 required changes do veredito CHANGE anterior estão implementados, testados adversarialmente e reproduzidos por mim nesta máquina — não aceitei nenhum número do dossiê sem rodar. A arquitetura elogiada foi preservada (commit exato, projeção não-mutante, metadata denial, workflows pinados) e a política deixou de ser fail-open nas classes enumeradas: caixa, credenciais, chaves, arquivos opacos e conteúdo público agora têm reason fechado e teste com reason exato. A exceção C6 é o desenho correto — digest-pinned, dois lados testados, morta por 1 byte, ratificada pelo dono, não é allowlist. Sobre o item 7 (emenda do contrato G6): CONFIRMO a decisão do implementador. O contrato antigo ("todo artefato formal visível ao git") é matematicamente incompatível com a boundary — um corpus operator-only untracked e não-ignorado entraria em toda projeção `git add -A` e o guard reprovaria para sempre; o contrato novo codifica a arquitetura de duas camadas na direção segura contra vazamento (público não-ignorável, privado obrigatoriamente ignorado) e endereça diretamente o risco de drift gitignore↔guard que o próprio Fugu listou. A ressalva: essa emenda vive num arquivo gitignored, sem histórico e fora do CI — é enforcement operator-local por design do corpus cego, aceitável, mas registrado abaixo. A wording do plano ("full history remains in private Git history") é o único ponto factualmente errado que encontrei — a história é pública — e vai como risco, não como bloqueio, porque o RC4 exigia gate de árvore-de-candidato e é isso que a boundary define.

REQUIRED_CHANGES: NONE

RISKS_MISSED:
1. Exposição histórica pública já consumada: b59a1c2 está em origin/main (github.com/maxkle1nz/m1nd) com 466 arquivos de docs/benchmarks/ trackeados — os paths pessoais dos 246 arquivos "retirados" permanecem publicados no histórico git público. A retirada protege apenas candidatos futuros. O plano ratificado chamava isso de "private Git history", o que é factualmente incorreto e pode dar ao dono uma falsa sensação de despublicação; a decisão (aceitar vs. reescrever histórico) é do dono e não foi posta diante dele nesses termos.
2. O PERSONAL_PATH_PATTERN é mais estreito que a doutrina que o justifica (AGENTS.md § no-leak): exige barra final nas variantes macOS/Linux (`/Users/<name>` em fim de linha escapa), é case-sensitive na variante Windows minúscula, e não cobre `/private/tmp/<...>` nem home-absolutos com `~` — classes que o próprio AGENTS.md proíbe. Hoje nenhum caso vivo existe na árvore (verifiquei), mas o gate é menor que a lei.
3. OPERATOR_SOURCE_PATHS (:99) permanece exact-match case-sensitive — uma cópia case-variant de um label-builder passa a path-policy; dobra no risco closed-list admitido, porém o comentário do guard (:33-35) promete imunidade de caixa que não alcança essa classe.
4. Lista de arquivos opacos omite containers relevantes para ESTE repo: `.whl`, `.crate`, `.zst`/`.tar.zst`, e zips disfarçados (.docx/.xlsx/.epub); `*.env` com basename não-oculto (`prod.env`) também passa.
5. A emenda do contrato G6 e a exceção C6 criam manutenção acoplada fora do CI: o teste emendado é gitignored (irrevisável por diff, nunca roda em CI), e qualquer emenda legítima futura do PRD exige atualizar guard + testes + nova ratificação em cadeia — fragilidade deliberada e ratificada, mas é bom o guardião sabê-la nomeada.
6. Não re-rodei cargo/npm (validei os scrubs C2/C5 por inspeção do diff — apenas comentários e literais de teste com semântica preservada); os números cargo/npm do dossiê ficam como claim do implementador, não como evidência minha.
```

## Consequence

All five required changes of the checkpoint-26 `CHANGE` verdict are implemented, adversarially
tested, and independently reproduced. The candidate-source boundary returns to `LOCAL_PROVEN` for
this exact source state. Risks 1-5 are registered as named follow-ups (risk 1 carries an owner
decision: accept public history or rewrite it — a separate ceremony, never part of this cut).
Candidate freeze is unblocked; every receipt must still bind to the frozen candidate digest.
