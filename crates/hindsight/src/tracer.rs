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
    /// Connect to a Hindsight server via Rapace
    ///
    /// # Example
    /// ```no_run
    /// # use hindsight::Tracer;
    /// # async fn example() -> Result<(), TracerError> {
    /// // TCP transport
    /// let transport = rapace::transport::StreamTransport::connect("localhost:9090").await?;
    /// let tracer = Tracer::new(transport).await?;
    ///
    /// // SHM transport (for same-machine communication)
    /// // let transport = rapace::transport::shm::ShmTransport::open("/tmp/hindsight.shm").await?;
    /// // let tracer = Tracer::new(transport).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new<T: Transport + 'static>(transport: T) -> Result<Self, TracerError> {
        // Detect service name (from env, or default)
        let service_name = std::env::var("HINDSIGHT_SERVICE_NAME")
            .unwrap_or_else(|_| "unknown".to_string());

        // Create Rapace session
        // IMPORTANT: Do NOT attach a tracer to this session!
        // (Prevents infinite loop)
        let session = Arc::new(RpcSession::new(Arc::new(transport)));

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
