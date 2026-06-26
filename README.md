# jqr

> A multi-format data query tool with an interactive terminal UI.

JSON, YAML, TOML, CSV — query them all with `jq` syntax, switch formats on the fly, and export results in any format.

## Why I Built This

I work with structured data every day — API responses, config files, data exports. I kept reaching for `jq` to inspect JSON, then switching to another tool for YAML, another for CSV. Each tool has its own query language, its own output format, its own quirks.

I wanted one tool that:

- **Speaks every format.** Load a YAML file, query it with `jq` syntax, export as CSV. No converters, no piping between tools.
- **Has a real interface.** Not just stdin → stdout, but an interactive workspace where I can type a query, see the result, refine, and compare — all without leaving the terminal.
- **Stays fast.** Rust binary, sub-millisecond startup, no runtime dependencies.

That's jqr.

## Screenshots

```
┌──────────────────────────────────────────────────────────────────┐
│ jqr · mode: Envelope · fmt: auto · tokens: 42   1:Env 2:Sch 3:Raw │
├──────────────────────────────────────────────────────────────────┤
│ › .agents | keys                  [Envelope]  105 tok             │
│ {"sample":["explore","hephaestus","librarian","metis","momus"],..}│
│                                                                   │
│ › .categories | length            [Envelope]  28 tok              │
│ {"schema":{"type":"integer"},"tokens_used":1,"value":14}          │
│                                                                   │
├──────────────────────────────────────────────────────────────────┤
│ › .                                                              │
├──────────────────────────────────────────────────────────────────┤
│ Enter run · Tab mode · Ctrl+O open · Ctrl+E export · ? help      │
└──────────────────────────────────────────────────────────────────┘
```

## Install

### From source (recommended)

