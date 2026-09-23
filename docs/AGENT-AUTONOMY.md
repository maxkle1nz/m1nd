# Agent autonomy: automatic first workspace preparation

## Proven slice: stdio with an explicit launcher root

A new `m1nd-mcp` stdio process can prepare its first code graph automatically
when the launcher supplies both of these independent locations:

- `M1ND_WORKSPACE_ROOT`: the repository the launcher has granted to the process.
- `M1ND_RUNTIME_DIR`: a private writable directory for derived graph and cache
  state. It may be outside a read-only source tree.

Before opening the public MCP surface, the process canonicalizes the granted
workspace and requires the runtime directory itself to already exist. It
validates the runtime as a real directory owned by the effective UID without
group/other write permission, and checks each canonical ancestor up to the
filesystem root for directory type and absence of group/other write permission.
Read-only ancestors may belong to another UID (for example `/`). A missing,
relative, symlinked, source-nested, or otherwise unsafe runtime refuses before
any graph or source mutation. When a grant is present, graph, plasticity, and
registry destinations are forced to the canonical runtime; absolute CLI, env,
and JSON-config overrides are not retained as persistence destinations.

If its bound graph is empty, the process does not load or download the static
embedding model for zero nodes; the actor-backed ingest rebuilds the semantic
engine when nodes become available. It scans that exact root through the
existing actor/checkpoint path. The first real `north` or retrieval call can
therefore use the graph without a preceding `--birth` command or public
`ingest` call. A later call and a clean process restart reuse the persisted
graph instead of recreating it.

`launcher_workspace_root` in an explicit JSON server configuration is the
equivalent configuration-file contract. `M1ND_WORKSPACE_ROOT` fills it only
when the file does not already provide one.

## Authority boundary

The launcher's saved root is the only authority for this automatic effect.
Tool arguments, prompts, query text, `scope`, README content, the current
directory, editor-specific environment aliases, and request headers cannot
extend it. Automatic preparation does not create a `HumanOrigin`, invoke the
legacy birth ceremony, or lower the generic ingest authority floor.

A runtime with persisted identity is reusable only when that identity is
non-empty and every declared root canonicalizes to the exact launcher grant.
An additional foreign or unresolvable persisted root refuses startup with
`launcher_workspace_conflicts_with_bound_graph`; duplicate declarations of the
same exact root remain valid. The refusal happens before ingest and does not
replace, merge, or shrink the graph or its logical root set. This holds for
both populated and zero-node snapshots: emptiness alone does not authorize
reassignment. The `workspace_root` inferred from the new grant cannot validate
that same grant, while an unresolvable launcher grant itself refuses with
`launcher_workspace_unresolvable`. These startup refusals exit nonzero without
answering `initialize`.

A scan that produces no nodes is an error, not an empty success. That check is
inside the actor transaction: roots and graph state roll back before the
startup refusal, and the same runtime can prepare successfully after valid
source appears. Normal EOF after a successfully started stdio session remains
a successful exit.

Source files are inputs only. Graph snapshots, checkpoints, embeddings,
registry state, and other derived state remain under the canonical private
runtime directory.

The subprocess battery removes inherited graph, plasticity, read-only,
workspace-alias, home, and temporary-directory overrides before installing its
fixture-owned environment. It also rejects MCP `isError: true` on positive
calls and verifies the reported graph/plasticity paths stay under that private
runtime.

## Public npm CLI and process status

With the matching native build, `m1nd agent first-minute --repo <repo> --json`
also reaches automatic preparation: its existing launcher passes the selected
repository as `M1ND_WORKSPACE_ROOT` and creates an external private stdio runtime
when no served owner is found. No `--birth` or generic `ingest` call is needed.
An isolated real-binary probe retrieved a source symbol from a virgin fixture
and independently found that symbol in the persisted snapshot, with the source
tree unchanged.

The default isolated `agent` path now keeps that runtime in an external cache
instead of minting a new temporary directory for every command. Its identity is
the canonical repository root plus, for Git workspaces, the current branch and
HEAD revision. The digest selects the directory; a separate identity manifest
inside it repeats those fields and refuses mismatched, unlabelled populated, or
unreadable state rather than adopting, erasing, or overwriting it. Canonically
different worktrees therefore cannot share a snapshot merely because they have
the same Git common directory. `M1ND_AGENT_CACHE_DIR` selects the cache base;
otherwise the CLI uses `XDG_CACHE_HOME` when set and the user's normal cache
tree under `.cache` as fallback. The selected base is created before use, must
be a real non-symlink directory owned by the current Unix user, and may not be
writable by group or other users; its canonical path, rather than a lexical
alias such as `/var`, determines the runtime location. This protects the cache
against a separately privileged local user or a shared cache root. It does not
claim to isolate mutually hostile processes running with the same operating
system UID, which already have authority to alter that user's private files.

