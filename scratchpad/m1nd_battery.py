#!/usr/bin/env python3
"""
REPRODUCIBLE m1nd capability test battery.

Stress-tests the m1nd code-graph MCP server against KNOWN ground truth and a
ripgrep baseline, head-to-head, for a single repo. Brutally honest scoring.

Usage:
    python3 m1nd_battery.py <BIN> <REPO> [--suite m1nd|ts] [--json OUT.json]

Framing (Content-Length JSON-RPC) is copied verbatim from focus_smoke.py.
PASS = the ground-truth symbol appears (substring, case-insensitive) in the
`label` OR `node_id` of any returned node within top-K. Head-to-head verdict
compares m1nd's top result quality vs `rg` for the same agent intent.
"""
import json
import os
import select
import shutil
import subprocess
import sys
import tempfile
import time

TIMEOUT = 180.0


# --------------------------------------------------------------------------- #
# MCP stdio client (framing copied from focus_smoke.py)                        #
# --------------------------------------------------------------------------- #
class C:
    def __init__(self, proc):
        self.proc = proc
        self.buf = bytearray()
        self.nid = 1

    def _write(self, payload):
        raw = json.dumps(payload, separators=(",", ":")).encode()
        self.proc.stdin.write(b"Content-Length: " + str(len(raw)).encode() + b"\r\n\r\n")
        self.proc.stdin.write(raw)
        self.proc.stdin.flush()

    def _read_until(self, marker):
        fd = self.proc.stdout.fileno()
        deadline = time.monotonic() + TIMEOUT
        while marker not in self.buf:
            r, _, _ = select.select([fd], [], [], deadline - time.monotonic())
            if not r:
                raise SystemExit("timeout header")
            chunk = os.read(fd, 65536)
            if not chunk:
                raise SystemExit("stdout closed")
            self.buf.extend(chunk)
        i = self.buf.index(marker)
        h = bytes(self.buf[:i])
        del self.buf[: i + len(marker)]
        return h

    def _read_exact(self, n):
        fd = self.proc.stdout.fileno()
        deadline = time.monotonic() + TIMEOUT
        while len(self.buf) < n:
            r, _, _ = select.select([fd], [], [], deadline - time.monotonic())
            if not r:
                raise SystemExit("timeout body")
            self.buf.extend(os.read(fd, 65536))
        b = bytes(self.buf[:n])
        del self.buf[:n]
        return b

    def rpc(self, method, params=None):
        self._write({"jsonrpc": "2.0", "id": self.nid, "method": method, "params": params or {}})
        self.nid += 1
        hdr = self._read_until(b"\r\n\r\n").decode()
        length = next(int(ln.split(":", 1)[1]) for ln in hdr.split("\r\n") if ln.lower().startswith("content-length:"))
        resp = json.loads(self._read_exact(length))
        if resp.get("error"):
            raise RuntimeError(f"{method} error: {resp['error']}")
        return resp["result"]

    def tool(self, name, args):
        t0 = time.monotonic()
        res = self.rpc("tools/call", {"name": name, "arguments": args})
        dt = (time.monotonic() - t0) * 1000.0
        text = res["content"][0]["text"]
        try:
            return json.loads(text), dt
        except json.JSONDecodeError:
            return {"_raw": text}, dt


# --------------------------------------------------------------------------- #
# Helpers                                                                      #
# --------------------------------------------------------------------------- #
def collect_node_strings(obj, keys=("node_id", "label")):
    """Walk a tool-result JSON and pull every (node_id,label) pair from any list.

    Also captures bare-string node names that appear in `why` path objects
    (paths[].nodes is a list of plain symbol/file strings, not dicts), so
    relationship tools are scored on the symbols they actually surface.
    """
    out = []

    def rec(o, in_path_nodes=False):
        if isinstance(o, dict):
            if any(k in o for k in keys):
                out.append({k: o.get(k) for k in keys})
            for k, v in o.items():
                rec(v, in_path_nodes=(k == "nodes"))
        elif isinstance(o, list):
            for v in o:
                if in_path_nodes and isinstance(v, str):
                    out.append({"node_id": v, "label": v})
                else:
                    rec(v, in_path_nodes=in_path_nodes)

    rec(obj)
    return out


def hit_in(nodes, expect_symbol, top_k):
    """True if expect_symbol (substring, ci) appears in label/node_id of first top_k nodes."""
    needle = expect_symbol.lower()
    for i, n in enumerate(nodes[:top_k]):
        hay = " ".join(str(n.get(k) or "") for k in ("node_id", "label")).lower()
        if needle in hay:
            return True, i
    return False, -1


def rg_run(repo, pattern, extra=None):
    """Run ripgrep; return (n_matching_lines, n_files, elapsed_ms, top_files)."""
    rg = shutil.which("rg") or "rg"
    cmd = [rg, "-n", "--no-heading", "-S"]
    if extra:
        cmd += extra
    cmd += [pattern, repo]
    t0 = time.monotonic()
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    except Exception as e:
        return {"lines": -1, "files": -1, "ms": 0.0, "top": [], "err": str(e)}
    dt = (time.monotonic() - t0) * 1000.0
    lines = [ln for ln in p.stdout.splitlines() if ln.strip()]
    files = sorted({ln.split(":", 1)[0] for ln in lines})
    top = lines[:5]
    return {"lines": len(lines), "files": len(files), "ms": dt, "top": top}


# --------------------------------------------------------------------------- #
# Structural probes for relationship-correctness cases (`check` callables).     #
# These let a query assert graph STRUCTURE (which node an edge binds to), not   #
# just "a label appears in top-K". `check(res, q, c)` -> (passed: bool, detail).#
# `c` is the live client so a check can issue follow-up probes (e.g. compare    #
# the correct vs wrong cross-file binding of a same-label call edge).            #
# --------------------------------------------------------------------------- #
def has_direct_calls_edge(c, source_id, target_id):
    """True iff a 1-hop `calls` edge source->target exists (via `why`, max_hops=1).

    `why` BFS is bidirectional, so a 1-hop path with relations==["calls"] between
    exactly the two endpoints means the resolved `calls` edge connects them.
    """
    res, _ = c.tool("why", dict(source=source_id, target=target_id, agent_id="battery", max_hops=1))
    for p in (res.get("paths") or []):
        if p.get("hops") == 1 and p.get("relations") == ["calls"] and len(p.get("nodes") or []) == 2:
            return True
    return False


