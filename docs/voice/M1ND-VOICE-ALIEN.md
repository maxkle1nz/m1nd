# M1ND VOICE — rodada 2: o polo ALIENÍGENA

> Segunda rodada do mesmo assento de artista (Fable), 2026-07-12.
> A dúvida do dono, verbatim: *"achei bem simples, e agora tô na dúvida — não
> sei se o simples é o que impacta melhor, ou se falta um toque ALIENÍGENA
> nisso."*
> Este doc explora o polo alienígena DE VERDADE, responde a dúvida com
> honestidade, e termina numa recomendação única.
> Leis intactas do veredito: ≤4 linhas no R1, ≤80 colunas, ~120 tokens pior
> caso, fato medido ou string verbatim, honestidade sob mismatch, wordmark
> "m1nd", fallback ASCII 1:1 provado, sem emoji. Companheiro da rodada 1:
> `M1ND-VOICE-DESIGN.md` (a SPINE e as leis de cadência valem aqui inteiras).
> Gate mecânico re-rodado: `width_check_alien.py`, ALL GREEN (classes de
> largura provadas via `unicodedata.east_asian_width` + ≤80 + ASCII 1:1).

---

## 1. O que "alienígena" significa aqui (e o que não significa)

Neon, HUD, terminal-hacker: banidos, continuam banidos. O alienígena que vale
é **estranheza estrutural** — quatro propriedades que nenhuma tool humana
comum tem, todas frias e calmas:

1. **Precisão de instrumento inumano.** Uma sonda não diz "tudo bem!" — diz
   o registro cru. A estranheza da telemetria é densidade posicional e valor
   exato, não ornamento.
2. **Notação em vez de prosa.** Símbolos que CARREGAM semântica exata:
   `⊢` (turnstile, "a evidência PROVA") para um sistema cuja alma é recibo;
   `∎` (QED) para pouso consumado; `≔` para definição. Notação de prova onde
   há prova — nunca como decoração.
3. **Assinatura não-social.** Sem frase de abertura, sem cortesia, sem
   emoji-sorriso. O cartão não conversa; ele REGISTRA. (A prosa verbatim do
   servidor é a exceção deliberada — a câmara humana.)
4. **Um comportamento que forma nenhuma teria:** a moldura que MUDA DE CORPO
   com o estado — a lombada que acorda.

**O achado técnico da rodada (provado por script):** os chars do polo
alienígena são MAIS estáveis que os da paleta calma. `unicodedata.east_asian_width`:

```
╷ U+2577 half-stem      N (narrow garantido)    │ U+2502 spine     A (ambíguo)
⊢ U+22A2 turnstile      N                       · U+00B7 middot    A
∎ U+220E QED            N                       — U+2014 em dash   A
∆ U+2206 increment      N                       ╭ ╰ arcos          A
∘ U+2218 ring           N                       ∴ therefore        A
∅ U+2205 empty set      N                       × multiply         A
≔ U+2254 colon-equals   N
⋮ U+22EE vert ellipsis  N
```

Classe N = nunca alarga nem em terminal CJK. A ressalva honesta que classe
nenhuma cobre: **presença na fonte**. Os math operators acima vivem em
DejaVu/Menlo/SF Mono/JetBrains/Cascadia/Fira; `╷` vive no mesmo bloco
box-drawing do `│`. Fonte bitmap mínima pode falhar qualquer um deles → a
pele ASCII 1:1 continua obrigatória e pronta.

## 2. Paleta expandida — cada char novo e o aluguel que paga

| Char | Código | EAW | Papel (o aluguel) | ASCII 1:1 |
|---|---|---|---|---|
| `╷` | U+2577 | N | célula de pulso calma (meia-haste) | `.` |
| `│` | U+2502 | A | célula de pulso ERGUIDA + goteira (já da casa) | `\|` |
| `⊢` | U+22A2 | N | sequente de prova: `<evidência> ⊢ <recibo>` — SÓ em linha de recibo | `>` |
| `∎` | U+220E | N | QED — SÓ em pouso consumado (missão landed) | `#` |
| `≔` | U+2254 | N | definição — SÓ na legenda do pulso (modo profundo) | `=` |
| `∘` | U+2218 | N | alfabeto B: estado limpo | `o` |
| `∆` | U+2206 | N | alfabeto B: sino (mudança que chama) | `^` |
| `∅` | U+2205 | N | alfabeto B: mismatch (não te contém) | `0` |
| `⋮` | U+22EE | N | "há mais" vertical no profundo (opcional, todas) | `:` |

