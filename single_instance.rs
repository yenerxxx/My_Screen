use crate::constants::{HOST, PORT};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

pub fn notify_existing_instance(cmd: &str) -> bool {
    let addr = format!("{HOST}:{PORT}");
    let Ok(sock_addr) = addr.parse() else { return false };
    if let Ok(mut stream) = TcpStream::connect_timeout(&sock_addr, Duration::from_millis(350)) {
        let _ = stream.write_all(cmd.as_bytes());
        return true;
    }
    false
}

pub fn start_instance_server(tx: Sender<String>) {
    thread::spawn(move || {
        let addr = format!("{HOST}:{PORT}");
        let Ok(listener) = TcpListener::bind(&addr) else { return };
        for stream in listener.incoming().flatten() {
            let mut s = stream;
            let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 64];
            let n = s.read(&mut buf).unwrap_or(0);
            let cmd = String::from_utf8_lossy(&buf[..n]).trim().to_uppercase();
            let valid = match cmd.as_str() {
                "CAPTURE" | "CAPTURE_FULL" | "SHOW" => cmd,
                _ => "SHOW".to_string(),
            };
            let _ = tx.send(valid);
        }
    });
}
