//! Interactive REPL for driving the real daemon by hand — the M2-M7
//! prototype engine end to end: grant a folder, index a file, and query it
//! back with real ranked, cited results. `cargo run -p vision-daemon` in
//! one terminal, then `cargo run -p vision-daemon --example repl` in
//! another.
//!
//! A typical demo run: `4` to grant a folder, drop/edit a file in it (the
//! watcher picks it up within ~2s) or `1` to ingest one explicitly, then
//! `2` and ask about its content — the ranked snippets that come back are
//! real `stores::vectors` cosine-search hits, not a fixed stub. See
//! `docs/TASKS.md`'s Parking Lot for what's still a stand-in (graph DB,
//! vector index, embedding model) versus real.

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{self, Write};

    use tokio_stream::StreamExt;
    use tonic::transport::Endpoint;
    use tonic::Request;

    use vision_daemon::transport::windows::{NamedPipeConnector, PIPE_NAME};
    use vision_proto::vision_api_client::VisionApiClient;
    use vision_proto::{
        DeleteAuditRequest, GetGraphRequest, GetPermissionsRequest, IngestEventRequest,
        IngestSource, ListAuditRequest, PermissionScope, PermissionScopeType, QueryRequest,
        RevokePermissionRequest, SetPermissionRequest,
    };

    fn prompt(label: &str) -> io::Result<String> {
        print!("{label}: ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        Ok(line.trim().to_string())
    }

    println!("connecting to {PIPE_NAME}...");
    let channel = Endpoint::from_static("http://[::]:0")
        .connect_with_connector(NamedPipeConnector::new(PIPE_NAME))
        .await?;
    let mut client = VisionApiClient::new(channel);
    println!("connected. see the doc comment at the top of this file for a demo walkthrough.\n");

    loop {
        println!(
            "1) IngestEvent  2) Query  3) GetPermissions  4) SetPermission\n\
             5) RevokePermission  6) ListAudit  7) DeleteAudit  8) GetGraph  q) quit"
        );
        let choice = prompt("choice")?;

        match choice.as_str() {
            "1" => {
                let path_or_url = prompt("path (a real file on disk)")?;
                let resp = client
                    .ingest_event(Request::new(IngestEventRequest {
                        source: IngestSource::Filesystem as i32,
                        path_or_url,
                        ..Default::default()
                    }))
                    .await?
                    .into_inner();
                println!("-> {resp:?}\n");
            }
            "2" => {
                let text = prompt("query text")?;
                let mut stream = client
                    .query(Request::new(QueryRequest { text }))
                    .await?
                    .into_inner();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    if chunk.is_final {
                        println!("\n\n-- sources --");
                        for (i, source) in chunk.sources.iter().enumerate() {
                            println!("{}. {}", i + 1, source.path);
                        }
                        println!();
                        break;
                    }
                    print!("{}", chunk.token);
                    io::stdout().flush()?;
                }
            }
            "3" => {
                let resp = client
                    .get_permissions(Request::new(GetPermissionsRequest {}))
                    .await?
                    .into_inner();
                println!("-> {resp:?}\n");
            }
            "4" => {
                let path = prompt("folder path to grant")?;
                let resp = client
                    .set_permission(Request::new(SetPermissionRequest {
                        permission: Some(PermissionScope {
                            path,
                            scope_type: PermissionScopeType::Folder as i32,
                            granted: true,
                        }),
                    }))
                    .await?
                    .into_inner();
                println!("-> {resp:?}\n");
            }
            "5" => {
                let path = prompt("folder path to revoke")?;
                let resp = client
                    .revoke_permission(Request::new(RevokePermissionRequest { path }))
                    .await?
                    .into_inner();
                println!("-> {resp:?}\n");
            }
            "6" => {
                let resp = client
                    .list_audit(Request::new(ListAuditRequest {}))
                    .await?
                    .into_inner();
                println!("-> {resp:?}\n");
            }
            "7" => {
                let id = prompt("audit entry id")?;
                let resp = client
                    .delete_audit(Request::new(DeleteAuditRequest { id }))
                    .await?
                    .into_inner();
                println!("-> {resp:?}\n");
            }
            "8" => {
                let resp = client
                    .get_graph(Request::new(GetGraphRequest {}))
                    .await?
                    .into_inner();
                println!("-> {} node(s):", resp.nodes.len());
                for n in &resp.nodes {
                    println!("   [{}] {} ({})", &n.id[..8.min(n.id.len())], n.path, n.source);
                }
                println!("-> {} edge(s):", resp.edges.len());
                for e in &resp.edges {
                    println!(
                        "   {} <-> {}  weight={:.3}",
                        &e.from_id[..8.min(e.from_id.len())],
                        &e.to_id[..8.min(e.to_id.len())],
                        e.weight
                    );
                }
                println!();
            }
            "q" | "quit" | "exit" => break,
            other => println!("unrecognized choice: {other:?}\n"),
        }
    }

    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("this example dials the named-pipe transport, which is Windows-only");
}