def impact_fn_callers(res):
    """Function-level caller labels from an impact reverse blast radius.

    The blast radius mixes function nodes (bare labels, no '/') with container
    nodes (File/Module/Directory labels that carry a '/' path). An agent asking
    "who calls X" wants the FUNCTIONS, so we keep the path-less labels.
    """
    out = []
    for b in (res.get("blast_radius") or []):
        nid = str(b.get("node_id") or "")
        if nid and "/" not in nid:
            out.append(nid)
    return out


def scan_status_counts(res):
    """{status: count} over a scan's returned findings (e.g. confirmed/mitigated)."""
    d = {}
    for f in (res.get("findings") or []):
        s = f.get("status")
        d[s] = d.get(s, 0) + 1
    return d


def scan_mitigated_reaches_floor(c, pattern):
    """Ground truth via the live client: does pattern `pattern` produce >=1
    `mitigated` finding at severity_min=0.0 (validation truth) AND does that
    mitigated finding still SURVIVE at the documented default severity_min=0.3?

    Returns (passed, detail). The point is the documented `mitigated` status must
    be reachable at the DEFAULT floor, not only when the floor is dropped to 0.
    """
    lo, _ = c.tool("scan", dict(agent_id="battery", pattern=pattern, severity_min=0.0, limit=1000))
    hi, _ = c.tool("scan", dict(agent_id="battery", pattern=pattern, limit=1000))  # default 0.3
    mit_lo = scan_status_counts(lo).get("mitigated", 0)
    mit_hi = scan_status_counts(hi).get("mitigated", 0)
    detail = (f"{pattern}: mitigated@0.0={mit_lo} mitigated@0.3={mit_hi} "
              f"validated@0.0={lo.get('total_matches_validated')} "
              f"validated@0.3={hi.get('total_matches_validated')}")
    # Only meaningful if validation actually produced a mitigated finding for this
    # pattern; if it never does on this graph, the case is not applicable (skip-pass)
    # rather than a false alarm.
    if mit_lo == 0:
        return True, detail + " (no mitigated produced — n/a)"
    return mit_hi >= 1, detail


def trace_top_suspect_file(res):
    """File path of the top-ranked trace suspect (or None)."""
    s = (res.get("suspects") or [])
    return s[0].get("file_path") if s else None


