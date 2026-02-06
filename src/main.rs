use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use hdrhistogram::Histogram;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run an actix-web server with a /health and /plaintext endpoint.
    ServeActix {
        #[arg(long, default_value = "127.0.0.1:18080")]
        addr: String,
        /// actix-web workers (0 = default)
        #[arg(long, default_value_t = 0)]
        workers: usize,
    },
    /// Run an axum server with a /health and /plaintext endpoint.
    ServeAxum {
        #[arg(long, default_value = "127.0.0.1:18082")]
        addr: String,
    },
    /// Run an axum router served by hyper (manual http1 connection loop).
    ServeAxumHyper {
        #[arg(long, default_value = "127.0.0.1:18084")]
        addr: String,
    },
    /// Run an axum server on tokio-uring runtime with a /health and /plaintext endpoint.
    ServeAxumUring {
        #[arg(long, default_value = "127.0.0.1:18085")]
        addr: String,
    },
    /// Run a may-minihttp server with a /health and /plaintext endpoint.
    ServeMay {
        #[arg(long, default_value = "127.0.0.1:18081")]
        addr: String,
    },
    /// Run a std-only HTTP server with a /health and /plaintext endpoint.
    ServeStd {
        #[arg(long, default_value = "127.0.0.1:18083")]
        addr: String,
        /// worker threads (0 = auto)
        #[arg(long, default_value_t = 0)]
        workers: usize,
    },
    /// Run an async HTTP benchmark against a single URL.
    Bench {
        #[arg(long)]
        url: String,
        #[arg(long, default_value_t = 64)]
        concurrency: usize,
        #[arg(long, default_value_t = 10)]
        duration_secs: u64,
        #[arg(long, default_value_t = 1)]
        warmup_secs: u64,
        /// Connect timeout in milliseconds (0 = disable).
        #[arg(long, default_value_t = 200)]
        connect_timeout_ms: u64,
        /// Request timeout in milliseconds (0 = disable).
        #[arg(long, default_value_t = 0)]
        request_timeout_ms: u64,
    },
    /// Spawn all servers and benchmark them sequentially.
    Compare {
        #[arg(long, default_value_t = 64)]
        concurrency: usize,
        #[arg(long, default_value_t = 10)]
        duration_secs: u64,
        #[arg(long, default_value_t = 1)]
        warmup_secs: u64,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 18080)]
        base_port: u16,
        /// Connect timeout in milliseconds (0 = disable).
        #[arg(long, default_value_t = 200)]
        connect_timeout_ms: u64,
        /// Request timeout in milliseconds (0 = disable).
        #[arg(long, default_value_t = 0)]
        request_timeout_ms: u64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::ServeActix { addr, workers } => serve_actix(&addr, workers).context("serve-actix"),
        Cmd::ServeAxum { addr } => serve_axum(&addr).context("serve-axum"),
        Cmd::ServeAxumHyper { addr } => serve_axum_hyper(&addr).context("serve-axum-hyper"),
        Cmd::ServeAxumUring { addr } => serve_axum_uring(&addr).context("serve-axum-uring"),
        Cmd::ServeMay { addr } => serve_may(&addr).context("serve-may"),
        Cmd::ServeStd { addr, workers } => serve_std(&addr, workers).context("serve-std"),
        Cmd::Bench {
            url,
            concurrency,
            duration_secs,
            warmup_secs,
            connect_timeout_ms,
            request_timeout_ms,
        } => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("build tokio runtime")?;
            rt.block_on(async move {
                let warmup = Duration::from_secs(warmup_secs);
                let duration = Duration::from_secs(duration_secs);
                let connect_timeout = Duration::from_millis(connect_timeout_ms);
                let request_timeout = Duration::from_millis(request_timeout_ms);

                eprintln!(
                    "bench: url={url} concurrency={concurrency} warmup={warmup_secs}s duration={duration_secs}s connect_timeout={connect_timeout_ms}ms request_timeout={request_timeout_ms}ms"
                );

                let result = run_bench(&url, concurrency, warmup, duration, connect_timeout, request_timeout).await?;
                result.print("bench");
                Ok(())
            })
        }
        Cmd::Compare {
            concurrency,
            duration_secs,
            warmup_secs,
            host,
            base_port,
            connect_timeout_ms,
            request_timeout_ms,
        } => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("build tokio runtime")?;
            rt.block_on(async move {
                let warmup = Duration::from_secs(warmup_secs);
                let duration = Duration::from_secs(duration_secs);
                let connect_timeout = Duration::from_millis(connect_timeout_ms);
                let request_timeout = Duration::from_millis(request_timeout_ms);
                run_compare(
                    &host,
                    base_port,
                    concurrency,
                    warmup,
                    duration,
                    connect_timeout,
                    request_timeout,
                )
                .await?;
                Ok(())
            })
        }
    }
}

