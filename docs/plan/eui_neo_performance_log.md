# EUI-NEO Performance Log

This file records measured EUI-NEO performance changes so optimization work has a durable history.

## Measurement Policy

- Prefer `--release` for performance conclusions.
- Keep correctness tests separate from performance claims.
- Record whether the number came from a real visible window, `SKY_NEO_PROFILE`, or a targeted test.
- Do not treat single-run FPS title values as precise benchmark numbers; use them as smoke-level real-app evidence.
- Keep raw profiler logs under `target/` only as temporary local artifacts.

## Results

| Date | Commit | Scenario | Measurement | Result | Notes |
| --- | --- | --- | --- | --- | --- |
| 2026-05-29 | `b712744` | Stress Lab release, periodic clock side-system A/B | `SKY_NEO_PROFILE=1`, screenshot frame 80, last 40 compose samples | Before avg `0.879ms`, median `0.845ms`, p95 `1.031ms`; after avg `0.860ms`, median `0.851ms`, p95 `0.939ms` | Correct but small win. Stress Lab mostly uses continuous `clock().seconds()` / `frame_index()`, so `clock().every(...)` only helps limited paths. |
| 2026-05-29 | `239b1bc` | Stress Lab release, dirty root id precompute A/B | `SKY_NEO_PROFILE=1`, screenshot frame 80, last 40 compose samples | Before avg `0.96ms`, median `0.91ms`, p95 `1.09ms`; after avg `0.86ms`, median `0.85ms`, p95 `0.93ms` | Real retained-reuse optimization. Main win is build/reuse bookkeeping, not layout. |
| 2026-05-29 | `239b1bc` | Stress Lab release, real visible window | Sampled window title 30 times at 500ms intervals | FPS min `454`, median `485`, max `489`, avg `483.2`; frame ms min `1.4`, median `2.1`, max `2.7`, avg `1.99` | User-visible FPS is still around 500, so compose wins are real but not enough to make the whole app obviously faster. No durable pre-optimization FPS baseline was saved before this row. |

