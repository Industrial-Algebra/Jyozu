# Figma First-Slice Contract Pack

## Status

This document defines a representative first-slice contract pack for the Rust Figma provider rewrite.

Its purpose is to give the rewrite effort a **precise, parity-oriented target** before attempting broad tool-surface migration.

This slice is intentionally small but strategically chosen. It covers:
- read operations
- safe mutation
- destructive mutation
- provider diagnostics
- safety semantics
- envelope behavior

It should be treated as the first concrete implementation target for the Rust provider.

---

## 1. Why This Slice

The initial rewrite should not attempt all Figma tools at once.

Instead, it should prove that the provider model can handle:
- inspection of current document state
- creation of new nodes
- mutation of existing nodes
- destructive operations with guardrails
- provider-level diagnostics

The selected first slice is designed to validate exactly those concerns.

---

## 2. Included Tools

### Read tools
- `figma.get_document`
- `figma.get_page`
- `figma.get_selection`
- `figma.get_node`

### Safe mutation tools
- `figma.create_frame`
- `figma.create_text`
- `figma.set_fill`
- `figma.move_node`

### Destructive tool
- `figma.delete_node`

### Provider diagnostics
- `figma-provider status`
- `figma-provider doctor`

---

## 3. General Contract Rules

These rules apply to all tools in this slice.

### 3.1 Canonical names
Use stable namespaced identifiers:
- `figma.get_document`
- `figma.create_frame`
- etc.

### 3.2 Request envelope
Lonis should invoke tools using the standardized provider request:

```json
{
  "tool": "figma.get_document",
  "schema_version": "1",
  "input": {},
  "context": {
    "request_id": "req_123",
    "profile": "default"
  }
}
```

### 3.3 Response envelope
Providers should return normalized structured envelopes.

Success:

```json
{
  "ok": true,
  "tool": "figma.get_document",
  "provider": "figma",
  "schema_version": "1",
  "result": {},
  "meta": {
    "duration_ms": 0,
    "warnings": []
  }
}
```

Error:

```json
{
  "ok": false,
  "tool": "figma.delete_node",
  "provider": "figma",
  "schema_version": "1",
  "error": {
    "code": "confirmation_required",
    "message": "Destructive action requires confirm=true",
    "details": {}
  },
  "meta": {
    "duration_ms": 0,
    "warnings": []
  }
}
```

### 3.4 Safety metadata
Each tool should declare:
- `destructive`
- `requires_confirmation`
- `supports_dry_run`
- `verification.status`

### 3.5 Session/state handling
If Figma session/channel state is required, it should be explicit in tool input or provider-owned in a documented way.

For v0, this contract pack does **not** force one session model, but it does require that hidden ambient state not be relied upon silently.

---

## 4. Shared Schema Conventions

The exact schema system may evolve, but these conventions should hold.

### Node identifiers
- represent Figma node IDs as strings

### Numeric values
- coordinates and sizes use numbers

### Colors
Use explicit RGBA-style objects where practical:

```json
{
  "r": 1,
  "g": 0,
  "b": 0,
  "a": 1
}
```

### Optional references
Parent/page/session references should be explicit optional fields, not implied by undocumented ambient state.

---

## 5. Tool Contracts

# 5.1 `figma.get_document`

## Purpose
Return document-level metadata and page inventory for the active Figma document/session.

## Tool metadata

```json
{
  "name": "figma.get_document",
  "provider": "figma",
  "schema_version": "1",
  "description": "Get metadata and pages for the active Figma document",
  "capabilities": ["read_external_application"],
  "side_effects": [],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": false,
    "requires_confirmation": false,
    "supports_dry_run": false
  }
}
```

## Input schema

```json
{
  "type": "object",
  "properties": {
    "session_id": { "type": "string" }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["document_id", "name", "pages"],
  "properties": {
    "document_id": { "type": "string" },
    "name": { "type": "string" },
    "pages": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["id", "name"],
        "properties": {
          "id": { "type": "string" },
          "name": { "type": "string" }
        }
      }
    }
  }
}
```

## Example success result

```json
{
  "document_id": "doc_abc",
  "name": "Marketing Site",
  "pages": [
    { "id": "1:1", "name": "Design" },
    { "id": "1:2", "name": "Components" }
  ]
}
```

---

# 5.2 `figma.get_page`

## Purpose
Return metadata for a specific page, or the current page if none is specified.

## Tool metadata

```json
{
  "name": "figma.get_page",
  "provider": "figma",
  "schema_version": "1",
  "description": "Get metadata for a page in the active Figma document",
  "capabilities": ["read_external_application"],
  "side_effects": [],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": false,
    "requires_confirmation": false,
    "supports_dry_run": false
  }
}
```

## Input schema

```json
{
  "type": "object",
  "properties": {
    "session_id": { "type": "string" },
    "page_id": { "type": "string" }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["id", "name"],
  "properties": {
    "id": { "type": "string" },
    "name": { "type": "string" },
    "node_count": { "type": "number" }
  }
}
```

---

# 5.3 `figma.get_selection`

## Purpose
Return the current selection in the active page/session.

## Tool metadata

