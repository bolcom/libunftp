# AGENTS.md

## Repository overview

This repository is a Rust Cargo workspace for libunftp, an asynchronous and extensible FTP(S) server library.

Workspace members:

- `libunftp` at the repository root: FTP server implementation and public server builder.
- `crates/unftp-core`: public service-provider interfaces shared by the server and backend crates.
- `crates/unftp-auth-jsonfile`: JSON-file authentication backend.
- `crates/unftp-auth-pam`: PAM authentication backend.
- `crates/unftp-auth-rest`: REST authentication backend.
- `crates/unftp-sbe-fs`: local filesystem storage backend.
- `crates/unftp-sbe-gcs`: Google Cloud Storage backend.

Authentication and storage extension interfaces belong in `unftp-core`. The root `libunftp` crate consumes those interfaces and implements FTP server behavior.

## Architecture

### Public API and server construction

`ServerBuilder<Storage, User>` in `src/server/ftpserver.rs` is the main public configuration API. `ServerBuilder::build()` validates configuration and creates a `Server`.

Public backend traits and related types are defined in `unftp-core`:

- Authentication APIs: `crates/unftp-core/src/auth`
- Storage APIs: `crates/unftp-core/src/storage`

Avoid moving backend-facing APIs into internal server modules. Backend implementations should depend on `unftp-core`, not on libunftp internals.

### Control channel

Control-channel code lives under `src/server/controlchan`.

Incoming FTP lines follow this path:

1. `FtpCodec` decodes a line.
2. `line_parser` parses it into a `Command`.
3. The command becomes an `Event`.
4. A middleware chain applies metrics, logging, FTPS policy, authentication policy, active/passive-mode policy, and notifications.
5. `PrimaryEventHandler` selects the command-specific `CommandHandler`.
6. The handler returns an FTP `Reply` or sends an internal message for asynchronous completion.

FTP commands normally have:

- A `Command` variant in `controlchan/command.rs`
- Parsing logic in `controlchan/line_parser/parser.rs`
- Parser tests in `controlchan/line_parser/tests.rs`
- A handler module in `controlchan/commands`
- Registration and re-export in `controlchan/commands/mod.rs`
- Dispatch logic in `controlchan/control_loop.rs`

Commands involving data transfer also interact with the data-channel command types and executor.

### Session and concurrency model

Per-connection state is stored in `Session<Storage, User>` and shared as an `Arc<tokio::sync::Mutex<_>>`.

Control- and data-channel tasks communicate through Tokio channels using the message types in `src/server/chancomms.rs`. Storage instances and authenticated user details are shared through `Arc`.

Do not hold a session mutex guard across slow storage or network operations unless the existing design specifically requires it. Copy or clone the required session values first and release the guard.

### Data channel

Data-transfer behavior lives primarily in `src/server/datachan.rs`.

The data channel handles commands such as:

- `RETR`
- `STOR`
- `APPE`
- `LIST`
- `NLST`
- `MLSD`

Transfer completion and storage failures are reported to the control channel through `ControlChanMsg`.

### Listener modes

Listener implementations live under `src/server/ftpserver`.

Supported modes include:

- Legacy listener mode
- Pooled listener mode
- Proxy Protocol mode when the `proxy_protocol` feature is enabled

Pooled and proxy modes use the switchboard in `src/server/switchboard.rs` to associate passive data connections with control sessions.

### Authentication

Authentication is split into two stages:

1. An `Authenticator` converts credentials into a `Principal`.
2. A `UserDetailProvider` converts the principal into the application's `UserDetail` type.

The root crate contains the server-side authentication pipeline. Reusable authentication interfaces belong in `unftp-core`.

### Storage capabilities

Storage backends implement `StorageBackend<User>` and its associated `Metadata` type.

Optional backend capabilities are advertised by `StorageBackend::supported_features()` using the `FEATURE_*` constants in `unftp-core`. The server uses these flags when advertising or accepting optional FTP behavior.

## Coding conventions

### Rust and formatting

