//! Transport-agnostic daemon connection (TCP, Unix socket, Windows named pipe).

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

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

    pub async fn read_line(&mut self, line: &mut String) -> std::io::Result<usize> {
        self.reader.read_line(line).await
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
        return Some(DaemonSession::from_unix(stream));
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

pub async fn connect_tcp(port: u16) -> Option<DaemonSession> {
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).await.ok()?;
    Some(DaemonSession::from_tcp(stream))
}