Attach/discovery still wins when a served owner covers the repository.
An explicit CLI `--runtime-dir` wins over a conflicting runtime argument in
`M1ND_MCP_ARGS`; without the CLI flag, either environmental syntax remains the
winner and the unused automatic cache is not created. Relative paths are resolved
from their respective caller contexts. `--shared-runtime` retains its distinct behavior. On the cached stdio
path the CLI closes stdin and waits for the owned child, then sends SIGTERM
if EOF does not stop it. `initialize` has a separate bounded 120-second cold
start budget (ordinary calls retain 30 seconds); the cooperative shutdown
budget is five seconds after EOF and up to 120 seconds after SIGTERM. It never
sends SIGKILL to a writer that may still be checkpointing, nor signals an
unrelated process. If streams remain open after that budget, the CLI reports
a bounded failure, leaves the cache owner proof in place for supervised recovery,
and does not claim the runtime reusable. Shutdown is complete only after the
child `close` event has drained its stdio streams. If startup or a command fails and teardown also fails,
the CLI reports both failures plus the captured native stderr; the teardown error
does not replace the primary cause. Once the owned child has exited, the CLI
releases its own lease even if that exit was not clean, so a crashed child cannot
strand a cache owner. If native
execution itself fails before a child starts (including `EACCES` or `ENOEXEC`),
the CLI reports a normalized spawn error and releases its lease because no child
can own the runtime.

Each isolated runtime also has a per-runtime Node ownership lease before the
identity manifest is created or validated and before the native process starts.
Concurrent commands for the same cache key serialize at that boundary, so a
contender never reads a partially published identity and only one native writer
is alive while its graph is loaded, queried, and checkpointed. The runtime
directory, owner directory, owner proof, and identity manifest must be real
non-symlink objects; on supported POSIX platforms file opens also use
`O_NOFOLLOW`. A pre-existing symlink refuses before the cache can read through
or write through it. The owner proves its random token before treating its lock
directory and live waiter markers as coordination state; a directory that was
already populated without an identity is still refused rather than adopted. The
owner releases only its own proof after its owned child has visibly closed (or
never spawned). A child that exits abnormally but emits `close` still releases
its proof; if shutdown fails without a witnessed child close, the proof remains
and contenders fail busy rather than risk two writers. Such a stranded proof
requires explicit owner-supervised recovery; it is never silently aged out.
A waiter that sees no release within its bounded `M1ND_AGENT_CACHE_OWNER_WAIT_MS`
budget returns an explicit cache-busy error. The default is 60,000 ms; a caller
may request an integer from 1,000 to 300,000 ms when a different bounded budget
is appropriate. It does not inspect PID liveness, age out the lease, remove runtime
state, appropriate ownership, or signal the current owner. An interrupted waiter can leave only its own inert waiter marker; that marker grants
no ownership and never makes the cache reusable. Distinct runtime keys have
distinct leases and remain independent.

When that automatic cache identity already exists, the new stdio process uses
the launcher's canonical grant as its caller identity and calls the existing
public `ingest {mode:"refresh", path:<exact root>}` door while it still holds the
same Node lease. Refresh completes before trust, orientation, or retrieval. It
does not create a second owner, change the cache key, infer authority from the
working tree, or weaken `replace`/`merge`; dirty Git and non-Git edits are
absorbed by the rescan even though branch and HEAD stay unchanged. A transport
error or application refusal stops the command with `ok:false`, exit 1, and an
honest `freshness` object carrying `isError`, `action`, `refused`, and `reason`.
Trust remains `not_evaluated`; unknown inventory is never summarized as fresh.
Cold caches are still prepared once by the launcher bootstrap and are not
immediately scanned a second time.

Correction to the 01i review claim: refresh refusals alone did not establish that
unknown inventory was never summarized as fresh. The public `am_i_stale` summary
still called an unknown-only result all-fresh; the 01j regression closes that
specific contradiction without manufacturing a baseline for unknown files.

