# Agent Feedback and Lonis Migration Prompt

## Purpose

This document is a corrective feedback prompt for the ongoing Rust rewrite work in this repository.

The most important clarification is this:

> The goal is **not** a generic Rust rewrite of the old MCP surface.
>
> The goal is a **Lonis-compatible tool provider** that exposes a proper **Lonis tool surface**.

A parity-first rewrite is still the right approach, but parity is meant to preserve semantics while moving toward a **Lonis-native provider contract**, not to preserve an MCP-shaped external interface.

---

## Core Clarification

The current implementation progress is promising, but it appears to have drifted toward a direct Rust port of the old MCP-facing tool surface.

That drift was **not intentional**.

### What we want
We want:
- a Rust Figma provider
- consumable by the future Lonis harness
- through the Lonis external provider contract
- exposing **Lonis-style tool identities and machine-readable contracts**

### What we do not want
We do **not** want:
- a generic standalone Rust replacement for the old MCP server
- MCP-shaped external naming as the durable API surface
- ad hoc CLI invocation as the lasting interface model
- safety semantics encoded primarily in prose descriptions

The rewrite should preserve tool semantics, not preserve the MCP-shaped exterior.

---

## Positive Feedback on Current Progress

The current work already has strong foundations:

- Rust provider executable exists
- bridge/session infrastructure exists
- `manifest`, `tools list`, `tools describe`, `call`, `status`, and `doctor` commands exist
- bridge HTTP/WebSocket runtime exists
- a contract inventory has been extracted
- tests are present and passing

This is good work.

The issue is not that the rewrite is going badly.
The issue is that the **target boundary needs to be corrected now**, before more implementation depth accumulates around the wrong external shape.

---

## The Required Directional Correction

### Preserve internally
Preserve parity in:
- tool semantics
- request/response meaning
- safety behavior
- dry-run behavior
- diagnostics intent
- bridge/session behavior where required

### Change externally
Move the external provider shape toward:
- Lonis-compatible namespaced tools
- Lonis provider protocol request/response shape
- Lonis contract metadata
- Lonis-oriented machine-readable discovery

This means the provider should act like:

```text
Lonis provider for Figma
```

not like:

```text
MCP server rewritten in Rust
```

---

## Most Important External-Surface Corrections

## 1. Tool naming must become Lonis-oriented

Current/legacy-style examples:
- `get_document_info`
- `get_page_info`
- `get_node_info`

Desired Lonis-facing examples:
- `figma.get_document`
- `figma.get_page`
- `figma.get_node`

This does **not** require changing the internal plugin command names immediately.

A mapping layer is acceptable and desirable:
- external provider contract uses Lonis names
- internal bridge/plugin translation maps to legacy runtime names as needed

That is likely the cleanest transition strategy.

## 2. `call` should move toward protocol-compliant machine invocation

Current implementation appears to accept tool args as a positional JSON CLI argument.

That is acceptable as temporary bootstrap behavior, but the target should be the Lonis provider protocol shape:
- canonical JSON request object
- ideally via stdin for machine mode
- canonical JSON envelope on stdout

The provider should optimize for AI/tool-harness consumption, not shell convenience.

## 3. Tool descriptions must become full contracts

Current descriptions appear to contain:
- name
- description
- input schema

The target contract should also include:
- provider
- schema version
- output schema
- capabilities
- side effects
- determinism
- cost
- verification status
- safety metadata

## 4. Safety semantics must become structured metadata

Current destructive semantics appear largely preserved in prose, e.g.:
- requires confirmation
- requires destructive startup gate

These need to be expressed as structured machine-readable fields, not only description text.

Examples:
- `destructive: true`
- `requires_confirmation: true`
- `supports_dry_run: true`
- `policy_tags: ["shared_resource_risk"]`

## 5. Diagnostics should remain first-class

`status` and `doctor` are absolutely the right direction.

Please keep them and align their outputs progressively toward structured Lonis/provider diagnostics.

---

## Migration Checklist: Current State -> Lonis-Compatible Provider

Use this checklist as the next-phase guide.

### Phase A: Freeze the external target
- [ ] Explicitly adopt **Lonis tool names** as the external canonical names
- [ ] Decide which legacy tool names remain internal-only aliases/mappings
- [ ] Confirm the first-slice tool set to implement against the contract pack
- [ ] Treat `docs/2026-03-28-figma-first-slice-contract-pack.md` as the reference target

### Phase B: Normalize discovery surfaces
- [ ] Update `manifest` to return richer provider metadata
- [ ] Update `tools list` to return structured tool summaries, not just raw names
- [ ] Update `tools describe` to return full Lonis-style tool contracts
- [ ] Include verification and safety metadata in tool descriptions

### Phase C: Normalize invocation
- [ ] Change `call` to accept the canonical Lonis/provider request shape
- [ ] Prefer stdin JSON request handling for machine mode
- [ ] Return canonical JSON envelopes on stdout
- [ ] Keep stderr for diagnostics only
- [ ] Decide and document exit semantics clearly

### Phase D: Introduce external/internal tool mapping
- [ ] Add a mapping layer from Lonis-facing tool names to bridge/plugin command names
- [ ] Ensure the plugin protocol can remain stable while the provider surface modernizes
- [ ] Keep mapping logic explicit and testable

