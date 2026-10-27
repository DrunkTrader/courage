# Contributing to Courage

Keep changes small and verifiable. The capture path is latency-sensitive, so
new work should state its memory bound, blocking behavior, and measurement.

Before opening a pull request:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
make media-validation
npm --prefix apps/desktop run build
```

Platform-specific capture changes must include the operating system, backend,
permissions tested, hardware/virtual display used, and known limitations.
Do not add cloud, account, telemetry, or social dependencies to the core
recorder without a separate architecture decision.
