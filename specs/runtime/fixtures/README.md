# Runtime fixture matrix

| Fixture | Expected | Boundary |
|---|---|---|
| `attached-health-request.valid.json` | valid | caller selects only a persisted Environment ID |
| `attached-health-report.valid.json` | valid | fixed-loopback reachability remains identity `unverified`, ownership `external`, mutation `denied` |
| `attached-health-report.invalid.json` | invalid | reachability cannot claim verified identity or Desktop ownership |
| `managed-runtime-start-request.valid.json` | valid | launch material remains backend-owned |
| `managed-runtime-status-request.valid.json` | valid | status is scoped to one persisted Managed Environment |
| `managed-runtime-stop-request.valid.json` | valid | destructive stop is generation-bound |
| `managed-runtime-report.valid.json` | valid | healthy publication requires owned generation output plus bounded TCP readiness |
| `managed-runtime-report.invalid.json` | invalid | a foreign process cannot publish a Managed endpoint |
| `managed-runtime-report.safe-stop.valid.json` | valid | recovery budget exhaustion publishes `safe_stop` with no endpoint and no auto-restart |
| `managed-runtime-report.recovery.valid.json` | valid | healthy generation may carry bounded recovery history (crashCount within budget) |
| `managed-runtime-report.crashed-recovery.invalid.json` | invalid | `crashed` state must not claim safeStop exhaustion |
| `managed-runtime-restart-request.valid.json` | valid | restart is scoped to one persisted Environment and the exact current generation |
| `managed-runtime-restart-request.invalid.json` | invalid | restart with generation 0 cannot target a retained generation |
| `diagnostics-report.valid.json` | valid | credential-free snapshot condenses healthy runtime, ready Surface, catalog revision and retained owned process into evidence |
| `diagnostics-report.invalid.json` | invalid | bootstrap URL/token/cookie and endpoint URL fields are rejected by `additionalProperties:false` |

## Semantic gates

- Attached `port=auto` is rejected before network access; Desktop never scans ports or starts/stops a process.
- Managed requests never accept executable, argv, cwd, host or port from the caller; they resolve the persisted Environment.
- Every successful Managed start increments generation and creates a new opaque instance ID. Stop requires the exact current generation; stale requests return `STALE_GENERATION`.
- `dsh web:` output is only a candidate. Publication additionally requires the emitting child handle to remain owned/current, either an exact legacy credential-free root or the ADR-0012 exact authenticated token root, configured fixed-port agreement when applicable, and a bounded TCP connection. Public serialization remains credential-free.
- TCP reachability without the owned output marker never publishes a Managed endpoint. Attached reachability continues to mean identity `unverified`.
- M1 stop acts only on the retained child/process-tree handle. It never reconstructs authority from PID or port, and reports whether graceful or forced cleanup was required.
- M2 restart stops the exact current generation and starts a new one from the same persisted Environment; recovery (`crashCount`/window/budget/`safeStop`) is public and credential-free, and exhaustion publishes `safe_stop` instead of unbounded restarts.
- M2-B Diagnostics is read-only and credential-free: the report never carries tokens, cookies, query strings, bootstrap URLs, full URLs, or PIDs; the runtime endpoint exposes only the fixed loopback host and port, and every evidence message is a bounded static string.
