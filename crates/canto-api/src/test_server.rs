//! A tiny HTTP server for tests: canned answers picked by the request's
//! path, and every request kept for checking what was asked. Plain threads,
//! so it serves any test runtime.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, PoisonError};

pub struct TestServer {
    /// Like "http://127.0.0.1:41234"
    pub base: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl TestServer {
    /// Answers each request with the first route whose needle is in its
    /// target ("/api/search?q=..."): that status and body. Anything else
    /// gets a 404.
    pub fn start(routes: &[(&str, u16, &str)]) -> Self {
        let routes: Vec<(String, u16, String)> = routes
            .iter()
            .map(|(needle, status, body)| (needle.to_string(), *status, body.to_string()))
            .collect();
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let base = format!("http://{}", listener.local_addr().expect("its address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let mut head = Vec::new();
                let mut buf = [0u8; 2048];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let target = head.split(' ').nth(1).unwrap_or_default().to_string();
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(head);
                let (status, body) = routes
                    .iter()
                    .find(|(needle, _, _)| target.contains(needle.as_str()))
                    .map_or((404, ""), |(_, status, body)| (*status, body.as_str()));
                let response = format!(
                    "HTTP/1.1 {status} Test\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Self { base, requests }
    }

    /// The heads (request line and headers) of the requests so far whose
    /// target has `needle`.
    pub fn requests(&self, needle: &str) -> Vec<String> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|head| {
                head.split(' ')
                    .nth(1)
                    .is_some_and(|target| target.contains(needle))
            })
            .cloned()
            .collect()
    }
}

/// An address nothing listens on: connecting fails at once.
pub fn closed_base() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("its address");
    drop(listener);
    format!("http://{addr}")
}

/// A client for the test server (no proxy from the environment).
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("a test client")
}