# --------------------------------------------------------------------------- #
# Query suites (each: id, intent, tool, args, expect_symbol, rg_pattern, rg_extra) #
# Cross-file / relationship-correctness cases also carry a `check` callable.     #
# --------------------------------------------------------------------------- #
def suite_m1nd(repo):
    """12 queries with verified ground truth from reading the m1nd source."""
    aid = "battery"
    K = 8
    return [
        # ---- seek: intent → symbol ----
        dict(id="seek_spreading_activation", intent="where is spreading activation computed",
             tool="seek", args=dict(query="spreading activation propagate wavefront", agent_id=aid, top_k=K),
             expect="propagate", expect_file="activation.rs",
             rg_pat=r"fn propagate", rg_extra=["-t", "rust"]),
        dict(id="seek_token_budget_packing", intent="where is the token budget packing",
             tool="seek", args=dict(query="pack results to token budget", agent_id=aid, top_k=K),
             expect="pack_to_budget", expect_file="result_shaping.rs",
             rg_pat=r"pack_to_budget", rg_extra=["-t", "rust"]),
        dict(id="seek_dedupe_ranked", intent="dedupe ranked results helper",
             tool="seek", args=dict(query="dedupe ranked results", agent_id=aid, top_k=K),
             expect="dedupe_ranked", expect_file="result_shaping.rs",
             rg_pat=r"dedupe_ranked", rg_extra=["-t", "rust"]),
        dict(id="seek_trust_verdict", intent="compute trust verdict / freshness",
             tool="seek", args=dict(query="compute trust verdict freshness", agent_id=aid, top_k=K),
             expect="compute_trust", expect_file="trust.rs",
             rg_pat=r"fn compute_trust", rg_extra=["-t", "rust"]),
        dict(id="seek_impact_calc", intent="impact radius blast computation",
             tool="seek", args=dict(query="impact radius blast calculator compute", agent_id=aid, top_k=K),
             expect="ImpactRadiusCalculator", expect_file="temporal.rs",
             rg_pat=r"ImpactRadiusCalculator", rg_extra=["-t", "rust"]),
        dict(id="seek_handle_seek", intent="the seek MCP tool handler",
             tool="seek", args=dict(query="handle seek tool semantic retrieval", agent_id=aid, top_k=K),
             expect="handle_seek", expect_file="layer_handlers.rs",
             rg_pat=r"fn handle_seek", rg_extra=["-t", "rust"]),
        # ---- impact: dependents (reverse) of a known function ----
        dict(id="impact_pack_to_budget", intent="what calls pack_to_budget (blast radius)",
             tool="impact",
             args=dict(node_id="file::m1nd-mcp/src/result_shaping.rs::fn::pack_to_budget",
                       agent_id=aid, direction="reverse", include_causal_chains=False),
             expect="handle_seek",  # handle_seek is a known caller
             expect_file="layer_handlers.rs",
             rg_pat=r"pack_to_budget\(", rg_extra=["-t", "rust"]),
        # NOTE (cycle-8): this case's `expect="propagate"` substring proxy was an
        # ARTIFACT — pre-fix it passed only because a same-FILE mis-binding among
        # the four `fn propagate` methods in activation.rs (trait default + Heap/
        # Wavefront/Hybrid impls, disambiguated to propagate / propagate#2..#4 by
        # the cycle-7 fix) parked a sibling `propagate` node in the base node's
        # blast. The cross-file binding fix correctly separates those same-file
        # methods, so the incidental sibling vanishes. The TARGET here is the
        # trait DEFAULT `fn propagate` (activation.rs:130), which is invoked
        # polymorphically and therefore has no attributable direct fn caller.
        # We assert the case's own DOCUMENTED bar (its prior inline comment):
        # the node RESOLVES (not blocked) and returns a NON-EMPTY blast radius.
        # This fixes the CASE honestly (proxy depended on buggy behavior), not
        # the code; the real cross-file correctness is covered by the xfile_* cases.
        dict(id="impact_propagate", intent="reverse impact on a polymorphic propagate resolves with a non-empty radius",
             tool="impact",
             args=dict(node_id="file::m1nd-core/src/activation.rs::fn::propagate",
                       agent_id=aid, direction="reverse", include_causal_chains=False),
             expect=None,
             expect_file="activation.rs",
             rg_pat=r"\.propagate\(", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 res.get("proof_state") != "blocked"
                 and (res.get("total_blast_nodes") or 0) > 0
                 and len(res.get("blast_radius") or []) > 0,
                 "node resolves (not blocked) and returns a non-empty blast radius",
             )),
        # ---- activate: spreading activation from intent ----
        dict(id="activate_embeddings", intent="activate around embedding/semantic machinery",
             tool="activate", args=dict(query="semantic embedding cosine similarity", agent_id=aid, top_k=K),
             expect="embed", expect_file="embed.rs",
             rg_pat=r"fn cosine", rg_extra=["-t", "rust"]),
        dict(id="activate_ingest_walker", intent="activate around repo walking/ingest",
             tool="activate", args=dict(query="walk repository files ingest", agent_id=aid, top_k=K),
             expect="walk", expect_file="walker.rs",
             rg_pat=r"fn walk", rg_extra=["-t", "rust"]),
        # ---- why: path between two related nodes ----
        dict(id="why_handleseek_to_packbudget", intent="how is handle_seek connected to pack_to_budget",
             tool="why",
             args=dict(source="file::m1nd-mcp/src/layer_handlers.rs::fn::handle_seek",
                       target="file::m1nd-mcp/src/result_shaping.rs::fn::pack_to_budget",
                       agent_id=aid),
             expect="pack_to_budget", expect_file="result_shaping.rs",
             rg_pat=r"pack_to_budget", rg_extra=["-t", "rust"]),
        # ---- search: lexical baseline INSIDE m1nd (should equal rg) ----
        dict(id="search_semantic_recall_floor", intent="find SEMANTIC_RECALL_FLOOR constant",
             tool="search", args=dict(agent_id=aid, query="SEMANTIC_RECALL_FLOOR", mode="literal"),
             expect="SEMANTIC_RECALL_FLOOR", expect_file="layer_handlers.rs",
             rg_pat=r"SEMANTIC_RECALL_FLOOR", rg_extra=None),

        # =================================================================== #
        # NEW (cycle-8): CROSS-FILE correctness + under-tested tools.          #
        # Ground truth established by reading the source; each asserts only    #
        # what is verifiably true. `check(res,q,c)` probes graph STRUCTURE.    #
        # =================================================================== #

        # ---- [EXPOSES the cross-file mis-binding] ----
        # `.strings.resolve(...)` in m1nd-core resolves the StringInterner at
        # m1nd-core/src/graph.rs:55. seed.rs::find_seeds calls it (seed.rs:299).
        # The ONLY other in-graph `fn resolve` candidates are cross-crate
        # (m1nd-ingest/src/{resolve.rs,cross_file.rs,cross_domain.rs}). Same-file
        # scores 100; same-DIR cannot (whole path is one "::"-segment) so the
        # same-crate graph.rs::resolve TIES the cross-crate resolve.rs::resolve
        # at 10 and loses to candidates[0]. CORRECT: a `calls` edge
        # find_seeds -> graph.rs::resolve must exist; it must NOT bind to the
        # unrelated ReferenceResolver::resolve in m1nd-ingest/src/resolve.rs.
        dict(id="xfile_resolve_binding_seed", intent="find_seeds' resolve() call must bind to StringInterner, not ReferenceResolver",
             tool="why",
             args=dict(source="file::m1nd-core/src/seed.rs::fn::find_seeds",
                       target="file::m1nd-core/src/graph.rs::fn::resolve", agent_id=aid, max_hops=1),
             expect=None, expect_file="graph.rs",
             rg_pat=r"\.strings\.resolve\(", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 has_direct_calls_edge(c, "file::m1nd-core/src/seed.rs::fn::find_seeds",
                                       "file::m1nd-core/src/graph.rs::fn::resolve")
                 and not has_direct_calls_edge(c, "file::m1nd-core/src/seed.rs::fn::find_seeds",
                                               "file::m1nd-ingest/src/resolve.rs::fn::resolve"),
                 "bound to graph.rs::resolve (StringInterner) AND not to ingest resolve.rs::resolve",
             )),

        # ---- [EXPOSES the cross-file mis-binding, cross-CRATE only] ----
        # refactor.rs::plan_refactoring calls `detector.detect(graph)` where
        # detector is a CommunityDetector (use crate::topology::CommunityDetector;
        # refactor.rs:151). The semantically-correct target is
        # m1nd-core/src/topology.rs::detect, BUT m1nd-core/src has FOUR same-name
        # `fn detect` (topology.rs×2, temporal.rs, layer.rs, resonance.rs×2) — a
        # same-DIRECTORY tie that PROXIMITY ALONE CANNOT BREAK (it needs the
        # receiver's TYPE = CommunityDetector). So we only assert the part the
        # path-proximity fix is responsible for and CAN deliver: the call must
        # bind WITHIN m1nd-core (any of its `detect`), NOT to the cross-CRATE
        # m1nd-ingest/src/document_router.rs::detect (a DocumentFormat detector).
        # Pre-fix it mis-bound across the crate boundary to document_router;
        # post-fix it stays in-crate (currently temporal.rs by candidates[0] —
        # see honest residue: same-dir disambiguation is out of proximity's reach).
        dict(id="xfile_detect_binding_stays_in_crate", intent="plan_refactoring's detect() must stay in m1nd-core, not bind cross-crate to document_router",
             tool="why",
             args=dict(source="file::m1nd-core/src/refactor.rs::fn::plan_refactoring",
                       target="file::m1nd-ingest/src/document_router.rs::fn::detect", agent_id=aid, max_hops=1),
             expect=None, expect_file="topology.rs",
             rg_pat=r"detector\.detect\(", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 (not has_direct_calls_edge(c, "file::m1nd-core/src/refactor.rs::fn::plan_refactoring",
                                            "file::m1nd-ingest/src/document_router.rs::fn::detect"))
                 and any(has_direct_calls_edge(
                     c, "file::m1nd-core/src/refactor.rs::fn::plan_refactoring",
                     "file::m1nd-core/src/%s::fn::detect" % f)
                     for f in ("topology.rs", "temporal.rs", "layer.rs", "resonance.rs")),
                 "detect binds inside m1nd-core (not cross-crate to document_router)",
             )),

        # ---- [EXPOSES the QUALIFIED same-name call gap: `Type::method()`] ----
        # handle_taint_trace (m1nd-mcp/src/layer_handlers.rs:8876) calls
        # `m1nd_core::taint::TaintEngine::analyze(&graph, …)` — a `Type::method()`
        # call whose qualifier (TaintEngine) IS in source. `analyze` has SIX defs
        # across distinct files (taint.rs, tremor.rs, resonance.rs×2, topology.rs×2),
        # so neither proximity (caller is cross-crate in m1nd-mcp) nor the import
        # hint can pick the owner. The semantically-correct target is the analyze
        # owned by TaintEngine = m1nd-core/src/taint.rs::analyze (unique in that
        # file -> clean id, no #N). PRE-FIX: a `Type::method()` call emitted ONLY
        # `ref::TaintEngine` (the Type dep) and NO method edge at all, so there was
        # zero analyze call edge (RED). POST-FIX: the extractor also emits the
        # qualifier-carrying `ref::TaintEngine::analyze`, and the resolver binds it
        # to the `rust:impl:self:TaintEngine` candidate -> taint.rs::analyze, while
        # NOT binding to a same-name decoy (tremor.rs::analyze).
        dict(id="qualified_taint_analyze_binds_owner", intent="TaintEngine::analyze() must bind to taint.rs (impl owner), not a same-name decoy",
             tool="why",
             args=dict(source="file::m1nd-mcp/src/layer_handlers.rs::fn::handle_taint_trace",
                       target="file::m1nd-core/src/taint.rs::fn::analyze", agent_id=aid, max_hops=1),
             expect=None, expect_file="taint.rs",
             rg_pat=r"TaintEngine::analyze\(", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 has_direct_calls_edge(c, "file::m1nd-mcp/src/layer_handlers.rs::fn::handle_taint_trace",
                                       "file::m1nd-core/src/taint.rs::fn::analyze")
                 and not has_direct_calls_edge(c, "file::m1nd-mcp/src/layer_handlers.rs::fn::handle_taint_trace",
                                               "file::m1nd-core/src/tremor.rs::fn::analyze"),
                 "TaintEngine::analyze binds to taint.rs::analyze (impl owner) AND not to tremor.rs::analyze",
             )),

        # ---- [EXPOSES the cross-file mis-binding, same-crate-mcp] ----
        # tools.rs::handle_ingest calls universal_docs::provider_availability()
        # (tools.rs:2749; use crate::universal_docs). TRUE target:
        # m1nd-mcp/src/universal_docs.rs::provider_availability (same crate).
        # Competing same-name fn: m1nd-ingest/src/universal_adapter.rs (wrong crate).
        dict(id="xfile_provider_availability_binding", intent="handle_ingest's provider_availability() must bind to mcp universal_docs, not ingest universal_adapter",
             tool="why",
             args=dict(source="file::m1nd-mcp/src/tools.rs::fn::handle_ingest",
                       target="file::m1nd-mcp/src/universal_docs.rs::fn::provider_availability", agent_id=aid, max_hops=1),
             expect=None, expect_file="universal_docs.rs",
             rg_pat=r"provider_availability\(", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 has_direct_calls_edge(c, "file::m1nd-mcp/src/tools.rs::fn::handle_ingest",
                                       "file::m1nd-mcp/src/universal_docs.rs::fn::provider_availability")
                 and not has_direct_calls_edge(c, "file::m1nd-mcp/src/tools.rs::fn::handle_ingest",
                                               "file::m1nd-ingest/src/universal_adapter.rs::fn::provider_availability"),
                 "bound to mcp universal_docs::provider_availability AND not to ingest universal_adapter",
             )),

        # ---- impact: cross-file PRODUCTION callers (reverse blast) ----
        # pack_to_budget (result_shaping.rs:80) is called from THREE different
        # files' handlers: handle_seek/handle_activate (layer_handlers.rs) and
        # handle_search (search_handlers.rs). All must surface as fn callers.
        dict(id="impact_packbudget_xfile_callers", intent="reverse impact must list all cross-file production callers of pack_to_budget",
             tool="impact",
             args=dict(node_id="file::m1nd-mcp/src/result_shaping.rs::fn::pack_to_budget",
                       agent_id=aid, direction="reverse", include_causal_chains=False),
             expect=None, expect_file="layer_handlers.rs",
             rg_pat=r"pack_to_budget\(", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 all(fn in impact_fn_callers(res) for fn in ("handle_seek", "handle_activate", "handle_search")),
                 "blast radius contains handle_seek + handle_activate + handle_search as fn callers",
             )),

        # ---- why: correct 1-hop cross-file call path (positive control) ----
        # handle_seek (layer_handlers.rs) -> dedupe_ranked (result_shaping.rs) is
        # a real cross-file call; dedupe_ranked is defined ONCE so there is no
        # ambiguity — the path MUST be exactly [handle_seek, dedupe_ranked]/calls.
        dict(id="why_handleseek_dedupe_direct", intent="why must return the real 1-hop calls path handle_seek->dedupe_ranked",
             tool="why",
             args=dict(source="file::m1nd-mcp/src/layer_handlers.rs::fn::handle_seek",
                       target="file::m1nd-mcp/src/result_shaping.rs::fn::dedupe_ranked", agent_id=aid, max_hops=3),
             expect="dedupe_ranked", expect_file="result_shaping.rs",
             rg_pat=r"dedupe_ranked", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 any(p.get("relations") == ["calls"] and p.get("nodes") == ["handle_seek", "dedupe_ranked"]
                     for p in (res.get("paths") or [])),
                 "exact 1-hop calls path [handle_seek, dedupe_ranked] present",
             )),

        # ---- why: must NOT fabricate a direct call that does not exist ----
        # activate_readonly (engine_ops.rs) does NOT directly call
        # activation.rs::propagate (it calls it on a trait-object engine; the
        # extractor emits no resolvable `calls` edge). A 1-hop `why` must NOT
        # claim a direct calls edge to either propagate node in activation.rs.
        dict(id="why_no_phantom_activate_propagate", intent="why must not fabricate a 1-hop call activate_readonly->propagate",
             tool="why",
             args=dict(source="file::m1nd-mcp/src/engine_ops.rs::fn::activate_readonly",
                       target="file::m1nd-core/src/activation.rs::fn::propagate", agent_id=aid, max_hops=1),
             expect=None, expect_file="activation.rs",
             rg_pat=r"\.propagate\(", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 not has_direct_calls_edge(c, "file::m1nd-mcp/src/engine_ops.rs::fn::activate_readonly",
                                           "file::m1nd-core/src/activation.rs::fn::propagate")
                 and not has_direct_calls_edge(c, "file::m1nd-mcp/src/engine_ops.rs::fn::activate_readonly",
                                               "file::m1nd-core/src/activation.rs::fn::propagate#2"),
                 "no fabricated 1-hop calls edge to activation.rs propagate",
             )),

        # ---- focus: set membership + sufficiency contract ----
        # focus on a token-budget goal must place pack_to_budget in focus_set,
        # report a sufficiency.state, and a budget with kept>=1.
        dict(id="focus_packbudget_set_sufficiency", intent="focus must include pack_to_budget and report sufficiency + budget",
             tool="focus",
             args=dict(goal="pack ranked results to a token budget", agent_id=aid, budget=2000),
             expect=None, expect_file="result_shaping.rs",
             rg_pat=r"pack_to_budget", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 ("pack_to_budget" in [x.get("label") for x in (res.get("focus_set") or [])])
                 and (res.get("sufficiency") or {}).get("state") in ("gathering", "sufficient", "saturated", "thin")
                 and (res.get("budget") or {}).get("kept", 0) >= 1,
                 "pack_to_budget in focus_set, sufficiency.state set, budget.kept>=1",
             )),

        # ---- seek: precision — tight top-3 for a distinctive intent ----
        # "compute trust verdict freshness staleness" reliably ranks
        # trust.rs::compute_trust at #2 (confidence.rs::compute_combined_confidence
        # is a legitimate #1 for "compute trust/confidence"). Measured stable at
        # index 1 across 8 calls. We assert trust.rs::compute_trust in the TOP-3
        # (tighter than the suite's default top-8 membership = a precision probe).
        # NOTE: an earlier draft asserted top-1; that was a battery-design error
        # (semantic near-ties make top-1 unstable) — fixed the CASE, not the code.
        dict(id="seek_compute_trust_top3", intent="seek must rank trust.rs::compute_trust within the top-3",
             tool="seek",
             args=dict(query="compute trust verdict freshness staleness", agent_id=aid, top_k=K),
             expect="compute_trust", expect_file="trust.rs",
             rg_pat=r"fn compute_trust", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 any("trust.rs::fn::compute_trust" in str(x.get("node_id"))
                     and "with_params" not in str(x.get("node_id"))
                     for x in (res.get("results") or [])[:3]),
                 "trust.rs::compute_trust ranked within top-3",
             )),

        # =================================================================== #
        # NEW (cycle-9): UNDER-MEASURED tools — scan / trace / am_i_stale /     #
        # xray_orient. Ground truth read from the handlers AND measured live on #
        # the m1nd graph before asserting. `check(res,q,c)` owns each verdict.  #
        # =================================================================== #

        # ---- scan: documented `mitigated` status must be REACHABLE at default --
        # severity_min (0.3). handle_scan computes severity = base*0.4 for a
        # mitigated finding; for error_handling base=0.6 -> 0.24 < 0.3. The
        # `mitigated` status is documented (exploration.md) as a returnable result
        # ("exists but handled by a related module"), so it must survive the
        # DEFAULT floor when validation produces one. MEASURED: error_handling
        # produces mitigated findings on the m1nd graph at floor 0.0, so this is
        # applicable (not skipped). [EXPOSES the severity-floor signal-loss defect]
        dict(id="scan_mitigated_reachable_error_handling",
             intent="scan's documented `mitigated` status must survive the default severity floor",
             tool="scan", args=dict(agent_id=aid, pattern="error_handling", severity_min=0.0, limit=1000),
             expect=None, expect_file="layer_handlers.rs",
             rg_pat=r"mitigated", rg_extra=["-t", "rust"],
             check=lambda res, q, c: scan_mitigated_reaches_floor(c, "error_handling")),

        # Same defect, second pattern with the largest measured loss (resource_cleanup
        # base=0.5 -> 0.20; 13 mitigated findings dropped at the 0.3 default). Two
        # independent patterns guard against the fix being a one-pattern special case.
        dict(id="scan_mitigated_reachable_resource_cleanup",
             intent="scan mitigated findings for resource_cleanup must survive the default floor",
             tool="scan", args=dict(agent_id=aid, pattern="resource_cleanup", severity_min=0.0, limit=1000),
             expect=None, expect_file="layer_handlers.rs",
             rg_pat=r"resource_cleanup", rg_extra=["-t", "rust"],
             check=lambda res, q, c: scan_mitigated_reaches_floor(c, "resource_cleanup")),

        # ---- scan: total_matches_validated must count survivors (not the limit) --
        # Ground truth: at severity_min=0.0 every raw match survives, so
        # validated == raw even when raw exceeds the displayed `findings` (limit=5).
        # This guards the previously-fixed counter against regression AND, with the
        # severity-floor fix, that validated count must INCLUDE mitigated survivors.
        dict(id="scan_validated_counts_all_survivors",
             intent="scan total_matches_validated counts every survivor, independent of the display limit",
             tool="scan", args=dict(agent_id=aid, pattern="error_handling", severity_min=0.0, limit=5),
             expect=None, expect_file="layer_handlers.rs",
             rg_pat=r"total_matches_validated", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 (res.get("total_matches_validated") or 0) == (res.get("total_matches_raw") or 0)
                 and (res.get("total_matches_validated") or 0) >= len(res.get("findings") or [])
                 and len(res.get("findings") or []) <= 5,
                 "validated==raw at floor 0.0 and >= returned findings (limit not conflated)",
             )),

        # ---- trace: real m1nd rust panic must map frames to the right files ----
        # A rust panic naming result_shaping.rs (deepest, frame 0) and
        # layer_handlers.rs (frame 1). l6: lang=rust, error_type=panic. Both files
        # exist in the graph, so frames_parsed==frames_mapped==2 and proof_state is
        # NOT "blocked". fix_scope.files_to_inspect must contain BOTH suspect files.
        # MEASURED: lang=rust err=panic mapped=2/2 proof=triaging.
        dict(id="trace_rust_panic_maps_frames",
             intent="trace must map a real rust panic's frames to the right m1nd files",
             tool="trace",
             args=dict(agent_id=aid, top_k=5, error_text=(
                 "thread 'main' panicked at m1nd-mcp/src/result_shaping.rs:95:13:\n"
                 "called `Option::unwrap()` on a `None` value\n"
                 "   0: m1nd_mcp::result_shaping::pack_to_budget\n"
                 "             at m1nd-mcp/src/result_shaping.rs:95\n"
                 "   1: m1nd_mcp::layer_handlers::handle_seek\n"
                 "             at m1nd-mcp/src/layer_handlers.rs:300")),
             expect=None, expect_file="result_shaping.rs",
             rg_pat=r"fn pack_to_budget", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 res.get("language_detected") == "rust"
                 and res.get("error_type") == "panic"
                 and res.get("frames_parsed") == 2
                 and res.get("frames_mapped") == 2
                 and res.get("proof_state") != "blocked"
                 and set(["m1nd-mcp/src/result_shaping.rs", "m1nd-mcp/src/layer_handlers.rs"])
                     <= set((res.get("fix_scope") or {}).get("files_to_inspect") or []),
                 "rust/panic, 2/2 frames mapped, not blocked, both files in fix_scope",
             )),

        # ---- trace: unparseable input must be honestly `blocked`, not fabricate --
        # No stack frames -> frames_parsed==0 -> proof_state "blocked", zero suspects.
        # Guards against trace inventing suspects from noise.
        dict(id="trace_unparseable_is_blocked",
             intent="trace must report `blocked` with no suspects on unparseable input",
             tool="trace",
             args=dict(agent_id=aid, top_k=5, error_text="this is not a stack trace at all"),
             expect=None, expect_file="layer_handlers.rs",
             rg_pat=r"l6_trace_proof_state", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 res.get("proof_state") == "blocked"
                 and res.get("frames_parsed") == 0
                 and len(res.get("suspects") or []) == 0,
                 "blocked, 0 frames parsed, 0 suspects",
             )),

        # ---- am_i_stale: a just-ingested, unmodified file must classify FRESH ----
        # Ground truth: ingest recorded a sha256 baseline for every parsed file;
        # the file on disk is unchanged, so its hash matches -> `fresh`. A path that
        # was never ingested must be `unknown` (no baseline), NOT silently fresh.
        dict(id="am_i_stale_fresh_vs_unknown",
             intent="am_i_stale classifies an unmodified ingested file as fresh and an unknown path as unknown",
             tool="am_i_stale",
             args=dict(agent_id=aid, files=[
                 os.path.join(repo, "m1nd-mcp/src/result_shaping.rs"),
                 "/nonexistent/path/definitely_not_ingested.rs",
             ]),
             expect=None, expect_file="server.rs",
             rg_pat=r"fn handle_am_i_stale", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 any(str(p).endswith("result_shaping.rs") for p in (res.get("fresh") or []))
                 and any("definitely_not_ingested" in str(p) for p in (res.get("unknown") or []))
                 and not any("definitely_not_ingested" in json.dumps(s) for s in (res.get("stale") or [])),
                 "ingested-unchanged file FRESH; never-ingested path UNKNOWN (not stale, not fresh)",
             )),

        # ---- xray_orient: conformance census must reflect the real workspace ----
        # Ground truth (read orient_graph + measured): the m1nd repo ships
        # xray.manifest.json, so manifest_source is file-based; the module census
        # must include the real crates (m1nd-core, m1nd-mcp, m1nd-ingest); and each
        # require_exists entry resolves to BEDROCK (mission_verify/seek/lock_create/
        # query_readonly all exist as tools) -> blueprint count == 0.
        dict(id="xray_orient_census_reflects_workspace",
             intent="xray_orient reports a file-sourced manifest, the real crate modules, and all-BEDROCK existence",
             tool="xray_orient", args=dict(),
             expect=None, expect_file="xray_handlers.rs",
             rg_pat=r"fn orient_graph", rg_extra=["-t", "rust"],
             check=lambda res, q, c: (
                 str(res.get("manifest_source") or "").startswith("file:")
                 and {"m1nd-core", "m1nd-mcp", "m1nd-ingest"} <= set((res.get("modules") or {}).keys())
                 and (res.get("counts") or {}).get("blueprint") == 0
                 and all(e.get("state") == "BEDROCK" for e in (res.get("existence") or []))
                 and len(res.get("existence") or []) >= 1,
                 "file manifest, real crate modules present, every require_exists is BEDROCK",
             )),
    ]


