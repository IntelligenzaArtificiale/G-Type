# Contributing to G-Type

Thanks for your interest in improving G-Type!

## How to contribute

1. **Fork** the repository and create a feature branch from `main`.
2. **Make your changes** — keep commits small and focused.
3. **Run quality checks** before pushing:
   ```bash
   cargo fmt
   cargo clippy -- -D warnings
   cargo test
   ```
4. **Open a Pull Request** against `main` with a clear description.

## What makes a good PR

- One concern per PR (bug fix, feature, docs).
- Tests for new behavior.
- No unrelated formatting changes.

## Bug reports

Open a [GitHub Issue](https://github.com/IntelligenzaArtificiale/g-type/issues) with:
- What you expected vs. what happened.
- OS, Rust version, and `g-type --help` output.
- Steps to reproduce.

## Code style

- `cargo fmt` for formatting.
- `cargo clippy -- -D warnings` must pass.
- Keep files under 400 lines. One module = one responsibility.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