**Prerequisites:** Rust 1.70+ ([install Rust](https://rustup.rs))

```bash
git clone https://github.com/your-username/jqr.git
cd jqr
cargo build --release
```

The binary is at `target/release/jqr`. Copy it somewhere in your `$PATH`:

```bash
sudo cp target/release/jqr /usr/local/bin/
# or
cp target/release/jqr ~/.local/bin/
```

Verify:

```bash
jqr --version
```

### Run without installing

```bash
cd jqr
echo '{"hello": "world"}' | cargo run --release -- '.'
```

## Quick Start

### Pipe mode (one-shot)

```bash
# Query JSON
echo '{"users":[{"name":"Alice"}]}' | jqr '.users[].name'

# Query a file
jqr -F data.json '.users | length'

# Switch to raw output (like jq)
echo '{"a":1}' | jqr -r '.'

# Token-budgeted output (great for LLM context windows)
echo '{"data":[1,2,3,4,5,6,7,8,9,10]}' | jqr -t 50 '.data'
```

### Interactive mode (TUI)

```bash
# Start with no data — shows welcome screen
jqr

# Pipe data in — enters interactive mode automatically
echo '{"users":[{"name":"Alice","age":30}]}' | jqr

# Load a file and enter interactive mode
jqr -i -F config.yaml
```

## Supported Formats

| Format | Read | Write/Export | File Extension |
|--------|------|--------------|----------------|
| JSON   | yes  | yes          | `.json`        |
| YAML   | yes  | yes          | `.yaml` `.yml` |
| TOML   | yes  | yes          | `.toml`        |
| CSV    | yes  | yes          | `.csv`         |

Format is auto-detected from content. Override with `--input` (`-I`).

## CLI Reference

### Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--input <fmt>` | `-I` | Input format: `auto`, `json`, `yaml`, `toml`, `csv` |
| `--output <mode>` | `-o` | Output mode: `schema`, `raw`, `compact` |
| `--raw` | `-r` | Raw JSON output (no schema envelope) |
| `--compact` | `-c` | Single-line JSON output |
| `--pretty` | `-p` | Pretty-printed output with indentation |
| `--tokens <N>` | `-t` | Cap output at roughly N tokens |
| `--sample-size <N>` | `-n` | Number of sample records (default: 5) |
| `--schema-only` | `-S` | Output schema only, no sample data |
| `--schema-format <f>` | `-f` | Schema style: `jsonschema`, `typescript`, `zod`, `pydantic` |
| `--repair` | `-R` | Repair malformed JSON before processing |
| `--explain` | `-x` | Analyze the filter without executing |
| `--file <path>` | `-F` | Read input from file instead of stdin |
| `--interactive` | `-i` | Force interactive mode |
| `--pretty` | `-p` | Pretty-print output |

### Subcommands

```bash
jqr mcp    # Start MCP server for AI agent integration
```

### Examples

```bash
# --- JSON ---
echo '{"name":"Alice","age":30}' | jqr '.name'
echo '{"name":"Alice","age":30}' | jqr -r '.'          # raw output
echo '{"name":"Alice","age":30}' | jqr -c '.'          # compact
echo '{"name":"Alice","age":30}' | jqr -S '.'          # schema only
echo '{"name":"Alice","age":30}' | jqr -p '.'          # pretty

# --- YAML ---
cat config.yaml | jqr -I yaml '.server.port'
jqr -I yaml -F config.yaml '.server'

# --- TOML ---
cat Cargo.toml | jqr -I toml '.package.version'

# --- CSV ---
cat users.csv | jqr -I csv '.[0].name'
cat users.csv | jqr -I csv 'map(.age) | add'

# --- Token budget ---
cat large.json | jqr -t 200 '.records'    # cap at ~200 tokens

# --- Schema export ---
jqr -S -f typescript -F api.json '.'      # TypeScript types
jqr -S -f jsonschema -F api.json '.'      # JSON Schema

# --- JSON repair (fix LLM output) ---
echo 'Here is the data: {"users": [{"name": "Alice"}, {"name": "Bob"' | jqr -R '.'

# --- Explain mode ---
jqr -x '.users | map(select(.age > 18)) | length'

# --- jq filter compatibility ---
echo '{"a":[1,2,3]}' | jqr '.a | length'              # 3
echo '{"a":[1,2,3]}' | jqr '.a | map(. * 2)'          # [2,4,6]
echo '{"a":[1,2,3]}' | jqr '.a | select(. > 1)'       # 2, 3
echo '{"a":1,"b":2}' | jqr 'keys'                      # ["a","b"]
echo '{"a":1,"b":2}' | jqr 'to_entries | map(.value)'  # [1,2]
```

## Interactive Mode (TUI)

### Keybindings

| Key | Action |
|-----|--------|
| `Enter` | Execute filter |
| `Tab` | Cycle output mode (Envelope → Schema → Raw → Compact → Pretty) |
| `Up` / `Down` | Recall filter history |
| `PgUp` / `PgDn` | Scroll transcript |
| `Ctrl+Up` / `Ctrl+Down` | Scroll one line |
| `Ctrl+O` | Open file (JSON/YAML/TOML/CSV) |
| `Ctrl+E` | Export data to multiple formats |
| `Ctrl+S` | Save current output |
| `Ctrl+F` | Cycle input format (Auto → JSON → YAML → TOML → CSV) |
| `Ctrl+R` | Reload input data |
| `Ctrl+L` | Clear transcript |
| `Ctrl+G` | Toggle English / 中文 |
| `?` | Toggle help overlay |
| `Esc` | Quit / cancel / clear input |
| `Ctrl+C` | Force quit |

### Features

**Transcript-style REPL.** Each query and its result stack as a "turn" in the transcript. Scroll back through history, compare results, refine queries — all without re-running the tool.

**Five output modes.** Press `Tab` to cycle:
- **Envelope** — schema + sample + token count (default, LLM-friendly)
- **Schema** — structure only, no values
- **Raw** — plain JSON (like `jq`)
- **Compact** — single-line JSON
- **Pretty** — indented JSON

**Seamless format switching.** Open a JSON file, query it, then open a YAML file — all in the same session. Press `Ctrl+O` to load any format.

**Multi-format export.** Press `Ctrl+E`, type a filename, and jqr exports the current data in every format you've used during the session. Worked with JSON and CSV? You get both `.json` and `.csv` files.

**Bilingual UI.** Press `Ctrl+G` to switch between English and Chinese.

## MCP Server

jqr includes a Model Context Protocol server for AI agent integration:

```bash
jqr mcp
```

Add to your MCP client config (Claude Desktop, OpenCode, Cursor, etc.):

```json
{
  "mcpServers": {
    "jqr": {
      "command": "jqr",
      "args": ["mcp"]
    }
  }
}
```

## How It Works

```
   stdin / file          jq filter           schema           token cap
        │                    │                   │                 │
        ▼                    ▼                   ▼                 ▼
   ┌────────┐          ┌──────────┐        ┌─────────┐     ┌──────────┐
   │ detect │─────────▶│ evaluate │───────▶│ infer   │────▶│ truncate │
   │ format │          │ filter   │        │ schema  │     │ + format │
   └────────┘          └──────────┘        └─────────┘     └──────────┘
        ▲                                                    │
        │                                                    ▼
   json / yaml /                                     schema-first output
   toml / csv
```

1. **Detect format.** Sniff the first bytes. JSON starts with `{` or `[`. YAML uses indentation. TOML has `=`. CSV uses delimiters.
2. **Evaluate filter.** Uses `jaq` (Rust port of `jq`) for full syntax compatibility.
3. **Infer schema.** Walk the result tree, track types and structure.
4. **Truncate to budget.** If `--tokens N` is set, include as many samples as fit.
5. **Format output.** Wrap in the schema-first envelope or output raw.

## Tech Stack

| Component | Crate | Why |
|-----------|-------|-----|
| Filter engine | `jaq` | Full `jq` syntax in pure Rust |
| JSON | `serde_json` | Standard, battle-tested |
| YAML | `serde_yaml` | Widely used |
| TOML | `toml` | Official crate |
| CSV | `csv` | Handles quoting, escaping |
| CLI | `clap` | Standard for Rust CLIs |
| Terminal | `crossterm` | Cross-platform raw mode + colors |
| MCP | `rmcp` | Official Rust MCP SDK |

## Project Structure

```
jqr/
├── src/
│   ├── main.rs            Entry point
│   ├── cli.rs             CLI argument parsing
│   ├── input/             Multi-format reader (JSON/YAML/TOML/CSV)
│   ├── filter/            jq-compatible filter engine (jaq wrapper)
│   ├── schema/            Schema inference and formatting
│   ├── output/            Envelope, truncation, token counting
│   ├── repair/            LLM JSON repair strategies
│   ├── interactive/       TUI: session, render, highlight, pipeline
│   ├── config/            Config loading + agent detection
│   └── mcp/               MCP server (optional feature)
├── test_files/            Sample files for each format
│   ├── demo.json
│   ├── demo.yaml
│   ├── demo.toml
│   └── demo.csv
├── Cargo.toml
└── README.md
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run linter
cargo clippy -- -D warnings

# Build release binary
cargo build --release

# Run CLI verification suite
./verify.sh
```

## License

MIT
