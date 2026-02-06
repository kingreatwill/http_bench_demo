# http_bench_demo

一个最小 demo，用同一套 client 压测逻辑对比：

- `actix-web`
- `may-minihttp`（crate: `may_minihttp`）
- `axum`
- `axum+hyper`
- `axum+tokio-uring`（Linux + io_uring）
- `volo-http`
- `std`（Rust 标准库：`std::net::TcpListener`）

## 运行

在仓库根目录执行：

```bash
# 1) 对比（会自动起五个 server，然后分别压测）
cargo run --release --manifest-path Cargo.toml -- compare \
  --concurrency 128 \
  --duration-secs 10 \
  --warmup-secs 1

# 默认端口：actix=18080, may=18081, axum=18082, std=18083, axum+hyper=18084, axum+tokio-uring=18085, volo-http=18086（可用 --base-port 调整）
```

也可以分别启动 server，再手动 bench：

```bash
# actix
cargo run --release --manifest-path ./Cargo.toml -- serve-actix --addr 127.0.0.1:18080

# may
cargo run --release --manifest-path ./Cargo.toml -- serve-may --addr 127.0.0.1:18081

# axum(推荐)
cargo run --release --manifest-path ./Cargo.toml -- serve-axum --addr 127.0.0.1:18082

# axum+hyper（手动 hyper http1 connection loop）
cargo run --release --manifest-path ./Cargo.toml -- serve-axum-hyper --addr 127.0.0.1:18084

# axum+tokio-uring（Linux + io_uring）
cargo run --release --manifest-path ./Cargo.toml -- serve-axum-uring --addr 127.0.0.1:18085

# volo-http
cargo run --release --manifest-path ./Cargo.toml -- serve-volo-http --addr 127.0.0.1:18086

# std（Rust 标准库）
cargo run --release --manifest-path ./Cargo.toml -- serve-std --addr 127.0.0.1:18083 --workers 0

# bench
cargo run --release --manifest-path ./Cargo.toml -- bench --url http://127.0.0.1:18086/plaintext --concurrency 128 --duration-secs 10 --request-timeout-ms 2000
```

## Go /plaintext（可选）

本仓库还提供 3 个 Go 版本的 `/health` + `/plaintext`，用于和 Rust 的 client bench 配合做对比：

```bash
# Go net/http（标准库）
go run ./go_plaintext/cmd/std --addr 127.0.0.1:28080

# Go fasthttp
go run ./go_plaintext/cmd/fasthttp --addr 127.0.0.1:28081

# Go hertz(推荐, 甚至优于axum)
go run ./go_plaintext/cmd/hertz --addr 127.0.0.1:28082

# 用本 demo 的 client 去压 Go（示例）
cargo run --release --manifest-path ./Cargo.toml -- bench --url http://127.0.0.1:28082/plaintext --concurrency 128 --duration-secs 10 --request-timeout-ms 2000
```

## 注意

- 这个 demo 主要用于快速对比，真实压测建议用 `wrk`/`oha` 等专业工具，并尽量固定 CPU 频率、关闭 Turbo、固定 worker 数、避免其他程序干扰。
- `may-minihttp` 在不同 fork/版本里 API 可能不一样；如果你本地编译不过，请优先修改 `src/main.rs` 里的 `serve_may` 实现来适配你用的版本。
- `bench` 默认不会实时打印进度，通常在 `warmup + duration` 结束后输出结果；如果 URL 不可达导致看起来“卡住”，可调小 `--connect-timeout-ms`/`--request-timeout-ms`（例如 `--connect-timeout-ms 200`）。
- `axum+tokio-uring` 仅在 Linux + io_uring 环境可用；非 Linux 会提示不支持并跳过 compare。
