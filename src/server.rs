extern crate std;
extern crate tokio;

use self::tokio::io;
use self::tokio::net::{TcpListener, TcpStream};
use self::tokio::prelude::*;

fn process_request(socket: TcpStream) {
    let task = io::write_all(socket, "Hello, world!\n")
        .then(|res| {
            println!("handled request, result: {:?}", res);
            Ok(())
        });
    tokio::spawn(task);
}

pub fn listen() {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    let server = listener.incoming().for_each(|socket| {
        process_request(socket);
        Ok(())
    })
        .map_err(|err| println!("got some error: {:?}", err));

    tokio::run(server);
}
