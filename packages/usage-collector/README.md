# Usage Collector

**Module ID:** `MOD-USAGE-COLLECTOR`
**Target milestone:** M3
**Canonical status:** [MOD-USAGE-COLLECTOR](../../tracking/modules/MOD-USAGE-COLLECTOR.yaml)

## Purpose

在 DSH 侧采集权威 usage seam 并输出 normalized telemetry。

## Owns

- dedupe/aggregation
- source/estimate metadata
- schema mapping

## Does not own

- Desktop charts
- provider credential exposure

## Inputs

- DSH usage events/projections

## Outputs

- UsageTelemetry

## Dependencies

- adapter-dsh

## Interfaces

- `IF-USAGE`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
