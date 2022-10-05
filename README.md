# lazyproxy

A TCP proxy that is so lazy that shuts down itself after a period of inactivity. Intended to be used in [Fly.io Machines](https://fly.io/docs/reference/machines/).

Inspired by [tired-proxy](https://github.com/superfly/tired-proxy) but works on TCP instead of HTTP.

## Usage

```bash
export RUST_LOG=info
lazyproxy --listen 0.0.0.0:8080 --target localhost:3000 --timeout-secs 60 --wait
```