fn serve_actix(addr: &str, workers: usize) -> Result<()> {
    use actix_web::{App, HttpResponse, HttpServer, web};

    let addr = addr.to_string();

    let system = actix_web::rt::System::new();
    let res: std::io::Result<()> = system.block_on(async move {
        let mut server = HttpServer::new(|| {
            App::new()
                .route(
                    "/health",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                )
                .route(
                    "/plaintext",
                    web::get().to(|| async {
                        HttpResponse::Ok()
                            .content_type("text/plain; charset=utf-8")
                            .body("OK")
                    }),
                )
        })
        .disable_signals()
        .bind(&addr)?;

        if workers > 0 {
            server = server.workers(workers);
        }

        server.run().await
    });

    res.context("actix server error")?;
    Ok(())
}

fn serve_axum(addr: &str) -> Result<()> {
    use axum::Router;
    use axum::http::{HeaderValue, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::get;

    fn plaintext() -> Response {
        let mut resp = "OK".into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        resp
    }

    let addr = addr.to_string();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    rt.block_on(async move {
        let app = Router::new()
            .route("/health", get(|| async { StatusCode::OK }))
            .route("/plaintext", get(|| async { plaintext() }));

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("bind {addr}"))?;

        axum::serve(listener, app).await.context("axum serve")?;

        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

fn serve_axum_hyper(addr: &str) -> Result<()> {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{HeaderValue, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::get;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;
    use tower::util::ServiceExt;

    fn plaintext() -> Response {
        let mut resp = "OK".into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        resp
    }

    let addr = addr.to_string();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    rt.block_on(async move {
        let app = Router::new()
            .route("/health", get(|| async { StatusCode::OK }))
            .route("/plaintext", get(|| async { plaintext() }));

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("bind {addr}"))?;

        loop {
            let (stream, _) = listener.accept().await.context("accept")?;
            let io = TokioIo::new(stream);
            let app = app.clone();

            tokio::spawn(async move {
                let svc = service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let app = app.clone();
                    async move {
                        let req = req.map(Body::new);
                        app.oneshot(req).await
                    }
                });

                if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                    eprintln!("hyper connection error: {err}");
                }
            });
        }
    })
}

fn serve_axum_uring(addr: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        return serve_axum_uring_linux(addr);
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = addr;
        bail!("serve-axum-uring requires Linux with io_uring support");
    }
}

#[cfg(target_os = "linux")]
fn serve_axum_uring_linux(addr: &str) -> Result<()> {
    use axum::Router;
    use axum::http::{HeaderValue, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::get;

    fn plaintext() -> Response {
        let mut resp = "OK".into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        resp
    }

    let addr = addr.to_string();
    let result = tokio_uring::start(async move {
        let app = Router::new()
            .route("/health", get(|| async { StatusCode::OK }))
            .route("/plaintext", get(|| async { plaintext() }));

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("bind {addr}"))?;

        axum::serve(listener, app)
            .await
            .context("axum tokio-uring serve")?;

        Ok::<_, anyhow::Error>(())
    });

    result?;
    Ok(())
}

fn serve_may(addr: &str) -> Result<()> {
    use may_minihttp::{HttpServer, HttpService, Request, Response};

    #[derive(Clone, Default)]
    struct PlaintextService;

    impl HttpService for PlaintextService {
        fn call(
            &mut self,
            req: Request<'_, '_, '_>,
            rsp: &mut Response<'_>,
        ) -> std::io::Result<()> {
            match req.path() {
                "/health" | "/plaintext" => {
                    rsp.status_code(200, "OK");
                    rsp.header("Content-Type: text/plain; charset=utf-8");
                    rsp.body("OK");
                }
                _ => {
                    rsp.status_code(404, "Not Found");
                    rsp.body("");
                }
            }
            Ok(())
        }
    }

    let handle = HttpServer(PlaintextService::default())
        .start(addr)
        .context("start may_minihttp server")?;
    let _ = handle.join();
    Ok(())
}

