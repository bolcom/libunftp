extern crate std;

extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_codec;
extern crate bytes;

use self::futures::prelude::*;

use self::tokio::net::TcpListener;
use self::tokio_codec::{Encoder, Decoder};

use self::bytes::{BytesMut, BufMut};

use commands;

pub struct FTPCodec {
    // Stored index of the next index to examine for a '\n' character. This is used to optimize
    // searching. For example, if `decode` was called with `abc`, it would hold `3`, because that
    // is the next index to examine. The next time `decode` is called with `abcde\n`, the moethod
    // will only look at `de\n` before returning.
    next_index: usize,
}

impl FTPCodec {
    fn new() -> Self {
        FTPCodec {
            next_index: 0,
        }
    }
}

impl Decoder for FTPCodec {
    type Item = String;
    type Error = std::io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<String>, std::io::Error> {
        // Look for a byte with the value '\n' in buf. Start searching from the search start index
        if let Some(newline_offset) = buf[self.next_index..].iter().position(|b| *b == b'\n') {
            // Found a '\n' in the buffer.

            // The index of the '\n' is at the sum of the start position + the offset found,
            let newline_index = newline_offset + self.next_index;

            // Split the buffer at the index of the '\n' + 1 to include the '\n'. `split_to`
            // returns a new buffer with the contents up to the index. The buffer on which
            // `split_to` is called will now start at this index.
            let line = buf.split_to(newline_index + 1);

            // Trim the '\n' from the buffer because it's part of the protocol, not the data.
            //let line = &line[..line.len() - 1];

            // Convert the bytes to a string and panic if the bytes are not valid utf-8.
            let line = std::str::from_utf8(&line).expect("invalid utf8 data");

            // Set the search start index back to 0
            self.next_index = 0;

            // Return Ok(Some(...)) to signal that a full frame has been produced.
            Ok(Some(line.to_string()))
        } else {
            // '\n' not found in the string

            // Tell the next call to start searching after the current length of the buffer since
            // all of it was scanned and no '\n' was found.
            self.next_index = buf.len();

            // Ok(None) signifies that more data is needed to produce a full frame.
            Ok(None)
        }
    }
}

impl Encoder for FTPCodec {
    type Item = String;
    type Error = std::io::Error;

    fn encode(&mut self, line: String, buf: &mut BytesMut) -> Result<(), std::io::Error> {
        // It's important to reserve the amount of space needed. The `bytes` API does not grow the
        // buffers implicitly. Reserve the length of the string + 1 for the '\n'.
        buf.reserve(line.len() + 1);

        // String implements IntoBuf, a trait used by the `bytes` API to work with types that can
        // be expressed as a sequence of bytes.
        buf.put(line);

        // Put the '\n' in the buffer.
        buf.put_u8(b'\n');

        // Return ok to signal that no error occured.
        Ok(())
    }
}

pub fn listen() {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    let server = listener.incoming().for_each(|socket| {
        let codec = FTPCodec::new();
        let framed_socket = codec.framed(socket);
        framed_socket.for_each(|frame| {
            let command = commands::Command::parse(&frame.as_bytes());
            match command {
                Ok(cmd) => println!("got command {:?}", cmd),
                Err(e) => println!("failed to parse command: {:?}", e),
            };
            Ok(())
        })
    })
    .map_err(|err| println!("got some error: {:?}", err));

    tokio::run(server);
}
