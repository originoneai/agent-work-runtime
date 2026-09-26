# Team source registry publish entrypoint (AWR-TMCP-041)

The Team project's **authoritative source registry** is maintained through a
**single team publish entrypoint**. Developers contribute via Git PRs; they do
not push ad-hoc full ledgers into the running service, and they do not copy the
runtime PostgreSQL database onto develop branches.

## Single publish path

1. Developer opens a PR against the agreed base with source / contract changes.
2. Maintainer reviews the PR (exact paths, graph integrity, acceptance text).
3. Authorized planning publish (MCP `awr_team_planning_publish` / HTTP
   `planning.publish`) writes the approved candidate through the registered sole
   source. Optional `activate` uses that registered source only — **client paths,
   URLs, and SQL are refused**.
4. Import / activate barriers (WS-022/023/032) keep unactivated candidates out of
   the live executable contract. Failed activation retains the previous
   `active_snapshot_id`.

Daily member clients **read** the activated contract through
`work.prepare` / controlled `source.content`. They do not obtain ledger-directory
write access on the server.

Owner/operator bootstrap of the first source bundle still follows
[team-workstream-service.md](team-workstream-service.md) and
[team-postgres.md](team-postgres.md) (ingest → approve → activate). After the
project is live, treat MCP/HTTP `planning.publish` (plus the registered source
writer) as the **only** normal publish entrypoint.

## Runtime vs develop versions

| Concern | Runtime host | Develop / PR branches |
| --- | --- | --- |
| Service binary | Pinned digest from a verified candidate | Workspace under development |
| Team schema | Matches that binary's `EXPECTED_SCHEMA_VERSION` | May advance ahead; not auto-pushed |
| Source registry | Activated snapshot in PG + registered sole source | PR diffs only |
| Database | Production / staging PG | Ephemeral lab DBs — **never** a copy of runtime PG |
| Ledgers | Server-authoritative runtime state | Do **not** export a full runtime ledger and commit it back |

### Update candidates (directed replace)

1. Build and test a candidate from the reviewed tip (unit + schema migrate on a
   disposable DB + MCP `capabilities` smoke).
2. Record binary digest, schema version, config hash, and active source
   snapshot id as the **rollback basis**.
3. Take an owner backup manifest (`scripts/team-deploy/backup.sh`).
4. Migrate as owner if the schema advances; `awr-server check` must pass.
5. Directed-replace the runtime binary / unit to the candidate (one writer).
6. Verify HTTPS MCP reachability and a member `capabilities` query.
7. On failure, restore the matching program + schema + config + source set from
   the rollback basis. Do not “partially” mix an old binary with a newer schema.

## Forbidden developer practices

- Copying runtime `AWR_TEAM_DATABASE_URL` data into a laptop develop checkout.
- Pushing an old **full** work-ledger dump back onto the service to “sync”.
- Bypassing publish with arbitrary SQL, `status=done` tools, or server path edits.
- Shipping owner credentials or ledger-directory mounts to coding-agent members.

## Member-visible surface

Members receive only what [team-member-handoff.md](team-member-handoff.md)
lists. Source contributions remain ordinary Git PRs against the public/internal
repo URL you hand them — not direct registry writes.