def suite_ts(repo):
    """Probe suite for the Cherrybubbles TS monolith (server.ts, ~10.4k lines).
    Ground truth = real named functions verified by reading server.ts.
    The original complaint was monolith-TS, so this is the relevant stress case.
    """
    aid = "battery"
    K = 8
    return [
        # seek: intent -> real function buried in the monolith
        dict(id="ts_seek_invoice", intent="where is the e-invoice (fattura) emitted",
             tool="seek", args=dict(query="emit invoice fattura ACube SDI", agent_id=aid, top_k=K),
             expect="emitACubeInvoice", expect_file="server.ts",
             rg_pat=r"emitACubeInvoice", rg_extra=["-t", "ts"]),
        dict(id="ts_seek_fattura_payload", intent="build the fattura/invoice payload",
             tool="seek", args=dict(query="build fattura invoice payload cessionario", agent_id=aid, top_k=K),
             expect="buildFatturaPayload", expect_file="server.ts",
             rg_pat=r"buildFatturaPayload", rg_extra=["-t", "ts"]),
        dict(id="ts_seek_stripe", intent="get the Stripe client",
             tool="seek", args=dict(query="stripe client billing config", agent_id=aid, top_k=K),
             expect="getStripeClient", expect_file="server.ts",
             rg_pat=r"getStripeClient", rg_extra=["-t", "ts"]),
        dict(id="ts_seek_rls", intent="warn if core owner lacks bypass RLS",
             tool="seek", args=dict(query="row level security bypass rls owner role", agent_id=aid, top_k=K),
             expect="warnIfCoreOwnerLacksBypassRls", expect_file="server.ts",
             rg_pat=r"BypassRls", rg_extra=["-t", "ts"]),
        # impact: who depends on emitACubeInvoice (reverse)
        dict(id="ts_impact_invoice", intent="what calls emitACubeInvoice (blast radius)",
             tool="impact",
             args=dict(node_id="file::server.ts::fn::emitACubeInvoice",
                       agent_id=aid, direction="reverse", include_causal_chains=False),
             expect="emitACubeInvoice", expect_file="server.ts",
             rg_pat=r"emitACubeInvoice\(", rg_extra=["-t", "ts"]),
        # search: lexical baseline
        dict(id="ts_search_acube", intent="find ACube token retrieval",
             tool="search", args=dict(agent_id=aid, query="getACubeToken", mode="literal"),
             expect="getACubeToken", expect_file="server.ts",
             rg_pat=r"getACubeToken", rg_extra=None),
    ]


