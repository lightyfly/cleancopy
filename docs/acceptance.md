# Desktop acceptance evidence

Assessed **2026-11-02** against the eight numbered criteria in
`/workspace/cleancopy-design/design.md`, §5 (lines 66–74). This is an evidence
report, **not a declaration that desktop acceptance is complete**.

## Evidence scope and status meanings

- **passed**: demonstrated within the explicitly stated Linux/mock scope.
- **failed**: concrete evidence contradicts a required threshold/behavior.
- **not verified**: required application/platform/measurement has not been
  exercised; supporting renderer tests do not establish end-user acceptance.
- Current workspace validation: **151 passed, 0 failed, 6 ignored**;
  `cargo fmt --all` and strict workspace/all-target Clippy succeeded without
  changes or warnings. See [workspace-validation.md](workspace-validation.md).
- Historical native evidence: [desktop-e2e-run.log](desktop-e2e-run.log), two
  passing Linux/X11 integration tests (`desktop_runtime_matrix` and
  `hotkey_pastes_formatted_text_and_restores_all_native_bytes`). These were
  **not rerun in this refresh**, and are ignored by the current workspace run.
  Do not add these historical passes to the current 151-test total.
- Native peers are independent X11 fixture processes with classes such as
  `gedit` and `WINWORD.EXE`, **not installed gedit or Microsoft Word**. Real
  ChatGPT/Gemini copy workflows, Word/WPS, Notepad/TextEdit, macOS, and Windows
  are **not verified**. Native macOS/Windows desktop automation adapters remain
  unimplemented. See [desktop-verification.md](desktop-verification.md) and
  [compat.md](compat.md).

Test names below passed in the current workspace run unless explicitly labeled
historical native evidence. Paths are relative to the repository.

## Eight design acceptance criteria

