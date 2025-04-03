#[cfg(not(unix))]
compile_error!("This program requires a Unix-based OS.");

use anyhow::{Result, anyhow};
use argh::FromArgs;
use axum::{
    Json, Router,
    extract::Multipart,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use bytes::Bytes;
use serde_json::json;
use std::{fs::Permissions, net::SocketAddr, os::unix::fs::PermissionsExt, path::PathBuf};
use tokio::{
    fs,
    fs::{File, set_permissions},
    io::AsyncWriteExt,
    process::Command,
    signal,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(FromArgs)]
/// Reach new heights.
struct Args {
    /// receive port number
    #[argh(option, default = "8080", short = 'p')]
    port: u16,

    /// save directory
    #[argh(option, short = 's')]
    save_dir: PathBuf,

    /// save file owner
    #[argh(option)]
    owner: Option<String>,

    /// file permission
    #[argh(option)]
    mode: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let args: Args = argh::from_env();

    let mode: Option<Permissions> = if let Some(mode_str) = &args.mode {
        match u32::from_str_radix(mode_str, 8) {
            Ok(mode) => Some(Permissions::from_mode(mode)),
            Err(e) => {
                return Err(anyhow!("invalid mode `{}`: {}", mode_str, e));
            }
        }
    } else {
        None
    };

    tokio::fs::create_dir_all(&args.save_dir).await?;
    let app = Router::new()
        .route("/", get(test_handler))
        .route(
            "/upload",
            post(move |mp| upload(mp, args.save_dir.clone(), mode, args.owner.clone())),
        )
        .fallback(handler_404);
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn upload(
    mut multipart: Multipart,
    save_dir: PathBuf,
    mode: Option<Permissions>,
    owner: Option<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut saved_files = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        if let Some(filename) = field.file_name() {
            let filepath = save_dir.join(filename);
            let Ok(data) = field.bytes().await else {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            };
            if let Err(err) = save_field_file(&filepath, &data, &mode, &owner).await {
                tracing::error!("{}", err);
                if filepath.is_file() && filepath.exists() {
                    if let Err(e) = fs::remove_file(&filepath).await {
                        tracing::error!("{}", e);
                    }
                }
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            println!("saved to {:?}", &filepath);
            saved_files.push(filepath);
        }
    }

    if saved_files.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(Json(json!({"saved_files": saved_files })))
}

async fn save_field_file(
    filepath: &PathBuf,
    data: &Bytes,
    mode: &Option<Permissions>,
    owner: &Option<String>,
) -> Result<()> {
    let Ok(mut file) = File::create(filepath).await else {
        return Err(anyhow!("Failed to create file"));
    };
    if let Err(e) = file.write_all(data).await {
        return Err(anyhow!("Failed to write file: {}", e));
    }
    if let Some(m) = mode {
        if let Err(e) = set_permissions(filepath, m.clone()).await {
            return Err(anyhow!("Failed to set permissions: {}", e));
        }
    }
    if let Some(o) = owner {
        let user = o.as_str();
        let Ok(status) = Command::new("chown")
            .arg(user)
            .arg(filepath.as_os_str())
            .status()
            .await
        else {
            return Err(anyhow!("Failed to execute chown"));
        };
        if !status.success() {
            return Err(anyhow!("Failed to chown file: {}", user));
        }
    }
    Ok(())
}

async fn test_handler() -> Html<&'static str> {
    Html(
        r##"
    <!DOCTYPE html>
    <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Hello World! Site Title</title>
        </head>
        <body>
            <h1>Hello World!</h1>
        </body>
    </html>
    "##,
    )
}

async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "nothing to see here")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    println!("signal received, starting graceful shutdown");
}
