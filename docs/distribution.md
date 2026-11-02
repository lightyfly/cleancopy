# Distribution preparation (W4, no release)

Initial configuration/plan verified locally **2026-11-01**; Linux x86_64 local
archives rebuilt **2026-11-02**. Build records and documentation are committed;
nothing was published, pushed, tagged, or released.

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

The current workspace also packages `cleancopy-desktop` (`cleancopy-app`), with
matching per-target archives. The list above records the original CLI plan,
not a claim that every target has been built.

Binary name remains `cleancopy` (`cleancopy.exe` on Windows). Planning is not
artifact building or target execution: in particular, Linux aarch64 is a planned
build target, not a tested platform. Both license texts are included as static
assets by cargo-dist's automatic license discovery once present.

## Linux x86_64 rebuild (executed 2026-11-02)

Using the existing cargo-dist 0.32.0 installation:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
git rev-parse HEAD
dist build --artifacts=local --target x86_64-unknown-linux-gnu
cp target/distrib/*-x86_64-unknown-linux-gnu.tar.xz dist-out/
cp target/distrib/*-x86_64-unknown-linux-gnu.tar.xz.sha256 dist-out/
sha256sum dist-out/*-x86_64-unknown-linux-gnu.tar.xz
```

Built from `d1c5fddba0623712fd49b88e58e47bf31abcc73a`. Exact hashes and extracted
binary smoke-check results are in [`../dist-out/README.md`](../dist-out/README.md).
The command repackaged valid optimized Cargo cache outputs; it was not a clean
recompilation. Only a pre-existing screenshot modification was pending; it was
not included in either archive. No Linux aarch64 build was run in this refresh.

`installers = []` is intentional: these are portable archives, not installers.
There is no generated `.deb`, `.rpm`, AppImage, macOS `.dmg`/`.pkg`, or Windows
`.msi`/setup program. Installer generation requires separate configuration and
installation/uninstallation validation; it was not silently enabled here.

## macOS native build instructions (NOT run here)

Use a Mac with Rust, Xcode Command Line Tools (`xcode-select --install`), and
cargo-dist 0.32.0. Check out the desired source commit and record its full hash.
On Apple Silicon run:

```sh
cargo install cargo-dist --version 0.32.0 --locked
rustup target add aarch64-apple-darwin
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
dist build --artifacts=local --target aarch64-apple-darwin
shasum -a 256 target/distrib/*-aarch64-apple-darwin.tar.xz
```

On an Intel Mac substitute `x86_64-apple-darwin` in the target and archive names.
Prefer native hosts for each architecture; cross-architecture compilation does
not establish runtime support. Extract the CLI and desktop archives and test on
the intended Mac. Archive generation does not sign/notarize the binaries or
validate Accessibility permissions, native hotkeys, paste, tray, or Word/WPS.
The native macOS desktop automation adapter is not implemented; a build or
preview window must not be reported as desktop acceptance. Linux-to-macOS
cargo-dist cross-compilation is not used/supported by this procedure.

## Windows native build instructions (NOT run here)

Use Windows x86_64 with Rust's MSVC toolchain and Visual Studio Build Tools
(C++ desktop workload and Windows SDK). In PowerShell, from the checked-out
repository:

```powershell
git rev-parse HEAD
cargo install cargo-dist --version 0.32.0 --locked
rustup target add x86_64-pc-windows-msvc
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
dist build --artifacts=local --target x86_64-pc-windows-msvc
Get-ChildItem target/distrib/*-x86_64-pc-windows-msvc.zip | Get-FileHash -Algorithm SHA256
```

Extract both ZIP archives; run `cleancopy.exe --print plain` with sample stdin
and `cleancopy-app.exe --help`, then perform actual GUI/application checks
separately. The native Windows desktop automation adapter is not implemented;
compilation is not evidence of global-hotkey or Word acceptance. No code signing
or installer is configured. Linux cross-building would require `cargo-xwin`,
MSVC CRT/SDK downloads, and Microsoft license acceptance; it was not attempted
in this refresh. These native-host instructions are **unexecuted steps**, not
successful-build claims.

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
