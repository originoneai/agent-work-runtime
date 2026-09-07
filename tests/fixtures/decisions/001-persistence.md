---
id: ADR-PERSISTENCE
status: Accepted
affected_keys: [W1]
paths: [src/runtime]
---
# Persist runtime state

## Decision

Store runtime state in SQLite.

### Transaction boundary

Commit derived facts with their source revision.

## Rationale

Allow the next session to resume reliably.

## Appendix

UNRELATED_RAW_TRANSCRIPT must not enter the decision projection.