The opt-in real-binary test `npm/test/agent-cli-cache-real.test.js` runs two
separate public CLI processes against one fixture, then a third against a second
root. It requires stable runtime identity and snapshot digest across the first
restart, independently reads the exact persisted root set, checks structured
search matches, proves the roots remain isolated, and compares source bytes and
permissions before and after. Additional real CLI invocations cover relative
runtime paths in both `--runtime-dir value` and `--runtime-dir=value` forms,
including the child's changed working directory. The ordinary npm battery
separately proves that a branch or HEAD revision change selects a different
cache identity.
The same battery also changes one non-Git fixture and one dirty Git fixture
without changing branch or HEAD, then requires a second public CLI process to
reuse the same runtime while the persisted snapshot replaces OLD with NEW. It
checks the exact root set and the refresh call's MCP/application status.

These real-binary cache probes require `M1ND_TEST_AGENT_CACHE_BINARY` and
`M1ND_TEST_EMBED_MODEL`; without both they are explicitly skipped rather than
claimed as ordinary npm evidence. `npm test` always runs the deterministic CLI,
spawn-failure, cache-symlink, and tarball-content checks, while a provisioned
native/model lane is required to claim the real-cache probes.

The focused real-binary concurrency battery
`npm/test/agent-cli-cache-concurrency-real.test.js` first holds the initial
identity file after its real exclusive open but before any JSON bytes are
written. It requires the second public CLI to reach the cache ownership waiter
while that barrier remains closed, releases the publisher while the contender
is waiting, and requires both commands to return structured source matches from
the same exact-root cache without a partial-manifest read. The battery also
holds the first runtime's real `initialize` response behind an explicit fixture
signal, forbids a second native owner during that interval, and uses a clean
third invocation to prove warm restart reuse of the non-empty snapshot.
Separate cases prove that populated-without-identity, malformed identity, and
foreign identity state remain byte-for-byte preserved on refusal; that a held
workspace does not block another workspace; and that timeout or interruption
of a losing waiter preserves the winner's owner proof and process.

Before choosing or writing an automatic cache, the launcher distinguishes a
non-Git tree, a branch with a resolved HEAD, detached HEAD, and a newly initialized
unborn branch. Git unavailability, timeout, metadata failure, or an ambiguous
branch/HEAD query refuses explicitly; it is never recorded as reusable non-Git or
detached identity, and an existing cache is left byte-for-byte unchanged. A
"not a git repository" diagnostic is not enough to classify a tree as non-Git:
local or ancestor `.git` markers, unreadable metadata, or explicit Git environment
hints make that failed probe ambiguous and therefore refuse automatic caching.

For `m1nd agent ...`, an envelope with `ok: false` now produces process exit code
1 after printing the envelope. Previously that application-level refusal could
exit 0. JSON consumers still receive the same refusal and recovery fields;
shell callers must handle nonzero status instead of interpreting it as success.
Successful envelopes retain exit 0. Startup errors also exit nonzero and report
their cause on stderr; they need not produce a JSON envelope. A successful
diagnostic or an empty search is not automatically a task error merely because
it contains no source matches. This change does not redefine the separate
`kickstart` command's status contract.

## Authenticated HTTP follow-up

The foreground loopback HTTP owner invokes the same actor-backed exact-root
preparation function before admitting requests. Authority, bearer security and
the socket listener are ready first, but discovery remains unpublished while
preparation runs. Only successful preparation publishes the listener's effective
address through the single-writer actor; a preparation failure therefore leaves
no attachable HTTP endpoint to withdraw.
The HTTP path does not add a second root policy: it passes the saved `McpConfig`
grant, bound session and project-brain registry to the function used by stdio.
Local bearer middleware is unchanged and still surrounds `/mcp` and every API route.
The stdio caller-root binding is deliberately outside that shared preparation
function. HTTP continues to resolve caller identity per MCP session/request: a
known session retains its identity when a later request omits the header, while
a brand-new headerless session remains unknown and a foreign caller remains
foreign after another caller succeeds.

`m1nd-mcp/tests/agent_autonomy_http_bootstrap.rs` is the real-binary transport
battery: it starts a temporary authenticated owner on a dynamically selected
loopback port; initializes an MCP HTTP session; requires an unauthenticated
request to return 401; retrieves a fixture symbol on the first tool call; checks
the persisted snapshot and complete root set independently; proves a sibling
root present only in the caller header and tool payload is not granted; and
requires a conflicting warm runtime to refuse without changing snapshot or
roots. Every runtime, registry, home, temp directory and credential is fixture
owned, and each child is stopped and waited.

