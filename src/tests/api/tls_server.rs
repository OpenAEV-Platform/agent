use rustls::pki_types::PrivatePkcs8KeyDer;
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// Serves HTTPS on a loopback port with a fresh self-signed certificate and returns its URL.
pub fn serve_self_signed_https() -> String {
    let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
    let key = PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certified.cert.der().clone()], key.into())
            .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("https://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut tls = StreamOwned::new(ServerConnection::new(config.clone()).unwrap(), stream);
            // a client rejecting the certificate aborts the handshake, so this read fails
            let mut request = [0u8; 4096];
            if tls.read(&mut request).is_ok() {
                let _ = tls.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                tls.conn.send_close_notify();
                let _ = tls.flush();
            }
        }
    });
    url
}
