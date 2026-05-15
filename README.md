# OpenSplit

A fast, cross-platform terminal harness for launching and splitting CLI tools
like [opencode](https://opencode.ai), codex, claude, aider, plain shells, or
anything else. Right-click any pane to split horizontally or vertically, drag
the divider to resize, and (optionally) inherit the source pane's SSH session
into the new split so you don't have to re-authenticate.

Built with Rust + Tauri + xterm.js. Ships as a single small native binary on
Windows, Linux, and macOS.

## Status

Early development. Core architecture in place:

- [x] Project scaffold
- [x] PTY abstraction over `portable-pty` (Windows ConPTY + Unix PTY)
- [x] Binary pane tree with horizontal/vertical splits
- [x] Drag-to-resize splitters
- [x] xterm.js per pane wired to backend PTY
- [x] Right-click context menu (split H / split V / close)
- [x] Profile config (default profile: `opencode`)
- [x] SSH inheritance scaffolding (ControlMaster + foreground-process detection)
- [ ] Polished theming / settings UI
- [ ] Tabs
- [ ] Session save/restore

## Quick start (development)

Prerequisites:

- Rust stable (`rustup default stable`)
- Node 20+
- Platform deps for Tauri 2 (see [tauri.app](https://tauri.app/start/prerequisites/))

```bash
# install JS deps
npm install

# run dev (hot-reload frontend, native window)
npm run tauri dev

# build a release binary + installer (.msi / .exe on Windows, .AppImage / .deb on Linux)
npm run tauri build
```

The built binary is at `src-tauri/target/release/opensplit(.exe)` and platform
installers under `src-tauri/target/release/bundle/`.

## Usage

```bash
opensplit                # launches the default profile (opencode)
opensplit codex          # launches the named profile
opensplit -- bash -l     # raw command, no profile
```

Inside a pane:

- **Right-click** → split horizontally / split vertically / close pane
- **Drag a splitter border** to resize
- **Ctrl+Shift+H / Ctrl+Shift+V** to split via keyboard
- **Ctrl+Shift+W** to close the focused pane

## Configuration

Config lives at:

- Windows: `%APPDATA%\opensplit\config.toml`
- Linux: `$XDG_CONFIG_HOME/opensplit/config.toml` (usually `~/.config/opensplit/config.toml`)
- macOS: `~/Library/Application Support/opensplit/config.toml`

A default config is created on first run. See `opensplit.example.toml` in the
repo for all options.

## SSH inheritance

When you split a pane whose foreground process is `ssh`, OpenSplit will try, in
order:

1. **OpenSSH ControlMaster** — if the source `ssh` invocation has a control
   socket, the new pane reuses it. Instant, no re-auth. Configure once in your
   `~/.ssh/config`:

   ```
   Host *
     ControlMaster auto
     ControlPath ~/.ssh/cm-%r@%h:%p
     ControlPersist 10m
   ```

2. **Command replay** — OpenSplit detects the `ssh user@host` command from the
   process tree and re-runs it in the new pane.

3. **Fallback** — spawns the default shell; you re-ssh manually.

## Architecture

```
opensplit/
├── src-tauri/              Rust backend
│   ├── src/
│   │   ├── main.rs         entry, window mgmt, command registration
│   │   ├── pty.rs          portable-pty wrapper, async read/write
│   │   ├── session.rs      foreground-process detection, SSH inheritance
│   │   ├── config.rs       TOML config + profile resolution
│   │   └── ipc.rs          Tauri command handlers, event bridge
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                    Svelte frontend
│   ├── lib/
│   │   ├── PaneTree.ts     recursive binary tree (Leaf | Split)
│   │   ├── ipc.ts          typed Tauri command wrappers
│   │   └── theme.ts
│   ├── components/
│   │   ├── App.svelte
│   │   ├── PaneView.svelte recursive renderer
│   │   ├── Terminal.svelte xterm.js host for one pane
│   │   ├── Splitter.svelte drag-resize divider
│   │   └── ContextMenu.svelte
│   └── main.ts
└── opensplit.example.toml
```

A pane is a binary tree node:

```ts
type PaneNode =
  | { kind: "leaf"; id: string; paneId: string; profile: string }
  | { kind: "split"; id: string; direction: "h" | "v"; ratio: number; a: PaneNode; b: PaneNode }
```

Splitting a leaf replaces it with a `split` node whose two children are the
original leaf + a new leaf. This makes "split twice for three panes, three
times for four, etc." trivial and recursive.

## Contributing

PRs welcome. Please run `cargo fmt && cargo clippy` and `npm run check` before
opening a PR.

## License

MIT — see [LICENSE](./LICENSE).