```json
{
  "name": "figma.get_selection",
  "provider": "figma",
  "schema_version": "1",
  "description": "Get currently selected nodes in the active Figma session",
  "capabilities": ["read_external_application"],
  "side_effects": [],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": false,
    "requires_confirmation": false,
    "supports_dry_run": false
  }
}
```

## Input schema

```json
{
  "type": "object",
  "properties": {
    "session_id": { "type": "string" }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["nodes"],
  "properties": {
    "nodes": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["id", "name", "type"],
        "properties": {
          "id": { "type": "string" },
          "name": { "type": "string" },
          "type": { "type": "string" }
        }
      }
    }
  }
}
```

---

# 5.4 `figma.get_node`

## Purpose
Return detailed information for a specific node.

## Tool metadata

```json
{
  "name": "figma.get_node",
  "provider": "figma",
  "schema_version": "1",
  "description": "Get detailed information for a Figma node by ID",
  "capabilities": ["read_external_application"],
  "side_effects": [],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": false,
    "requires_confirmation": false,
    "supports_dry_run": false
  }
}
```

## Input schema

```json
{
  "type": "object",
  "required": ["node_id"],
  "properties": {
    "session_id": { "type": "string" },
    "node_id": { "type": "string" }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["id", "name", "type"],
  "properties": {
    "id": { "type": "string" },
    "name": { "type": "string" },
    "type": { "type": "string" },
    "parent_id": { "type": "string" },
    "x": { "type": "number" },
    "y": { "type": "number" },
    "width": { "type": "number" },
    "height": { "type": "number" }
  }
}
```

---

# 5.5 `figma.create_frame`

## Purpose
Create a new frame in the active document/page or under a specified parent.

## Tool metadata

```json
{
  "name": "figma.create_frame",
  "provider": "figma",
  "schema_version": "1",
  "description": "Create a Figma frame",
  "capabilities": ["mutates_external_application"],
  "side_effects": ["writes_figma_document"],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": false,
    "requires_confirmation": false,
    "supports_dry_run": false
  }
}
```

## Input schema

```json
{
  "type": "object",
  "required": ["name", "x", "y", "width", "height"],
  "properties": {
    "session_id": { "type": "string" },
    "parent_id": { "type": "string" },
    "name": { "type": "string" },
    "x": { "type": "number" },
    "y": { "type": "number" },
    "width": { "type": "number" },
    "height": { "type": "number" }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["node_id", "name", "type"],
  "properties": {
    "node_id": { "type": "string" },
    "name": { "type": "string" },
    "type": { "type": "string" }
  }
}
```

---

# 5.6 `figma.create_text`

## Purpose
Create a text node in the active document/page or under a specified parent.

## Tool metadata

```json
{
  "name": "figma.create_text",
  "provider": "figma",
  "schema_version": "1",
  "description": "Create a Figma text node",
  "capabilities": ["mutates_external_application"],
  "side_effects": ["writes_figma_document"],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": false,
    "requires_confirmation": false,
    "supports_dry_run": false
  }
}
```

## Input schema

```json
{
  "type": "object",
  "required": ["text", "x", "y"],
  "properties": {
    "session_id": { "type": "string" },
    "parent_id": { "type": "string" },
    "text": { "type": "string" },
    "name": { "type": "string" },
    "x": { "type": "number" },
    "y": { "type": "number" }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["node_id", "type", "text"],
  "properties": {
    "node_id": { "type": "string" },
    "type": { "type": "string" },
    "text": { "type": "string" }
  }
}
```

---

# 5.7 `figma.set_fill`

## Purpose
Set the fill color on a target node.

## Tool metadata

```json
{
  "name": "figma.set_fill",
  "provider": "figma",
  "schema_version": "1",
  "description": "Set the fill color of a Figma node",
  "capabilities": ["mutates_external_application"],
  "side_effects": ["writes_figma_document"],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": false,
    "requires_confirmation": false,
    "supports_dry_run": false
  }
}
```

## Input schema

```json
{
  "type": "object",
  "required": ["node_id", "color"],
  "properties": {
    "session_id": { "type": "string" },
    "node_id": { "type": "string" },
    "color": {
      "type": "object",
      "required": ["r", "g", "b"],
      "properties": {
        "r": { "type": "number" },
        "g": { "type": "number" },
        "b": { "type": "number" },
        "a": { "type": "number" }
      }
    }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["node_id"],
  "properties": {
    "node_id": { "type": "string" },
    "fill_applied": { "type": "boolean" }
  }
}
```

---

# 5.8 `figma.move_node`

## Purpose
Move a node to a new position.

## Tool metadata

```json
{
  "name": "figma.move_node",
  "provider": "figma",
  "schema_version": "1",
  "description": "Move a Figma node to a new position",
  "capabilities": ["mutates_external_application"],
  "side_effects": ["writes_figma_document"],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": false,
    "requires_confirmation": false,
    "supports_dry_run": false
  }
}
```

## Input schema

