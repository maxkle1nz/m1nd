# M1ND VOICE — a voz visível do m1nd

> Spec de design da interface de texto (`human_view`) que agentes renderizam na
> conversa quando o m1nd contribuiu estruturalmente numa missão.
> Assento: Fable artista, ordem expressa do dono, 2026-07-12.
> Lei regente: `ASKGOD-VERDICT-HUMAN-VIEW.md` (as 10 emendas). Matéria-prima:
> `docs/uml/spine-north.md` + strings verbatim garimpadas de `server.rs` e
> `session.rs` do repo real (read-only).
> Todos os cartões deste documento passaram por gate mecânico: ≤80 colunas,
> skin ASCII 1:1 em largura, goteira fixa (script `width_check.py`, ALL GREEN).

---

## 0. As leis que este design obedece (mapa de conformidade com o veredito)

| # | Emenda do oráculo | Onde aterrissa aqui |
|---|---|---|
| 1 | campo = `human_view` | §9 (wire shape) |
| 2 | Budget Law: ≤4 linhas, ≤80 chars/linha, teto de chars, battery re-run | §3 (anatomia), §9 (custo) |
| 3 | sob `caller_root_mismatch` o cartão É o aviso ou se omite | §2 estado S3, §5 |
| 4 | `needs_ingest` é cartão honesto legítimo | §2 estado S4, §5 |
| 5 | linha de honestidade REUSA strings exatas de `honest_gaps` | §3 (lei do verbatim) |
| 6 | cadência como default NEGATIVO nas instructions | §2 (anti-poluição) |
| 7 | glifo é decisão do DONO; fallback ASCII; âncora = palavra "m1nd" | §7 (candidatos) |
| 8 | só fatos medidos (G1) — nunca adjetivo não calibrado | §3 (léxico), todo cartão |
| 9 | ritual de escada (§C11) | fora do escopo do artista; anotado p/ implementador |
| 10 | doc-gate no mesmo PR | §9 (checklist) |

Estética da casa: calma, humana, papel-washi — nunca HUD-neon, nunca terminal-hacker.

---

## 1. A tese — uma LOMBADA, não uma caixa

A ideia inteira do design cabe numa frase: **o cartão do m1nd é uma lombada
(spine), não uma caixa.**

Três razões, em ordem de peso:

**1. A marca já está no nome.** "m1nd" com o dígito no meio JÁ é um logotipo —
puro ASCII, risco de renderização zero, inconfundível em qualquer screenshot.
O traço vertical `│` ao lado dele ecoa o "1": a marca e a estrutura são o mesmo
gesto. A palavra pendura na margem como a assinatura de um impressor (colofão);
a coluna de `│` desce carregando a continuação. Quem vê uma coluna de barras
sob a palavra "m1nd" numa thumbnail reconhece o produto sem ler uma letra.

**2. Uma caixa é estática; uma lombada respira.** Caixa fechada (`┌───┐`) tem
borda direita — e borda direita é exatamente o que quebra quando o conteúdo
varia, quando a fonte muda, quando um char ambíguo alarga. A lombada não tem
lado direito: cresce de 1 linha (sussurro) a 14 (modo profundo) sem redesenhar
nada e sem NUNCA desalinhar. O requisito "não seja estático" do dono está
resolvido na geometria, não em lógica.

**3. O produto já se chama assim.** O doc canônico do pacote norte é "The
Spine" (`spine-north.md`). A voz visível usa a mesma anatomia da arquitetura:
uma espinha que sustenta fatos honestos. Coerência de marca de graça.

O tom: o cartão fala como um lavrador de cartório calmo — só registra fatos
medidos, uma linha por fato, e quanto melhor o estado do mundo, MENOS ele fala
(estado limpo = 1 linha). A urgência aparece como conteúdo (o sino), nunca como
ornamento (sem cores berrantes, sem `!!!`, sem emoji).

---

## 2. Gramática de estados — a forma segue o estado

O cartão tem **3 degraus de profundidade** e **6 estados**. Um degrau é
"quanto"; um estado é "o quê".

### Os degraus (a escada anti-estática)

| Degrau | Nome | Tamanho | Quando |
|---|---|---|---|
| R0 | sussurro | 1 linha | estado limpo — nada exige o humano |
| R1 | cartão | 2–4 linhas | há UM sinal (sino, coerência, aviso, ingest) |
| R2 | profundo | ≤14 linhas | o humano pediu, ou momento de pouso (ver abaixo) |

