use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Debug)]
pub(super) struct Request {
    pub method: String,
    pub target: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Clone)]
pub(super) struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    chunked: bool,
}

impl Response {
    pub fn json(value: &serde_json::Value) -> Self {
        Self::text(200, &value.to_string())
    }

    pub fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.to_owned(),
            chunked: false,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn chunked(mut self) -> Self {
        self.chunked = true;
        self
    }
}

type Routes = Arc<Mutex<HashMap<String, VecDeque<Response>>>>;

pub(super) struct Fixture {
    pub base: String,
    routes: Routes,
    requests: Arc<Mutex<Vec<Request>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    pub async fn new() -> Self {
        let listener = good(TcpListener::bind("127.0.0.1:0").await);
        let base = format!("http://{}", good(listener.local_addr()));
        let routes: Routes = Arc::default();
        let requests: Arc<Mutex<Vec<Request>>> = Arc::default();
        let task_routes = Arc::clone(&routes);
        let task_requests = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let Ok(request) = read_request(&mut socket).await else {
                    continue;
                };
                let response = lock(&task_routes)
                    .get_mut(&request.target)
                    .and_then(VecDeque::pop_front)
                    .unwrap_or_else(|| Response::text(404, "not found"));
                lock(&task_requests).push(request);
                let _ = socket.write_all(&encode_response(&response)).await;
            }
        });
        Self {
            base,
            routes,
            requests,
            task,
        }
    }

    pub fn route(&self, path: &str, response: Response) {
        lock(&self.routes)
            .entry(path.to_owned())
            .or_default()
            .push_back(response);
    }

    pub fn requests(&self) -> Vec<Request> {
        lock(&self.requests).clone()
    }

    pub fn paths(&self) -> Vec<String> {
        self.requests()
            .into_iter()
            .map(|request| request.target)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn good<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("fixture operation failed: {error:?}"),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    good(mutex.lock())
}

async fn read_request(socket: &mut TcpStream) -> Result<Request, std::io::Error> {
    let mut bytes = Vec::new();
    let head_end = loop {
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        let mut chunk = [0; 2048];
        let count = socket.read(&mut chunk).await?;
        if count == 0 || bytes.len() > 128 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    };
    let head = String::from_utf8_lossy(&bytes[..head_end]);
    let mut lines = head.lines();
    let first = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>();
    let method = first.first().copied().unwrap_or_default().to_owned();
    let target = first.get(1).copied().unwrap_or_default().to_owned();
    let headers: HashMap<_, _> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let length = headers
        .get("content-length")
        .and_then(|length| length.parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < head_end + length {
        let mut chunk = [0; 2048];
        let count = socket.read(&mut chunk).await?;
        if count == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let body = String::from_utf8_lossy(&bytes[head_end..head_end + length]).into_owned();
    Ok(Request {
        method,
        target,
        headers,
        body,
    })
}

fn encode_response(response: &Response) -> Vec<u8> {
    let mut head = format!(
        "HTTP/1.1 {} Fixture\r\nConnection: close\r\nContent-Type: application/json\r\n",
        response.status
    );
    for (name, value) in &response.headers {
        good(write!(head, "{name}: {value}\r\n"));
    }
    if response.chunked {
        head.push_str("Transfer-Encoding: chunked\r\n\r\n");
        good(write!(
            head,
            "{:X}\r\n{}\r\n0\r\n\r\n",
            response.body.len(),
            response.body
        ));
    } else {
        good(write!(
            head,
            "Content-Length: {}\r\n\r\n{}",
            response.body.len(),
            response.body
        ));
    }
    head.into_bytes()
}
