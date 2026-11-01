# egui-winit 0.31.1 X11-only feature patch

Source: crates.io `egui-winit` 0.31.1, https://github.com/emilk/egui/tree/0.31.1/crates/egui-winit.
Upstream copyright: Copyright (c) 2018-2021 Emil Ernerfeldt <emil.ernerfeldt@gmail.com>.
Licensed MIT OR Apache-2.0. See repository root [LICENSE-MIT](../../LICENSE-MIT), which preserves the upstream MIT copyright notice and permission terms, and [LICENSE-APACHE](../../LICENSE-APACHE).
Upstream license reference: https://github.com/emilk/egui/blob/0.31.1/LICENSE-MIT.

Only `Cargo.toml` feature wiring differs from upstream:
- Remove `smithay-clipboard` from `clipboard`.
- Keep an explicit `smithay-clipboard = ["dep:smithay-clipboard"]` feature for upstream Rust cfg checks, and enable it only through `wayland`.

Eframe 0.31.1 unconditionally enables egui-winit's clipboard feature. Upstream couples that feature to smithay-clipboard, which brings Wayland even when the application enables only X11. This patch retains X11 arboard/editor clipboard support and compiles no Wayland packages. No Rust implementation changes. `Cargo.toml.orig` is retained as the upstream reference. Revisit when upgrading eframe.