Regra de ouro mantida: cada símbolo aparece SOMENTE onde seu significado é o
fato — turnstile sem prova é fantasia, e fantasia é o cyberpunk que a casa
baniu.

---

## 3. Variante A — TELEMETRIA (o quadro de instrumento)

**Conceito (2 linhas):** a linha de identidade vira registro de sonda — labels
curtos, valores CRUS como estão no pacote (`9024`, sem vírgula), fingerprint
do cérebro falante. A prosa verbatim do servidor fica como "voz de solo": a
segunda câmara, humana.

Limpo:
```
m1nd │ trust full · nodes 9024 · mem 30 · maps 4 · fp 3fa2c9
```

Sino:
```
m1nd │ trust full · nodes 9024 · mem 30 · maps 4 · fp 3fa2c9
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door
```

Profundo (o ⊢ aparece: evidência ⊢ recibo — notação exata do que o m1nd é):
```
m1nd │ trust full · nodes 9024 · mem 30 · maps 4 · fp 3fa2c9
     │
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door
     │
     │   msn_e2c93413069d · merge_wait
     │   441 tests green ⊢ receipt sha256:5b58d701 — awaiting your stamp
     │   (the tray lists the other 2)
     │
     │ next: open the tray
```

Pouso consumado (evento sino N→N−1; o ∎ fecha a missão como um QED):
```
m1nd │ msn_e2c93413069d landed · receipt sha256:5b58d701 ∎
```

Pele ASCII (sino):
```
m1nd | trust full . nodes 9024 . mem 30 . maps 4 . fp 3fa2c9
     | 3 mission(s) in merge_wait await the human landing - the tray is the door
```

**Prós:** os valores crus são MAIS honestos (byte-a-byte do pacote); `fp`
responde "qual cérebro está falando?" criptograficamente — telemetria real,
não figurino; `441 tests green ⊢ receipt sha256:…` é a frase mais m1nd que já
desenhei: a alma do produto (prova → recibo) em notação exata.
**Contras:** `fp` na linha 1 de TODO cartão é peso diário para valor raro
(pertence ao profundo); labels crus (`mem 30`) esfriam a leitura para não-devs.
**Envelhece?** O registro de instrumento envelhece BEM (não é moda, é função) —
mas o cartão inteiro neste tom derrapa para "log de máquina": o humano para de
ler. O ⊢ e o ∎, esses envelhecem como notação matemática: nunca.

---

## 4. Variante B — ALFABETO (a lombada que fala)

**Conceito (2 linhas):** um sistema de escrita m1nd de 6 letras — a goteira É
a letra do estado, repetida coluna abaixo: `∘` limpo, `∆` sino, `~` deriva,
`∅` mismatch, `·` needs_ingest, `∎` pousado. Lê-se o estado à distância, antes
de qualquer palavra.

Limpo:
```
∘ m1nd · trust full · nodes 9024 · mem 30 · maps 4
```

Sino:
```
∆ m1nd · trust full · nodes 9024 · mem 30 · maps 4
∆ 3 mission(s) in merge_wait await the human landing — the tray is the door
```

Profundo:
```
∆ m1nd · trust full · nodes 9024 · mem 30 · maps 4
∆
∆ 3 mission(s) in merge_wait await the human landing — the tray is the door
∆
∆   msn_e2c93413069d · merge_wait · 441 tests green
∆   receipt sha256:5b58d701 — awaiting your stamp
∆   (the tray lists the other 2)
∆
∆ next: open the tray
```