R0/R1 nascem MONTADOS no servidor (campo `human_view`, emenda do oráculo).
R2 é renderizado pelo AGENTE a partir dos campos estruturados do pacote
(`landing_bell`, missões, recibos, mapas) — MESMA gramática, mesma goteira; o
degrau profundo não pesa no orçamento do pacote porque não viaja no wire.

### Os estados

| # | Estado | Forma | Gatilho |
|---|---|---|---|
| S0 | orientação limpa | R0 (1 linha) | primeiro `north` estrutural da sessão, full trust, sem sinais |
| S1 | sino tocando | R1 (2 linhas) | `landing_bell.merge_wait > 0` — recibos aguardam carimbo humano |
| S2 | aviso de coerência | R1 (até 4) | `skeleton_coherence.status == "mismatch"` ou bloco stale |
| S3 | mismatch de repo | R1 (aviso puro) | `reception.match == "caller_root_mismatch"` — o cartão É o aviso, SEM estatísticas (elas descreveriam o cérebro errado) |
| S4 | needs_ingest | R1 (até 3) | grafo vazio/desvinculado — cartão honesto de "não conheço este repo" |
| S5 | modo profundo | R2 | pedido humano ("what's the bell?", "mostra o m1nd") OU momento de pouso (humano prestes a carimbar) OU primeiro orient do dia com sino tocando |

Prioridade quando 2+ sinais coexistem (o cartão compacto mostra o topo; o
profundo mostra todos): **mismatch > needs_ingest > trust degradado > sino >
coerência > gap de memória.** Aviso vence chamado; chamado vence sinal.

### Anti-poluição — o default é NÃO aparecer

Regra escrita como default NEGATIVO (vai verbatim para M1ND_INSTRUCTIONS e
para as 3 skills, emenda 6):

> **Do NOT render the m1nd card unless ALL three hold:** (a) m1nd contributed
> STRUCTURALLY to this beat; (b) the card's state signature differs from the
> last card shown in this session; (c) the previous assistant message did not
> carry a card. Never render two identical cards in one session. Never render
> in consecutive messages. When in doubt, stay silent — silence is the honest
> card.

"Estruturalmente" significa pelo menos um: focus_nodes/anchors do `north`
moldaram o plano declarado; uma memória do pacote foi USADA na resposta; um
estado-sinal está presente (S1–S4 ou trust degradado); um evento de missão
aconteceu neste beat (verify/close/pouso).

`state_sig` (chave mecânica do anti-repetição, servida no campo): tupla
`trust_mode | merge_wait | coherence | reception/needs`. Mudou a assinatura →
o cartão pode voltar. Não mudou → silêncio, mesmo que o agente chame `north`
de novo.

Momentos naturais de aparição (e os únicos): primeiro orient da sessão; sino
tocou ou mudou de contagem; trust/coerência mudou de estado; missão pousou
(sino diminuiu); fechamento de sessão em que m1nd foi estrutural. NUNCA no
meio de narração de implementação; NUNCA em beats de conversa pura; sob S3,
uma vez — e repete apenas se um WRITE estiver prestes a acontecer (writes sob
mismatch são proibidos por doutrina, o cartão relembra na hora certa).

---

## 3. Anatomia do cartão (R0/R1 — o que o servidor monta)

```
m1nd │ <L1 identidade — fatos vitais medidos>
     │ <L2 sinal — string honest_gaps VERBATIM, embrulhada se preciso>
     │   <L2 continuação de embrulho, indentada +2>
     │ next: <L3 next_move VERBATIM — inteiro ou nada>
```

**L1 — identidade.** Formato: `m1nd │ <trust> · <N> nodes · <M> memories ·
<K> maps ratified`. Cada segmento vem de um CAMPO medido (`trust_mode`,
contagem de nós, `memory_exists`, blocos ratificados). Segmento sem valor é
OMITIDO, nunca zerado como enfeite (espelha a lei do `landing_bell`: ausente,
não nulo). Zero só aparece quando o zero É a mensagem (`0 nodes` em S4).