- Workspace crates use Rust edition 2024.
- Follow `rustfmt.toml`.
- Use four-space indentation.
- The configured maximum line width is 160.
- Let rustfmt reorder imports.
- Prefer field-init shorthand and `?` shorthand where applicable.
- Do not introduce unsafe code. The workspace denies `unsafe_code`.

Run rustfmt rather than manually approximating its output.

### Lints and documentation

The workspace denies:

- Missing documentation on public APIs
- All Clippy lints configured by the `all` lint group

Every new public type, trait, method, field, enum variant, and relevant module must have useful rustdoc.

Public examples should compile as doctests unless they are explicitly marked otherwise. Update examples and documentation when changing a public API.

### Async code

- The runtime is Tokio.
- Async extension traits commonly use `async_trait`.
- Trait objects and values crossing spawned tasks generally require `Send`, `Sync`, and `'static` bounds.
- Use Tokio synchronization and channels in async paths.
- Avoid blocking operations in async server code.

### Errors

- Use typed errors and `thiserror` for public or structured error types.
- Propagate recoverable configuration, I/O, authentication, and storage failures as `Result`.
- Reserve panics and `expect` for internal invariants that indicate programmer errors.
- Builder configuration errors should be reported by `ServerBuilder::build()` rather than panicking.
- Map storage errors to the appropriate FTP reply codes in the control-channel layer.

### Logging and instrumentation

The server uses structured `slog` logging and `tracing` instrumentation.

- Preserve the session logger when spawning related work so trace ID, source, and username context remain available.
- Use the existing logging level conventions: debug for detailed protocol state, info for lifecycle and successful operations, warn for recoverable failures, and error for failures that should not occur.
- Follow nearby `#[tracing_attributes::instrument]` usage for async handlers and backend operations.
- Do not log passwords or credential contents.

### FTP protocol changes

Use the protocol specifications as the source of truth. The command modules follow a convention of naming the defining RFC in their module-level documentation, for example ``//! The RFC 959 Account (`ACCT`) command``. When implementing a command, include the relevant, reasonably scoped RFC text as nearby comments when it explains sequencing, arguments, or reply-code behavior. Also add a direct link to the authoritative RFC or section in the module documentation.

Primary specifications used by this repository include:

- [RFC 959 - File Transfer Protocol](https://www.rfc-editor.org/rfc/rfc959)
- [RFC 2228 - FTP Security Extensions](https://www.rfc-editor.org/rfc/rfc2228)
- [RFC 2389 - Feature Negotiation Mechanism for FTP](https://www.rfc-editor.org/rfc/rfc2389)
- [RFC 2428 - FTP Extensions for IPv6 and NATs](https://www.rfc-editor.org/rfc/rfc2428)
- [RFC 3659 - Extensions to FTP](https://www.rfc-editor.org/rfc/rfc3659)

When adding or changing an FTP command, verify all relevant layers:

- Command enum
- Parser
- Parser tests
- Authentication and FTPS middleware behavior
- Command handler
- Control-loop dispatch
- Data-channel behavior, if applicable
- Reply code and message
- `FEAT` advertisement, if the feature is discoverable
- Storage capability flags, if backend support is optional
- Integration tests over an actual control connection

Preserve CRLF framing and use the existing `Reply` constructors rather than formatting protocol responses ad hoc.

### Backend naming

New ecosystem crates should use the established prefixes:

- `unftp-auth-*` for authentication backends
- `unftp-sbe-*` for storage backends

## Testing workflow

### During development

Run the narrowest relevant tests first. Examples:

```sh
cargo test -p libunftp
cargo test -p unftp-core
cargo test -p unftp-sbe-fs
cargo test --test <integration-test-name>
cargo test <test-name>
```

Many libunftp integration tests start local TCP servers on fixed ports. They may fail in restricted sandboxes or when another process already occupies the port. Such tests require permission to bind local sockets.

Root integration tests share test servers through helpers in `tests/common.rs`. Follow that pattern when extending an existing suite.

### Required checks before submitting a pull request

The repository-provided aggregate check is:

```sh
make pr-prep
```

Developers and agents should also ensure the CI-equivalent checks pass:

```sh
cargo fmt --all -- --check
cargo clippy --all-features --workspace -- -D warnings
cargo test --verbose --workspace --exclude 'unftp-sbe-gcs*'
cargo test --doc --workspace
cargo build --examples --workspace
cargo build --workspace
cargo doc --workspace --no-deps
```

Check the supported feature combinations:

```sh
cargo check --workspace --no-default-features --features aws_lc_rs
cargo check --no-default-features --features aws_lc_rs
cargo check --workspace --no-default-features --features all
cargo check --no-default-features --features all
cargo check --no-default-features --features aws_lc_rs,prometheus
cargo check --no-default-features --features aws_lc_rs,proxy_protocol
```

The GitHub workflow currently uses Rust 1.92.0 and treats warnings as errors during tests.

### GCS integration tests

`unftp-sbe-gcs` integration tests are not part of the normal CI test command. They launch `fsouza/fake-gcs-server` through Docker and use fixed local ports.

Run them separately when changing the GCS backend, after reading:

```text
crates/unftp-sbe-gcs/tests/README.md
```

Do not assume Docker is available. Do not commit credentials or generated test data.

### PAM

Building and testing the PAM backend on Unix requires PAM development libraries. CI installs `libpam-dev` on Linux.

### Platform and feature coverage

CI builds on:

- Linux GNU
- Linux MUSL
- Windows
- macOS Intel
- macOS ARM

Avoid introducing Unix-only behavior into shared crates without appropriate `cfg` guards. Keep optional functionality behind its existing Cargo feature.

At least one cryptographic provider must be enabled. The default is `aws_lc_rs`; `ring` is also supported. Building libunftp without either provider intentionally fails.

## Repository-specific conventions

- Workspace dependency versions are centralized under `[workspace.dependencies]` where practical.
- Every workspace crate opts into the workspace lint configuration.
- Keep public backend SPI changes in `unftp-core` and consider their effect on external backend crates.
- Treat changes to public traits, generic bounds, public context fields, and builder method signatures as compatibility-sensitive.
- Prefer adding public API only when there is a demonstrated use case; public fields and trait requirements are difficult to retract.
- Preserve the distinction between FTP control-channel and data-channel behavior.
- Authentication and FTPS policy are enforced by middleware before most command handlers run.
- Storage feature flags describe optional backend capabilities; they are not general-purpose application feature flags.
- The filesystem and GCS backends are bundled implementations and useful references for implementing `StorageBackend`.
- Root integration tests use the filesystem backend and exercise the server through real FTP control/data sockets.
- Tests commonly use `#[tokio::test(flavor = "current_thread")]`.

## Release conventions

Follow `RELEASE-CHECKLIST.md`.

For a release:

- Update the affected crate versions.
- When releasing `unftp-core` APIs, update its version and affected dependants.
- Search for old version strings, including `html_root_url` and documentation examples.
- Update the relevant changelog entries.
- Run `make pr-prep`.
- Use release commits naming the crate and version.
- Tags use `{component}-{version}`, for example `libunftp-0.23.0`.

Do not publish crates or create tags unless explicitly requested.

## Guidance for future AI agents

- Inspect the current worktree before making changes. Preserve unrelated user modifications.
- Read the root manifest, relevant member manifest, and nearby modules before changing a cross-crate API.
- Do not treat `libunftp::server` internals as public extension APIs; most server modules are intentionally crate-private.
- When changing an FTP command, trace it through parsing, middleware, dispatch, replies, storage capabilities, and integration tests.
- When changing a backend trait, inspect every bundled implementation and remember that external implementations may exist.
- Do not expose internal session state through a public API without considering long-term compatibility.
- Do not silently add blocking filesystem, process, or network operations to async request paths.
- Tests that bind sockets may require execution outside a restricted sandbox; a permission failure is not necessarily a product failure.
- GCS tests have Docker and fixed-port requirements and should be handled separately from the ordinary workspace suite.
- Format only intended files when unrelated worktree changes exist.
- Report which checks ran, which were skipped, and why.
- Do not update versions, changelogs, lockfiles, generated fixtures, release tags, or published crates unless the task explicitly requires it.