Pele ASCII (sino): `∆→^`
```
^ m1nd . trust full . nodes 9024 . mem 30 . maps 4
^ 3 mission(s) in merge_wait await the human landing - the tray is the door
```

**Prós:** o mais radicalmente "escrita própria" das três — estado legível em
thumbnail sem ler UMA palavra; grep por `∆ ` no transcript acha todo chamado
da história; margem uniforme é hipnótica no bom sentido.
**Contras (pesados):** exige aprendizado — seis letras que o humano tem que
memorizar para o cartão fazer sentido, e a semântica JÁ está nas palavras ao
lado (redundância que custa estranheza sem pagar informação); `∆` na margem
tem conotação parasita de diff/change para olho de dev; a pele ASCII (`^`,
`o`, `0`) degrada a caligrafia a sopa de chars.
**Envelhece?** MAL. Cada estado novo do produto quebra o alfabeto memorizado
(a 7ª letra ninguém decora); é a variante-manifesto: linda hoje, museu em um
ano. Registro com carinho e recomendo contra.

---

## 5. Variante C — PULSO (a lombada que acorda) ← a defendida

**Conceito (2 linhas):** bicameral. Linha 1 = wordmark + **pulso de 5
células** + os campos humanos da SPINE; linhas seguintes = a SPINE intacta.
Cada célula é um órgão vital com alarme REAL no pacote — meia-haste `╷` =
calmo, haste cheia `│` = chamando. Não se decodifica célula a célula no
compacto; lê-se como EXPRESSÃO: tudo baixo = calma; uma haste de pé = olhe.

Fileira fixa, para sempre (lei anti-equalizador):
`pulse ≔ trust · graph · focus · bell · coherence`
(trust ergue se ≠ full; graph se needs_ingest; focus se "No focus nodes
activated"; bell se merge_wait>0; coherence se skeleton mismatch/stale.
Sob reception mismatch o pulso INTEIRO some — mediria o cérebro errado.)

Limpo (cinco hastes baixas — a máquina respirando devagar):
```
m1nd ╷╷╷╷╷  full trust · 9,024 nodes · 30 memories · 4 maps ratified
```

Sino (a 4ª célula acorda — nada grita, uma haste fica de pé):
```
m1nd ╷╷╷│╷  full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door
```

Profundo (a legenda `≔` ensina a ler o pulso; o ⊢ assina o recibo):
```
m1nd ╷╷╷│╷  full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │
     │ pulse ≔ trust ╷ · graph ╷ · focus ╷ · bell │ · coherence ╷
     │
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door
     │
     │   msn_e2c93413069d · merge_wait
     │   441 tests green ⊢ receipt sha256:5b58d701 — awaiting your stamp
     │   (the tray lists the other 2)
     │
     │ next: open the tray
```

needs_ingest (célula graph ergue; resto da gramática = SPINE):
```
m1nd ╷│╷╷╷  needs_ingest · 0 nodes
     │ The graph is empty or unbound — no codebase context is available
     │   until ingest runs.
```

Pouso consumado (o pulso volta a baixar; ∎ fecha):
```
m1nd ╷╷╷╷╷  msn_e2c93413069d landed ∎ · receipt sha256:5b58d701
```

Pele ASCII (sino) — e repare que a semântica sobrevive: ponto baixo = calmo,
barra = de pé:
```
m1nd ...|.  full trust . 9,024 nodes . 30 memories . 4 maps ratified
     | 3 mission(s) in merge_wait await the human landing - the tray is the door
```
(No limpo a pele ASCII vira `m1nd .....` — cinco pontos, uma reticência
respirando. Bonito por acidente, de novo.)

