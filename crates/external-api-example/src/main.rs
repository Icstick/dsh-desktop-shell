//! Standalone demo of the unified external API loop (ADR-0018 decision 5).
//!
//! 1. starts an `ExampleServer` on 127.0.0.1:<random port>
//! 2. connects an `ExampleClient` with a one-time credential
//! 3. negotiates Hello → Agreement (granted: `system.ping`)
//! 4. invokes `system.ping` (success) and `browser.list_browsers`
//!    (denied — not granted by the default policy)
//!
//! Run with: `cargo run -p dsh-external-api-example`

use std::thread;
use std::time::Duration;

use dsh_external_api_example::catalog::{browser, system};
use dsh_external_api_example::client::{ClientError, ExampleClient};
use dsh_external_api_example::server::ExampleServer;
use dsh_local_transport::Limits;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = ExampleServer::bind(Limits::default())?;
    let addr = server.addr();
    let credential = server.issue_credential(Duration::from_secs(300));

    let server_thread = thread::spawn(move || {
        // Wait for the client to connect and authenticate, serve it, then exit.
        loop {
            if let Some(conn) = server.take_connection() {
                server.serve_connection(conn);
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });

    let mut client = ExampleClient::connect(addr, &credential, &Limits::default())?;
    let agreement = client.negotiate(vec![system(), browser()])?;
    println!("negotiated activation \"{}\"", agreement.activation_id);
    println!("  granted:     {:?}", agreement.granted);
    for unavailable in &agreement.unavailable {
        println!(
            "  unavailable: {}/{} ({:?})",
            unavailable.coordinate.api_version, unavailable.coordinate.kind, unavailable.reason
        );
    }

    let ping = client.invoke(
        system(),
        "ping",
        serde_json::json!({ "message": "hello from an external tool" }),
    )?;
    println!("system.ping -> {ping}");

    match client.invoke(browser(), "list_browsers", serde_json::json!({})) {
        Ok(payload) => println!("browser.list_browsers -> {payload}"),
        Err(ClientError::Remote { code, message, .. }) => {
            println!("browser.list_browsers -> DENIED ({code}: {message})")
        }
        Err(other) => return Err(other.into()),
    }

    drop(client);
    let _ = server_thread.join();
    println!("demo complete");
    Ok(())
}