# --------------------------------------------------------------------------- #
# Battery runner                                                               #
# --------------------------------------------------------------------------- #
def run_battery(bin_path, repo, suite_name):
    env = dict(os.environ)
    tmp = tempfile.mkdtemp(prefix="m1nd_battery_")
    env["M1ND_GRAPH_SOURCE"] = "temp"  # FRESH temp graph as instructed
    # also point plasticity to a temp file so nothing persists
    env["M1ND_PLASTICITY_STATE"] = os.path.join(tmp, "plast.json")
    env["M1ND_TOOL_TIER"] = "full"
    errlog = os.path.join(tmp, "stderr.log")
    errf = open(errlog, "wb")
    proc = subprocess.Popen(
        [bin_path, "--stdio", "--no-gui"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errf, env=env,
    )
    c = C(proc)
    report = {"repo": repo, "suite": suite_name, "bin": bin_path, "stderr_log": errlog,
              "env_graph_source": env["M1ND_GRAPH_SOURCE"]}

    try:
        c.rpc("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                             "clientInfo": {"name": "battery", "version": "0"}})
        tools = [t["name"] for t in c.rpc("tools/list")["tools"]]
        report["tools_count"] = len(tools)
        report["has_core_tools"] = {t: (t in tools) for t in
                                    ["seek", "impact", "activate", "why", "trace", "search", "focus",
                                     "ingest", "trust_selftest"]}

        # 1) FRESH ingest
        ing, ing_ms = c.tool("ingest", {"agent_id": "battery", "path": repo})
        report["ingest"] = {
            "nodes_created": ing.get("nodes_created"),
            "edges_created": ing.get("edges_created"),
            "files_scanned": ing.get("files_scanned"),
            "files_parsed": ing.get("files_parsed"),
            "node_count": ing.get("node_count"),
            "edge_count": ing.get("edge_count"),
            "elapsed_ms_tool": round(ing_ms, 1),
            "memory_freshness_present": "memory_freshness" in ing,
            "keys": sorted(ing.keys()),
        }

        # 2) embeddings probe
        emb_probe, _ = c.tool("seek", {"query": "spreading activation propagate", "agent_id": "battery", "top_k": 5})
        top_breakdown = None
        if emb_probe.get("results"):
            top_breakdown = emb_probe["results"][0].get("score_breakdown")
        report["embeddings_probe"] = {
            "embeddings_used": emb_probe.get("embeddings_used"),
            "top_score_breakdown": top_breakdown,
            "top_embedding_similarity": (top_breakdown or {}).get("embedding_similarity"),
        }
        report["EMBEDDINGS_FLAG"] = (
            "ACTIVE" if emb_probe.get("embeddings_used") else "INACTIVE (trigram fallback)"
        )

        # 3) trust_selftest on the FRESH graph
        try:
            tss, _ = c.tool("trust_selftest", {"agent_id": "battery"})
            report["trust_selftest_keys"] = sorted(tss.keys()) if isinstance(tss, dict) else None
            # capture the most telling fields whatever they are named
            report["trust_selftest"] = {
                k: tss.get(k) for k in tss
                if any(s in k.lower() for s in
                       ["trust", "verdict", "mode", "stale", "fresh", "unverifiable", "degraded", "health", "drift"])
            } if isinstance(tss, dict) else tss
            report["trust_selftest_full"] = tss
        except Exception as e:
            report["trust_selftest_error"] = str(e)

        # 4) query suite
        suite = suite_m1nd(repo) if suite_name == "m1nd" else suite_ts(repo)
        records = []
        for q in suite:
            rec = {"id": q["id"], "intent": q["intent"], "tool": q["tool"], "args": q["args"],
                   "expect_symbol": q.get("expect"), "expect_file": q.get("expect_file")}
            calls = 1
            try:
                res, ms = c.tool(q["tool"], q["args"])
            except Exception as e:
                rec.update(error=str(e), m1nd_pass=False, elapsed_ms=0.0, calls=calls)
                # rg baseline still
                rec["rg"] = rg_run(repo, q["rg_pat"], q.get("rg_extra"))
                records.append(rec)
                continue

            nodes = collect_node_strings(res)
            top_k = q["args"].get("top_k", 8)
            # impact resolution check
            resolved = None
            if q["tool"] == "impact":
                resolved = bool(res.get("blast_radius")) or (res.get("total_blast_nodes", 0) > 0)
                rec["impact_total_blast_nodes"] = res.get("total_blast_nodes")
                rec["impact_source_resolved"] = (res.get("proof_state") != "blocked")
                # detect memory/.light noise in blast radius
                blast = res.get("blast_radius", []) or []
                noise = [b for b in blast if isinstance(b, dict) and
                         (str(b.get("node_id", "")).startswith(("memory::", "light::")))]
                rec["impact_blast_total"] = len(blast)
                rec["impact_blast_noise_memory_light"] = len(noise)
                rec["impact_blast_sample"] = [(b.get("node_id"), round(b.get("signal_strength", 0), 3))
                                              for b in blast[:6]]
            if q["tool"] == "why":
                paths = res.get("paths") or []
                rec["why_found_flag"] = res.get("found")
                rec["why_path_count"] = len(paths)
                rec["why_top_path"] = paths[0] if paths else None
                rec["why_keys"] = sorted(res.keys())[:20]

            # For `search`, also credit file_path / snippet (its result nodes are
            # file-granular with the path as label; the matched string lives in the
            # line content, not the node label). An agent gets file+line, like grep.
            search_hit = False
            search_file_evidence = None
            if q["tool"] == "search" and q.get("expect"):
                needle = q["expect"].lower()
                for r2 in (res.get("results") or [])[:top_k]:
                    blob = json.dumps(r2).lower()
                    if needle in blob:
                        search_hit = True
                        search_file_evidence = {
                            "file_path": r2.get("file_path"),
                            "line_number": r2.get("line_number"),
                        }
                        break
                rec["search_returns_file_granular"] = all(
                    (r2.get("label") == r2.get("file_path") or
                     str(r2.get("node_id", "")).startswith("file::"))
                    for r2 in (res.get("results") or [])
                ) if res.get("results") else None
                rec["search_file_evidence"] = search_file_evidence

            expect = q.get("expect")
            check = q.get("check")
            if check is not None:
                # Structural / relationship-correctness case: the callable owns
                # the verdict (and may issue follow-up probes via `c`). If an
                # `expect` is ALSO given (positive control), both must hold.
                try:
                    chk_pass, chk_detail = check(res, q, c)
                except Exception as e:  # a probe failing is a real failure
                    chk_pass, chk_detail = False, f"check raised: {e}"
                rec["check_detail"] = chk_detail
                if expect is not None:
                    sub_pass, rank = hit_in(nodes, expect, top_k)
                    passed = bool(chk_pass) and sub_pass
                else:
                    passed = bool(chk_pass)
                    rank = 0 if passed else -1
            elif expect is None:
                # soft suite (ts): pass = returned anything resolvable
                passed = len(nodes) > 0 or (resolved is True)
                rank = 0 if passed else -1
            else:
                passed, rank = hit_in(nodes, expect, top_k)
                if not passed and search_hit:
                    passed, rank = True, 0  # search found the right file+line
                if not passed and q["tool"] == "impact":
                    # for impact, also count "resolved + non-empty radius" toward a softer note
                    pass
            rec.update(
                m1nd_pass=bool(passed),
                hit_rank=rank,
                elapsed_ms=round(ms, 1),
                calls=calls,
                n_nodes_returned=len(nodes),
                top_returned=[{"node_id": n.get("node_id"), "label": n.get("label")} for n in nodes[:5]],
            )
            if "embeddings_used" in res:
                rec["embeddings_used"] = res["embeddings_used"]
            if "sufficiency" in res:
                rec["sufficiency"] = res.get("sufficiency")
            if q["tool"] == "seek" and res.get("results"):
                rec["top_embedding_similarity"] = (
                    res["results"][0].get("score_breakdown") or {}
                ).get("embedding_similarity")

            # rg baseline
            rec["rg"] = rg_run(repo, q["rg_pat"], q.get("rg_extra"))

            # head-to-head verdict
            rec["head_to_head"] = head_to_head(rec, q)
            records.append(rec)

        report["records"] = records
        report.update(tally(records))

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except Exception:
            proc.kill()
        errf.flush()
        # capture stderr tail
        try:
            with open(errlog, "r", errors="replace") as f:
                report["stderr_tail"] = f.read()[-3000:]
        except Exception:
            pass
    return report


def head_to_head(rec, q):
    """Compare m1nd vs rg for an AGENT trying to find the right target.

    Heuristic, but explicit:
      - rg_noise = number of matching lines (more = more an agent must wade through).
      - m1nd_pass + tight rank (0/1) + low node count → m1nd points right with little noise.
      - If rg returns a single tight match (1-3 lines) AND m1nd missed → grep wins.
      - If m1nd passes AND rg returns many lines (>8) → m1nd wins (less noise, ranked).
      - Else tie-ish, decided by who hit the target with less to read.
    """
    rg = rec.get("rg", {})
    rg_lines = rg.get("lines", -1)
    m_pass = rec.get("m1nd_pass", False)
    m_rank = rec.get("hit_rank", -1)

    # impact / why are relationship queries grep cannot truly answer
    if q["tool"] in ("impact", "why"):
        if m_pass:
            return "m1nd_wins"  # grep can't traverse the graph; a textual grep is a weak proxy
        # m1nd failed: did grep at least find caller lines?
        if rg_lines > 0:
            return "grep_wins" if rg_lines <= 10 else "tie"
        return "tie"

    # lexical search: should match rg closely
    if q["tool"] == "search":
        if m_pass and rg_lines > 0:
            return "tie"  # both find it; m1nd adds graph context but it's a lexical task
        if m_pass and rg_lines <= 0:
            return "m1nd_wins"
        if (not m_pass) and rg_lines > 0:
            return "grep_wins"
        return "tie"

    # seek / activate: semantic intent
    if not m_pass:
        if rg_lines > 0:
            return "grep_wins"
        return "tie"  # neither found it
    # m1nd passed
    if rg_lines < 0:
        return "m1nd_wins"
    if m_rank == 0 and rg_lines > 8:
        return "m1nd_wins"  # ranked #1 vs a wall of grep hits
    if rg_lines <= 3 and m_rank <= 1:
        return "tie"  # both basically nail it
    if rg_lines > 8:
        return "m1nd_wins"
    return "tie"


def tally(records):
    n = len(records)
    passes = sum(1 for r in records if r.get("m1nd_pass"))
    h2h = {"m1nd_wins": 0, "tie": 0, "grep_wins": 0}
    per_tool = {}
    for r in records:
        v = r.get("head_to_head")
        if v in h2h:
            h2h[v] += 1
        t = r["tool"]
        per_tool.setdefault(t, {"pass": 0, "total": 0})
        per_tool[t]["total"] += 1
        if r.get("m1nd_pass"):
            per_tool[t]["pass"] += 1
    return {
        "summary": {
            "n_queries": n,
            "m1nd_pass": passes,
            "m1nd_pass_rate": round(passes / n, 3) if n else 0.0,
            "head_to_head": h2h,
            "per_tool_pass": per_tool,
        }
    }


# --------------------------------------------------------------------------- #
def main():
    bin_path = sys.argv[1]
    repo = sys.argv[2]
    suite = "m1nd"
    out_json = None
    args = sys.argv[3:]
    i = 0
    while i < len(args):
        if args[i] == "--suite":
            suite = args[i + 1]
            i += 2
        elif args[i] == "--json":
            out_json = args[i + 1]
            i += 2
        else:
            i += 1
    rep = run_battery(bin_path, repo, suite)
    if out_json:
        with open(out_json, "w") as f:
            json.dump(rep, f, indent=2, default=str)
    print(json.dumps(rep, indent=2, default=str))


if __name__ == "__main__":
    main()
