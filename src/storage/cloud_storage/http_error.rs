use crate::storage::Error;
use failure::Context;
use hyper::body::Chunk;
use hyper::StatusCode;

pub struct HttpError {
    pub status: StatusCode,
    body: Chunk,
}

impl HttpError {
    pub fn new(status: StatusCode, body: Chunk) -> Self {
        Self { status, body }
    }
}
