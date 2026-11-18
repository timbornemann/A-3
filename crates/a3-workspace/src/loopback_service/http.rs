use a3_application::{LoopbackVerificationFailure as Failure, ProcessRunControl};
use a3_domain::{
    ContentHash, LoopbackHeaderChecks, LoopbackHttpObservation, LoopbackProbe,
    MAX_LOOPBACK_BODY_BYTES, PolicyDecisionId,
};
use http_body_util::{BodyExt, Empty};
use hyper::body::{Body, Bytes};
use hyper::header::{ALLOW, CONTENT_TYPE, HOST, HeaderMap, HeaderName};
use hyper::{Request, client::conn::http1};
use hyper_util::rt::TokioIo;
use std::future::{Future, poll_fn};
use std::net::{SocketAddrV4, TcpStream};
use std::pin::pin;
use std::task::Poll;
use std::time::Duration;

const MAX_HEADERS: usize = 32;
const MAX_BUFFER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = MAX_LOOPBACK_BODY_BYTES as usize;

#[cfg(target_os = "linux")]
pub(super) type ProbeConnection = http1::Connection<TokioIo<tokio::net::TcpStream>, Empty<Bytes>>;

pub(super) async fn probe(
    stream: TcpStream,
    endpoint: SocketAddrV4,
    probe: &LoopbackProbe,
    decision: PolicyDecisionId,
    remaining: Duration,
    control: &dyn ProcessRunControl,
) -> Result<LoopbackHttpObservation, Failure> {
    // The caller proved the server-side tuple for this socket. A high-level client
    // could reconnect or follow a redirect and would lose that binding.
    let stream = tokio::net::TcpStream::from_std(stream).map_err(|_| Failure::Unavailable)?;
    #[cfg(target_os = "linux")]
    super::linux::configure_probe_close(&stream).map_err(|_| Failure::Unavailable)?;
    let exchange = exchange(stream, endpoint, probe, decision);
    tokio::select! {
        biased;
        () = cancelled(control) => Err(Failure::Cancelled),
        result = tokio::time::timeout(remaining, exchange) => {
            result.map_err(|_| Failure::TimedOut)?
        }
    }
}

async fn exchange(
    stream: tokio::net::TcpStream,
    endpoint: SocketAddrV4,
    probe: &LoopbackProbe,
    decision: PolicyDecisionId,
) -> Result<LoopbackHttpObservation, Failure> {
    let request = Request::builder()
        .method(probe.method().as_str())
        .uri(probe.path())
        .header(HOST, endpoint.to_string())
        .body(Empty::<Bytes>::new())
        .map_err(|_| Failure::Denied)?;
    let (mut sender, connection) = http1::Builder::new()
        .max_headers(MAX_HEADERS)
        .max_buf_size(MAX_BUFFER_BYTES)
        .handshake(TokioIo::new(stream))
        .await
        .map_err(|_| Failure::Unavailable)?;

    let mut connection = pin!(connection);
    let mut response = pin!(async move {
        let response = sender
            .send_request(request)
            .await
            .map_err(|_| Failure::InvalidResponse)?;
        let (parts, mut body) = response.into_parts();
        if parts.status.is_informational() {
            return Err(Failure::InvalidResponse);
        }
        if body.size_hint().lower() > MAX_BODY_BYTES as u64 {
            return Err(Failure::ResourceLimit);
        }
        let mut bytes = Vec::new();
        while let Some(frame) = body.frame().await {
            let data = frame
                .map_err(|_| Failure::InvalidResponse)?
                .into_data()
                .map_err(|_| Failure::InvalidResponse)?;
            if data.len() > MAX_BODY_BYTES - bytes.len() {
                return Err(Failure::ResourceLimit);
            }
            bytes.extend_from_slice(&data);
        }
        LoopbackHttpObservation::new(
            decision,
            parts.status.as_u16(),
            ContentHash::from_bytes(*blake3::hash(&bytes).as_bytes()),
            u32::try_from(bytes.len()).map_err(|_| Failure::ResourceLimit)?,
            LoopbackHeaderChecks {
                plain_utf8: exact_header(
                    &parts.headers,
                    CONTENT_TYPE,
                    b"text/plain; charset=utf-8",
                ) && std::str::from_utf8(&bytes).is_ok(),
                allow_get: exact_header(&parts.headers, ALLOW, b"GET"),
            },
        )
        .map_err(|_| Failure::InvalidResponse)
    });
    // Keep both futures under this owner. Even an early error or cancellation drops
    // the connection in this scope without leaving an I/O task behind.
    poll_fn(|context| {
        #[cfg(target_os = "linux")]
        let driven = super::linux::poll_probe_connection(connection.as_mut().get_mut(), context);
        #[cfg(not(target_os = "linux"))]
        let driven = connection.as_mut().poll(context);
        match response.as_mut().poll(context) {
            Poll::Pending if driven.is_ready() => Poll::Ready(Err(Failure::InvalidResponse)),
            result => result,
        }
    })
    .await
}

fn exact_header(headers: &HeaderMap, name: HeaderName, expected: &[u8]) -> bool {
    let all = headers.get_all(name);
    let mut values = all.iter();
    values
        .next()
        .is_some_and(|value| value.as_bytes() == expected)
        && values.next().is_none()
}

async fn cancelled(control: &dyn ProcessRunControl) {
    while !control.is_cancelled() {
        tokio::time::sleep(super::POLL).await;
    }
}
