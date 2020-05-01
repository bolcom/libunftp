pub trait Async2Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin {}
impl Async2Stream for tokio::net::TcpStream {}
impl Async2Stream for tokio_rustls::server::TlsStream<tokio::net::TcpStream> {}
impl Async2Stream for tokio_rustls::server::TlsStream<Box<dyn Async2Stream>> {}

pub trait AsAsyncIo {
    fn as_async_io(self) -> Box<dyn Async2Stream>;
}

impl AsAsyncIo for tokio::net::TcpStream {
    fn as_async_io(self) -> Box<dyn Async2Stream> {
        Box::new(self)
    }
}

impl AsAsyncIo for tokio_rustls::server::TlsStream<Box<dyn Async2Stream>> {
    fn as_async_io(self) -> Box<dyn Async2Stream> {
        Box::new(self)
    }
}
