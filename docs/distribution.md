# Distribution preparation (W4, no release)

Verified locally **2026-11-01**. This is configuration and planning only.
Nothing was published, pushed, tagged, committed, or released.

## Tool and initialization evidence

```sh
cargo install cargo-dist --locked
# Installed cargo-dist 0.32.0; its executable is named dist.
~/.cargo/bin/dist --version
# cargo-dist 0.32.0
~/.cargo/bin/dist init --yes --skip-generate
```

`cargo dist` was tried but is not an available Cargo subcommand for this
installation. Use `dist` with `$HOME/.cargo/bin` on PATH. Installation succeeded;
the upstream locked dependency graph emitted yanked-package warnings. This did
not change CleanCopy's Cargo.lock.

Noninteractive init added `[profile.dist]` to Cargo.toml and created
`dist-workspace.toml`. Afterwards `ci` was deliberately set to `[]` and installers
left empty: no generated release automation, shell installer, Homebrew formula,
Scoop manifest, registry publication, or GitHub hosting is enabled. No repository
remote is configured here; no speculative repository URL was added.

## Local plan

```sh
dist plan
```

Passed locally. The command prints `announcing v0.1.0` as its **local plan label**;
it does not create a git tag, announcement, or remote release. Planned artifacts:

- `source.tar.gz`, `sha256.sum`, and per-archive SHA-256 files;
- `cleancopy-cli-aarch64-apple-darwin.tar.xz`;
- `cleancopy-cli-x86_64-apple-darwin.tar.xz`;
- `cleancopy-cli-aarch64-unknown-linux-gnu.tar.xz`;
- `cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz`;
- `cleancopy-cli-x86_64-pc-windows-msvc.zip`.

Binary name remains `cleancopy` (`cleancopy.exe` on Windows). Planning is not
artifact building or target execution: in particular, Linux aarch64 is a planned
build target, not a tested platform. Both license texts are included as static
assets by cargo-dist's automatic license discovery once present.

## Workflow safeguards

`.github/workflows/release.yml` is hand-written and named
**Distribution plan (no release)**. It has only `workflow_dispatch`, read-only
`contents` permission, checkout without persisted credentials, pinned cargo-dist
0.32.0 installation, and `dist plan --output-format=json`. There is no tag/push
trigger and no upload, publish, GitHub release, package manager, or tag-creation
step. The hosted workflow was **not run**; the equivalent plan was run locally.

Future release work requires separate authorization, real repository metadata,
platform/application validation, installer design, signing decisions, and a
reviewed publication workflow. Do not replace this workflow with generated
release CI as part of W4 preparation.
