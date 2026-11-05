# CleanCopy

**复制一次 Markdown，同时提供富文本、易读纯文本与 Markdown，供目标应用选择。**

[English](README.md)

CleanCopy 是本地运行的 Rust 命令行工具，并提供 Linux X11 桌面应用。目标应用决定采用哪种剪贴板格式；“粘到哪里都对”是目标，并非已经验证的兼容性保证。

## 为什么需要它

AI 输出的 Markdown 适合终端和编辑器，但粘到文档或聊天窗口时，标题、表格和格式标记不一定如你所愿。[Claude Code issue #18170](https://github.com/anthropics/claude-code/issues/18170) 是本项目的需求来源之一。**发布前必须重新核对 GitHub stars 和 issue reactions 数量**，本文不引用未经核实的人气数字。

转换与剪贴板操作在本地完成，不向服务器发送文档，不需要服务端或 API key。安装依赖可能需要联网。

## 从源码安装

安装 Rust 工具链与 Cargo 后，在此仓库根目录执行：

```sh
cargo install --path crates/cleancopy-cli
```

将 Cargo 的二进制目录（通常为 `$HOME/.cargo/bin`）加入 `PATH`。命令名是 `cleancopy`，不是 `cleancopy-cli`。这里仅提供源码安装流程，不声称已有已发布的 crates.io 包、Homebrew/Scoop 包、安装脚本或预编译二进制。独立的打包工作流见[分发规划](docs/distribution.md)。

## 用法

```sh
cleancopy notes.md                    # HTML + 纯文本 + 自定义 Markdown
printf '# Hello\n\n- One\n- Two\n' | cleancopy
cleancopy                            # stdin 是终端时读取当前剪贴板
cleancopy - < notes.md               # 明确指定 stdin
cleancopy --plain notes.md           # 仅整理后的纯文本
cleancopy --html notes.md            # HTML + 纯文本兜底
cleancopy --markdown notes.md        # 普通文本通道中放 Markdown
cleancopy --print plain notes.md     # 输出，不写剪贴板
cleancopy --print html notes.md
cleancopy --print markdown notes.md
cleancopy inspect                    # 原生格式名称与字节数
```

指定文件优先于 stdin；未指定文件时，优先读取管道/重定向的 stdin，仅当 stdin 是终端时读取剪贴板。文件与 stdin 必须为 UTF-8。模式参数互斥。`--print` 不写剪贴板，但无文件且 stdin 是终端时仍会读取剪贴板。

| 模式 | 普通文本通道 | 其他格式 |
| --- | --- | --- |
| 默认 multi | 整理后的纯文本 | HTML、自定义 Markdown |
| `--plain` | 整理后的纯文本 | 无 |
| `--html` | 整理后的纯文本 | HTML |
| `--markdown` | 轻度清理的 Markdown 源文 | 无 |

编辑器忽略自定义 Markdown 类型，或粘贴 HTML 后转换有损时，使用 `--markdown`。**multi 模式下使用“粘贴为纯文本”快捷键得到的是整理后的纯文本，不能恢复原始 Markdown。** 提供自定义 Markdown 类型不代表目标应用会读取它。

退出码：`0` 成功（包括帮助/版本），`1` 输入为空或全为空白，`2` 剪贴板或 I/O 错误，`3` 参数错误。`inspect` 显示原生字节大小，可能包含终止符/内存分配填充；不是原子的剪贴板快照。

## 渲染行为

解析器启用表格、任务列表、删除线、自动链接和脚注。纯文本采用编号列表、四空格缩进、`【二至六级标题】`、`☐`/`☑`、代码分隔线、可读链接和编号脚注。表格按 Unicode 显示宽度对齐，超过 60 列转为记录式。详见[纯文本规则](docs/plain-text-rules.md)。

HTML 是完整的最小文档，代码与表格使用内联样式，任务项是静态字符。原始 HTML 被转义显示，不作为标签执行；危险链接/图片地址由 comrak 阻止。当前无语法高亮，CLI 也不输出 RTF（桌面应用的 Word/WPS 格式会附带 RTF）。Markdown 透传会移除终端转义/控制序列（保留制表符和换行）、裁剪首尾空白并统一换行，不是逐字节保留，也不会重新排版为 CommonMark。纯文本和 Markdown 在 Windows 使用 CRLF，其他平台使用 LF；HTML 始终使用 LF。非空纯文本以换行结束，Markdown 裁剪首尾空白。

## 原生剪贴板与验证边界

| 平台 | multi 模式格式 | 当前证据 |
| --- | --- | --- |
| macOS | `public.html`、`public.utf8-plain-text`、`net.daringfireball.markdown` | **仅编译检查，未运行验证** |
| Windows | `HTML Format`（CF_HTML，UTF-8 字节偏移）、`CF_UNICODETEXT`（UTF-16）、注册格式 `text/markdown` | **仅编译检查，未运行验证** |
| Linux X11 | `text/html`、`text/markdown`、`UTF8_STRING`、`TEXT`、`text/plain`、`text/plain;charset=utf-8`、`text/plain;charset=UTF-8` | **唯一运行验证的平台**，使用隔离 Xvfb |

Linux CLI 退出后由后台 holder 继续响应剪贴板请求，直到其他程序接管。需要可用的 X11 连接（`DISPLAY`）。不支持原生 Wayland；XWayland 可能提供 X11 连接，但到原生 Wayland 应用的桥接尚未验证。未实现剪贴板管理器交接。

Xvfb 读回不等于真实应用粘贴测试。Word、WPS、微信、飞书、Obsidian、Typora 等仍需单独实测。参见[兼容性状态](docs/compat.md)与[验证记录](docs/verification.md)。[设计文档](docs/design.md)包含提案及历史里程碑，不应当作功能完成或版本发布的证明。

## 桌面应用（仅 Linux X11）

`cleancopy-app` 是常驻后台的助手，在粘贴时转换内容。目前只能从源码构建，没有打包或发布：

```sh
cargo install --path crates/cleancopy-desktop
cleancopy-app              # 运行后台服务
cleancopy-app --settings   # 打开设置
```

- **快捷键：** Ctrl+Shift+V 是智能粘贴，按前台应用配置的格式粘贴；Ctrl+Alt+V 打开完整粘贴菜单。两者都可修改，普通 Ctrl+V 不受影响。
- **格式：** 编号纯文本、Word/WPS 富文本（HTML + RTF）、Markdown 原样、去符号纯文本、HTML 源码（默认关闭）。超过 2 MB 的文本不提供 Word/WPS 富文本，静默粘贴时回退为编号纯文本。
- **来源：** 剪贴板中的 Markdown 或 HTML，例如来自 ChatGPT、Gemini 或浏览器。普通文本默认原样粘贴。
- **剪贴板：** 目标应用读取完成后恢复原内容；如果你已复制了新内容，则不恢复。
- **其他：** 托盘菜单、新手引导、TOML 设置与按应用规则、撤销上一次转换，以及反复崩溃后进入安全模式。

热键后台、原生粘贴和托盘仅在 Linux X11 上提供。不支持原生 Wayland。**macOS 和 Windows 只有设置与预览界面。** 运行证据来自隔离的 Xvfb，不是真实 Word/WPS 粘贴测试。详见[桌面端 README](crates/cleancopy-desktop/README.md)与[验收记录](docs/acceptance.md)。

## Claude Code / Codex 集成

先安装 CLI，确保 agent 的 shell 能找到 `cleancopy` 并访问桌面剪贴板。远程或容器内的 agent 不会自动操作本机笔记本的剪贴板。先审阅集成文件；复制操作会覆盖剪贴板，内容也可能被剪贴板历史软件保存。

### 项目本地安装 Claude Code 模板

从 CleanCopy 根目录将集成模板复制到**目标项目**（替换路径），而非全局配置：

```sh
PROJECT=/absolute/path/to/your-project
mkdir -p "$PROJECT/.claude/commands" "$PROJECT/.claude/skills/cleancopy"
cp integrations/claude-code/commands/copy.md "$PROJECT/.claude/commands/copy.md"
cp integrations/claude-code/skills/cleancopy/SKILL.md "$PROJECT/.claude/skills/cleancopy/SKILL.md"
```

已有同名文件时先比较，不要直接覆盖。命令模板用于显式 `/copy`；skill 说明何时调用 CLI，包括纯文本场景的 `--plain`。模板是否已提供与 CLI 功能是两回事；这不是已实现的 `cleancopy integrate` 安装命令。

Codex 用户可审阅 `integrations/codex/AGENTS.snippet.md`，手动合并到目标项目的 `AGENTS.md`，不要覆盖现有内容。不依赖模板也可以先把回答写入 UTF-8 文件，再执行：

```sh
cleancopy /absolute/path/to/answer.md
cleancopy --plain /absolute/path/to/answer.md
```

不要直接把不可信回答插入 shell 命令。优先使用文件；使用 quoted heredoc 时，分隔符不能出现在内容中。权限提示及命令执行仍受 agent 环境控制。

### 可选 Stop hook：绝不自动启用

每次回答后自动复制会覆盖其他剪贴板内容，因此只能由用户主动选择，不包含在上述安装步骤中。Hook 输入的当前核实结果见[集成验证记录](docs/verification.md)。本项目不提供可直接安装的 Stop-hook 模板。如果自行实现，请先核对当前 Claude Code hook schema 与获取最后回答的方式，再手动合并到目标项目的 `.claude/settings.json`，保留已有配置。Hook 事件 JSON 不一定含回答正文，**不可将原始事件 JSON 直接管道输入 `cleancopy`**。Hook 应提取目标回答、在无内容时不覆盖剪贴板，并避免递归执行。CleanCopy 自身不会安装或启用 Stop hook。

## 开发

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

图形剪贴板和桌面端测试会覆盖剪贴板，默认忽略；CI 在隔离的 Xvfb 上运行它们。隔离 Xvfb 命令、安全注意事项及兼容性报告见[贡献指南](CONTRIBUTING.md)。

工作区包括解析/渲染库 `cleancopy-core`、原生访问与 CF_HTML 库 `cleancopy-clipboard`、命令行 `cleancopy-cli`、桌面应用 `cleancopy-desktop`。输入样例在 `fixtures/`，golden 快照在 `tests/snapshots/`。

## 路线图（尚未实现的承诺）

- macOS / Windows 剪贴板运行验证及真实应用粘贴矩阵。
- macOS 和 Windows 桌面适配（热键、原生粘贴、托盘）。
- 原生 Wayland、剪贴板管理器互操作。
- CLI 监听模式、交互式集成安装。
- 语法高亮、编辑器/浏览器扩展。
- 验证后的分发打包与真实录屏演示。

CLI 不支持 `--watch`、`--normalize`、daemon/config/integrate 子命令或自动识别前台应用；热键、设置和按应用规则属于桌面应用。这里没有演示录屏，也不暗示已经制作。

## 许可

可自行选择 **MIT OR Apache-2.0**，完整条款见 [LICENSE-MIT](LICENSE-MIT) 与 [LICENSE-APACHE](LICENSE-APACHE)。贡献采用相同双许可；参见 [CONTRIBUTING.md](CONTRIBUTING.md) 和 [CHANGELOG.md](CHANGELOG.md)。
