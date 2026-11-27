//! Transport-agnostic daemon connection (TCP, Unix socket, Windows named pipe).

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// The longest JSON-RPC line a daemon or proxy accepts from its peer.
pub const MAX_LINE_BYTES: u64 = 32 * 1024 * 1024;

/// Reads one line of at most `limit` bytes. A peer that sends more without a
/// newline gets an `InvalidData` error instead of growing `line` without end.
pub async fn read_line_capped<R>(
    reader: &mut R,
    line: &mut String,
    limit: u64,
) -> std::io::Result<usize>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let n = reader.take(limit.saturating_add(1)).read_line(line).await?;
    if n as u64 > limit {
        line.clear();
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("line longer than {limit} bytes; connection closed"),
        ));
    }
    Ok(n)
}

pub struct DaemonSession {
    pub reader: BufReader<Box<dyn AsyncRead + Unpin + Send>>,
    pub writer: Box<dyn AsyncWrite + Unpin + Send>,
}

impl DaemonSession {
    pub fn from_io<T>(io: T) -> Self
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (read, write) = tokio::io::split(io);
        Self {
            reader: BufReader::new(Box::new(read)),
            writer: Box::new(write),
        }
    }

    pub fn from_tcp(stream: TcpStream) -> Self {
        Self::from_io(stream)
    }

    #[cfg(unix)]
    pub fn from_unix(stream: tokio::net::UnixStream) -> Self {
        Self::from_io(stream)
    }

    #[cfg(windows)]
    pub fn from_pipe(client: tokio::net::windows::named_pipe::NamedPipeClient) -> Self {
        Self::from_io(client)
    }

    pub fn into_split(
        self,
    ) -> (
        BufReader<Box<dyn AsyncRead + Unpin + Send>>,
        Box<dyn AsyncWrite + Unpin + Send>,
    ) {
        (self.reader, self.writer)
    }

    /// Reads one line of at most [`MAX_LINE_BYTES`]; a longer line is an error.
    pub async fn read_line(&mut self, line: &mut String) -> std::io::Result<usize> {
        read_line_capped(&mut self.reader, line, MAX_LINE_BYTES).await
    }

    /// Like `read_line`, reading at most `limit` bytes.
    pub async fn read_line_limited(
        &mut self,
        line: &mut String,
        limit: u64,
    ) -> std::io::Result<usize> {
        use tokio::io::AsyncReadExt;
        (&mut self.reader).take(limit).read_line(line).await
    }

    pub async fn write_bytes(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

pub async fn connect_path(path: &str) -> Option<DaemonSession> {
    if cfg!(windows) && path.starts_with("\\\\.\\pipe\\") {
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ClientOptions;
            let client = ClientOptions::new().open(path).ok()?;
            return Some(DaemonSession::from_pipe(client));
        }
        #[cfg(not(windows))]
        {
            return None;
        }
    }

    #[cfg(unix)]
    {
        let stream = tokio::net::UnixStream::connect(path).await.ok()?;
        Some(DaemonSession::from_unix(stream))
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

pub async fn connect_tcp(port: u16) -> Option<DaemonSession> {
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .ok()?;
    Some(DaemonSession::from_tcp(stream))
}
