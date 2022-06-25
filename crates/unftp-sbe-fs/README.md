# unftp-sbe-fs

[![Crate Version](https://img.shields.io/crates/v/unftp-sbe-fs.svg)](https://crates.io/crates/unftp-sbe-fs)
[![API Docs](https://docs.rs/unftp-sbe-fs/badge.svg)](https://docs.rs/unftp-sbe-fs)
[![Crate License](https://img.shields.io/crates/l/unftp-sbe-fs.svg)](https://crates.io/crates/unftp-sbe-fs)
[![Follow on Telegram](https://img.shields.io/badge/Follow%20on-Telegram-brightgreen.svg)](https://t.me/unftp)

This unftp-sbe-fs crate allows you to use a regular Filesystem with
[libunftp](https://github.com/bolcom/libunftp) and work like a regular
FTP server.

## Getting started

If you've got Rust and cargo installed, create your project with

```sh
cargo new myftp
```

Add the libunftp and tokio crates to your project's dependencies in `Cargo.toml`.

```toml
[dependencies]
libunftp = "0.18.5"
unftp-sbe-fs = "0.2.1"
tokio = { version = "1", features = ["full"] }
```

Now you're ready to develop your server!
Add the following to `src/main.rs`:

```rust
use unftp_sbe_fs::ServerExt;

#[tokio::main]
pub async fn main() {
    let ftp_home = std::env::temp_dir();
    let server = libunftp::Server::with_fs(ftp_home)
        .greeting("Welcome to my FTP server")
        .passive_ports(50000..65535);

    server.listen("127.0.0.1:2121").await;
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

## Getting help and staying informed

Support is given on a best effort basis. You are welcome to engage us on [the discussions page](https://github.com/bolcom/libunftp/discussions)
or create a Github issue.

You can also follow news and talk to us on [Telegram](https://t.me/unftp) 

## Contributing

Thank you for your interest in contributing to unftp-sbe-fs!

Please feel free to create a Github issue if you encounter any problems.

Want to submit a feature request or develop your own storage or authentication back-end? Then head over to 
our [contribution guide (CONTRIBUTING.md)](../../CONTRIBUTING.md).

## License

You're free to use, modify and distribute this software under the terms of the [Apache License v2.0](http://www.apache.org/licenses/LICENSE-2.0).