const STD_HEALTH_RESP: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";
const STD_PLAINTEXT_RESP: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nOK";
const STD_404_RESP: &[u8] =
    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";
const STD_405_RESP: &[u8] =
    b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";

fn serve_std(addr: &str, workers: usize) -> Result<()> {
    use std::net::TcpListener;
    use std::sync::mpsc;

    let listener = TcpListener::bind(addr).with_context(|| format!("bind {addr}"))?;
    eprintln!("std server listening on {addr}");

    let workers = match workers {
        0 => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
        n => n,
    };

    // NOTE: Small bounded queue per worker to avoid unlimited memory growth on overload.
    let mut senders: Vec<mpsc::SyncSender<std::net::TcpStream>> = Vec::with_capacity(workers);
    for _ in 0..workers {
        let (tx, rx) = mpsc::sync_channel::<std::net::TcpStream>(1024);
        senders.push(tx);
        std::thread::spawn(move || {
            while let Ok(mut stream) = rx.recv() {
                let _ = stream.set_nodelay(true);
                let _ = handle_std_conn(&mut stream);
            }
        });
    }

    let mut next_worker = 0usize;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let tx = &senders[next_worker];
                if tx.send(stream).is_err() {
                    break;
                }
                next_worker += 1;
                if next_worker >= senders.len() {
                    next_worker = 0;
                }
            }
            Err(err) => eprintln!("accept error: {err}"),
        }
    }

    Ok(())
}

fn handle_std_conn(stream: &mut std::net::TcpStream) -> std::io::Result<()> {
    use std::io::{Read, Write};

    let mut buf = [0u8; 8192];
    let mut len = 0usize;

    loop {
        let Some(header_end) = find_double_crlf(&buf[..len]) else {
            if len == buf.len() {
                // Request headers are too large for this tiny demo.
                return Ok(());
            }
            let n = stream.read(&mut buf[len..])?;
            if n == 0 {
                return Ok(());
            }
            len += n;
            continue;
        };

        let request = &buf[..header_end];
        let Some(line_end) = find_crlf(request) else {
            return Ok(());
        };
        let request_line = &request[..line_end];

        let Some(sp1) = request_line.iter().position(|&b| b == b' ') else {
            return Ok(());
        };
        let Some(sp2) = request_line[sp1 + 1..].iter().position(|&b| b == b' ') else {
            return Ok(());
        };
        let sp2 = sp1 + 1 + sp2;

        let method = &request_line[..sp1];
        let mut path = &request_line[sp1 + 1..sp2];
        if let Some(q) = path.iter().position(|&b| b == b'?') {
            path = &path[..q];
        }

        let resp = if method != b"GET" {
            STD_405_RESP
        } else if path == b"/health" {
            STD_HEALTH_RESP
        } else if path == b"/plaintext" {
            STD_PLAINTEXT_RESP
        } else {
            STD_404_RESP
        };

        stream.write_all(resp)?;

        // Shift any remaining bytes (pipelined requests) to the front.
        let remaining = len - header_end;
        if remaining > 0 {
            buf.copy_within(header_end..len, 0);
        }
        len = remaining;
    }
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    for i in 1..buf.len() {
        if buf[i - 1] == b'\r' && buf[i] == b'\n' {
            return Some(i - 1);
        }
    }
    None
}

#[derive(Clone, Debug)]
struct BenchResult {
    ok: u64,
    err: u64,
    elapsed: Duration,
    hist_micros: Histogram<u64>,
}

impl BenchResult {
    fn rps(&self) -> f64 {
        if self.elapsed.as_secs_f64() == 0.0 {
            return 0.0;
        }
        self.ok as f64 / self.elapsed.as_secs_f64()
    }

    fn fmt_micros(v: u64) -> String {
        if v >= 1000 {
            format!("{:.3}ms", (v as f64) / 1000.0)
        } else {
            format!("{v}µs")
        }
    }

