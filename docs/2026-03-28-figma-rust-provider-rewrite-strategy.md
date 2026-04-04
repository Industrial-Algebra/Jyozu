# Figma Rust Provider Rewrite Strategy

## Status

This document captures the recommended strategy for rewriting the existing Figma MCP bridge/tooling into a **Rust-based provider** suitable for consumption by the future **Lonis** harness.

It is intended both as:
- a design decision record
- a kickoff document for another agent beginning the rewrite process

---

## Decision Summary

The Figma integration should be rewritten in **Rust** as a provider/bridge that preserves the **existing tested tool API surface** from Kai Jyozu as closely as possible.

The rewrite should **not** first target the TypeScript MCP SDK unless a very specific implementation constraint forces that choice.

### Rationale

The most valuable asset in the existing system is not the current MCP server implementation itself, but:
- the tool semantics
- the request/response shapes
- the safety behavior
- the diagnostics model
- the operational behavior already tested and verified

That contract should be preserved while the runtime architecture is modernized and aligned with the emerging Lonis provider model.

---

## Strategic Position

### Old world
Kai Jyozu currently presents a Figma tool surface through an MCP-oriented architecture.

### New world
The future architecture should be:

```text
Lonis harness
  -> figma provider (Rust)
  -> bridge/session layer
  -> Figma plugin
  -> Figma canvas
```

The **AI-facing boundary** should become the provider contract consumed by Lonis, not MCP.

---

## Core Rewrite Principle

> Preserve semantics first, rewrite implementation second, redesign surface only after parity is proven.

This is the safest path because it retains the value of the existing verification effort while avoiding unnecessary design churn during the rewrite.

---

## What Should Be Preserved

The Rust rewrite should preserve as much of the current Figma tool contract as possible, especially for the first migration pass.

### Preserve
- tool names
- tool categories/grouping concepts
- input fields
- output fields
- error categories where practical
- destructive confirmation semantics
- dry-run semantics
- safety-related behavior
- diagnostics concepts (`status`, `doctor`)
- verification distinctions (especially where tools are known verified)

### Do not preserve blindly
- MCP-specific transport assumptions
- client-specific MCP formatting quirks
- TypeScript-specific implementation structure
- server lifecycle choices that only exist because of MCP

---

## Why Rust Is the Right Target

Rust is a strong target for this rewrite because it provides:
- alignment with the future Lonis ecosystem
- a clean external-provider executable shape
- better long-term consistency with the monorepo direction
- better control over typed contracts and runtime boundaries
- a clearer path to a native Lonis-compatible provider

If Figma becomes one of the primary real-world Lonis providers, implementing it in Rust gives it long-term architectural coherence.

---

## Why Not the TypeScript MCP SDK First

A TypeScript MCP SDK rewrite would likely produce a transitional architecture that is already known not to be the end state.

Risks of that path:
- duplicated work
- migration overhead twice instead of once
- semantic drift while retooling transport
- maintenance burden on an intermediate system
- unnecessary re-centering around MCP

Unless a particular bridge/plugin constraint demands it, this should be avoided.

---

## Role of the Existing Kai Jyozu System

The Kai Jyozu reference should be treated as:
- the source of truth for current tool behavior
- the source of truth for current safety behavior
- the source of truth for operational diagnostics expectations
- the parity target for the first Rust rewrite passes

It should not necessarily be treated as the ideal final architecture.

The rewrite should extract what is valuable from it without inheriting unnecessary MCP-specific complexity.

---

## Recommended Target Shape

The Rust Figma provider should eventually expose a Lonis-compatible external provider interface such as:

```bash
figma-provider manifest
figma-provider tools list
figma-provider tools describe figma.get_document
figma-provider call figma.get_document
figma-provider call figma.create_frame
figma-provider status
figma-provider doctor
```

This lets Lonis consume the Figma tool surface uniformly with other providers.

---

## Rewrite Goals

The rewrite should aim for the following:

### 1. Contract preservation
The logical tool surface should remain stable enough that existing tested behavior can be compared directly.

### 2. Parity-first implementation
Early milestones should focus on tool parity, not redesign.

### 3. Safety parity
Current guardrails must not be lost in the rewrite.

### 4. Diagnostics parity
Operational clarity must be preserved.

### 5. Lonis compatibility
The resulting provider must fit the emerging Lonis provider protocol cleanly.

---

## Initial Constraints

The first rewrite phase should assume:
- explicit local invocation
- machine-readable JSON interfaces
- provider-owned bridge/session complexity
- no dependency on MCP as the primary interface
- no premature redesign of the tool surface

---

## Recommended Phased Plan

## Phase 1: Contract freeze and reference extraction

Before implementing the Rust provider, extract and document the current effective contract.

Tasks:
- inventory the currently exposed Kai Jyozu tool surface
- identify which tools are truly verified
- capture representative request/response shapes
- capture destructive-tool semantics
- capture dry-run semantics
- capture `status` and `doctor` behavior expectations
- identify MCP-specific quirks that should **not** be preserved

Deliverable:
- a contract reference pack for parity testing

## Phase 2: Provider skeleton in Rust

Implement the minimal external-provider shape:
- `manifest`
- `tools list`
- `tools describe`
- `status`
- `doctor`
- `call <tool>` plumbing

Do not attempt full tool parity yet.

Deliverable:
- runnable Rust provider executable with empty or partial tool set

