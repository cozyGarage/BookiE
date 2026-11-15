use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
};

use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::oneshot, task::JoinHandle};
use tokio_rustls::TlsAcceptor;

const TEST_CA: &[u8] = include_bytes!("../fixtures/tls-root-ca.pem");
const TEST_CERTIFICATE: &[u8] = include_bytes!("../fixtures/tls-server-cert.pem");
const TEST_KEY: &[u8] = include_bytes!("../fixtures/tls-server-key.pem");

fn server_config() -> Arc<rustls::ServerConfig> {
    let certificates = CertificateDer::pem_slice_iter(TEST_CERTIFICATE)
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("test server certificate parses");
    let key = PrivateKeyDer::from_pem_slice(TEST_KEY).expect("test server key parses");
    Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("ring supports safe default protocol versions")
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .expect("test server identity is valid"),
    )
}

async fn start_server() -> (u16, oneshot::Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind test server");
    let port = listener.local_addr().expect("test server address").port();
    let (result_sender, result_receiver) = oneshot::channel();
    let acceptor = TlsAcceptor::from(server_config());
    let server = tokio::spawn(async move {
        let client_sent_data = accept_and_read_client_hello(&listener, &acceptor).await;
        let _ = result_sender.send(client_sent_data);
    });
    (port, result_receiver, server)
}

async fn accept_and_read_client_hello(listener: &TcpListener, acceptor: &TlsAcceptor) -> String {
    let Ok((socket, _)) = listener.accept().await else {
        return "TCP accept failed".into();
    };
    let mut stream = match acceptor.accept(socket).await {
        Ok(stream) => stream,
        Err(error) => return format!("TLS handshake failed: {error}"),
    };
    let mut byte = [0; 1];
    match stream.read(&mut byte).await {
        Ok(1) => "client sent data after the TLS handshake".into(),
        Ok(_) => "client closed the TLS stream without data".into(),
        Err(error) => format!("TLS stream read failed: {error}"),
    }
}

fn invoke_worker(
    port: u16,
    native_roots: PathBuf,
    native_cert_directory: PathBuf,
    explicit_ca: Option<PathBuf>,
) -> Output {
    let executable = std::env::current_exe().expect("integration test executable");
    let mut command = Command::new(executable);
    command
        .args([
            "--ignored",
            "--exact",
            "tls_roots::mongodb_tls_root_worker",
            "--test-threads=1",
        ])
        .env("BOOKIE_MONGODB_TLS_ROOT_WORKER", "1")
        .env("BOOKIE_MONGODB_TLS_ROOT_PORT", port.to_string())
        .env("SSL_CERT_FILE", native_roots)
        .env("SSL_CERT_DIR", native_cert_directory);
    if let Some(path) = explicit_ca {
        command.env("BOOKIE_MONGODB_TLS_EXPLICIT_CA", path);
    }
    command.output().expect("run isolated TLS client")
}

async fn client_sends_data_after_tls(
    native_roots: &Path,
    native_cert_directory: &Path,
    explicit_ca: Option<&Path>,
) -> (String, String) {
    let (port, received_data, server) = start_server().await;
    let native_roots = native_roots.to_path_buf();
    let native_cert_directory = native_cert_directory.to_path_buf();
    let explicit_ca = explicit_ca.map(Path::to_path_buf);
    let worker =
        tokio::task::spawn_blocking(move || invoke_worker(port, native_roots, native_cert_directory, explicit_ca));
    let output = tokio::time::timeout(std::time::Duration::from_secs(8), worker)
        .await
        .expect("TLS worker completes")
        .expect("TLS worker thread completes");
    let diagnostics = format!(
        "client status: {}; stdout: {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let server_result = tokio::time::timeout(std::time::Duration::from_secs(2), received_data)
        .await
        .expect("test server receives a connection")
        .expect("test server reports the handshake result");
    server.await.expect("test TLS server completes");
    (server_result, diagnostics)
}

#[tokio::test]
async fn mongodb_tls_uses_a_native_system_root_when_no_ca_file_is_set() {
    let directory = tempfile::tempdir().expect("temporary trust store");
    let native_roots = directory.path().join("native-roots.pem");
    let native_cert_directory = directory.path().join("native-certs");
    std::fs::create_dir(&native_cert_directory).expect("create empty native certificate directory");
    std::fs::write(&native_roots, TEST_CA).expect("write native test root");

    let (server_result, diagnostics) = client_sends_data_after_tls(&native_roots, &native_cert_directory, None).await;
    assert!(
        server_result.starts_with("client sent data"),
        "MongoDB TLS should accept a server trusted by the native system store; {server_result}; {diagnostics}"
    );
}

#[tokio::test]
async fn mongodb_tls_uses_an_explicit_ca_file_without_native_roots() {
    let directory = tempfile::tempdir().expect("temporary trust store");
    let native_roots = directory.path().join("empty-native-roots.pem");
    let native_cert_directory = directory.path().join("native-certs");
    let explicit_ca = directory.path().join("named-ca.pem");
    std::fs::create_dir(&native_cert_directory).expect("create empty native certificate directory");
    std::fs::write(&native_roots, b"").expect("write empty native trust store");
    std::fs::write(&explicit_ca, TEST_CA).expect("write configured test root");

    let (server_result, diagnostics) =
        client_sends_data_after_tls(&native_roots, &native_cert_directory, Some(&explicit_ca)).await;
    assert!(
        server_result.starts_with("client sent data"),
        "MongoDB TLS should accept a server trusted by the configured CA file; {server_result}; {diagnostics}"
    );

    std::fs::write(&native_roots, TEST_CA).expect("write native test root");
    std::fs::write(&explicit_ca, b"").expect("write empty configured trust store");
    let (server_result, diagnostics) =
        client_sends_data_after_tls(&native_roots, &native_cert_directory, Some(&explicit_ca)).await;
    assert!(
        !server_result.starts_with("client sent data"),
        "MongoDB TLS must not augment an explicit CA file with native roots; {server_result}; {diagnostics}"
    );
}

#[tokio::test]
#[ignore]
async fn mongodb_tls_root_worker() {
    if std::env::var_os("BOOKIE_MONGODB_TLS_ROOT_WORKER").is_none() {
        return;
    }
    let port = std::env::var("BOOKIE_MONGODB_TLS_ROOT_PORT")
        .expect("test server port")
        .parse::<u16>()
        .expect("test server port is valid");
    let ca_option = std::env::var("BOOKIE_MONGODB_TLS_EXPLICIT_CA")
        .map(|path| format!("&tlsCAFile={path}"))
        .unwrap_or_default();
    let uri = format!(
        "mongodb://127.0.0.1:{port}/?tls=true&directConnection=true&serverSelectionTimeoutMS=3000&connectTimeoutMS=1000{ca_option}"
    );
    let client = mongodb::Client::with_uri_str(uri)
        .await
        .expect("MongoDB TLS URI is valid");
    let _ = client
        .database("admin")
        .run_command(mongodb::bson::doc! { "ping": 1 })
        .await;
}
