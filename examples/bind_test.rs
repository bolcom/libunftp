/*
extern crate tokio;

use tokio::prelude::*;
use tokio::net::{TcpListener, TcpStream};

fn wait_for_con() ->  Async<(TcpStream, std::net::SocketAddr)> {
    let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    println!("listenenig on {:?}", listener.local_addr().unwrap());
    listener.poll_accept().unwrap()
}
*/

fn main() {
}
