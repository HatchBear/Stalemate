# Test fixtures (DATA-008)

- Fixtures come from the author's own matches only (T-013).
- Chat is stripped before a fixture is saved.
- Never commit `.dem`, `.bcast` or `.vpk` files, or any other Valve data.

## FX-BC layout (read by tests/compliance, TST-030)

```text
tests/fixtures/fx-bc/<match>/meta.json     {"match_id": 1, "me": 100, "started_unix": 1700000000}
tests/fixtures/fx-bc/<match>/events.jsonl  one normalized ParserEvent per line, chat stripped
```

The fixture recorder (T-013) and the Broadcast parser must produce this. If the format
changes, change `tests/compliance/tests/live_safe_payload.rs` with it.