**L2 — o sinal.** A lei do verbatim (emenda 5): esta linha REUSA a string
exata já composta em `honest_gaps` — nunca uma segunda redação do mesmo fato.
Strings reais garimpadas do servidor que este slot carrega:

- sino: `3 mission(s) in merge_wait await the human landing — the tray is the door`
- coerência: `Skeleton coherence sickness: serving brain expects slug ...`
- grafo vazio: `The graph is empty or unbound — no codebase context is available until ingest runs.`
- foco vazio: `No focus nodes activated for this task — ...`

**L3 — o próximo passo.** `next: ` + string `next_move` verbatim. Regra
inteiro-ou-nada: se a string verbatim não cabe no orçamento restante, a linha
CAI (o agente tem `next_move` no pacote e vai agir de qualquer jeito) — nunca
parafrasear para caber.

**Embrulho e queda.** Linha >80 → quebra em fronteira de palavra, continuação
indentada +2 dentro da coluna de conteúdo. Estourou 4 linhas → derrubar na
ordem: L3 primeiro, depois embrulho de L2 vira reticência `…` no fim da última
linha permitida. L1 e a primeira linha do sinal NUNCA caem — exceto em S3,
onde L1 é SUBSTITUÍDA pelo aviso (estatística nenhuma sob mismatch).

**Léxico.** Palavras de doutrina permitidas porque JÁ são o vocabulário do
produto: bell, tray, stamp, landing, door, brain, map. Proibido por G1: melhor,
rápido, inteligente, poderoso, qualquer adjetivo não calibrado, qualquer claim
de benefício. Números exatamente como o pacote formata (`9,024` — o cartão
nunca reformata fato). A palavra "m1nd" é sempre minúscula, inclusive em
começo de frase — é a constante da marca.

**Idioma.** O cartão nasce em INGLÊS no servidor. O agente traduz o CONTEÚDO
para a língua da conversa mantendo intocados: a geometria (goteira, coluna),
o wordmark, ids, hashes, nomes de tools e tokens de estado (`merge_wait`,
`needs_ingest`). Amostra pt-BR em §5.

---

## 4. As quatro variantes — lado a lado, para o olho do dono

Mesmos dois estados (S0 limpo, S1 sino), mesmos dados reais, quatro
identidades. Cada uma com skin unicode e fallback ASCII 7-bit (mapa 1:1 de
largura estável — §6).

### V1 — SPINE (a lombada) ← minha recomendada

```
m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified
```
```
m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door
```
ASCII:
```
m1nd | full trust . 9,024 nodes . 30 memories . 4 maps ratified
     | 3 mission(s) in merge_wait await the human landing - the tray is the door
```

Prós: wordmark pendurado na margem = assinatura própria (nenhuma outra tool
formata assim); `│` ecoa o "1"; sem borda direita = cresce sem quebrar; goteira
constante na coluna 6 dá ritmo de colofão; degrau profundo é a MESMA forma,
só mais longa. Contras: 7 colunas de prefixo (conteúdo máximo 73); o `│`
sozinho não é proprietário (é a composição que é).

### V2 — ARCO (cantos suaves)

```
╭ m1nd · full trust · 9,024 nodes · 30 memories · 4 maps ratified
```
```
╭ m1nd · full trust · 9,024 nodes · 30 memories · 4 maps ratified
╰ 3 mission(s) in merge_wait await the human landing — the tray is the door
```
Com três linhas o meio ganha `│`:
```
╭ m1nd · full trust · 9,024 nodes · 30 memories · 4 maps ratified
│ 3 mission(s) in merge_wait await the human landing — the tray is the door
╰ next: open the tray
```
ASCII (╭→/ ╰→\ │→|):
```
/ m1nd . full trust . 9,024 nodes . 30 memories . 4 maps ratified
\ 3 mission(s) in merge_wait await the human landing - the tray is the door
```

Prós: o mais "caligráfico" — o arco abre e fecha um pensamento; prefixo de só
2 colunas (conteúdo 78); fallback `/ | \` é bonito por acidente. Contras:
cartão de 1 linha fica assimétrico (um `╭` que nunca fecha); a última linha
muda de prefixo conforme o cartão cresce (mais lógica de montagem); cantos
lembram levemente "caixa de TUI", puxando para ferramenta e não para voz.

### V3 — SELO (glifo + indentação, sem moldura)

```
∴ m1nd — full trust · 9,024 nodes · 30 memories · 4 maps ratified
```
```
∴ m1nd — full trust · 9,024 nodes · 30 memories · 4 maps ratified
  3 mission(s) in merge_wait await the human landing — the tray is the door
