# jqr

**English** | [中文](README_zh.md)

A multi-format data query tool with an interactive terminal UI.

JSON, YAML, TOML, CSV — query them all with `jq` syntax, switch formats on the fly, and export results in any format.

---

## Why I Built This

I work with structured data every day — API responses, config files, data exports. I kept reaching for `jq` to inspect JSON, then switching to another tool for YAML, another for CSV. Each tool has its own query language, its own output format, its own quirks.

I wanted one tool that:

- **Speaks every format.** Load a YAML file, query it with `jq` syntax, export as CSV. No converters, no piping between tools.
- **Has a real interface.** Not just stdin → stdout, but an interactive workspace where I can type a query, see the result, refine, and compare — all without leaving the terminal.
- **Stays fast.** Rust binary, sub-millisecond startup, no runtime dependencies.

## Screenshots

![jqr interactive mode](images/screenshot-1.png)

![jqr query results](images/screenshot-2.png)

## Install

### From source

**Prerequisites:** Rust 1.70+ ([install Rust](https://rustup.rs))

```bash
git clone https://github.com/Apageoflove/jqr.git
cd jqr
cargo build --release
```

The binary is at `target/release/jqr`. Copy it to your PATH:

```bash
sudo cp target/release/jqr /usr/local/bin/
```

Verify:

```bash
jqr --version
```

## Quick Start

```bash
# Query JSON (default: schema-first output)
echo '{"users":[{"name":"Alice"}]}' | jqr '.users[].name'

# Raw output (like jq)
echo '{"a":1}' | jqr -r '.'

# Query a file
jqr -F data.json '.users | length'

# YAML input
jqr -I yaml -F config.yaml '.server.port'

# Token budget (for LLM context)
echo '{"data":[1,2,3,4,5,6,7,8,9,10]}' | jqr -t 50 '.data'

# Interactive mode
echo '{"users":[{"name":"Alice","age":30}]}' | jqr
```

## Supported Formats

| Format | Read | Export | Extension |
|--------|------|--------|-----------|
| JSON   | yes  | yes    | `.json`   |
| YAML   | yes  | yes    | `.yaml`   |
| TOML   | yes  | yes    | `.toml`   |
| CSV    | yes  | yes    | `.csv`    |

## CLI Reference

| Flag | Short | Description |
|------|-------|-------------|
| `--input <fmt>` | `-I` | Input format: auto, json, yaml, toml, csv |
| `--output <mode>` | `-o` | Output: schema, raw, compact |
| `--raw` | `-r` | Raw JSON (like jq) |
| `--compact` | `-c` | Single-line JSON |
| `--pretty` | `-p` | Pretty-printed output |
| `--tokens <N>` | `-t` | Cap output at ~N tokens |
| `--sample-size <N>` | `-n` | Sample records (default 5) |
| `--schema-only` | `-S` | Schema only, no data |
| `--schema-format <f>` | `-f` | jsonschema, typescript, zod, pydantic |
| `--repair` | `-R` | Fix malformed JSON |
| `--explain` | `-x` | Analyze filter without running |
| `--file <path>` | `-F` | Read from file |
| `--interactive` | `-i` | Force interactive mode |

## Interactive Mode

| Key | Action |
|-----|--------|
| `Enter` | Run filter |
| `Tab` | Cycle output mode |
| `Ctrl+O` | Open file (any format) |
| `Ctrl+E` | Export to multiple formats |
| `Ctrl+F` | Switch input format |
| `Ctrl+S` | Save output |
| `Ctrl+G` | Toggle English / 中文 |
| `Up/Down` | Filter history |
| `PgUp/PgDn` | Scroll transcript |
| `Ctrl+L` | Clear transcript |
| `?` | Help overlay |
| `Esc` | Quit |

## Examples

```bash
# JSON
echo '{"name":"Alice","age":30}' | jqr '.name'

# YAML
jqr -I yaml -F config.yaml '.server'

# TOML
cat Cargo.toml | jqr -I toml '.package.version'

# CSV
cat users.csv | jqr -I csv '.[0].name'

# Schema export
jqr -S -f typescript -F api.json '.'

# JSON repair (fix broken LLM output)
echo 'Here is data: {"users": [{"name": "Alice"}, {"name": "Bob"' | jqr -R '.'

# jq filters
echo '{"a":[1,2,3]}' | jqr '.a | length'           # 3
echo '{"a":[1,2,3]}' | jqr '.a | map(. * 2)'       # [2,4,6]
echo '{"a":1,"b":2}' | jqr 'keys'                    # ["a","b"]
```

## MCP Server

```bash
jqr mcp
```

Add to your MCP client config:

```json
{
  "mcpServers": {
    "jqr": { "command": "jqr", "args": ["mcp"] }
  }
}
```

## Development

```bash
cargo build            # build
cargo test             # run tests
cargo clippy -- -D warnings   # lint
cargo build --release  # release binary
./verify.sh            # CLI verification suite
```

## Project Structure

```
jqr/
  src/
    main.rs            Entry point
    cli.rs             CLI parsing
    input/             Multi-format reader
    filter/            jq filter engine (jaq)
    schema/            Schema inference
    output/            Envelope, truncation, tokens
    repair/            JSON repair
    interactive/       Terminal UI (session, render, highlight)
    config/            Config + agent detection
    mcp/               MCP server
  test_files/          Sample files (json/yaml/toml/csv)
  tests/               Integration tests
```

## Tech Stack

Rust, jaq, serde (json/yaml/toml/csv), clap, crossterm, rmcp

## License

MIT
