# Jyozu

> A talk-to-Figma MCP server, in Rust.

Jyozu is a Rust port of [kai-jyozu](https://github.com/justinelliottcobb/kai-jyozu),
a Model Context Protocol (MCP) server that lets AI agents interact with Figma —
reading designs, creating and editing nodes, and bridging the gap between
natural-language design intent and Figma's data model.

## What it does

- Exposes Figma operations as MCP tools: read, safe mutation, destructive
  mutation, provider diagnostics, and envelope behavior.
- Defines a precise, parity-oriented contract surface for migrating Figma
  tooling to a Rust provider (see [`docs/`](docs/) for the first-slice
  contract pack).
- Designed for embedding in agent harnesses that consume MCP tooling (e.g.,
  [Wallace](../Wallace), [Dominic](../Dominic)).

## Status

**Active port.** The first-slice contract pack targeting read operations, safe
and destructive mutation, and provider diagnostics is defined; the Rust
provider rewrite is in progress.

## Related

- [kai-jyozu](https://github.com/justinelliottcobb/kai-jyozu) — the original
  implementation this ports from
- [Lonis](../Lonis) — bitmap-to-structured-design-data analyzer (complementary
  design-tooling)

## License

Dual-licensed under MIT OR Apache-2.0.
