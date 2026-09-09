# Native npm and PyPI distributions

AWR 0.2.0 is a stable package release. npm uses the `latest` channel and PyPI uses
`0.2.0` without a prerelease suffix. Both install the same Rust CLI and MCP server;
no separate JavaScript or Python SDK is included.

```sh
npm install -g @originoneai/agent-work-runtime@0.2.0
# or, in a virtual environment
python -m pip install agent-work-runtime==0.2.0
awr --version
awr-mcp --version
```

| Target | Compatibility |
| --- | --- |
| macOS arm64 | macOS 15+; `macosx_15_0_arm64` wheel |
| Linux x64 GNU | glibc 2.39+; `manylinux_2_39_x86_64` wheel |
| Windows x64 | `win_amd64` wheel; native checks run on Windows Server 2025 |

Python launchers require Python 3.9+, npm launchers Node 22.14+. Distribution checks
run Python 3.12 and Node 24. Other architectures and older operating systems need
separate verification. Git-bound operations require Git; SQLite is bundled.

The npm wrapper depends on exact-version optional native packages ending in
`-darwin-arm64`, `-linux-x64-gnu` and `-win32-x64`. Keep optional dependencies enabled.
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
artifact hashes and successful installation receipts for all three platforms. It
produces four npm archives, three wheels, a release manifest and `SHA256SUMS`.
Native archives and wheels carry Apache-2.0 and applicable third-party notices.

Stable versions publish with npm `latest`; development/prerelease versions use
`next` and their PEP 440 equivalent. A release number cannot be overwritten. If an
upload partly succeeds, compare the actual registry bytes before any retry. Use the
verified assembled archives for a manual npm upload if interactive authentication
is required; never replace them with a fresh unverified build.

After publication, download all seven registry files, compare hashes, and install
both channels into fresh environments. GitHub release assets contain the same
packages, checksums and aggregate manifest. Raw execution records stay local or in
isolated CI artifacts and are not committed to the repository.
