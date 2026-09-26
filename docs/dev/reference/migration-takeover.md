# Migration drill and mainline takeover (WS-050)

## Intent

Prove **backup → preview → migrate → restore** on an independent realistic
fixture first, then run this project's mainline takeover along that proven path.
Keep a single fact source and existing identities.

Owner-only Team PG surfaces (`awr-server access backup-*`, `history-*`) remain
the operational tools. `awr_core::migration_takeover` is the fixture-first
oracle that encodes acceptance rules without inventing a parallel migration
stack.

## Independent fixture

Path: `tests/fixtures/workstreams/migration-takeover/`

The fixture covers:

- Goal lines **Team / EVO / DEC / AUTO** with person owners
- Actors (persons and agents), sessions, claims, reviews
- Provable and unprovable person→agent delegations
- Incomplete work, an active session, and historical release evidence

## Classification rules

| History | Result |
| --- | --- |
| Person actor | Identity retained (`person_retained`) |
| Agent with provable delegation, non-sensitive role | `preserved` under the person binding |
| Agent with provable delegation declared as owner / independent approver | `pending_confirmation` — never auto-promoted |
| Agent without provable person-delegation | `pending_confirmation` |
| Unknown actor id | `pending_confirmation` |

Agents are never auto-promoted to owner or independent approver.

## Drill

```text
backup(fixture) → preview(backup) → migrate(backup, preview) → restore(backup, result)
```

Project takeover dry-run is refused until `fixture_drill_ok` is true for a
successful independent drill digest.

## Receipts

Operator receipts for this workstream are written under
`.local/awr-workstream-implementation-20260921/migration/` (local only; not
published in the public tree). Run:

```sh
python3 scripts/team-deploy/migration-takeover-drill.py
```
