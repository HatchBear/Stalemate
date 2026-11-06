# Stalemate: project rules

A free companion app for Valve's Deadlock (scouting, live recommendations, post-match review). Rust, Slint UI, Flatpak on KDE Plasma first, then Windows. Repo: github.com/HatchBear/Stalemate. Flatpak app ID: `io.github.hatchbear.stalemate`. Binary and path name: `stalemate`.

## Source of truth
The design docs `00`–`19` in this folder are the specification. Start at `00-index.md`.
- Requirements and IDs: `02-requirements.md`. Tasks, order and dependencies: `17-roadmap.md`. Tests: `16-testing-and-qa.md`. Per-program starter code and test cases: `12-programs-and-code.md`.
- If the docs are unclear or wrong, stop and report it. Do not guess. Any change to a decision goes through `19-decision-log.md` and needs the author's approval.
- Every threshold and constant lives in config (`14-data-and-content-assets.md` DATA-001), never in code (NFR-MAINT-002). No hard-coded hero or item lists (NFR-MAINT-001).

## Fair play is non-negotiable (doc 03)
- Never read or write game memory, inject code, hook graphics, capture packets, or send input to the game.
- In Live Mode, only Live-Safe fields may reach the UI, and only through the `live_safe` module. A field marked Verify stays blocked until the author verifies it in-game.
- Never recover data for players who opted out of deadlock-api. Never call the Steam Web API for other players.
- Never implement a Valve game-coordinator client. If a task seems to need one, stop and report.
- No fight tracking during a match. Chat is never displayed.

## Repo hygiene
- Never commit anything from `private-reference/` or any Valve art, game files, `.dem`, `.bcast` or `.vpk` files. Hero and item art is loaded at runtime, never bundled.
- Do not commit, push, or change git config unless the author asks. Do not edit the design docs unless your role says so.
- Keep the licence GPL-3.0 compatible: check any new dependency's licence.
- Match the docs' voice: plain English, no filler.

## Tasks only the author can do
T-006 (capture `console.log` lines), T-007 (verify scoreboard fields), T-013 (record matches), the hand-labelling in T-020 and T-023, and supplying UI reference screenshots. The author has no ranked access, so ranked-only behavior (such as the pre-game swap phase) can't be tested; leave it to Beta testers and don't block on it. When your work depends on one of these, stop, say exactly what you need, and continue with anything unblocked.

## Done means
Code builds, `cargo fmt --check` and `cargo clippy -- -D warnings` pass, the TC and TST cases listed for the task pass, and the task's requirement IDs are cited in the summary.