    fn print(&self, name: &str) {
        let avg = self.hist_micros.mean();
        let p50 = self.hist_micros.value_at_quantile(0.50);
        let p90 = self.hist_micros.value_at_quantile(0.90);
        let p99 = self.hist_micros.value_at_quantile(0.99);
        let max = self.hist_micros.max();

        println!("{name}:");
        println!(
            "  ok={} err={} elapsed={:.2}s rps={:.0}",
            self.ok,
            self.err,
            self.elapsed.as_secs_f64(),
            self.rps()
        );
        println!(
            "  latency: avg={} p50={} p90={} p99={} max={}",
            Self::fmt_micros(avg as u64),
            Self::fmt_micros(p50),
            Self::fmt_micros(p90),
            Self::fmt_micros(p99),
            Self::fmt_micros(max),
        );
    }
}

async fn run_bench(
    url: &str,
    concurrency: usize,
    warmup: Duration,
    duration: Duration,
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<BenchResult> {
    let mut builder = reqwest::Client::builder()
        .pool_max_idle_per_host(concurrency)
        .http1_only();

    if connect_timeout > Duration::ZERO {
        builder = builder.connect_timeout(connect_timeout);
    }
    if request_timeout > Duration::ZERO {
        builder = builder.timeout(request_timeout);
    }

    let client = builder.build().context("build reqwest client")?;

    if warmup > Duration::ZERO {
        eprintln!("warmup: {:.2}s", warmup.as_secs_f64());
        bench_once(&client, url, concurrency, warmup).await?;
    }

    eprintln!("benching: {:.2}s", duration.as_secs_f64());
    let result = bench_once(&client, url, concurrency, duration).await?;
    Ok(result)
}

async fn bench_once(
    client: &reqwest::Client,
    url: &str,
    concurrency: usize,
    duration: Duration,
) -> Result<BenchResult> {
    let end = Instant::now() + duration;
    let url = url.to_string();

    let mut tasks = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let client = client.clone();
        let url = url.clone();
        tasks.push(tokio::spawn(async move {
            let mut ok = 0u64;
            let mut err = 0u64;

            // Track up to 60s in microseconds with 3 digits precision.
            let mut hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)
                .expect("histogram bounds must be valid");

            while Instant::now() < end {
                let start = Instant::now();
                let res = client.get(&url).send().await;
                match res {
                    Ok(resp) => {
                        let status = resp.status();
                        let _ = resp.bytes().await;
                        if status.is_success() {
                            ok += 1;
                            let micros = start.elapsed().as_micros() as u64;
                            let _ = hist.record(micros.max(1));
                        } else {
                            err += 1;
                        }
                    }
                    Err(_) => {
                        err += 1;
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                }
            }

            Ok::<_, anyhow::Error>((ok, err, hist))
        }));
    }

    let start = Instant::now();
    let mut ok = 0u64;
    let mut err = 0u64;
    let mut merged = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)
        .expect("histogram bounds must be valid");

    for t in tasks {
        let (t_ok, t_err, hist) = t.await.context("join bench task")??;
        ok += t_ok;
        err += t_err;
        merged.add(hist).ok();
    }

    Ok(BenchResult {
        ok,
        err,
        elapsed: start.elapsed(),
        hist_micros: merged,
    })
}

