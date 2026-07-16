# Pre-Edit Context Scan

- Root: `/home/bender/projects/audiobookgen`
- Stack: `{'node': ['package.json', 'package-lock.json'], 'rust': ['Cargo.toml', 'Cargo.lock']}`

## Guidance
- `AGENTS.md`
- `README.md`

## Targets
- No target paths supplied

## Call-Site Signals
- No symbols supplied or extracted

## Query Matches
- Query: `audio playback current sentence highlighting timeupdate`
- No direct matches

## Likely Tests
- No nearby test files detected

## Verification Candidates
- `package.json:typecheck`: `npm run typecheck`
- `package.json:test`: `npm run test`
- `package.json:build`: `npm run build`
- `rust-fallback`: `cargo fmt --check`
- `rust-fallback`: `cargo clippy -- -D warnings`
- `rust-fallback`: `cargo test`

## Must Read Before Write
- `AGENTS.md`
- `README.md`