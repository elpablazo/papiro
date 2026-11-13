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

The UI is plain Preact + htm in `web/` with no build step. It ships in English and Spanish: English strings live in the code wrapped in `t()`, and each language is a dictionary keyed by the English text (see `ES` at the top of `web/app.js`). **New languages are very welcome.** Some code comments are still in Spanish; new ones should be in English.
