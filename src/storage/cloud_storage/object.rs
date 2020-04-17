//! The File type for the CloudStorage

use core::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncRead;

/// The File type for the CloudStorage
#[derive(Clone, Debug)]
pub struct Object {
    data: Vec<u8>,
    index: usize,
}

impl Object {
    pub(crate) fn new(data: Vec<u8>) -> Object {
        Object { data, index: 0 }
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, std::io::Error> {
        for (i, item) in buffer.iter_mut().enumerate() {
            if i + self.index < self.data.len() {
                *item = self.data[i + self.index];
            } else {
                self.index += i;
                return Ok(i);
            }
        }
        self.index += buffer.len();
        Ok(buffer.len())
    }
}

impl AsyncRead for Object {
    fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        Poll::Ready(self.get_mut().read(buf))
    }
}
