# Native npm and PyPI distributions

AWR 0.3.2 is a stable package release. npm uses the `latest` channel and PyPI uses
`0.3.2` without a prerelease suffix. Both install the same Rust CLI and MCP server;
no separate JavaScript or Python SDK is included.

```sh
npm install -g @originoneai/agent-work-runtime@0.3.2
# or, in a virtual environment
python -m pip install agent-work-runtime==0.3.2
awr --version
awr-mcp --version
```

| Target | Compatibility |
| --- | --- |
| macOS arm64 | macOS 15+; `macosx_15_0_arm64` wheel |
| macOS Intel x64 | macOS 15+; `macosx_15_0_x86_64` wheel |
| Linux x64 GNU | glibc 2.39+; `manylinux_2_39_x86_64` wheel |
| Windows x64 | `win_amd64` wheel; native checks run on Windows Server 2025 |

Python launchers require Python 3.9+, npm launchers Node 22.14+. Distribution checks
run Python 3.12 and Node 24. Other architectures and older operating systems need
separate verification. Git-bound operations require Git; SQLite is bundled.

The npm wrapper depends on exact-version optional native packages ending in
`-darwin-arm64`, `-darwin-x64`, `-linux-x64-gnu` and `-win32-x64`. Keep optional dependencies enabled.
There is no install script or network downloader. Wheels embed the binaries.
Native payloads include source identity, binary hashes and dependency license notices.
Only explicit packaging inputs and binaries are included; internal records and AWR
runtime databases are excluded.

## Build and verify

Use a separate environment with `scripts/release/requirements.txt`. On Linux also
install `auditwheel` and `patchelf`. From a clean committed source tree:

```sh
python scripts/release/build_packages.py --output .local/distribution-001
python scripts/release/smoke_install.py .local/distribution-001
```

Each run uses a new output directory. The installation check uses isolated pip/npm
installs, verifies both native versions and errors, initializes a Unicode project,
compiles task context, checks intake diagnosis and discovers the eight MCP tools.
These are installation checks, not proof of every real-agent business scenario.

## Publish reviewed builds

The `distributions.yml` workflow builds and checks each native platform. Ordinary
push and PR runs never publish. Manual publication is restricted to the `main`
branch and the `package-registries` environment. npm and PyPI have independent jobs
so an authentication failure in one registry does not hide the other's result.
Trusted publishers bind this repository, workflow and environment.

The assembler requires matching source/version identities, clean trees, matching
artifact hashes and successful installation receipts for all four platforms. It
produces five npm archives, four wheels, a release manifest and `SHA256SUMS`.
Native archives and wheels carry Apache-2.0 and applicable third-party notices.

Stable versions publish with npm `latest`; development/prerelease versions use
`next` and their PEP 440 equivalent. A release number cannot be overwritten. If an
upload partly succeeds, compare the actual registry bytes before any retry. Use the
verified assembled archives for a manual npm upload if interactive authentication
is required; never replace them with a fresh unverified build.

After publication, download all nine registry files, compare hashes, and install
both channels into fresh environments. GitHub release assets contain the same
packages, checksums and aggregate manifest. Raw execution records stay local or in
isolated CI artifacts and are not committed to the repository.


## Pinned host payload (0.3.2)

AWR `0.3.2` uses database schema 4. Host applications can bundle the same native CLI and MCP server without the registry launchers. A locally built payload has its own recorded source and artifact identity. The matching macOS arm64 or Intel x64 payload can be embedded directly in an application; runtime users need no Node/Python/Rust installation and no PATH changes. The build/verification machine still needs its development tools. The host is responsible for application signing, notarization and updating its bundled binary.

From clean committed source:

```sh
python scripts/release/build_host_bundle.py --output .local/host-payload-001 --batch candidate-001
```

