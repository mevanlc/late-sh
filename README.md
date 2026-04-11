# late.sh

> A cozy terminal clubhouse for developers. Lofi beats, casual games, chat, and tech news, all via SSH.

```bash
ssh late.sh
```

`late.sh` is a terminal-first social app: real-time chat, music, games, news, profiles, and a shared always-on space you can enter from any SSH client.

## Status

This repository is the main codebase for `late.sh`.

- The project is open for source reading, local development, audits, and contributions.
- The public hosted `late.sh` service remains the canonical deployment.
- The code is source-available, not OSI open source, during the FSL protection period.

Read the details in [LICENSE](LICENSE), the plain-English policy in [LICENSING.md](LICENSING.md), and contribution rules in [CONTRIBUTING.md](CONTRIBUTING.md).

## What It Includes

- SSH TUI with dashboard, chat, profile, news, and arcade screens
- Real-time global chat and shared activity feed
- Audio streaming via Icecast/Liquidsoap with browser and CLI pairing
- Terminal games including 2048, Sudoku, Nonograms, Minesweeper, and Solitaire
- Web frontend for landing, connect flow, and paired-client experiences
- Companion CLI for local audio playback and synced visualizer data

## Workspace

This is a Rust workspace with four crates:

| Crate | Role |
|-------|------|
| `late-cli` | Companion CLI for local audio playback, paired controls, and visualizer sync |
| `late-core` | Shared domain code, database layer, migrations, and infrastructure helpers |
| `late-ssh` | SSH server and terminal UI application |
| `late-web` | Web server, landing page, connect flow, and browser pairing |

The stack is backed by PostgreSQL, Icecast, and Liquidsoap.

## Quick Start

Try the live service:

```bash
ssh late.sh
```

Install the companion CLI:

```bash
curl -fsSL https://cli.late.sh/install.sh | bash
```

Build from source:

```bash
git clone https://github.com/mpiorowski/late-sh
cd late-sh
mise install
cargo build --release --bin late
```

## Local Development

Prerequisites:

- Docker
- Rust toolchain
- `mise` recommended for matching repo tool versions

Bring up infrastructure:

```bash
docker compose up -d postgres icecast liquidsoap
```

Run the apps locally:

```bash
cargo run -p late-ssh
cargo run -p late-web
```

Cargo in this repo expects:

```bash
export CARGO_TARGET_DIR=/target
export CARGO_HOME=$HOME/.cargo
```

## Verification

Typical local verification commands:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --all-targets
```

Some integration tests require Docker via testcontainers.

## Contributing

Contributions are welcome, but read the project policy first:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [LICENSING.md](LICENSING.md)
- [LICENSE](LICENSE)

This repository uses DCO sign-off for commits:

```bash
git commit -s
```

If you distribute a fork, do not present it as the official `late.sh` service or use the project branding as your own.

## More Context

- [CONTEXT.md](CONTEXT.md) for architecture, invariants, and internal working context
- [late-cli/README.md](late-cli/README.md) for CLI-specific usage and behavior
