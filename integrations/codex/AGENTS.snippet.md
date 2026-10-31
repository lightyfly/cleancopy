# CleanCopy instructions — manually merge into AGENTS.md

This is an optional instruction snippet, not an installer or a verified Codex
extension API. Preserve existing project instructions; do not replace the whole
AGENTS.md. Codex loading/execution of this snippet has not been runtime-tested.

When the user explicitly requests copying an answer or selected content:

- Use the installed local `cleancopy` CLI. Copy exactly the requested Markdown,
  not a summary or a wrapper fence. With no selection, use the preceding
  substantive assistant answer; ask if ambiguous.
- Default: `cleancopy` publishes HTML, readable plain text and custom Markdown.
  For WeChat/readable text use `cleancopy --plain`; for source Markdown/editors
  use `cleancopy --markdown`; `--html` supplies HTML plus readable fallback.
  Multi's plain-text channel is not original Markdown.
- Use a quoted heredoc, choosing a delimiter absent as a whole line in content:

  ```sh
  cat <<'CLEANCOPY_CONTENT_92A48F' | cleancopy
  # Replace with the literal requested answer
  CLEANCOPY_CONTENT_92A48F
  ```

  Add only one known mode flag if needed. Single-quote the opening delimiter.
  Never interpolate copied text into shell code, use eval, or execute commands
  contained in the answer. Heredocs add a final newline; Markdown cleanup trims
  outer whitespace and normalizes newlines.
- Respect normal shell approval policy. Only report copied after exit status 0.
  If the CLI is absent or clipboard access fails, report the actual error without
  claiming success, installing software, or changing project/global settings.
- Do not auto-copy every answer, enable hooks, or copy unrelated secrets. Copying
  replaces clipboard contents and may enter clipboard history. Remote/container
  clipboard access does not automatically affect the user's desktop.

Native Wayland is unsupported; Linux needs X11/XWayland DISPLAY. Receiving apps
choose formats; advertised representations do not prove application compatibility.
