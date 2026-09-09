# npm and PyPI development previews

The same AWR Rust CLI and MCP server are distributed through two package registries.
No Python/JavaScript SDK is included. V1 acceptance and stable release remain governed
by the existing contract and ledger; this preparation does not mark them complete.

## Package names and versions

| Registry | Public package | Current preview version |
| --- | --- | --- |
| npm | `@originoneai/agent-work-runtime` | `0.1.0-dev`, dist-tag `next` |
| PyPI | `agent-work-runtime` | `0.1.0.dev0` |

Both versions are derived from `workspace.package.version` in `Cargo.toml`.
`-dev.N`, `-alpha.N`, `-beta.N` and `-rc.N` map to PEP 440 development/prerelease
versions. The native `--version` output retains the Cargo version. A preview is
never published as npm `latest`. The publication assembler rejects stable versions.
Names must be checked again before the first upload; a registry 404 does not reserve
a name or guarantee that an upload will be accepted.

The npm wrapper has three exact-version optional native dependencies:

- `@originoneai/agent-work-runtime-darwin-arm64`
- `@originoneai/agent-work-runtime-linux-x64-gnu`
- `@originoneai/agent-work-runtime-win32-x64`

The npm package contains no install script or network downloader. Python wheels
contain the binaries directly. The native payload carries source commit, compiler,
binary hashes and dependency license notices. No local AWR runtime state is packaged.

## Supported distribution targets

| Target | Build and installation runner | Distribution compatibility |
| --- | --- | --- |
| macOS arm64 | `macos-15` | macOS 15 or newer; `macosx_15_0_arm64` wheel |
| Linux x64 GNU | `ubuntu-24.04` | glibc 2.39 or newer; wheel repaired by auditwheel |
| Windows x64 MSVC | `windows-2025` | `win_amd64` wheel; other Windows versions need separate verification |

Python launchers require Python 3.9+, npm launchers Node 22.14+. The current install
matrix executes Python 3.12 and Node 24; other interpreter versions are not credited
as tested. The npm Linux launcher rejects musl and older glibc. Intel macOS, Linux
arm64 and Windows arm64 are not included. SQLite is bundled; Git-bound operations
still require an installed Git executable.

## Build and inspect locally

Use a dedicated virtual environment, leaving the main development environment alone:

```sh
python3 -m venv .local/dist-tools
.local/dist-tools/bin/python -m pip install -r scripts/release/requirements.txt
.local/dist-tools/bin/python scripts/release/build_packages.py --output .local/distribution-preview-01
.local/dist-tools/bin/python scripts/release/smoke_install.py .local/distribution-preview-01
```

On Windows use the environment's `Scripts/python.exe`. On Linux install `auditwheel`
and `patchelf` in that environment before building. Each output directory must be new.
The wheel, wrapper tarball and native tarball appear under `publish/`; `manifest.json`,
`SHA256SUMS` and `installation-checks.json` bind the package bytes and verification.
License overrides under `packaging/licenses/` supply upstream, commit-pinned license
files omitted from published crate archives; hashes are checked while building.

The installation check creates isolated pip/npm installations, installs without a
registry fallback or lifecycle scripts, checks both commands' help/version, verifies
an error's exit code and stderr, initializes a Unicode/spaced project path, obtains
JSON status and discovers the eight MCP tools over actual stdio. These are package
installation checks, not real-agent business acceptance.

## Registry account setup

1. Finish each account's email verification and required 2FA in the registry website.
   Passwords, OTPs and recovery codes do not belong in this repository.
2. Create the npm `originoneai` organization on the free public-packages plan. A
   personal account named `originoneai` occupies that same scope: npm provides an
   account-to-organization conversion flow which requires a separate personal owner.
3. Apply for the PyPI `originoneai` organization with accurate organization details.
   Corporate organizations have a subscription and require administrator review;
   free community eligibility must not be assumed just because code is open source.
   PyPI organizations do not provide scoped package names. An ordinary verified
   account can publish `agent-work-runtime` before an organization application is approved.
4. After the workflow is reviewed and integrated, configure its trusted publishing
   identity using repository owner `originoneai`, repository `agent-work-runtime`,
   workflow `distributions.yml`, and environment `package-registries`.
   PyPI supports a pending trusted publisher for the new project. npm publisher
   settings must be configured on each of the four packages after first publication.

Official procedures: [npm organizations](https://docs.npmjs.com/creating-an-organization/),
[npm account conversion](https://docs.npmjs.com/converting-your-user-account-to-an-organization/),
[PyPI organizations](https://docs.pypi.org/organization-accounts/org-acc-faq/),
[npm trusted publishers](https://docs.npmjs.com/trusted-publishers/),
[PyPI pending publishers](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/).

## Build, collect, then publish

The `npm and PyPI preview distributions` workflow builds and installs both packages
on three native runners. Push/PR runs only build and check. A manual run also only
builds by default. Its optional publish job requires the registry identities above.
Before using manual dispatch, the workflow must exist on the repository's default
branch; this preparation branch does not change that branch automatically.

Download the three `distribution-*` artifacts into separate subdirectories. Assemble
them using the actual build commit:

```sh
python scripts/release/assemble_release.py /path/to/downloads \
  --output .local/assembled-preview --expected-sha FULL_BUILD_COMMIT
```

Assembly requires one clean build and successful installation receipt per platform,
all on the same source commit and version. It verifies every file hash and the same
platform-independent npm wrapper before producing four npm archives and three wheels.

For the first npm upload, authenticate with the owner account and complete npm's
verification in the browser, then use the already assembled artifacts:

```sh
npm login --registry=https://registry.npmjs.org/
python scripts/release/publish_npm.py .local/assembled-preview
```

The platform packages publish first, then the wrapper. Once their trusted publisher
settings are configured, subsequent versions can use the workflow's OIDC publish job
with provenance; no registry tokens need to be committed. PyPI can publish its first
version directly through a pending trusted publisher.

Registry uploads are not an atomic transaction. If an upload partially succeeds,
inspect the actual published package/version and its hashes before retrying. Never
overwrite an existing version or interpret a skipped/failed upload as success. Record
the exact registry receipts and verify installation from the registries after upload.
The current build and installation receipts alone do not claim a registry release.
