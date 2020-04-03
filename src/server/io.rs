pub trait Async2Stream: tokio02::io::AsyncRead + tokio02::io::AsyncWrite + Send + Unpin {}
impl Async2Stream for tokio02::net::TcpStream {}
impl Async2Stream for tokio02tls::TlsStream<tokio02::net::TcpStream> {}
impl Async2Stream for tokio02tls::TlsStream<Box<dyn Async2Stream>> {}

pub trait AsAsyncIo {
    fn as_async_io(self) -> Box<dyn Async2Stream>;
}

impl AsAsyncIo for tokio02::net::TcpStream {
    fn as_async_io(self) -> Box<dyn Async2Stream> {
        Box::new(self)
    }
}

impl AsAsyncIo for tokio02tls::TlsStream<Box<dyn Async2Stream>> {
    fn as_async_io(self) -> Box<dyn Async2Stream> {
        Box::new(self)
    }
}
