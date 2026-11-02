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

## Rebuild 2026-11-02 10:20 (UTC+8)
Built from commit 358232c2296f6979b24384b42d8b27a7a90a4080 with `dist build --artifacts=local --target x86_64-unknown-linux-gnu`.
- cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz: 608d5e3f36165997b37b9d08b75a913f43ea059db664fbb2a31fcf99aa72dcaa
- cleancopy-desktop-x86_64-unknown-linux-gnu.tar.xz: 116fe9176ca08f8f17016af3952d6f739a13c764b2ebbb49b7306d567f0a2758

## Rebuild 2026-11-02 11:03 (UTC+8)
Built from commit e4c6af4ebc4d712d408fd7c97a293d611a1c4b8a with `dist build --artifacts=local --target x86_64-unknown-linux-gnu`.
- cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz: d8e23e1badc37f261ecfa35cb86e786cf29d6f1921f0ed8ee90b21473b257bd1
- cleancopy-desktop-x86_64-unknown-linux-gnu.tar.xz: 7b7a1d696c142e59af169ba73015f5355ab57db54949a91587c7d81c8a407731

## Rebuild 2026-11-02 11:12 (UTC+8)
Built from commit 0513dcd1797b63d8243ed335f29c6c80b9e4bcb9 with `dist build --artifacts=local --target x86_64-unknown-linux-gnu`.
- cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz: 604e4c2f297ad7708faa9e6cd7eb7f5e565fe26e378886706d84b802b150829e
- cleancopy-desktop-x86_64-unknown-linux-gnu.tar.xz: e1512ec1743565cd8e219d52e76374874d74fd8a72d255b55e21c711674020ab

## Rebuild 2026-11-02 11:17 (UTC+8)
Built from commit 531dc7e654ea1763b56a71df15f1bd93a930a6f2 with `dist build --artifacts=local --target x86_64-unknown-linux-gnu`.
- cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz: cc0da5140256ec4138402cc86d74244c232287e25949b33a32838f5e413b6d9f
- cleancopy-desktop-x86_64-unknown-linux-gnu.tar.xz: 67ad0398c80cc47213f46f9871690d8abafcaa29c646259347abc0567d5d46fc

## Rebuild 2026-11-02 11:28 (UTC+8)
Built from commit 16c6aa9d322f5f018464b672640a899dd4c04efd with `dist build --artifacts=local --target x86_64-unknown-linux-gnu` (palette #2B2B33 + onboarding).
- cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz: 93e778a8389f62a87c84c744ec34d8069a06d520efb0dffd112bc9abc741eac1
- cleancopy-desktop-x86_64-unknown-linux-gnu.tar.xz: 9bdf22225b45a87104dcf17dee74a544069cd716fb2a9f3a9b64f2fa9bf9858a

## Rebuild 2026-11-02 11:51 (UTC+8)
Built from commit 720b4f4e4b8bfbe5136354b6268f80b80f2fe4c9 with `dist build --artifacts=local --target x86_64-unknown-linux-gnu` (round v3: B-1..B-4, item 6 numbering, onboarding tweaks). Smoke checks re-run on the extracted archives.
- cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz: 9414874b75de9601fec0fcb5414b2cc6877812ce90779221874b4b75a323c960
- cleancopy-desktop-x86_64-unknown-linux-gnu.tar.xz: 1d351b8bc115be82d6daf3fd2f4068adc83946948947103683638a316c7811a1
