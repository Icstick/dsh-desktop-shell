# dsh-adapter-dsh (MOD-ADAPTER-DSH, M5-C)

Legacy DSH adapter crate: consumes the DSH public HTTP/WS surface and maps it
onto desktop capabilities. Rust, sync, no async runtime, no tauri dependency.

## Capabilities (M5-C slice, ADR-0018 decision 6)

- Notification (complete): launch-token auth, /api/remote.mux WS stream,
  $events consumption, allowlist, mapping to AdapterNotification (shaped
  after specs/notification/notification-request.schema.json).
- Usage (partial): token-usage aggregation from session flows
  (assistant/message data.usage + usage chunks), output aligned to
  specs/usage/usage-record.schema.json + usage-snapshot.schema.json.
- Restart hints (downgraded, ADR-0018 decision 6): DSH has no native
  restart-hint surface; cordis/dynamic-package, cordis/dynamic-retract and
  settings/document-updated events produce a generic config-changed hint
  (dedupe key config-changed).

## Wire surface (verified 2026-08-30 against D:\deepseek-harness source)

- Auth: GET /?token=<launch token> -> 303 + Set-Cookie
  dsh-auth-<b64url(sha256(authority))> (HttpOnly, SameSite=Strict, Path=/,
  Max-Age 30d). The adapter echoes the cookie on /api and on the WS upgrade.
- /api: POST /api/<ns>/<method> with envelope
  {"type":"client-request","rpcId","method","payload":{"args":{...}}};
  response {"type":"server-response","rpcId","result":{"ok":true,"value"|...}}.
- WS: single route /api/remote.mux; logical streams opened with
  {"type":"open","streamId","endpoint","payload":{"args":{}}}; server frames
  item/error/end. $events is a logical endpoint; its item values are
  ready/emit/waterfall/cancel frames. Waterfall results are answered over
  HTTP POST /api/$events/result (EventOutcome).
- No API versioning anywhere: parsing is shape-driven, tolerant of extra
  fields, and fails closed on missing required fields (AdapterError).

## Module map

- auth: launch token -> cookie (pure, unit-tested)
- http: minimal loopback HTTP/1.1 codec (pure) + TcpHttpTransport
- ws: RFC 6455 frame codec (pure) + TungsteniteTransport (sync, no TLS)
- mux: MuxClient over the remote.mux protocol (open/cancel/item/error/end)
- events: EventMessage parsing + M5-C allowlist subset
- notify: EventMessage -> AdapterNotification mapping (title-only policy)
- usage: UsageAggregator -> UsageRecord/UsageTotals (isEstimate always)
- jsonrpc: envelope, ApiClient, $events/result submission
- client: DshClient (auth + /api + ws_url), EventStream, SessionFlow,
  AdapterPipeline

## Usage sketch (wiring lives in the tauri layer, outside this crate)

    let config = DshClientConfig::new("http://127.0.0.1:6800", launch_token);
    let http = TcpHttpTransport::new(addr);
    let mut client = DshClient::new(config, http);
    client.authenticate()?;
    let ws = TungsteniteTransport::connect(&client.ws_url()?, client.cookie())?;
    let mut pipeline = AdapterPipeline::new(EventStream::open(ws)?);
    while let Some(notification) = pipeline.next_notification()? {
        // deliver to NotificationService (desktop side)
    }
    let (records, totals) = pipeline.snapshot_usage(unix_ms());

## Degradation semantics (ADR-0018 decision 4)

Every failure is an explicit AdapterError (Http/Rpc/Protocol/Auth/Transport).
No panic paths; no unsafe (forbid(unsafe_code)); the caller keeps the L0
baseline (DSH process + HTTP Web UI) untouched. Malformed frames fail the
stream rather than being guessed.

## Known gaps (documented, ADR-0018 decision 6)

- SessionFlow: the mux machinery is real and tested, but the session/follow
  endpoint name and open payload are UNVERIFIED against a live DSH (marked
  in client.rs, SESSION_FOLLOW_ENDPOINT). Verify against a running DSH
  before relying on it.
- api-session/removed -> TurnCompleted is an inference (no verified
  completed event exists in the 18-event allowlist).
- WS client: tungstenite 0.30.0 (sync, default features = handshake only).
  Selected because the crate layer must stay sync and the DSH surface is
  loopback ws:// (wss:// is rejected at runtime; HTTPS is out of scope).
- Notification body: adapter always uses TitleOnly; approval/question
  details belong to the desktop UI flow.

## Quality

- cargo fmt --all --check
- cargo clippy -p dsh-adapter-dsh --all-targets -- -D warnings
- cargo test -p dsh-adapter-dsh (53 unit + 9 fake-DSH integration tests)
