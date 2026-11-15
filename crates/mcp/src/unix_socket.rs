use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio_util::sync::CancellationToken;

use crate::bridge::McpBridge;
use crate::server::serve_json_rpc_lines;

const MAX_CONNECTIONS: usize = 8;

pub fn default_socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join("bookie").join("mcp.sock"))
}

pub async fn serve_unix_socket_until(
    bridge: Arc<McpBridge>,
    path: PathBuf,
    shutdown: CancellationToken,
) -> Result<(), String> {
    let listener = bind_private_socket(&path).await?;
    let owner = std::fs::metadata(&path).map_err(|error| error.to_string())?.uid();
    tracing::info!(path = %path.display(), "MCP socket listening");
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    loop {
        let accepted = tokio::select! {
            () = shutdown.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        let (stream, _) = accepted.map_err(|error| error.to_string())?;
        if !stream.peer_cred().is_ok_and(|peer| peer.uid() == owner) {
            continue;
        }
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            continue;
        };
        let bridge = bridge.clone();
        tokio::spawn(async move {
            let _slot = slot;
            let (reader, writer) = stream.into_split();
            let _ = serve_json_rpc_lines(bridge, BufReader::new(reader), writer).await;
        });
    }
    let _ = std::fs::remove_file(&path);
    Ok(())
}

async fn bind_private_socket(path: &Path) -> Result<UnixListener, String> {
    let directory = path
        .parent()
        .ok_or_else(|| "the MCP socket path has no directory".to_string())?;
    match std::fs::symlink_metadata(directory) {
        Ok(metadata) => {
            let ours = std::fs::metadata("/proc/self").map(|own| own.uid()).ok();
            if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 || Some(metadata.uid()) != ours {
                return Err(format!(
                    "{} must be a directory that only you own and can open (mode 0700)",
                    directory.display()
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)
                .map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    if let Ok(existing) = std::fs::symlink_metadata(path) {
        if !existing.file_type().is_socket() {
            return Err(format!("{} exists and is not a socket", path.display()));
        }
        if UnixStream::connect(path).await.is_ok() {
            return Err(format!("another BookiE is already serving {}", path.display()));
        }
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    let listener = UnixListener::bind(path).map_err(|error| error.to_string())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())?;
    Ok(listener)
}

pub async fn relay_to_socket<R, W>(path: &Path, input: R, output: W) -> Result<(), String>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin,
{
    let stream = UnixStream::connect(path)
        .await
        .map_err(|error| format!("cannot reach the BookiE MCP socket {}: {error}", path.display()))?;
    let (mut from_socket, mut to_socket) = stream.into_split();
    let upstream = tokio::spawn(async move {
        let mut input = input;
        let _ = tokio::io::copy(&mut input, &mut to_socket).await;
        let _ = to_socket.shutdown().await;
    });
    let mut output = output;
    let result = tokio::io::copy(&mut from_socket, &mut output).await;
    upstream.abort();
    result.map(|_| ()).map_err(|error| error.to_string())
}