### Phase E: Formalize first-slice contracts
- [ ] Implement first-slice contracts for:
  - [ ] `figma.get_document`
  - [ ] `figma.get_page`
  - [ ] `figma.get_selection`
  - [ ] `figma.get_node`
  - [ ] `figma.create_frame`
  - [ ] `figma.create_text`
  - [ ] `figma.set_fill`
  - [ ] `figma.move_node`
  - [ ] `figma.delete_node`
- [ ] Add output schemas for these tools
- [ ] Add safety metadata for these tools
- [ ] Add verification metadata for these tools

### Phase F: Formalize safety semantics
- [ ] Preserve confirmation-required behavior
- [ ] Preserve dry-run behavior
- [ ] Preserve policy-block behavior where applicable
- [ ] Stop relying on prose-only safety descriptions
- [ ] Add structured error codes like:
  - [ ] `confirmation_required`
  - [ ] `permission_denied`
  - [ ] `provider_unavailable`
  - [ ] `not_found`
  - [ ] `invalid_input`

### Phase G: Diagnostics alignment
- [ ] Keep `status` and `doctor`
- [ ] Align outputs with the contract pack and provider protocol docs
- [ ] Ensure bridge/session information is surfaced clearly
- [ ] Preserve operational visibility as a first-class feature

### Phase H: Parity testing
- [ ] Add contract/parity tests for the first slice
- [ ] Test representative success paths
- [ ] Test destructive refusal paths
- [ ] Test dry-run behavior
- [ ] Test provider discovery output
- [ ] Test provider stdout/stderr discipline

---

## Concrete Recommendation on Naming

Please adopt this principle now:

> **External names are Lonis names. Internal names may remain legacy names until replaced.**

Example:

- external: `figma.get_document`
- internal provider mapping: `get_document_info`
- plugin command mapping: `getDocumentInfo` or equivalent bridge command

That lets us preserve runtime parity without freezing the wrong API surface.

---

## Concrete Recommendation on Request Shape

Move toward this model for `call`:

```json
{
  "tool": "figma.get_document",
  "schema_version": "1",
  "input": {
    "session_id": "optional"
  },
  "context": {
    "request_id": "req_123",
    "profile": "default"
  }
}
```

And return:

```json
{
  "ok": true,
  "tool": "figma.get_document",
  "provider": "figma",
  "schema_version": "1",
  "result": {
    "document_id": "...",
    "name": "...",
    "pages": []
  },
  "meta": {
    "duration_ms": 12,
    "warnings": []
  }
}
```

This is the shape Lonis can consume cleanly.

---

## Concrete Recommendation on Legacy Compatibility

If it is useful during transition, it is acceptable to:
- temporarily support legacy names internally
- temporarily keep legacy command mappings
- temporarily allow compatibility shims

But the canonical provider contract should now be treated as:
- Lonis-facing
- namespaced
- machine-oriented
- schema-rich

---

## Prompt to Continue the Work

Use the following prompt for the next implementation pass:

---

You are continuing the Rust Figma provider rewrite in this repository.

Important correction:
- The goal is **not** a direct Rust rewrite of the old MCP-facing API surface.
- The goal is a **Lonis-compatible Figma provider** with a proper Lonis tool surface.
- Preserve semantics and safety behavior from the legacy system, but do **not** preserve MCP-shaped external naming or ad hoc invocation as the durable interface.

What to do next:
1. Treat the current implementation as a promising bootstrap, not the final external contract.
2. Introduce a mapping layer so the external provider uses Lonis-style canonical tool names such as:
   - `figma.get_document`
   - `figma.get_page`
   - `figma.get_selection`
   - `figma.get_node`
   - `figma.create_frame`
   - `figma.create_text`
   - `figma.set_fill`
   - `figma.move_node`
   - `figma.delete_node`
3. Preserve the internal bridge/plugin behavior as needed for parity.
4. Move `call` toward the Lonis provider request/response protocol.
5. Expand `tools describe` so it returns full Lonis-style contracts, including:
   - output schema
   - capabilities
   - side effects
   - verification status
   - safety metadata
6. Keep `status` and `doctor` as first-class provider diagnostics.
7. Focus on the first-slice contract pack before expanding the full tool surface.

Priority references:
- `docs/2026-03-28-figma-first-slice-contract-pack.md`
- `docs/2026-03-28-figma-rust-provider-rewrite-strategy.md`
- `/home/lucien/working/industrial-algebra/Lonis/docs/plans/2026-03-28-lonis-external-provider-protocol-spec-v0.md`
- `/home/lucien/working/industrial-algebra/Lonis/docs/plans/2026-03-28-lonis-provider-interface-spec-v0.md`
- `/home/lucien/working/industrial-algebra/Lonis/docs/plans/2026-03-28-lonis-figma-provider-mapping-draft.md`

Please produce implementation-oriented progress, not a fresh broad redesign.

---

## Final Direction Reminder

The rewrite should preserve the **value** of the old system while changing the **boundary** of the system.

The value is:
- semantics
- safety
- diagnostics
- verified behavior

The boundary should become:
- Lonis provider
- Lonis tool surface
- machine-readable contract
- AI-oriented invocation
