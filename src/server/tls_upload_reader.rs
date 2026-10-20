//! Graceful, authenticated TLS upload EOF without changing the storage API.

use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Finish the TLS write side before exposing a clean upload EOF to storage. FTPS clients wait for our
/// close_notify before accepting the transfer; read errors, including TLS truncation, remain errors.
pub(super) struct TlsUploadReader<R> {
    reader: R,
    eof: bool,
    closed: bool,
}

impl<R> TlsUploadReader<R> {
    pub(super) fn new(reader: R) -> Self {
        Self {
            reader,
            eof: false,
            closed: false,
        }
    }
}

impl<R: AsyncRead + AsyncWrite + Unpin> AsyncRead for TlsUploadReader<R> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if buf.remaining() == 0 || this.closed {
            return Poll::Ready(Ok(()));
        }
        if !this.eof {
            let before = buf.filled().len();
            match Pin::new(&mut this.reader).poll_read(cx, buf) {
                Poll::Ready(Ok(())) if buf.filled().len() == before => this.eof = true,
                result => return result,
            }
        }
        // Keep the EOF state while shutdown is pending, without polling the read side again.
        match Pin::new(&mut this.reader).poll_shutdown(cx) {
            Poll::Ready(Ok(())) => {
                this.closed = true;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::NotConnected
                ) || (cfg!(windows) && error.kind() == std::io::ErrorKind::ConnectionAborted && error.raw_os_error() == Some(10053)) =>
            {
                // After an authenticated TLS EOF, a peer may close its read side without waiting for our alert
                // (RFC 8446 section 6.1). This exception never applies to a read error or TLS truncation.
                this.closed = true;
                Poll::Ready(Ok(()))
            }
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[derive(Default)]
    struct Io {
        data: Option<&'static [u8]>,
        read_error: Option<Error>,
        shutdown_error: Option<Error>,
        pending_shutdown: bool,
        reads: usize,
        shutdowns: usize,
    }

    impl AsyncRead for Io {
        fn poll_read(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
            self.reads += 1;
            if let Some(error) = self.read_error.take() {
                return Poll::Ready(Err(error));
            }
            if let Some(data) = self.data.take() {
                buf.put_slice(data);
            }
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for Io {
        fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, data: &[u8]) -> Poll<std::io::Result<usize>> {
            Poll::Ready(Ok(data.len()))
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            self.shutdowns += 1;
            if self.pending_shutdown {
                self.pending_shutdown = false;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(self.shutdown_error.take().map_or(Ok(()), Err))
        }
    }

    fn read(reader: &mut TlsUploadReader<Io>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(reader).poll_read(&mut Context::from_waker(std::task::Waker::noop()), buf)
    }

    #[test]
    fn zero_capacity_and_reads_with_new_data_do_not_start_shutdown() {
        let mut reader = TlsUploadReader::new(Io {
            data: Some(b"a"),
            ..Default::default()
        });
        assert!(matches!(read(&mut reader, &mut ReadBuf::new(&mut [])), Poll::Ready(Ok(()))));
        assert_eq!((reader.reader.reads, reader.reader.shutdowns), (0, 0));
        let mut bytes = [0; 4];
        let mut buf = ReadBuf::new(&mut bytes);
        buf.put_slice(b"x");
        assert!(matches!(read(&mut reader, &mut buf), Poll::Ready(Ok(()))));
        assert_eq!(buf.filled(), b"xa");
        assert_eq!(reader.reader.shutdowns, 0);
        assert!(matches!(read(&mut reader, &mut buf), Poll::Ready(Ok(()))));
        assert_eq!(buf.filled(), b"xa", "an existing filled prefix must not hide a true EOF");
        assert_eq!(reader.reader.shutdowns, 1);
    }

    #[test]
    fn pending_shutdown_preserves_eof_without_repolling_read_and_completed_eof_is_stable() {
        let mut reader = TlsUploadReader::new(Io {
            pending_shutdown: true,
            ..Default::default()
        });
        let mut bytes = [0; 1];
        let mut buf = ReadBuf::new(&mut bytes);
        assert!(read(&mut reader, &mut buf).is_pending());
        assert_eq!((reader.reader.reads, reader.reader.shutdowns), (1, 1));
        assert!(matches!(read(&mut reader, &mut buf), Poll::Ready(Ok(()))));
        assert_eq!((reader.reader.reads, reader.reader.shutdowns), (1, 2));
        assert!(matches!(read(&mut reader, &mut buf), Poll::Ready(Ok(()))));
        assert_eq!((reader.reader.reads, reader.reader.shutdowns), (1, 2));
    }

    #[test]
    fn truncation_and_read_side_errors_never_reach_the_shutdown_exception() {
        let errors = [
            ErrorKind::UnexpectedEof.into(),
            ErrorKind::ConnectionReset.into(),
            ErrorKind::BrokenPipe.into(),
            Error::from_raw_os_error(10053),
        ];
        for error in errors {
            let (kind, code) = (error.kind(), error.raw_os_error());
            let mut reader = TlsUploadReader::new(Io {
                read_error: Some(error),
                ..Default::default()
            });
            let mut bytes = [0; 1];
            let Poll::Ready(Err(error)) = read(&mut reader, &mut ReadBuf::new(&mut bytes)) else {
                panic!("read error must propagate")
            };
            assert_eq!((error.kind(), error.raw_os_error()), (kind, code));
            assert_eq!(reader.reader.shutdowns, 0);
            assert!(!reader.closed);
        }
    }

    #[test]
    fn unknown_shutdown_errors_and_other_connection_aborts_are_not_success() {
        let errors = [
            ErrorKind::Other.into(),
            ErrorKind::PermissionDenied.into(),
            ErrorKind::WriteZero.into(),
            ErrorKind::ConnectionAborted.into(),
            Error::from_raw_os_error(1236),
        ];
        for error in errors {
            let (kind, code) = (error.kind(), error.raw_os_error());
            let mut reader = TlsUploadReader::new(Io {
                shutdown_error: Some(error),
                ..Default::default()
            });
            let mut bytes = [0; 1];
            let Poll::Ready(Err(error)) = read(&mut reader, &mut ReadBuf::new(&mut bytes)) else {
                panic!("shutdown error must propagate")
            };
            assert_eq!((error.kind(), error.raw_os_error()), (kind, code));
            assert!(!reader.closed);
        }
    }

    #[test]
    fn already_closed_peer_errors_are_nonfatal_only_after_a_successful_read_eof() {
        for kind in [ErrorKind::BrokenPipe, ErrorKind::ConnectionReset, ErrorKind::NotConnected] {
            let mut reader = TlsUploadReader::new(Io {
                shutdown_error: Some(kind.into()),
                ..Default::default()
            });
            let mut bytes = [0; 1];
            assert!(matches!(read(&mut reader, &mut ReadBuf::new(&mut bytes)), Poll::Ready(Ok(()))));
            assert!(reader.eof && reader.closed);
            assert_eq!((reader.reader.reads, reader.reader.shutdowns), (1, 1));
        }
    }

    #[test]
    fn windows_10053_is_a_shutdown_exception_only_on_windows() {
        let mut reader = TlsUploadReader::new(Io {
            shutdown_error: Some(Error::from_raw_os_error(10053)),
            ..Default::default()
        });
        let mut bytes = [0; 1];
        let result = read(&mut reader, &mut ReadBuf::new(&mut bytes));
        if cfg!(windows) {
            assert!(matches!(result, Poll::Ready(Ok(()))));
            assert!(reader.closed);
        } else {
            assert!(matches!(result, Poll::Ready(Err(_))));
            assert!(!reader.closed);
        }
    }
}
