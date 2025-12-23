use hindsight::Tracer;
use rapace::Transport;
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Hindsight server via TCP
    println!("🔌 Connecting to Hindsight server at 127.0.0.1:1990...");
    let stream = TcpStream::connect("127.0.0.1:1990").await?;
    let transport = Transport::stream(stream);
    let tracer = Tracer::new(transport).await?;
    println!("✅ Connected!");

    // Send 10 test spans
    println!("📊 Sending 10 test spans...");
    for i in 0..10 {
        let span = tracer
            .span("test_span")
            .with_attribute("iteration", i)
            .with_attribute("test", "simple_client")
            .start();

        // Simulate some work
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        span.end();
        println!("  ✓ Sent span {}/10", i + 1);
    }

    // Wait for batching to complete
    println!("⏳ Waiting for batching to complete...");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    println!("🎉 Done! Sent 10 spans successfully!");
    Ok(())
}
