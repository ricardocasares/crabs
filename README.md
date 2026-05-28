# crabs

A Rust monorepo of web framework adapters and utilities.

## Crates

| Crate | Description | crates.io |
|---|---|---|
| [`better-auth-poem`](crates/better-auth-poem) | [Poem](https://github.com/poem-web/poem) integration for [better-auth-rs](https://github.com/better-auth-rs/better-auth-rs) | *(coming soon)* |
| [`vercel-poem`](crates/vercel-poem) | Run Poem applications on the [Vercel Rust runtime](https://github.com/vercel/vercel/tree/main/packages/rust) | *(coming soon)* |

## Examples

| Example | Description |
|---|---|
| [`sqlx-custom-entities`](examples/sqlx-custom-entities) | End-to-end SaaS app with custom PostgreSQL entities, organizations, and invitations |

## Development

```bash
cargo build --workspace
cargo test --workspace

# E2E tests require a Postgres database
DATABASE_URL=postgresql://localhost:5432/crabs_dev cargo test -p sqlx-custom-entities
```

### Adding a new crate

```bash
cargo new --lib crates/my-crate
# Add "crates/my-crate" to workspace.members in Cargo.toml
# Inherit shared metadata in crates/my-crate/Cargo.toml:
#   edition.workspace = true
#   license.workspace = true
#   repository.workspace = true
#   rust-version.workspace = true
```

### Pre-commit hooks

Install [prek](https://github.com/j178/prek) and run:

```bash
prek install
```

Runs `cargo fmt --check` and `cargo clippy -D warnings` before every commit.

## Publishing

Each crate is published independently via git tags:

```bash
# bump version in crates/<name>/Cargo.toml, commit, then:
git tag <crate-name>-v<version>
git push --tags
```

GitHub Actions picks up the tag and runs `cargo publish -p <crate-name>`.

## License

MIT OR Apache-2.0