```
ASCII (∴→* — único mapeamento sem par 1:1 semântico):
```
* m1nd - full trust . 9,024 nodes . 30 memories . 4 maps ratified
  3 mission(s) in merge_wait await the human landing - the tray is the door
```

Prós: o mais leve em tinta; `∴` ("portanto") tem significado perfeito para um
motor de inferência — "dado o grafo, portanto". Contras: `∴` é classe
East-Asian-Ambiguous (vira largo em terminal CJK), fonte bitmap pode não tê-lo,
e o fallback `*` degrada o selo a bullet genérico; sem goteira, as linhas de
continuação flutuam — em cartão de 4+ linhas a coesão se perde.

### V4 — COLOFÃO (mínimo absoluto, zero char de moldura)

```
m1nd — full trust · 9,024 nodes · 30 memories · 4 maps ratified
```
```
m1nd — full trust · 9,024 nodes · 30 memories · 4 maps ratified
       3 mission(s) in merge_wait await the human landing — the tray is the door
```
ASCII:
```
m1nd - full trust . 9,024 nodes . 30 memories . 4 maps ratified
       3 mission(s) in merge_wait await the human landing - the tray is the door
```

Prós: risco de renderização literalmente zero (um em dash é o único não-ASCII);
humildade máxima. Contras: NÃO marca — parece linha de log de qualquer CLI; a
indentação órfã (7 espaços sem goteira) colapsa em superfícies que não
preservam espaço; modo profundo vira um bloco de texto sem espinha. Falha o
requisito "icônico" do pedido.

---

## 5. Os seis estados na variante recomendada (SPINE), dados reais

Dados verdadeiros do dia: full_trust, 9.024 nós, 30 memórias, 4 mapas
ratificados, sino com 3 recibos, missão `msn_e2c93413069d` em merge_wait com
441 testes verdes, recibo `sha256:5b58d701`.

**S0 — orientação limpa (primeira do dia).** Uma linha. Quanto melhor o
mundo, menor a voz:
```
m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified
```

**S1 — sino tocando.** O estado mais importante: um chamado ao humano. A
linha 2 é a string do servidor VERBATIM (73 chars — cabe exato):
```
m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door
```

**S2 — aviso de coerência.** String do servidor verbatim, embrulhada (slugs
ilustrativos); 4 linhas — o teto, e ele aguenta:
```
m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │ Skeleton coherence sickness: serving brain expects slug `spine-north`,
     │   but the SystemBlock store carries `spine-north-v0` — signal only;
     │   reads and writes remain available.
```

**S3 — mismatch de repo.** O cartão É o aviso (emenda 3). Nenhuma
estatística — ela descreveria o cérebro errado. L1 = string `honest` da
reception verbatim; `next:` = a chamada literal de `options[]`:
```
m1nd │ this graph does NOT cover your repo
     │ bound: /home/user/repo-alpha · yours: /home/user/repo-beta
     │ next: ingest project_root=/home/user/repo-beta
```

**S4 — needs_ingest.** Cartão honesto legítimo (emenda 4). O gap verbatim já
carrega o reparo; `next_move` verbatim não coube em 4 linhas → caiu inteiro
(lei inteiro-ou-nada):
```
m1nd │ needs_ingest · 0 nodes
     │ The graph is empty or unbound — no codebase context is available
     │   until ingest runs.
```

**S5 — modo profundo, sino** (agente renderiza dos campos; pedido humano ou
momento de pouso). A mesma lombada, esticada; respiro = linha de goteira vazia:
```
m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door
     │
     │   msn_e2c93413069d · merge_wait · 441 tests green
     │   receipt sha256:5b58d701 — awaiting your stamp
     │   (the tray lists the other 2)
     │
     │ next: open the tray
```

**S5b — modo profundo, mapa em miniatura** (o humano pediu "que mapas você
tem?"; slugs reais do repo):
```
m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │
     │ ratified maps · 4
     │   spine-north        ok
     │   medulla            ok
     │   mission-control    ok
     │   routing-reception  ok
     │
     │ next: Call `surgical_context` on the top focus node to ground the task
     │   before editing.
