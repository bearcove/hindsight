use hindsight_protocol::*;
use rapace::{RpcSession, Transport};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Main entry point for sending spans
pub struct Tracer {
    inner: Arc<TracerInner>,
}

struct TracerInner {
    service_name: String,
    span_tx: mpsc::UnboundedSender<Span>,
    _session: Arc<dyn std::any::Any + Send + Sync>,
}

impl Tracer {
    /// Connect to a Hindsight server via HTTP upgrade to Rapace
    ///
    /// This performs an HTTP upgrade handshake to switch to raw Rapace protocol.
    /// Works through HTTP proxies and allows single-port server architecture.
    ///
    /// # Example
    /// ```no_run
    /// # use hindsight::Tracer;
    /// # async fn example() -> Result<(), hindsight::TracerError> {
    /// let tracer = Tracer::connect_http("localhost:1990").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect_http(addr: impl AsRef<str>) -> Result<Self, TracerError> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let addr = addr.as_ref();

        // Connect to server
        let mut stream = TcpStream::connect(addr).await
            .map_err(|e| TracerError::ConnectionFailed(format!("Failed to connect to {}: {}", addr, e)))?;

        // Send HTTP upgrade request
        let host = addr.split(':').next().unwrap_or("localhost");
        let request = format!(
            "GET / HTTP/1.1\r\n\
             Host: {}\r\n\
             Upgrade: rapace\r\n\
             Connection: Upgrade\r\n\
             \r\n",
            host
        );

        stream.write_all(request.as_bytes()).await
            .map_err(|e| TracerError::ConnectionFailed(format!("Failed to send upgrade request: {}", e)))?;

        // Read response until we get \r\n\r\n
        let mut response = Vec::new();
        let mut buf = [0u8; 1];

        loop {
            stream.read_exact(&mut buf).await
                .map_err(|e| TracerError::ConnectionFailed(format!("Failed to read upgrade response: {}", e)))?;
            response.push(buf[0]);

            // Check for \r\n\r\n
            if response.len() >= 4
                && response[response.len() - 4..] == [b'\r', b'\n', b'\r', b'\n']
            {
                break;
            }

            // Prevent infinite loop on malformed response
            if response.len() > 8192 {
                return Err(TracerError::ConnectionFailed("HTTP upgrade response too large".to_string()));
            }
        }

        // Parse response - look for "HTTP/1.1 101"
        let response_str = String::from_utf8_lossy(&response);
        if !response_str.contains("101") && !response_str.contains("Switching Protocols") {
            return Err(TracerError::ConnectionFailed(
                format!("HTTP upgrade failed: {}", response_str.lines().next().unwrap_or("unknown error"))
            ));
        }

        // HTTP upgrade successful, switching to Rapace protocol

        // Create transport from the upgraded stream
        let transport = Transport::stream(stream);
        Self::new(transport).await
    }

    /// Connect to a Hindsight server via Rapace
    ///
    /// # Example
    /// ```no_run
    /// # use hindsight::Tracer;
    /// # use hindsight::TracerError;
    /// # async fn example() -> Result<(), TracerError> {
    /// // TCP transport
    /// let stream = tokio::net::TcpStream::connect("localhost:9090").await?;
    /// let transport = rapace::Transport::stream(stream);
    /// let tracer = Tracer::new(transport).await?;
    ///
    /// // SHM transport (for same-machine communication)
    /// // let (client, server) = rapace::Transport::shm_pair();
    /// // let tracer = Tracer::new(client).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(transport: Transport) -> Result<Self, TracerError> {
        // Detect service name (from env, or default)
        let service_name = std::env::var("HINDSIGHT_SERVICE_NAME")
            .unwrap_or_else(|_| "unknown".to_string());

        // Create Rapace session
        // IMPORTANT: Do NOT attach a tracer to this session!
        // (Prevents infinite loop)
        let session = Arc::new(RpcSession::new(transport));

        // Spawn session runner
        let session_clone = session.clone();
        tokio::spawn(async move {
            if let Err(e) = session_clone.run().await {
                eprintln!("Hindsight client session error: {:?}", e);
            }
        });

        // Create Rapace client
        let client = HindsightServiceClient::new(session.clone());

        // Channel for buffering spans before sending
        let (span_tx, mut span_rx) = mpsc::unbounded_channel();

        // Background task to batch and send spans
        tokio::spawn(async move {
            let mut batch = Vec::new();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            let spans = std::mem::take(&mut batch);
                            let _ = client.ingest_spans(spans).await;
                        }
                    }
                    Some(span) = span_rx.recv() => {
                        batch.push(span);
                        if batch.len() >= 100 {
                            let spans = std::mem::take(&mut batch);
                            let _ = client.ingest_spans(spans).await;
                        }
                    }
                    else => break,
                }
            }

            // Flush remaining spans on shutdown
            if !batch.is_empty() {
                let _ = client.ingest_spans(batch).await;
            }
        });

        let inner = Arc::new(TracerInner {
            service_name,
            span_tx,
            _session: session,
        });

        Ok(Self { inner })
    }

    /// Start building a new span
    pub fn span(&self, name: impl Into<String>) -> crate::span_builder::SpanBuilder {
        crate::span_builder::SpanBuilder::new(
            name.into(),
            self.inner.service_name.clone(),
            self.inner.span_tx.clone(),
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TracerError {
    #[error("failed to connect to server: {0}")]
    ConnectionFailed(String),

    #[error("transport error: {0}")]
    TransportError(#[from] rapace::TransportError),
}
