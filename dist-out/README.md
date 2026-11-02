# dist-out (local build, not published)

Rebuilt on **2026-11-02 UTC** with cargo-dist **0.32.0**:

```sh
dist build --artifacts=local --target x86_64-unknown-linux-gnu
```

Source HEAD at build time: **`d1c5fddba0623712fd49b88e58e47bf31abcc73a`**.
Cargo used the optimized `dist` profile (including valid cached compilation
outputs). The pre-existing working-tree modification to
`docs/screenshots/e2e-nonsilent-1.png` was left untouched and is not a compiled
input or archive asset. No Rust source changes were pending.

The archives and cargo-dist checksum sidecars were copied from `target/distrib/`
to this directory. SHA-256 was independently recomputed from both copied archives
and matched the corresponding sidecars:

```text
347177fba2b488e8ec934249047b18d0ab7ea61fbb007b69aeacad2a41471cb1  cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz
8b6f2c228fcababe3825b582b5594cf35e507087b9909cad56dee7999d5342ed  cleancopy-desktop-x86_64-unknown-linux-gnu.tar.xz
```

Both archives were extracted to temporary directories and their actual binaries
executed successfully:

- CLI: `cleancopy --print plain`, with input `# Archive check\n\n**bold**\n`,
  asserted exact stdout `Archive check\n\nbold\n` and exit status 0.
- Desktop: `cleancopy-app --help`, asserted `Usage:` in stdout and exit status 0.
  This is an executable smoke check, not a GUI acceptance test.

**No installers were generated:** `dist-workspace.toml` has `installers = []`.
Linux x86_64 archives are produced locally; no `.deb`, `.rpm`, AppImage, or
installation/upgrade/uninstall validation is claimed. Archives and sidecars are
git-ignored; only this README is committed.

**macOS and Windows were not built or run here.** See
[`docs/distribution.md`](../docs/distribution.md) for native-host commands and
limitations. No artifacts were published, pushed, tagged, or released.
