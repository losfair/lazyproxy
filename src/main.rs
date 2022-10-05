use std::{
    future::poll_fn,
    net::SocketAddr,
    task::{Context as TaskContext, Poll},
    time::Duration,
};

use anyhow::{Context, Result};
use structopt::StructOpt;
use tokio::{
    net::{lookup_host, TcpListener, TcpStream},
    sync::mpsc::Sender,
};
use tracing_subscriber::{fmt::SubscriberBuilder, EnvFilter};

#[derive(StructOpt)]
#[structopt(
    name = "lazyproxy",
    about = "A TCP proxy that is so lazy that shuts down itself after a period of inactivity."
)]
struct Opt {
    /// Address to listen on in `host:port` form.
    #[structopt(long)]
    listen: String,

    /// Target to connect to in `host:port` form.
    #[structopt(long)]
    target: String,

    /// Timeout in seconds.
    #[structopt(long, default_value = "60")]
    timeout_secs: u64,

    /// Wait for target.
    #[structopt(long)]
    wait: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    SubscriberBuilder::default()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();
    let opt = Opt::from_args();
    let listen = lookup_host(&opt.listen)
        .await
        .with_context(|| "failed to resolve listen address")?
        .next()
        .context("listen address resolved to nothing")?;
    let target = lookup_host(&opt.target)
        .await
        .with_context(|| "failed to resolve target address")?
        .next()
        .context("target address resolved to nothing")?;
    let timeout = Duration::from_secs(opt.timeout_secs);

    if opt.wait {
        tracing::info!("waiting for target to become available");

        loop {
            match TcpStream::connect(&target).await {
                Ok(_) => break,
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    let listener = TcpListener::bind(&listen)
        .await
        .with_context(|| "failed to bind")?;

    let (refresh_tx, mut refresh_rx) = tokio::sync::mpsc::channel::<()>(1);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                x = refresh_rx.recv() => {
                    x.unwrap();
                }
                _ = tokio::time::sleep(timeout) => {
                    tracing::info!("shutting down");
                    std::process::exit(0);
                }
            }
        }
    });

    tracing::info!("ready");

    loop {
        let (client, _) = listener.accept().await.with_context(|| "accept failed")?;
        let refresh_tx = refresh_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = proxy(client, target, &refresh_tx).await {
                tracing::error!(error = %e, "proxy failed");
            }
        });
    }
}

async fn proxy(mut client: TcpStream, target: SocketAddr, refresh_tx: &Sender<()>) -> Result<()> {
    let mut conn = TcpStream::connect(&target)
        .await
        .with_context(|| "connect failed")?;

    let refresh_fut = poll_fn(refresh_poll(refresh_tx));
    let copy_fut = Box::pin(tokio::io::copy_bidirectional(&mut client, &mut conn));

    tokio::select! {
        biased;
        _ = refresh_fut => {}
        x = copy_fut => { x?; }
    }

    Ok(())
}

fn refresh_poll<'a>(
    refresh_tx: &'a Sender<()>,
) -> impl (for<'s, 't> FnMut(&'s mut TaskContext<'t>) -> Poll<()>) + 'a {
    |_| {
        let _ = refresh_tx.try_send(());
        Poll::Pending
    }
}
