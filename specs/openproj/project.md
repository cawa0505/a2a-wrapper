# Project: A2AWrapper

## Metadata

- id: a2awrapper
- name: A2AWrapper
- status: PoC (greenfield)

## Description

Rust sidecar wrapper that turns unmodified CLI coding agents (PoC target:
**aider**) into A2A v1 protocol nodes. Core dependencies: `a2a-lf` (core types)
and `a2a-client-lf` (client). Differentiator: stdio/UDS custom binding (A2A spec
§12) + PTY process supervision + VT100-grid completion detection.

## Capabilities

- `pty-supervision` — PTY child supervision + VT100 grid view (in progress via
  change `pty-vt100-harness`)
- `a2a-transport` — stdio/UDS framing + A2A server role (planned, not started)
- `adapter-aider` — aider prompt/artifact translation (planned, not started)
- `completion-detection` — VT100 grid state machine + silent timeout (planned,
  not started)
