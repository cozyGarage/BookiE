//! A TCP proxy that counts network round trips, shared by the driver round-trip tests (#3738, #3739).
//! One client-to-server burst after server-to-client traffic is one round trip. `trace(true)` prints
//! the printable start of each burst. It forwards plain TCP, so the driver under test must have TLS off.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Default)]
pub struct Trips {
    pub last_was_client: bool,
    pub bursts: u64,
}

pub struct CountingProxy {
    pub port: u16,
    trips: Arc<Mutex<Trips>>,
    trace: Arc<std::sync::atomic::AtomicBool>,
}

impl CountingProxy {
    pub async fn start(upstream_port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let trips = Arc::new(Mutex::new(Trips::default()));
        let trace = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shared = trips.clone();
        let tracing = trace.clone();
        tokio::spawn(async move {
            loop {
                let Ok((client, _)) = listener.accept().await else {
                    return;
                };
                let server = TcpStream::connect(("127.0.0.1", upstream_port)).await.unwrap();
                let (mut client_read, mut client_write) = client.into_split();
                let (mut server_read, mut server_write) = server.into_split();
                let up = shared.clone();
                let tracing = tracing.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 65_536];
                    while let Ok(read) = client_read.read(&mut buffer).await {
                        if read == 0 {
                            return;
                        }
                        {
                            let mut trips = up.lock().unwrap();
                            if !trips.last_was_client {
                                trips.bursts += 1;
                                if tracing.load(std::sync::atomic::Ordering::Relaxed) {
                                    let text: String = buffer[..read]
                                        .iter()
                                        .map(|byte| {
                                            if byte.is_ascii_graphic() || *byte == b' ' {
                                                *byte as char
                                            } else {
                                                '.'
                                            }
                                        })
                                        .take(90)
                                        .collect();
                                    println!("  burst {}: {text}", trips.bursts);
                                }
                            }
                            trips.last_was_client = true;
                        }
                        if server_write.write_all(&buffer[..read]).await.is_err() {
                            return;
                        }
                    }
                });
                let down = shared.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 65_536];
                    while let Ok(read) = server_read.read(&mut buffer).await {
                        if read == 0 {
                            return;
                        }
                        down.lock().unwrap().last_was_client = false;
                        if client_write.write_all(&buffer[..read]).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });
        Self { port, trips, trace }
    }

    pub fn trace(&self, on: bool) {
        self.trace.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.trips.lock().unwrap().bursts = 0;
    }

    pub fn bursts(&self) -> u64 {
        self.trips.lock().unwrap().bursts
    }
}
