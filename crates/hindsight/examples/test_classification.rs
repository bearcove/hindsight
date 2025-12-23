use hindsight::Tracer;
use rapace::Transport;
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing trace classification...");

    // Connect to Hindsight
    let stream = TcpStream::connect("127.0.0.1:1990").await?;
    let transport = Transport::stream(stream);
    let tracer = Tracer::new(transport).await?;

    // 1. Generic trace (no special attributes)
    println!("\n1️⃣  Sending Generic trace...");
    let span = tracer
        .span("generic_operation")
        .with_attribute("operation", "data_processing")
        .start();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    span.end();

    // 2. Picante trace (with picante.* attributes)
    println!("2️⃣  Sending Picante trace...");
    let span = tracer
        .span("query_execution")
        .with_attribute("picante.query", true)
        .with_attribute("picante.query_kind", "parse_file")
        .with_attribute("picante.query_key", "src/main.rs")
        .with_attribute("picante.cache_status", "hit")
        .start();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    span.end();

    // 3. Rapace RPC trace
    println!("3️⃣  Sending Rapace RPC trace...");
    let span = tracer
        .span("calculator_add")
        .with_attribute("rpc.system", "rapace")
        .with_attribute("rpc.service", "Calculator")
        .with_attribute("rpc.method", "add")
        .start();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    span.end();

    // 4. Dodeca trace
    println!("4️⃣  Sending Dodeca trace...");
    let span = tracer
        .span("page_render")
        .with_attribute("dodeca.build", true)
        .with_attribute("dodeca.page", "index.html")
        .with_attribute("dodeca.template", "page.html")
        .start();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    span.end();

    // 5. Mixed trace (Picante + Rapace)
    println!("5️⃣  Sending Mixed trace (Picante + Rapace)...");

    // Parent: Picante query
    let parent = tracer
        .span("picante_query_with_rpc")
        .with_attribute("picante.query", true)
        .with_attribute("picante.query_kind", "compile_code")
        .start();

    // Child: Rapace RPC call
    let child = tracer
        .span("external_service_call")
        .with_attribute("rpc.system", "rapace")
        .with_attribute("rpc.service", "CompilerService")
        .with_attribute("rpc.method", "compile")
        .with_parent(parent.context().clone())
        .start();

    tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
    child.end();

    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    parent.end();

    // Wait for batching
    println!("\n⏳ Waiting for batching to complete...");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    println!("✅ Sent 5 test traces with different types!");
    println!("\nRun `cargo run -p hindsight --example query_traces` to verify classification.");

    Ok(())
}
