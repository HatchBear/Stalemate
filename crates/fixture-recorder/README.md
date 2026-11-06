# Fixture recorder

A developer tool for T-013. It turns your own matches into test fixtures. The app never uses it.

## What it does

- `record` saves the raw Broadcast files of a live match into a folder you name.
- `normalize` replays those files through the app's own Broadcast client and writes
  `tests/fixtures/fx-bc/<match>/events.jsonl` and `meta.json`.
- `api` saves the deadlock-api responses for the same match to `tests/fixtures/fx-api/<match>/`.

`normalize` and `api` replace every other player's name with `Player N` and their Account ID with a
made-up one. The same player gets the same ID in every file of a match. Your own ID and name stay
(DEC-060). Chat is dropped. If an original ID or name is still in the output, the tool refuses to
write it.

Raw files hold chat and positions. They live in `fixtures-raw/`, which git ignores. Never commit
them, and never commit `.dem` or `.bcast` files.

## How to record a match

1. Find your Account ID: the 32-bit number in your deadlock-api or Steam profile URL (the app
   also resolves it from Steam files).
2. Start the match. Once you are in the game, run from the repo root:
   `just fixture-record <match_id>`
   The match ID is in `console.log`. The tool asks deadlock-api for the Broadcast URL (the
   per-match lookup is limited to 6 per hour) and waits up to 10 minutes for it to appear. If you
   already have the URL, add `--url <url>` by running the binary directly.
3. Leave it running until the match ends. It stops after 90 seconds with nothing new.
4. Run `just fixture-normalize <match_id> <your_account_id>`.
5. Look at the files in `tests/fixtures/fx-bc/<match_id>/` and `tests/fixtures/fx-api/<match_id>/`.
   Check that no real names are in them, then commit them. Do not commit `fixtures-raw/`.

Repeat for 20 matches (doc 14, DATA-008).

## Notes

- The API step reads stored data only. It never asks deadlock-api to refetch from Valve. A player
  with no public data (404) is skipped and not retried.
- Rate limits come from `config/rules.toml`. Pass `--rules <file>` to use another file.
- Run `cargo run -p stalemate-fixture-recorder -- --help` for all options.