The output directory is new and never overwritten. Its archive name binds purpose, version, platform, source SHA and batch. The payload contains `bin/awr`, `bin/awr-mcp`, `build.json`, `capabilities.json`, `SHA256SUMS`, Apache-2.0 and dependency license notices. `host-manifest.json` binds the archive digest and every payload file. Verify these hashes and the source commit before embedding; keep the same reviewed bytes through signing and installation, recording any signing changes separately. Both macOS architectures require macOS 15+. Other target builds and application integrations require their own native receipts.

Invoke the executable by its absolute application-resource path with `--project <original project root> --json`. Capability negotiation requires no project, database, model configuration, shell, or language interpreter. Initialization, source reads and work context likewise run natively; Git is a separate optional requirement for Git-bound workflows. SQLite is bundled. Do not use `npx`, `pip`, a shell command string or an opportunistic downloader as the embedded invocation path.

## Matched upgrade and rollback checklist

AWR 0.3.2 exposes a native `runtime.matched_snapshot` capability for
`runtime binding`, `backup`, `check`, `restore-preview`, `restore`, `restore-status`
and `restore-recover`. See the [snapshot contract](../reference/runtime-snapshots.md)
for exact program/configuration/source matching and offline history restoration.
Cross-schema downgrade and host files
outside `.awr` still require the broader matched inventory below.

Use the same project directory and identity. Arbitrary relocation, cloud synchronization and long-lived dual writers are outside this contract.

1. Quiesce host/CLI/MCP writers and execution supervisors. Record the one application owner that will resume writes. Negotiate the candidate's capabilities/schema before opening the existing database; retain the previous executable and its source/version/hash. Check the current database's integrity and foreign keys with its matching program.
2. Retain an immutable baseline of the program(s), `.awr/project.toml`, every registered original source (including explicitly allowed external paths), and **all** runtime files. Use SQLite's backup API for a consistent `state.db` snapshot; copying a live database while ignoring its WAL can lose history. Events, checkpoints, sessions, claims, evidence and artifacts are not disposable projections. Include managed artifacts, execution references, mutation/checkpoint journals and retained recovery bytes; keep secrets local. Bind file hashes, schema, project/root identity and table/record inventories in the snapshot manifest.
3. Test the candidate against an isolated fixture/candidate environment first. At the original project root, use the explicit source refresh/initialization path to migrate eligible older schemas, then compare project/work/source IDs and historical rows. Source mapping changes need their own reviewed configuration change. Do not silently interpret old completion flags as current evidence.
4. On failure, stop candidate writes and retain the entire current runtime plus all post-upgrade user changes before rollback. Verify the baseline hashes. Restore its matching program, database/runtime history, configuration and necessary source snapshots together. Compare each current original source to the recorded expected version first; changed content requires explicit review, never blind overwrite. Restore only listed original files and keep newly added user/APP files. A program downgrade alone cannot make a newer schema readable. Retain the upgraded runtime separately so new history is not discarded. Verify the restored schema/integrity and original project IDs before reopening one writer.
5. Keep pre-AWR APP records in read-only storage with stable reference mappings. Reindex the authoritative project sources and link only semantics that are actually understood. Legacy completion reports and native-session references never become verified completion or resumable AWR sessions merely by import. Switch the APP's write path once; do not maintain two authoritative writers indefinitely.

The fixture-only rehearsal is reproducible with an older compatible executable:

```sh
python scripts/release/host_upgrade_drill.py --old-awr /absolute/retained/awr --new-awr /absolute/payload/bin/awr --output .local/upgrade-drill-001
```

It creates new isolated projects, captures a matched SQLite/runtime/source/program snapshot, upgrades while preserving identities and history, checks old-program rejection for a newer schema, and restores the baseline. It also checks changed-source rollback refusal, preserved new user files, read-only legacy APP records, absence of verification promotion, and isolation between two projects sharing task key `W`. This is release engineering verification, not a backup service or a claim of full APP/native-client business acceptance. Never point this rehearsal at a user's live project.