| # | Criterion from design §5 | Status | Concrete evidence and remaining limits |
|---|---|---|---|
| 1 | ChatGPT/Gemini web and desktop → Word/WPS: Heading 1/2 styles, editable tables, preserved bold | **not verified** | `crates/cleancopy-core/tests/desktop_html_detection.rs`: `word_headings_have_semantics_builtin_styles_and_outline_levels`, `setext_headings_and_heading_inline_formatting_survive`, `word_rendering_preserves_base_body_for_non_heading_content`; `tests/rtf.rs`: `built_in_heading_styles_and_scoped_inline_formatting`, `editable_tables_preserve_alignment_empty_cells_and_formatting`. Historical log lines 27–28 confirms semantic Word HTML received by a fixture. No real source-copy/application matrix, Word styles-pane inspection, or table editing was performed. |
| 2 | Notepad/TextEdit: no Markdown syntax, selected numbering style, nesting deeper than three levels | **not verified** | `crates/cleancopy-core/tests/numbered.rs`: `exact_settings_preview_and_newline_contract`, `alternative_settings_previews`, `deep_lists_and_sibling_resets_in_all_styles` (five levels), `inline_ast_stripping_tables_quotes_and_verbatim_code`. `crates/cleancopy-desktop/tests/engine.rs`: `silent_numbered_rule_renders_expected_chinese_and_restores_all_formats`. Historical log lines 25–26: exact numbered paste to Linux fixture. Actual Notepad/TextEdit not tested. Literal Markdown-like characters inside code/escaped content are intentionally preserved. |
| 3 | Hotkey → menu ≤100 ms; 10,000-character conversion + paste ≤300 ms on M1/i5-class hardware | **failed** | Historical log line 29: secondary menu **430.17 ms >100 ms**. Line 48: 10k conversion + native paste **431.31 ms >300 ms** (approximately 432 ms). See measurements below. Hardware equivalence is not established. The E2E test prints these measurements but does not assert the performance budgets. |
| 4 | Unmapped App must open secondary menu; remembering makes the next invocation recognized | **passed** (silent mode, Markdown, popup fallback) | `unknown_secondary_remembers_and_honors_configured_items`, `unchecked_remember_does_not_create_rule_and_checked_choice_survives_toml` in `crates/cleancopy-desktop/tests/engine.rs`; `remembering_each_identifier_survives_restart_and_reselection` in `tests/config.rs`. Historical log lines 29–34: actual checkbox click, key 2, persisted rule, next paste silent. [Secondary menu](screenshots/e2e-secondary.png), [remember checked](screenshots/e2e-remember-checked.png). The design's literal “must” is scoped by the current silent-mode rules: silent off opens the full menu; configured fixed fallback/non-Markdown original-paste behavior need not open this menu. |
| 5 | Undo restores original text; after paste the clipboard equals the originally copied content | **passed** (Linux fixtures and mocked engine) | `recognized_silent_word_restores_every_original_byte_after_150ms`, `numeric_choices_one_through_five_restore_exact_original`, `undo_is_once_same_target_and_restores_current_not_old_clipboard`, `undo_effect_order_is_undo_then_original_paste_then_exact_current_restore` in `tests/engine.rs`. Historical log lines 25–49 repeatedly compares native bytes/type/format; lines 39–42 records actual Ctrl+Z then original paste, and lines 99–100 records native smoke restoration. [Undo menu](screenshots/e2e-undo.png). Restoration is delayed; new external clipboard content is intentionally not overwritten. This does not validate real Word/WPS consumption timing or cross-platform restoration. |
| 6 | Settings preview refresh ≤50 ms; closing/reopening preserves configuration | **not verified** | `ui::settings::tests::view_renders_every_format_without_changing_configuration` checks rendering, **not latency**. `atomic_roundtrip_replaces_existing_file_and_creates_parent`, `all_formats_styles_and_fallbacks_roundtrip_through_disk` in `tests/config.rs` verify persistence. Historical log lines 50–53 and [settings](screenshots/e2e-settings.png)/[preview](screenshots/e2e-preview.png) are launch probes only. No measured ≤50 ms refresh or actual edit → close → reopen workflow evidence. |
| 7 | Permission denial does not crash; writes clipboard + notice and offers reauthorization guidance | **not verified** | `fallible_effects_do_not_panic_or_lose_required_restoration` in `tests/engine.rs` covers generic injected errors, not macOS Accessibility denial. No native denial/regrant, “已复制，请按 ⌘V” notice, yellow tray dot, or settings deep-link workflow was run. Native macOS automation is not implemented. |
| 8 | Screenshot match for spacing/alignment/colors; platform-specific macOS/Windows shortcut symbols | **not verified** | [menu](screenshots/menu.png), [secondary](screenshots/menu-secondary.png), [settings](screenshots/settings.png), [tray preview](screenshots/tray.png) and native captures exist. `ui::menus::tests::panels_fit_and_submenu_flips`, `keyboard_right_enter_and_escape` check layout/navigation, not pixel fidelity. No certified reference-image comparison or native macOS/Windows screenshots. Historical log line 68 reports no StatusNotifier host, so real tray-host integration is not demonstrated. |

Rows 4–5 are scoped functional passes, not whole-platform certification. The
full eight-criterion release gate is **not passed**.

## Current silent-mode rules (supersede original design defaults)

The original design §0 says default off and §1.2 describes double-press execution.
The current requested contract instead defaults **ON** and uses double press for
the **full menu**. These are intentional overrides, not tests of the old wording.

