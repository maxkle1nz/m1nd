# M1nd 10 — owner ratification record

Date: 2026-07-18  
Authority: human owner, current Codex task  
Evidence class: session record; not a cryptographic signature

## Decision

The owner explicitly replied:

> perfeito ratifico pode comecar coloque o goal e ligue o loop me entregue quando tudo sera pronto e testado

For this implementation program, that reply ratifies the already-authored PRD/UML baseline,
its proposed G0-G10 order, the stated R2/R6/R8/R9/R10 thresholds, and the bootstrap/target
choice recorded by those contracts:

```text
APPROVE — bootstrap HUMAN_GATED; target FULL_AUTONOMY after G9
```

The exact frozen inputs are:

| Contract | SHA-256 |
|---|---|
| `docs/M1ND-10-PRD.md` | `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38` |
| `docs/M1ND-10-UML.md` | `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b` |

## Authority boundary

This record authorizes implementation and local proof. It is not any of the following:

- an `AutonomyActivationReceiptV1`;
- a final G10 ratification or a `GateReceiptV1`;
- a cryptographic MetricSpec signature;
- authorization to commit, tag, push, publish, install over the live owner, rotate production
  keys, or activate `POLICY_AUTONOMOUS` / `FULL_AUTONOMY`.

Until an exact release candidate passes G9 and the previous authority emits a valid activation
receipt, the only honest active-mode statement remains `HUMAN_GATED`. Session ratification may
be bound into local MetricSpecs as `OWNER_RATIFIED_IN_SESSION`, but must retain
`cryptographic_signature: NOT_INSTALLED`.
