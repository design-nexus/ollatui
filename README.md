# ollatui

A terminal UI for [Ollama](https://ollama.com). Chat with local models, pull from the Ollama library or a searchable Hugging Face GGUF catalog, and theme the whole interface, including following the Omarchy theme when it is installed.

ollatui talks to the Ollama HTTP server at `http://127.0.0.1:11434` by default. It does not run models itself.

## Install

You need a stable Rust toolchain (`cargo`) and [Ollama](https://ollama.com/download) installed separately.

From a clone:

```bash
git clone https://github.com/design-nexus/ollatui.git
cd ollatui
./install.sh
```

Or in one step:

```bash
curl -fsSL https://raw.githubusercontent.com/design-nexus/ollatui/main/install.sh | bash
```

The binary is installed to `~/.local/bin/ollatui`. Use another location with `--prefix`:

```bash
./install.sh --prefix "$HOME/.local"
```

If `~/.local/bin` is not on your `PATH`, the script prints the line to add.

Build without installing:

```bash
cargo build --release
./target/release/ollatui
```

## Usage

```bash
ollatui
```

Tab and Shift+Tab move between Chat, Models, Marketplace, and Settings. `?` opens the keymap. `q` quits when you are not typing. Ctrl+C always quits.

The mouse wheel, Page Up, and Page Down scroll a reply. From a one-line prompt, Up and Down scroll it too. A bar on the right shows your place. Scrolling up stays put while a reply is still streaming.

Enter sends. Alt+Enter or Ctrl+J inserts a line. Esc stops the reply. Ctrl+N starts a chat, Ctrl+M opens the model list, and Ctrl+P edits the system prompt. On a thinking reply, Esc moves to the transcript and Enter expands the thinking block.

### Slash commands

Type these in the prompt and press Enter.

| Command | What it does |
| --- | --- |
| `/start` `/stop` `/restart` | Start, stop, or restart the local Ollama service. A password prompt appears when the service needs administrator permission. |
| `/model` | Open the installed-model list. |
| `/model name` | Select the installed model whose name matches. |
| `/new` | Start a new chat. |
| `/clear` | Wipe the current chat. |
| `/cancel` | Stop the reply that is generating. |
| `/system` | Edit the system prompt. `/system text` sets it directly. |
| `/theme` | Show the theme. `/theme dracula` switches to one. |
| `/pull name` | Download a model (`llama3.2`, `qwen3:8b`, or `hf.co/user/repo:file.gguf`). |
| `/help` | Show the keymap and commands. |
| `/quit` | Leave the app. |

### Screens

**Models.** Lists what is installed and what is loaded. Enter chats with the highlighted model. `d` deletes it, `u` unloads it, and `p` pulls by name.

**Marketplace.** Searches the Hugging Face Hub for GGUF repos, sorted by downloads, likes, or last update. Enter a repo to list its files, then Enter again to pull one. The download goes through Ollama as `hf.co/user/repo:file.gguf`, so resume and registration stay with Ollama. A progress bar at the bottom shows bytes received. Gated repos need `HF_TOKEN` in the Ollama service environment. Hugging Face serves the file from a CDN host; ollatui allows that redirect for `hf.co` pulls.

**Settings.** Host, default model, temperature, top P, context length, seed, keep alive, and token stats. Two switches control the service: start Ollama when ollatui opens, and stop it when ollatui quits. Both are off until you turn them on. Changes are saved immediately and sent as Ollama options on the next reply.

## Themes

The built-in list is the set that shows up across terminal and editor roundups: Catppuccin (Mocha, Macchiato, Frappé, Latte), Tokyo Night (Night, Storm, Day), Dracula, Gruvbox (dark and light), Nord, One Dark Pro and One Light, Solarized (dark and light), Rosé Pine (main, Moon, Dawn), Kanagawa, Everforest Dark, and Monokai Pro.

**Follow Omarchy** reads `~/.local/state/omarchy/current/theme/colors.toml` and the theme name beside it. Older installs are read from `~/.config/omarchy/current/`. The first launch uses this when the file exists, otherwise Catppuccin Mocha. While Follow Omarchy is selected, changing the desktop theme repaints the app. The row stays in the list on machines without Omarchy and cannot be applied there.

**Terminal** uses ANSI colors, so the UI picks up the palette of Ghostty, Kitty, Alacritty, Foot, or whatever you are running.

## Files

| Path | Contents |
| --- | --- |
| `~/.config/ollatui/config.toml` | Host, theme, sampling, and the autostart switches |
| `~/.local/share/ollatui/chats/` | Saved conversations |

## Develop

```bash
cargo test
cargo run
```