## Phase 3: Representative tool slice

Port a small but meaningful subset of tools.

Recommended first slice:
- `figma.get_document`
- `figma.get_page`
- `figma.get_selection`
- `figma.get_node`
- `figma.create_frame`
- `figma.create_text`
- `figma.set_fill`
- `figma.move_node`
- `figma.delete_node`

This slice exercises:
- read-only operations
- safe writes
- destructive writes
- schema handling
- diagnostics and bridge assumptions

## Phase 4: Safety and policy parity

Port and validate:
- confirmation rules
- destructive gating
- protected page logic
- sandbox-only logic if retained
- dry-run support
- audit visibility

This phase is critical; it should not be treated as polish.

## Phase 5: Full surface expansion

Once the provider model is proven with the representative slice, expand toward broader tool parity using the existing compatibility matrix and tests as reference.

---

## What Counts as Success

The rewrite is successful when:
- the Rust provider exposes a Lonis-compatible external provider surface
- the representative tools behave compatibly with the current verified tool contract
- diagnostics and safety semantics are preserved
- the provider can serve as a concrete reference implementation for Lonis provider design

---

## Testing Strategy

Testing should be structured around parity and safety.

### 1. Contract tests
For representative tools:
- same inputs yield equivalent outputs
- equivalent error categories are returned
- required fields remain stable

### 2. Safety tests
Explicitly test:
- confirmation-required behavior
- protected-page blocking behavior
- dry-run behavior
- dangerous-action refusal semantics

### 3. Diagnostics tests
Test:
- `status` shape and meaning
- `doctor` shape and failure reporting
- bridge/session detection behavior

### 4. Provider protocol tests
Test:
- manifest output
- tools list output
- tools describe output
- `call` request/response handling
- stdout/stderr discipline
- exit semantics

---

## Important Architectural Boundary

The Rust Figma provider should own:
- Figma-domain schemas
- bridge mechanics
- plugin/session/channel logic
- Figma-specific diagnostics
- Figma-specific safety enforcement details

Lonis should own:
- unified discovery surface
- normalized invocation surface
- envelope normalization
- registry integration
- global profile/policy handling if added

This separation must remain clear.

---

## Risks to Watch For

### 1. Accidental redesign during rewrite
Temptation to improve awkward tools during porting may undermine parity.

Mitigation:
- preserve first, refine second

### 2. Loss of safety semantics
Rewrites often preserve happy-path behavior while silently weakening guardrails.

Mitigation:
- treat safety parity as core scope, not later cleanup

### 3. MCP-shaped baggage
Some current behaviors may exist only because of MCP transport/client constraints.

Mitigation:
- explicitly identify which quirks are transport artifacts vs real domain contract

### 4. Underestimating diagnostics complexity
Operational clarity is part of the product surface, not just dev tooling.

Mitigation:
- port diagnostics intentionally

---

## Recommended Deliverables for the First Agent

The first agent working on this process should aim to produce:

1. a contract inventory of the current Figma tool surface
2. a parity-focused subset definition
3. a Rust provider executable skeleton
4. a first-pass protocol compatibility test plan
5. a list of MCP-specific behaviors to discard vs preserve

---

## Prompt for Another Agent

Use this prompt to start another agent on the rewrite process:

---

You are helping begin a Rust rewrite of the existing Figma MCP bridge/tool surface into a Lonis-compatible external provider.

Context:
- The current reference implementation lives at:
  `/home/lucien/working/industrial-algebra/Jyozu/reference/kai-jyozu`
- Lonis is pivoting into a strict AI-oriented tool harness.
- The Figma integration should become a Rust external provider consumed by Lonis.
- We want to preserve the **existing tested tool API surface** as closely as practical for the first rewrite phase.
- We do **not** want to center the rewrite around the TypeScript MCP SDK unless absolutely necessary.

Your goals:
1. Analyze the existing Kai Jyozu repo and identify the effective tool contract.
2. Extract a representative first-slice tool set for parity-focused rewrite work.
3. Identify which behaviors are domain contract vs MCP-specific transport artifacts.
4. Propose a Rust provider skeleton implementing these commands:
   - `manifest`
   - `tools list`
   - `tools describe <tool>`
   - `call <tool>`
   - `status`
   - `doctor`
5. Produce a concrete rewrite kickoff note with:
   - proposed crate layout
   - protocol/test approach
   - parity risks
   - recommended first implementation milestone

Important constraints:
- Preserve semantics first, rewrite implementation second.
- Do not redesign the tool surface prematurely.
- Treat safety semantics and diagnostics as first-class scope.
- Be explicit about what should be preserved and what should be discarded.

Please read at minimum:
- `docs/plans/2026-03-28-lonis-external-provider-protocol-spec-v0.md`
- `docs/plans/2026-03-28-lonis-provider-interface-spec-v0.md`
- `docs/plans/2026-03-28-lonis-figma-provider-mapping-draft.md`
- `docs/plans/2026-03-28-figma-rust-provider-rewrite-strategy.md`
- and the relevant design/docs files in the Kai Jyozu reference repo.

Deliver a concrete, implementation-oriented analysis rather than a vague summary.

---

## Recommended Next Step

After the first agent completes the kickoff analysis, the next document should likely be:
- a Figma provider contract pack for the representative first-slice tools
- or a Rust crate/workspace proposal for the provider implementation
