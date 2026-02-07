# Repository Guidelines

## Project Structure & Module Organization
Core library code lives in `src/`:
- `engine.rs` contains authorization/scoping logic and `EngineBuilder`.
- `store.rs` defines async store traits (`TenantStore`, `RoleStore`, `GlobalRoleStore`).
- `permission.rs`, `types.rs`, and `error.rs` define domain types and validation.
- Optional adapters are feature-gated (`memory_store.rs`, `memory_cache.rs`, `axum.rs`).

Performance and integration checks are outside `src/`:
- `tests/perf.rs` for ignored manual perf tests.
- `benches/criterion_engine.rs` for Criterion benchmarks.
- `docs/` is available for design notes and API examples.

## Build, Test, and Development Commands
- `cargo check` verifies compile health quickly.
- `cargo test --offline` runs default test targets.
- `cargo test --offline --features memory-store,memory-cache` runs cache/store feature tests.
- `cargo test --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture` runs manual perf tests.
- `cargo bench --features criterion-bench,memory-store,memory-cache` runs Criterion benchmarks.
- `cargo fmt --all` formats code; `cargo clippy --all-targets --all-features -D warnings` enforces lint quality.

## Coding Style & Naming Conventions
Use idiomatic Rust (edition 2024) with `rustfmt` defaults (4-space indentation, trailing commas where helpful). Follow naming rules used in this codebase: `snake_case` for functions/tests, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants. Keep public APIs documented with `///`. Prefer `Result` + `?` and typed errors (`thiserror`), and reserve `unwrap` for tests/bench code. Permission literals should follow `resource:action` and lowercased semantics.

## Testing Guidelines
Place unit tests next to implementation (`mod tests` in source files). Existing test names use behavior-oriented style, e.g. `authorize_should_allow_exact_permission`. For new authorization paths, include allow and deny cases, plus feature-gated variants when applicable. No fixed coverage threshold is defined; contributors should add tests for every behavior change.

## Commit & Pull Request Guidelines
Current history favors short, single-line summaries (Chinese or English), e.g. `性能优化`, `fix bug`, `更新文档`. Keep commits focused and atomic; recommended format is `<area>: <change>` (example: `engine: fix wildcard deny behavior`). PRs should include:
- What changed and why.
- Feature flags affected (if any).
- Test/benchmark commands executed and outcomes.
- Benchmark deltas for performance-sensitive changes.

## Feature & Configuration Notes
This crate is feature-driven (`memory-store`, `memory-cache`, `axum`, `axum-jwt`, `criterion-bench`, `serde`). Keep new code behind explicit features when optional, and preserve the default deny-by-default authorization behavior.
