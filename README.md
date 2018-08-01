# firetrap

FTP server implementation in Rust.

## Example

```rust
extern crate firetrap;

fn main() {
  let addr = "127.0.0.1:2121";
  let server = firetrap::Server::with_root("/srv/ftp");
  server.listen(addr);
}
```

For more examples checkout out the [examples](./examples) directory.

## Usage

##### Continuously run Cargo on changes
`make watch`

##### Run example
`make run`

##### Open documentation in browser
`make doc`

##### Build
`make build`

##### Debug build
`make debug`

##### Run tests
`make test`
