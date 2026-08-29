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

## Semantic gates

- Attached `port=auto` is rejected before network access; Desktop never scans ports or starts/stops a process.
- Managed requests never accept executable, argv, cwd, host or port from the caller; they resolve the persisted Environment.
- Every successful Managed start increments generation and creates a new opaque instance ID. Stop requires the exact current generation; stale requests return `STALE_GENERATION`.
- `dsh web:` output is only a candidate. Publication additionally requires the emitting child handle to remain owned/current, either an exact legacy credential-free root or the ADR-0012 exact authenticated token root, configured fixed-port agreement when applicable, and a bounded TCP connection. Public serialization remains credential-free.
- TCP reachability without the owned output marker never publishes a Managed endpoint. Attached reachability continues to mean identity `unverified`.
- M1 stop acts only on the retained child/process-tree handle. It never reconstructs authority from PID or port, and reports whether graceful or forced cleanup was required.
