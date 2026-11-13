# Contributing

Issues and pull requests are welcome. A few things keep Papiro what it is:

- **Small and boring beats clever.** One binary, SQLite, no new services. A new dependency needs a reason.
- **Never lose a document.** Anything that touches ingest, storage, backup or migrations needs a test or a reproducible check (see `bench/tools/crash_papiro.sh` and `papiro verify`).
- **Schema changes** go in a new entry in `MIGRATIONS` (`src/db.rs`). Never edit a migration that has shipped.
- **Measure, don't guess.** Performance claims in the README come from `bench/`. If you change them, include the numbers and how you got them.

```bash
cargo test                       # unit tests
PAPIRO_WEB_DIR=web cargo run     # serves the UI from disk: edit web/ and reload, no rebuild
```

The UI is plain Preact + htm in `web/` with no build step. It is Spanish-only today; **an English UI (i18n) is the most wanted contribution.** Code comments are in Spanish for historical reasons; new ones can be in English.
