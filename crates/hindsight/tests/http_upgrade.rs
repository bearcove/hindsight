use hindsight::Tracer;
use std::time::Duration;
use tokio::time::timeout;

/// Integration test for HTTP upgrade functionality
#[tokio::test]
async fn test_http_upgrade_connect() {
    // Start a Hindsight server in the background
    let server_handle = tokio::spawn(async {
        hindsight_server::run_server("127.0.0.1", 19900, 19901, 3600, false).await
    });

    // Give the server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test HTTP upgrade connection
    let result = timeout(
        Duration::from_secs(5),
        Tracer::connect_http("127.0.0.1:19900"),
    )
    .await;

    assert!(result.is_ok(), "HTTP upgrade connection timed out");
    let tracer = result.unwrap().expect("Failed to connect via HTTP upgrade");

    // Send a test span
    let span = tracer
        .span("test_http_upgrade")
        .with_attribute("test", "integration")
        .start();

    tokio::time::sleep(Duration::from_millis(10)).await;
    span.end();

    // Wait for batch to be sent
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Cleanup
    server_handle.abort();
}

/// Test that raw TCP (port 19901) still works alongside HTTP upgrade
#[tokio::test]
async fn test_raw_tcp_still_works() {
    use rapace::Transport;
    use tokio::net::TcpStream;

    // Start a Hindsight server in the background
    let server_handle = tokio::spawn(async {
        hindsight_server::run_server("127.0.0.1", 19910, 19911, 3600, false).await
    });

    // Give the server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test raw TCP connection to port 19911
    let stream = TcpStream::connect("127.0.0.1:19911")
        .await
        .expect("Failed to connect to TCP port");

    let transport = Transport::stream(stream);
    let tracer = Tracer::new(transport)
        .await
        .expect("Failed to create tracer");

    // Send a test span
    let span = tracer
        .span("test_raw_tcp")
        .with_attribute("transport", "tcp")
        .start();

    tokio::time::sleep(Duration::from_millis(10)).await;
    span.end();

    // Wait for batch to be sent
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Cleanup
    server_handle.abort();
}

/// Test invalid HTTP upgrade (should fail gracefully)
#[tokio::test]
async fn test_invalid_upgrade_fails_gracefully() {
    use tokio::net::TcpListener;

    // Start a fake server that doesn't support upgrade
    let listener = TcpListener::bind("127.0.0.1:19920")
        .await
        .expect("Failed to bind fake server");

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            use tokio::io::AsyncWriteExt;
            // Send a 400 Bad Request instead of 101 Switching Protocols
            let response = b"HTTP/1.1 400 Bad Request\r\n\r\n";
            let _ = socket.write_all(response).await;
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to connect - should fail
    let result = Tracer::connect_http("127.0.0.1:19920").await;
    assert!(
        result.is_err(),
        "Expected connection to fail with invalid upgrade response"
    );

    match result {
        Err(e) => {
            let err_msg = e.to_string();
            assert!(
                err_msg.contains("upgrade failed") || err_msg.contains("400"),
                "Error message should mention upgrade failure: {}",
                err_msg
            );
        }
        Ok(_) => panic!("Expected error, got success"),
    }
}
