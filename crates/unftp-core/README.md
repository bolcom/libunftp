# unftp-core

[![Crate Version](https://img.shields.io/crates/v/unftp-core.svg)](https://crates.io/crates/unftp-core)
[![API Docs](https://docs.rs/unftp-core/badge.svg)](https://docs.rs/unftp-core)
[![Crate License](https://img.shields.io/crates/l/unftp-core.svg)](https://crates.io/crates/unftp-core)
[![Follow on Telegram](https://img.shields.io/badge/Follow%20on-Telegram-brightgreen.svg)](https://t.me/unftp)

When you need to FTP, but don't want to.

![logo](../../logo.png)

[**Website**](https://unftp.rs) | [**API Docs**](https://docs.rs/unftp-core) | [**libunftp**](https://github.com/bolcom/libunftp) | [**unFTP**](https://github.com/bolcom/unFTP)

This crate contains the core traits and types for [unFTP](https://unftp.rs/) backends.

This crate was split of from `libunftp` and defines an API that authentication and storage backend (extention/plug-in) implementations in
the unFTP ecosystem should implement. The [`libunftp`](https://unftp.rs/libunftp/) crate provides the server implementation and depends on this core crate too.

Existing backend implementations exists on crates.io ([search for `unftp-`](https://crates.io/search?q=unftp-)). To implement your own follow the [API Documentation](https://docs.rs/unftp-core).
