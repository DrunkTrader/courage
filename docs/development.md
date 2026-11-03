# Development

## Fast loop

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
make media-validation
bash scripts/benchmark-core.sh
npm --prefix apps/desktop run build
```

The CLI is the first reproducible media test. It creates frames in memory,
pushes them through the bounded buffer and exporter, then stores a clip row in SQLite.

## Performance notes

- Keep the buffer byte budget explicit; frame count alone is not enough when
  resolution or pixel format changes.
- Never do filesystem work while holding the buffer mutex.
- Keep FFmpeg work outside the capture path; add a bounded queue only when
  repeated saves need backpressure.
- Measure end-to-end latency, export throughput, peak resident memory, and
  dropped frames on real target hardware before enabling hardware encoders by
  default.

The benchmark script records three wall-clock runs for a deterministic
synthetic workload. It is a regression signal for the pipeline, not a claim
about native capture performance on a user's GPU.