```json
{
  "type": "object",
  "required": ["node_id", "x", "y"],
  "properties": {
    "session_id": { "type": "string" },
    "node_id": { "type": "string" },
    "x": { "type": "number" },
    "y": { "type": "number" }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["node_id", "x", "y"],
  "properties": {
    "node_id": { "type": "string" },
    "x": { "type": "number" },
    "y": { "type": "number" }
  }
}
```

---

# 5.9 `figma.delete_node`

## Purpose
Delete a node from the document.

## Tool metadata

```json
{
  "name": "figma.delete_node",
  "provider": "figma",
  "schema_version": "1",
  "description": "Delete a node from the active Figma document",
  "capabilities": ["mutates_external_application"],
  "side_effects": ["writes_figma_document", "deletes_user_content"],
  "determinism": "deterministic",
  "cost": "low",
  "verification": { "status": "verified" },
  "safety": {
    "destructive": true,
    "requires_confirmation": true,
    "supports_dry_run": true,
    "policy_tags": ["shared_resource_risk"]
  }
}
```

## Input schema

```json
{
  "type": "object",
  "required": ["node_id", "confirm"],
  "properties": {
    "session_id": { "type": "string" },
    "node_id": { "type": "string" },
    "confirm": { "type": "boolean" },
    "dry_run": { "type": "boolean" }
  },
  "additionalProperties": false
}
```

## Output schema

```json
{
  "type": "object",
  "required": ["node_id"],
  "properties": {
    "node_id": { "type": "string" },
    "deleted": { "type": "boolean" },
    "dry_run": { "type": "boolean" }
  }
}
```

## Required behavior
- if `confirm` is false and destructive execution is required, return structured `confirmation_required`
- if `dry_run` is true, provider may return preview/refusal-safe information without deleting
- policy blocks should return structured `permission_denied`

## Example error

```json
{
  "ok": false,
  "tool": "figma.delete_node",
  "provider": "figma",
  "schema_version": "1",
  "error": {
    "code": "confirmation_required",
    "message": "Destructive action requires confirm=true",
    "details": {
      "field": "confirm"
    }
  },
  "meta": {
    "duration_ms": 2,
    "warnings": []
  }
}
```

---

## 6. Diagnostics Contracts

# 6.1 `figma-provider status`

## Purpose
Return current provider operational state.

## Output schema

```json
{
  "type": "object",
  "required": ["provider", "ok"],
  "properties": {
    "provider": { "type": "string" },
    "ok": { "type": "boolean" },
    "bridge_reachable": { "type": "boolean" },
    "plugin_connected": { "type": "boolean" },
    "active_sessions": { "type": "number" },
    "lock_present": { "type": "boolean" },
    "audit_log_path": { "type": "string" }
  }
}
```

## Example result

```json
{
  "provider": "figma",
  "ok": true,
  "bridge_reachable": true,
  "plugin_connected": true,
  "active_sessions": 1,
  "lock_present": true,
  "audit_log_path": "/tmp/figma-provider-audit.log"
}
```

---

# 6.2 `figma-provider doctor`

## Purpose
Run deeper checks and emit structured diagnostics.

## Output schema

```json
{
  "type": "object",
  "required": ["provider", "ok", "checks"],
  "properties": {
    "provider": { "type": "string" },
    "ok": { "type": "boolean" },
    "checks": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "ok"],
        "properties": {
          "name": { "type": "string" },
          "ok": { "type": "boolean" },
          "message": { "type": "string" }
        }
      }
    }
  }
}
```

## Example result

```json
{
  "provider": "figma",
  "ok": false,
  "checks": [
    {
      "name": "bridge",
      "ok": true,
      "message": "Bridge reachable"
    },
    {
      "name": "plugin_connection",
      "ok": false,
      "message": "No active plugin connection found"
    }
  ]
}
```

---

## 7. Error Expectations for First Slice

The first slice should consistently use these top-level error codes where applicable:
- `invalid_input`
- `not_found`
- `provider_unavailable`
- `confirmation_required`
- `permission_denied`
- `timeout`
- `internal_error`

Examples:
- missing `node_id` -> `invalid_input`
- unknown node -> `not_found`
- bridge down -> `provider_unavailable`
- delete without confirmation -> `confirmation_required`
- protected page block -> `permission_denied`

---

## 8. Parity Expectations

The Rust rewrite does **not** need to clone implementation details from Kai Jyozu.

But this first slice **does** need to preserve parity in:
- tool semantics
- expected fields
- safety behavior
- diagnostic intent
- basic result/error categories

Where the current Kai Jyozu surface contains transport-specific or MCP-specific artifacts, those may be omitted if they are clearly not domain-essential.

---

## 9. Acceptance Criteria for First Slice

This slice is complete when:

1. provider manifest and discovery commands work
2. all tools in this slice have machine-readable contracts
3. all tools in this slice are callable via the external provider protocol
4. `status` and `doctor` return structured results
5. `figma.delete_node` preserves confirmation/dry-run safety behavior
6. a parity-oriented test suite exists for the slice

---

## 10. Recommended Next Step

The next implementation-oriented move should be to convert this contract pack into:

- concrete schema files
- provider protocol fixtures
- contract tests
- or a Rust crate skeleton matching this slice

This document should be used as the target reference for that work.
