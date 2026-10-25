# unftp-core

[![Crate Version](https://img.shields.io/crates/v/unftp-core.svg)](https://crates.io/crates/unftp-core)
[![API Docs](https://docs.rs/unftp-core/badge.svg)](https://docs.rs/unftp-core)
[![Crate License](https://img.shields.io/crates/l/unftp-core.svg)](https://crates.io/crates/unftp-core)
[![Follow on Telegram](https://img.shields.io/badge/Follow%20on-Telegram-brightgreen.svg)](https://t.me/unftp)

When you need to FTP, but don't want to.

![unFTP logo](https://raw.githubusercontent.com/bolcom/libunftp/master/logo.png)

[**Website**](https://unftp.rs) | [**API docs**](https://docs.rs/unftp-core) |
[**libunftp**](https://github.com/bolcom/libunftp) | [**unFTP server**](https://github.com/bolcom/unFTP)

This crate contains the core traits and types for [unFTP](https://unftp.rs/) backends (extensions).

## About the wider unFTP project

unFTP is an open-source, cloud-focused FTP and FTPS server project. It is an FTP server, not an SFTP server. You can run
the standalone [unFTP server](https://github.com/bolcom/unFTP), embed the [libunftp library](https://github.com/bolcom/libunftp) in your application and add existing extensions or create your own extensions. Its pluggable authentication, storage, user-detail providers and other extensions let you connect the server to the systems your application uses.

## About `unftp-core`

`unftp-core` defines the shared Rust traits and types used to write extensions in the unFTP ecosystem. Authentication and storage backend crates implement these interfaces, and `libunftp` uses them to implement a server. This crate provides the interfaces; it is not itself an FTP server or a storage backend.

Use `unftp-core` when building an authentication or storage backend for the unFTP ecosystem. Browse existing
[authentication backends](https://crates.io/search?q=unftp-auth-) and [storage backends](https://crates.io/search?q=unftp-sbe-),
or follow the [API documentation](https://docs.rs/unftp-core) to implement your own.

The unFTP ecosystem has more extension points, which will, as they mature be moved to `unftp-core`.
