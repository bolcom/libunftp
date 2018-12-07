# [firetrap](https://github.com/koenw/firetrap)

[![Crate Version](https://img.shields.io/crates/l/firetrap.svg)](https://crates.io/crates/firetrap)
[![Crate License](https://img.shields.io/crates/v/firetrap.svg)](https://crates.io/crates/firetrap)
[![Build Status](https://travis-ci.org/koenw/firetrap.svg)](https://travis-ci.org/koenw/firetrap)
[![API Docs](https://docs.rs/firetrap/badge.svg)](https://docs.rs/firetrap)

Firetrap helps you create modern, safe and extensible FTP servers in Rust.

Because of its plugable authentication and storage backends (e.g. local filesystem, Google Buckets) it's more flexible than traditional FTP servers and a perfect match for the cloud.

It is currently under heavy development and not yet recommended for production use.
**API MAY BREAK**

[API Documentation](https://docs.rs/firetrap)

## Prerequisites

You'll need [Rust](https://rust-lang.org) 1.31 or higher to build firetrap.
There are no runtime dependencies besides the OS and libc.

## Getting started

If you've got Rust and cargo installed, create your project with

```sh
cargo new my_project
```

Then add firetrap to your project's dependencies in `Cargo.toml`:

```toml
[dependencies]
firetrap = "0.1"
```

Now you're ready to write your server!
Add the following to `src/main.rs`:

```rust
extern crate firetrap;

fn main() {
  let server = firetrap::Server::with_root(std::env::temp_dir());
  server.listen("127.0.0.1:2121");
}
```

You can now run your server with `cargo run` and connect to `localhost:2121` with your favourite FTP client :)

For more examples checkout out the [examples](./examples) directory.

For more information checkout the [API Documentation](https://docs.rs/firetrap).

## Contributing

First of all, thank you for your interest in contributing to firetrap!
Please feel free to create a github issue if you encounter any problems,
want to submit a feature request, or just feel like it :)

Run `make help` in the root of this repository to see the available *make* commands.

## License

You're free to use, modify and distribute this software under the terms of the MIT or Apache-2.0 license, whichever has your preference.
