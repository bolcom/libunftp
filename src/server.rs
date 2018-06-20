extern crate std;
extern crate tokio;

use self::tokio::io;
use self::tokio::net::TcpListener;
use self::tokio::prelude::*;

pub fn listen() {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    let server = listener.incoming().for_each(|socket| {
        let task = io::write_all(socket, "hello world\n")
            .then(|res| {
                println!("wrote some message to some socket, result: {:?}", res);
                Ok(())
            });
        tokio::spawn(task);
        Ok(())
    })
        .map_err(|err| println!("got some error: {:?}", err));

    tokio::run(server);
}