```

**Amostra de tradução (dever do agente — geometria intocada, conteúdo na
língua da conversa, ids/estados preservados):**
```
m1nd │ confiança plena · 9,024 nós · 30 memórias · 4 mapas ratificados
     │ 3 missões em merge_wait aguardam o pouso humano — a bandeja é a porta
```

---

## 6. Paleta de chars — pequena de propósito

A paleta inteira do cartão são **cinco chars não-ASCII**. Só.

| Char | Código | Papel | Fallback 1:1 |
|---|---|---|---|
| `│` | U+2502 | a lombada (goteira) | `\|` |
| `╭` | U+256D | só na variante ARCO | `/` |
| `╰` | U+2570 | só na variante ARCO | `\` |
| `·` | U+00B7 | separador de campos | `.` |
| `—` | U+2014 | separador de cláusula (vem nas strings do servidor) | `-` |

**A lei da largura.** Os box-drawing e o middle dot são classe
East-Asian-Ambiguous — em terminal CJK configurado ambiguous-wide eles alargam.
Por isso o design tem DUAS peles e UMA geometria: o mapa ASCII acima é 1:1 em
colunas (provado no gate — cada linha ASCII tem exatamente a largura da
unicode), então trocar de pele nunca move nada. Host que detecta superfície
burra ou CJK-wide aplica o mapa e pronto.

**Nunca, em nenhuma variante:**

- **Emoji — nenhum, nem 🔔 para o sino, nem ⛩.** Largura dupla/instável,
  apresentação VS15/VS16 imprevisível, cor forçada quebra a calma e faz cada
  plataforma tirar um screenshot diferente (mata a consistência de marca).
- **Linha pesada ou dupla** `┃ ║ ╔ ═ ╬` — voz de HUD-neon, banida pela lei da
  casa.
- **Blocos de sombra** `░ ▒ ▓ █` — textura terminal-hacker, banida.
- **Geométricos ambíguos na estrutura** `◆ ● ■ ▶ ★ ◈` — viram largos em CJK e
  entortam a espinha; se nem no conteúdo, melhor.
- **Borda direita — nunca.** Nenhum `┐ ┘`, nenhuma coluna de fechamento. É a
  primeira coisa que quebra com embrulho de linha e a marca registrada de
  "caixa de ferramenta velha".
- Braille de spinner, combining chars, ZWJ — jamais.

**Regras de alinhamento:** ≤80 colunas (lei do veredito), L1 mira ≤72
(conforto em pane estreita); goteira imóvel na coluna 6; embrulho indenta +2;
respiro (linha `     │` vazia) só no modo profundo; o cartão vive num fenced
code block no chat (monospace garantido) e inline em TUI.

---

## 7. O glifo da marca — candidatos para o DONO carimbar

A âncora da marca é a PALAVRA `m1nd` (lei do oráculo, emenda 7); glifo é
decoração opcional. Cinco candidatos, com a verdade de renderização de cada um:

| # | Glifo | A favor | Contra | Meu voto |
|---|---|---|---|---|
| 1 | **nenhum — o wordmark é o glifo** | o "1" já é o logo; ASCII puro, risco zero; disciplina G1 gosta | sem ícone pictórico p/ outros usos (site resolve isso fora do cartão) | **RECOMENDO** |
| 2 | **`│` U+2502 — a própria lombada** | já é estrutural; oráculo aprovou; significado honesto (a continuidade da memória, o "1" exteriorizado) | ubíquo em TUI — sozinho não é proprietário; é a COMPOSIÇÃO (wordmark pendurado + coluna) que é | **RECOMENDO (junto c/ 1)** |
| 3 | `∴` U+2234 (therefore) | significado perfeito p/ motor de inferência: "dado isto, portanto"; discreto | classe ambígua (largo em CJK); cobertura de fonte irregular; fallback ASCII não existe em 1:1 (`*` degrada a bullet) | só como selo de abertura do modo profundo, skin unicode; dispensável |
| 4 | `»` U+00BB | Latin-1 = estreito GARANTIDO até em CJK (único decorativo sem risco de largura); "adiante" | cheira a prompt de shell/citação; adiciona pressa, não calma | não |
| 5 | `⛩` U+26E9 | distintivo em thumbnail (única virtude, dita com honestidade) | vetado pelo oráculo como default: conotação religiosa/geográfica, cunhado de passagem (zero ocorrências no repo), vira emoji colorido em muitos renderers, largura instável — screenshots divergem por plataforma, o oposto do objetivo | **contra**; se o dono insistir: unicode-only, linha 1 only, VS15, cai na skin ASCII |

Proposta default: **wordmark + lombada (1+2). A marca é a palavra; a coluna é
a assinatura.** Nenhum char novo é cunhado; nada a ratificar além do que o
oráculo já aceitou.

---

## 8. Cor (nota curta)

- **No chat: nenhuma.** O cartão viaja em fenced code block, texto puro. A
  calma vem da tipografia e do espaço, não de tinta.
- **Em TUI (tray, CLI), opcional e discreto:** goteira `│` e wordmark em cinza
  suave (256-color 245 ou SGR dim); linha do sino em âmbar pastel (179); linha
  de mismatch em vermelho dessaturado (167). Mais nada. Nunca bold no cartão
  inteiro, nunca fundo pintado, nunca blink, nunca os neons 226/196.

---

## 9. Para o implementador (amarrando as emendas 1, 2, 3, 4, 9, 10)

Forma sugerida do campo (nasce MONTADO, goteira incluída — o agente só junta
com `\n` dentro de um code block, ou aplica o mapa ASCII 1:1 antes):

```json
"human_view": {
  "schema": "m1nd-human-view-v0",
  "state": "clean",
  "state_sig": "full_trust|bell:0|coh:ok|recv:match",
  "lines": [
    "m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified"
  ]
}
```

(`lines` carrega o cartão já montado em QUALQUER estado — no S1 entram as duas
linhas do sino, etc. O exemplo acima usa o estado limpo porque a linha do sino
tem 80 colunas exatas e não caberia entre aspas num JSON de exemplo ≤80.)

- Custo típico: 2 linhas ≈ 150 chars ≈ ~40 tokens; pior caso 4×80 + envelope
  ≈ ~120 tokens. Battery re-run (`north_packet_within_budget`) com o número
  novo REGISTRADO — emenda 2.
- Testes obrigatórios do veredito: forma sob `caller_root_mismatch` (cartão =
  aviso, zero estatística — emenda 3) e forma `needs_ingest` (emenda 4); mais
  o cap (≤4 linhas, ≤80 chars pós-embrulho) e o 1:1 do mapa ASCII.
- Reception é computada por ÚLTIMO no `handle_north` — o `human_view` tem que
  ser montado DEPOIS dela (ou remontado sob mismatch), nunca antes.
- A cadência negativa (§2) entra verbatim em M1ND_INSTRUCTIONS + 3 skills no
  MESMO PR (emendas 6 e 10); a escada vira emenda §C11 (emenda 9).
- O degrau R2 (profundo) é do agente: renderiza dos campos estruturados
  (`landing_bell`, tray, blocos) na MESMA gramática; nunca inventa fato.

---

## 10. Recomendação — honesta, um parágrafo

**SPINE (V1), com wordmark-como-glifo.** É a única variante que resolve os
quatro requisitos do pedido AO MESMO TEMPO: marca (o wordmark pendurado com a
coluna ecoando o "1" é uma assinatura que nenhuma outra tool tem — visível em
qualquer screenshot, até em thumbnail), não polui (estado limpo = UMA linha de
63 colunas; e o default é não aparecer), não é estática (a mesma geometria
estica de 1 a 14 linhas sem redesenho — a escada R0→R2 é o "quando preciso
mostre em profundidade" resolvido em estrutura), e é honesta por construção
(sem borda direita não há moldura para quebrar nem enfeite para mentir; cada
linha é um campo medido ou uma string do servidor verbatim). O ARCO é mais
bonito num frame isolado, mas paga lógica de montagem (última linha muda de
prefixo) e flerta com "caixa de TUI"; o SELO depende de um char frágil; o
COLOFÃO é humilde demais para marcar. A lombada é papel washi com uma única
linha de tinta — calma, útil, e inconfundivelmente m1nd.

---

*Gate mecânico desta spec: todas as linhas de cartão verificadas ≤80 colunas,
skin ASCII 1:1 em largura, goteira fixa col. 6 — `width_check.py`, ALL GREEN,
2026-07-12.*
