use hindsight_protocol::{HindsightServiceClient, TraceFilter};
use rapace::{RpcSession, Transport};
use std::sync::Arc;
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Querying Hindsight server for traces...");

    // Connect to server
    let stream = TcpStream::connect("127.0.0.1:1990").await?;
    let transport = Transport::stream(stream);
    let session = Arc::new(RpcSession::new(transport));

    // Spawn session runner
    let session_clone = session.clone();
    tokio::spawn(async move {
        if let Err(e) = session_clone.run().await {
            eprintln!("Session error: {:?}", e);
        }
    });

    let client = HindsightServiceClient::new(session);

    // Query for all traces
    let traces = client.list_traces(TraceFilter::default()).await?;

    println!("\n📊 Found {} trace(s):", traces.len());
    for trace in traces {
        println!("  - Trace ID: {}", trace.trace_id);
        println!("    Service: {}", trace.service_name);
        println!("    Spans: {}", trace.span_count);
        println!("    Type: {:?}", trace.trace_type);
        if let Some(duration) = trace.duration_nanos {
            println!("    Duration: {}ms", duration / 1_000_000);
        }
        println!();
    }

    Ok(())
}