**Prós:** é a "lombada que acorda" pedida no brief — o cartão MUDA DE CORPO
com o estado sem custar uma linha nem um token a mais (5 chars que já
existiam como espaço); gestalt instantânea (ninguém precisa decorar nada para
ver que uma haste está de pé); os órgãos de prova (`⊢`, `∎`) só falam onde há
prova; `╷` é narrow-garantido (classe N — provado); goteira das linhas 2+
alinha exatamente sob a célula 1 — a lombada NASCE do pulso; 100% compatível
com a SPINE (remova o pulso e ela volta — mesma geometria, mesmas leis, zero
retrabalho de cadência/orçamento).
**Contras:** as 5 células sem legenda são opacas no compacto (mitigado: a
legenda mora no profundo, e a gestalt não exige decodificação); `╷` é o char
de cobertura menos universal da casa (bloco box-drawing completo — fontes
mínimas podem falhar → pele ASCII pronta); tentação futura de adicionar
células viraria equalizador-neon conceitual — por isso a fileira é LEI fixa.
**Envelhece?** Bem, SE a fileira ficar congelada em 5. O pulso não é moda —
é um instrumento de leitura (o mesmo motivo pelo qual o eletrocardiograma
não envelhece). O risco real é disciplina, não estética.

---

## 6. Lado a lado — o olho decide (estado sino, dados reais)

```
SPINE (rodada 1, o polo calmo):

m1nd │ full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door

A TELEMETRIA:

m1nd │ trust full · nodes 9024 · mem 30 · maps 4 · fp 3fa2c9
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door

B ALFABETO:

∆ m1nd · trust full · nodes 9024 · mem 30 · maps 4
∆ 3 mission(s) in merge_wait await the human landing — the tray is the door

C PULSO:

m1nd ╷╷╷│╷  full trust · 9,024 nodes · 30 memories · 4 maps ratified
     │ 3 mission(s) in merge_wait await the human landing — the tray is the door
```

---

## 7. A resposta honesta à dúvida do dono

**"O simples impacta melhor, ou falta alienígena?" — Falta UM órgão
alienígena; não falta um corpo alienígena.**

A SPINE da rodada 1 acertou as leis, a calma e a honestidade — mas a linha de
identidade dela, sozinha, poderia ser de qualquer CLI bem-educada. O dono
sentiu isso; a intuição está certa.

Só que o polo oposto inteiro também erra: um cartão TODO instrumento
(TELEMETRIA pura) esfria a voz até o humano parar de ler, e um alfabeto
próprio (B) cobra mensalidade de memorização por uma informação que as
palavras ao lado já dão. Fantasia total é o cyberpunk de novo, só que de
gelo.

O alienígena que marca é o das sondas de verdade: **99% silêncio, 1% número
exato.** Estranheza como órgão preciso num corpo calmo:

1. **O pulso** — a lombada acorda quando o mundo exige o humano. Forma que
   nenhuma tool tem, custo zero em linhas/tokens, gestalt imediata.
2. **O sequente `⊢`** — só na linha de recibo: `441 tests green ⊢ receipt
   sha256:5b58d701`. A alma do produto (evidência prova recibo) em notação
   exata.
3. **O `∎`** — só no pouso consumado. O QED de uma missão que aterrissou.

Três órgãos, todos com significado medido, zero decoração. E — ironia provada
por script — os chars alienígenas são narrow-garantidos (classe N), mais
estáveis que a própria paleta calma.

## 8. Recomendação — UMA

**PULSO (variante C), que é a SPINE com os três órgãos implantados — não uma
ruptura, uma evolução.** Se o dono olhar e preferir o silêncio puro, a SPINE
continua válida e o caminho de volta é remover os órgãos sem tocar em lei
nenhuma (mesma geometria, mesma cadência, mesmo orçamento). Mas a minha
defesa de artista é o PULSO: é a única das sete formas (4 da rodada 1 + 3
desta) em que o cartão parece o que o m1nd É — um instrumento vivo que
respira baixinho, ergue uma haste quando precisa de você, e assina as provas
com notação de prova.

---

*Gate mecânico: classes EAW provadas via `unicodedata.east_asian_width`;
todos os cartões ≤80 colunas; pele ASCII 1:1 em largura (mapa estendido
`╷→. ⊢→> ∎→# ≔→= ∘→o ∆→^ ∅→0 ⋮→:`) — `width_check_alien.py`, ALL GREEN,
2026-07-12.*