| Rule | Status | Concrete evidence |
|---|---|---|
| Silent mode defaults **ON**, including missing/empty/partial configuration | **passed** (configuration tests) | `defaults_and_platform_identities`, `default_toml_roundtrip_preserves_every_field_and_silent_override`, `missing_empty_and_partial_configs_use_complete_defaults` in `crates/cleancopy-desktop/tests/config.rs`. |
| Single press uses App→format and pastes without a menu | **passed** (mapped Markdown; Linux fixtures/mock engine) | `recognized_silent_word_restores_every_original_byte_after_150ms`, `silent_numbered_rule_renders_expected_chinese_and_restores_all_formats`, `silent_markdown_rule_preserves_source_and_restores_opaque_original` in `tests/engine.rs`; historical log lines 25–28. Default mappings route TextEdit/Notepad/gedit to numbered text, Word/WPS to rich text, Notion/Obsidian to Markdown; `every_builtin_alias_matches_individually_without_using_display_names` checks aliases. A single press waits 400 ms to arbitrate double press, not immediate paste. |
| Unknown App → secondary menu with **记住此 App** choice | **passed** (Markdown, popup fallback) | `unknown_secondary_remembers_and_honors_configured_items`, `unchecked_remember_does_not_create_rule_and_checked_choice_survives_toml`; historical log lines 29–34 and [remember checkbox](screenshots/e2e-remember-checked.png). Remember starts unchecked; selecting it persists the app rule. Fixed fallback and ordinary non-Markdown input have different configured behavior. |
| Double press → full menu, not two pastes | **passed** (Linux/mock engine) | `inclusive_double_press_always_full_and_never_double_pastes` covers 0/1/399/400 ms; `late_second_press_consumes_pending_only_once` covers late presses. Historical log lines 35–38; [full menu](screenshots/e2e-full.png). |
| Silent OFF → menu on every completed press | **passed** (Linux/mock engine) | `silent_off_opens_menu_on_every_completed_press`, `silent_false_first_menu_second_ignored_and_full_formats_respect_disabled_secondary`; historical log lines 43–47, [first](screenshots/e2e-nonsilent-1.png)/[second](screenshots/e2e-nonsilent-2.png) menu captures. A press while a menu is already open is ignored; subsequent completed invocations each reopen it. |
| Restore clipboard byte-exactly, not just visible plain text | **passed** (Linux native fixture + mock snapshots) | `recognized_silent_word_restores_every_original_byte_after_150ms`, `numeric_choices_one_through_five_restore_exact_original`, `silent_markdown_rule_preserves_source_and_restores_opaque_original`; native `desktop_runtime_matrix` compares payload bytes and native property type/format (only TARGETS ordering normalized). Historical log lines 25–49: five advertised targets = four payload formats plus TARGETS; native smoke lines 99–100 covers four targets. `no_overlapping_transaction_and_external_change_not_overwritten` and `restoration_errors_retain_state_retry_and_block_hotkeys` cover safety exceptions/retries. Not verified on macOS/Windows or real Word. |

## Recorded latency failures

These are the existing log's measurements, **not newly collected timings**:

| Native operation | Time | Required budget | Assessment |
|---|---:|---:|---|
| Unknown-app single press → secondary window mapped | 430.17 ms (~430 ms) | ≤100 ms | **failed** |
| Double press → full window mapped | 84.67 ms | ≤100 ms | Below budget in this sample only |
| Double press → undo menu mapped | 85.78 ms | ≤100 ms | Below budget in this sample only |
| Nonsilent single press → full menu, first / second | 47.47 / 47.82 ms | ≤100 ms | Below budget in these samples only |
| 10k characters, hotkey → conversion + native paste | 431.31 ms (~432 ms) | ≤300 ms | **failed** |

Source: `docs/desktop-e2e-run.log` lines 29, 35, 39, 43, 45, 48.
Single-press timings include the intentional **400 ms arbitration window** in
`crates/cleancopy-desktop/src/engine.rs`. Menu timing measures window mapping;
panel pixel readiness is checked separately by the harness. These are not
percentile measurements, hardware-qualified benchmarks, or evidence that the
performance acceptance gate passed merely because the native test exited 0.

## Screenshot provenance and outstanding work

The historical run recorded eight 1280×800 root-window captures (log lines
30–52). `docs/screenshots/e2e-nonsilent-1.png` was already modified in the working
tree before this refresh; it was neither regenerated nor staged here. Its current
bytes must not be represented as a newly verified capture of that historical run.
Other screenshots are evidence artifacts, not a pixel-perfect acceptance verdict.

Outstanding: fix the two measured latency failures; exercise real source/target
applications and editable Word/WPS tables/styles; measure preview refresh and
settings reopening; implement and validate macOS/Windows native workflows,
permission degradation and platform symbols; verify real tray integration and
reference visual fidelity. Native-platform build instructions in
[distribution.md](distribution.md) were documented but **not executed here**.