async fn run_compare(
    host: &str,
    base_port: u16,
    concurrency: usize,
    warmup: Duration,
    duration: Duration,
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<()> {
    let exe = std::env::current_exe().context("current_exe")?;

    let actix_addr = format!("{host}:{base_port}");
    let may_addr = format!("{host}:{}", base_port + 1);
    let axum_addr = format!("{host}:{}", base_port + 2);
    let std_addr = format!("{host}:{}", base_port + 3);
    let axum_hyper_addr = format!("{host}:{}", base_port + 4);
    #[cfg(target_os = "linux")]
    let axum_uring_addr = format!("{host}:{}", base_port + 5);

    let actix_url = format!("http://{actix_addr}/plaintext");
    let may_url = format!("http://{may_addr}/plaintext");
    let axum_url = format!("http://{axum_addr}/plaintext");
    let std_url = format!("http://{std_addr}/plaintext");
    let axum_hyper_url = format!("http://{axum_hyper_addr}/plaintext");
    #[cfg(target_os = "linux")]
    let axum_uring_url = format!("http://{axum_uring_addr}/plaintext");

    let mut actix = spawn_server(&exe, "serve-actix", &actix_addr, &[])?;
    wait_ready(
        format!("http://{actix_addr}/health"),
        connect_timeout,
        request_timeout,
    )
    .await?;
    let actix_res = run_bench(
        &actix_url,
        concurrency,
        warmup,
        duration,
        connect_timeout,
        request_timeout,
    )
    .await;
    stop_child(&mut actix);
    let actix_res = actix_res?;

    let mut may = spawn_server(&exe, "serve-may", &may_addr, &[])?;
    wait_ready(
        format!("http://{may_addr}/health"),
        connect_timeout,
        request_timeout,
    )
    .await?;
    let may_res = run_bench(
        &may_url,
        concurrency,
        warmup,
        duration,
        connect_timeout,
        request_timeout,
    )
    .await;
    stop_child(&mut may);
    let may_res = may_res?;

    let mut axum = spawn_server(&exe, "serve-axum", &axum_addr, &[])?;
    wait_ready(
        format!("http://{axum_addr}/health"),
        connect_timeout,
        request_timeout,
    )
    .await?;
    let axum_res = run_bench(
        &axum_url,
        concurrency,
        warmup,
        duration,
        connect_timeout,
        request_timeout,
    )
    .await;
    stop_child(&mut axum);
    let axum_res = axum_res?;

    let mut axum_hyper = spawn_server(&exe, "serve-axum-hyper", &axum_hyper_addr, &[])?;
    wait_ready(
        format!("http://{axum_hyper_addr}/health"),
        connect_timeout,
        request_timeout,
    )
    .await?;
    let axum_hyper_res = run_bench(
        &axum_hyper_url,
        concurrency,
        warmup,
        duration,
        connect_timeout,
        request_timeout,
    )
    .await;
    stop_child(&mut axum_hyper);
    let axum_hyper_res = axum_hyper_res?;

    #[cfg(target_os = "linux")]
    let axum_uring_res = {
        let mut axum_uring = spawn_server(&exe, "serve-axum-uring", &axum_uring_addr, &[])?;
        wait_ready(
            format!("http://{axum_uring_addr}/health"),
            connect_timeout,
            request_timeout,
        )
        .await?;
        let axum_uring_res = run_bench(
            &axum_uring_url,
            concurrency,
            warmup,
            duration,
            connect_timeout,
            request_timeout,
        )
        .await;
        stop_child(&mut axum_uring);
        axum_uring_res?
    };

    let mut std = spawn_server(&exe, "serve-std", &std_addr, &["--workers", "0"])?;
    wait_ready(
        format!("http://{std_addr}/health"),
        connect_timeout,
        request_timeout,
    )
    .await?;
    let std_res = run_bench(
        &std_url,
        concurrency,
        warmup,
        duration,
        connect_timeout,
        request_timeout,
    )
    .await;
    stop_child(&mut std);
    let std_res = std_res?;

    println!("\n--- compare ---");
    actix_res.print("actix-web");
    axum_res.print("axum");
    axum_hyper_res.print("axum+hyper");
    #[cfg(target_os = "linux")]
    axum_uring_res.print("axum+tokio-uring");
    #[cfg(not(target_os = "linux"))]
    println!("axum+tokio-uring: skipped (requires Linux + io_uring)");
    may_res.print("may-minihttp");
    std_res.print("std");

    Ok(())
}

fn spawn_server(
    exe: &std::path::Path,
    subcmd: &str,
    addr: &str,
    extra_args: &[&str],
) -> Result<Child> {
    let mut cmd = Command::new(exe);
    cmd.arg(subcmd).arg("--addr").arg(addr);
    cmd.args(extra_args);

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    cmd.spawn()
        .with_context(|| format!("spawn server {subcmd} {addr}"))
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

async fn wait_ready(
    url: String,
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<()> {
    let connect_timeout = if connect_timeout > Duration::ZERO {
        connect_timeout
    } else {
        Duration::from_millis(200)
    };
    let request_timeout = if request_timeout > Duration::ZERO {
        request_timeout
    } else {
        Duration::from_millis(500)
    };

    let client = reqwest::Client::builder()
        .http1_only()
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .build()
        .context("build reqwest client")?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if Instant::now() >= deadline {
            bail!("timeout waiting server ready: {url}");
        }
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
}
