# Patches to vendored haste

Upstream: https://github.com/blukai/haste, commit `d2cdbf2cc13cadf12ec6c6b765f689a5894a6b36`.
Licence: BSD-3-Clause (`license.txt`). Decision: DEC-049.

Date vendored and patched: 2026-11-05.

## What was left out
`tools/` and `examples/`. The `[[example]]` entries and the `tools/*` workspace member were
removed from the root `Cargo.toml`. Nothing else in `Cargo.toml` changed.

## Patch 1: multi-argument template types
Files: `crates/haste_vartype/src/tokenizer.rs`, `crates/haste_vartype/src/parser.rs`.

Deadlock's entity serializers contain a field typed
`CNetworkUtlVectorBase< CNetworkUtlVectorBase< EEvent, NetworkVar_m_vecActiveEvents , -1, int > >`.
Upstream's tokenizer fails on the comma (`UnknownChar(',')`), so parsing a Deadlock broadcast
stops at the serializer table.

- The tokenizer now emits a `Comma` token and reads a leading `-` before digits as part of a
  number literal (`-1`).
- The parser accepts `, arg` after the first template argument, parses the extra
  arguments, and drops them. Only the first argument decides how a field is decoded, so
  `Expr` is unchanged and `fieldmetadata.rs` needs no edit.
- New test `parser::tests::template_with_extra_args` covers the type string above and two
  malformed inputs.

## Replaces the probe patch
The earlier probe patch stripped text from each `,` to the next `>` before parsing.
That was crude and is not used.

## Git dependencies (not vendored)
- `valveprotos` at rev `63620c46236ed0f03a9e8b75954a4dafd0fe8e53` (BSD-3-Clause). Needs `protoc` at build time.
- `mars` at rev `5d831c5` (github.com/blukai/mars). It has no licence. See DEC-049 and `scripts/release-check.sh`.

## Cargo metadata
Added `license = "BSD-3-Clause"` to each vendored `Cargo.toml` so `cargo deny` can see the licence.
