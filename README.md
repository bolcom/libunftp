# [libunftp](https://github.com/bolcom/libunftp)

[![Crate Version](https://img.shields.io/crates/l/libunftp.svg)](https://crates.io/crates/libunftp)
[![Crate License](https://img.shields.io/crates/v/libunftp.svg)](https://crates.io/crates/libunftp)
[![Linux Build Status](https://github.com/bolcom/libunftp/workflows/Rust%20build%20on%20linux/badge.svg)](https://github.com/bolcom/libunftp/workflows/Rust%20build%20on%20linux/badge.svg)
[![Macos Build Status](https://github.com/bolcom/libunftp/workflows/Rust%20build%20on%20macos/badge.svg)](https://github.com/bolcom/libunftp/workflows/Rust%20build%20on%20macos/badge.svg)
[![API Docs](https://docs.rs/libunftp/badge.svg)](https://docs.rs/libunftp)

When you need to FTP, but don't want to.

![logo](logo.png)

The libunftp library is a safe, fast and extensible FTP server implementation in [Rust](https://rust-lang.org) brought to you by the [bol.com techlab](https://techlab.bol.com).

Because of its plugable authentication and storage backends (e.g. local filesystem, [Google Cloud Storage](https://cloud.google.com/storage)) it's more flexible than traditional FTP servers and a perfect match for the cloud.

It runs on top of the [Tokio](https://tokio.rs) asynchronous run-time and so tries to make use of Async IO as much as possible.

**libunftp is currently under heavy development and not yet recommended for production use.
The API MAY BREAK**

[API Documentation](https://docs.rs/libunftp)

## Prerequisites

You'll need [Rust](https://rust-lang.org) 1.31 or higher to build libunftp.
There are no runtime dependencies besides the OS and libc.

## Getting started

If you've got Rust and cargo installed, create your project with

```sh
cargo new my_project
```

Then add the libunftp, tokio & futures crates to your project's dependencies in `Cargo.toml`:

```toml
[dependencies]
libunftp = "0.2"
tokio = "0.1"
futures = "0.1"
```

Now you're ready to develop your server!
Add the following to `src/main.rs`:

```rust
use futures::future::Future;
use tokio::runtime::Runtime;

fn main() {
    let ftp_home = std::env::temp_dir();
    let server = libunftp::Server::with_root(ftp_home)
        .greeting("Welcome to my FTP server")
        .passive_ports(50000..65535);

    let bind_addr = "127.0.0.1:2121";
    let mut runtime = Runtime::new().unwrap();
    runtime.spawn(server.listener(&bind_addr));
    runtime.shutdown_on_idle().wait().unwrap();
}
```

You can now run your server with `cargo run` and connect to `localhost:2121` with your favourite FTP client e.g.:

```sh
lftp -p 2121 localhost
```

For more help refer to:

- the [examples](./examples) directory.
- the [API Documentation](https://docs.rs/libunftp).
- [unFTP server](https://github.com/bolcom/unFTP), a server from the bol.com techlab that is built on top of libunftp.

## Contributing

First of all, thank you for your interest in contributing to libunftp!
Please feel free to create a github issue if you encounter any problems,
want to submit a feature request, or just feel like it :)

Run `make help` in the root of this repository to see the available *make* commands.

## License

You're free to use, modify and distribute this software under the terms of the Apache-2.0 license.