A socket-free startup-sequence regression blocks preparation on a channel and
reads the real instance registry while that production seam is paused. It requires
no running endpoint until release, then the exact effective address. A second
case injects a preparation failure after actor start, requires no discoverable
owner, and completes actor shutdown cleanly. Neither case uses timing sleeps.

The two HTTP cases passed on a loopback-capable macOS runner, alongside the
stdio-bootstrap, capability-contract, legacy-birth, and router-authentication
regression batteries. The implementing sandbox's earlier
`EPERM` was an execution limitation, not a behavioral result. Real execution
then exposed an actor-start ordering defect; preparation now follows the last
pre-actor setup access, and the same cold-start probe passes. Search assertions
inspect actual results rather than a query echoed in an envelope. A separate
public-protocol probe also verified persisted roots/symbols, unchanged source
trees, unauthenticated 401, and graceful owner termination.

These checks used the optional static-embedding-disabled fallback lane; they
do not prove model-backed retrieval. They are macOS evidence, not cross-platform CI.

## Verificação de desempenho e frescor no próprio repositório

O resolvedor constrói um índice reverso de identidade local a cada chamada, em
vez de percorrer `id_to_node` novamente para cada referência. Identidades ausentes
ou com múltiplos aliases continuam sem resolução; desempates, tags e ownership
mantêm a mesma semântica. O índice não sobrevive à chamada e não altera a política
de refresh, os limites de autoridade ou os timeouts.

A regressão determinística mede visitas reais ao mapa, não um limite de tempo
sensível à máquina:

```bash
cargo test -p m1nd-ingest --lib reverse_identity_map_is_traversed_at_most_once_per_resolution_call
```

Na comparação local, o caso de 65 identidades passou de 8.320 visitas para 65.
O probe diferencial de 1.000–8.000 nós, com três execuções por tamanho no mesmo
perfil dev, preservou as decisões e os registros de ownership em todas as
execuções. O ganho de 20,2× no maior caso pertence **apenas ao resolvedor**;
não é um ganho medido de ponta a ponta.

Para verificar a experiência distribuída em um repositório grande, compile o
perfil `release` padrão e selecione explicitamente esse binário com `--binary`.
O build dev continuou excedendo o timeout público de inicialização. Na execução
local com `release`, cache vazio e o repositório inteiro, a primeira consulta levou
16,5 s; edição sem commit, reinício e contexto levaram entre 19,1 e 23,0 s.
Nenhum timeout foi aumentado. Isso não é uma garantia de latência para outras
máquinas, modelos ou repositórios.

O teste público verificou OLD→NEW nos identificadores persistidos do grafo,
remoção do identificador antigo, raízes exatas e continuidade entre processos no
mesmo cache. Ao remover a fixture, verificou também sua remoção do grafo.
Fonte, branch, HEAD, índice Git e entradas staged foram preservados.
Refresh continua sendo uma nova varredura, mesmo quando o resultado não muda.
Os testes usaram o fallback sem embeddings estáticos e não certificam a busca
com modelo. Esses resultados pertencem ao candidato isolado, não a uma instalação
ou publicação já realizada.

Se o verificador exigir preservação byte a byte do índice Git, use
`git -c diff.autoRefreshIndex=false diff --check`. O comando sem esse override
pode atualizar ctime/mtime do stat-cache, mesmo com `GIT_OPTIONAL_LOCKS=0`, sem
alterar conteúdo staged. Não mude a configuração global para contornar esse efeito.

## Honest limits of this slice

This slice proves direct stdio, persistent sequential and same-root concurrent
cache reuse through the isolated public npm CLI path, and the foreground
authenticated HTTP path with an explicit launcher grant. Concurrency is bounded
serialization around one native owner, not parallel graph execution. The key
prevents reuse across a Git branch or HEAD revision change. Warm automatic-cache
commands rescan the exact declared root before trust/retrieval; this is real
reindexing cost, not a zero-cost freshness oracle. Attach, shared-runtime, and
explicit-runtime paths do not inherit this automatic-cache refresh claim. It does not
establish hosted cross-root bootstrap, symlink/parent/child overlap cases,
custom exclusions, or advanced access. The legacy human birth path remains available
for launchers that do not supply this explicit automatic-preparation contract;
its broader retirement is not claimed here.
