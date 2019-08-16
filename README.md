# [libunftp](https://github.com/bolcom/libunftp)

[![Crate Version](https://img.shields.io/crates/l/libunftp.svg)](https://crates.io/crates/libunftp)
[![Crate License](https://img.shields.io/crates/v/libunftp.svg)](https://crates.io/crates/libunftp)
[![Build Status](https://travis-ci.org/bolcom/libunftp.svg)](https://travis-ci.org/bolcom/libunftp)
[![API Docs](https://docs.rs/libunftp/badge.svg)](https://docs.rs/libunftp)

The libunftp library is a safe, fast and extensible FTP server implementation in Rust.

Because of its plugable authentication and storage backends (e.g. local filesystem, Google Buckets) it's more flexible than traditional FTP servers and a perfect match for the cloud.

It is currently under heavy development and not yet recommended for production use.
**API MAY BREAK**

[API Documentation](https://docs.rs/libunftp)

## Prerequisites

You'll need [Rust](https://rust-lang.org) 1.31 or higher to build libunftp.
There are no runtime dependencies besides the OS and libc.

## Getting started

If you've got Rust and cargo installed, create your project with

```sh
cargo new my_project
```

Then add libunftp to your project's dependencies in `Cargo.toml`:

```toml
[dependencies]
libunftp = "0.1"
```

Now you're ready to write your server!
Add the following to `src/main.rs`:

```rust
extern crate libunftp;

fn main() {
  let server = libunftp::Server::with_root(std::env::temp_dir());
  server.listen("127.0.0.1:2121");
}
```

You can now run your server with `cargo run` and connect to `localhost:2121` with your favourite FTP client.

For more examples checkout out the [examples](./examples) directory.

For more information checkout the [API Documentation](https://docs.rs/libunftp).

## Contributing

First of all, thank you for your interest in contributing to libunftp!
Please feel free to create a github issue if you encounter any problems,
want to submit a feature request, or just feel like it :)

Run `make help` in the root of this repository to see the available *make* commands.

## License

You're free to use, modify and distribute this software under the terms of the Apache-2.0 license.