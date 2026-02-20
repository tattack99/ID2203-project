The current repo is an example/demo distributed KV using OmniPaxos, designed for manual / benchmark runs. It is not testable via automated fault-injection harnesses yet and thus requires modifications in several areas.

1) Replace Manual CLI with a Programmatic API

Existing state: Server/client binaries accept commands from stdin / CLI and communicate via custom TCP.

Required changes:

Add an HTTP or TCP JSON API that can be consumed by an external test driver (Jepsen/Maelstrom):

PUT /kv/:key

GET /kv/:key

POST /kv/:key/cas

Standardized response semantics (ack / fail / timeout / unknown)

Expose indeterminate result states:

Timeouts → map to :unknown (Jepsen semantics)

Failures → map to :fail

Ensure the current client binary can optionally drop into this API layer rather than only CLI interaction.

Suggested implementation modules:

New api.rs service (using axum, warp, or hyper)

Tie API endpoints to underlying OmniPaxos request handling

2) Add Consensus-Aware Linearizable Reads

By default, many replicated log KV stores allow stale reads when served locally. Jepsen checks for linearizability require reads to be globally consistent.

You need to modify the KV store logic to enforce linearizable reads:

Options:

Leader-verified reads:
Every read must first confirm current leader and quorum state. Reject or redirect if follower.

Consensus roundtrip per read:
Append a no-op entry or use a ReadIndex mechanism to force the read through consensus.

Quorum read:
Collect latest state from a majority before responding.

In practice:
Modify apply_read() inside the Paxos handler to run a consensus check (e.g., requires leader lease or equivalent mechanism) rather than local state read.

This change is required for Jepsen validation to be meaningful, otherwise linearizability checker will flag stale reads.

3) Timeouts & Partial Failure Handling

The server implementation currently does not have explicit timeouts or failure semantics that Jepsen can observe.

Required changes:

Set explicit RPC timeouts (for both client and node-to-node internal messaging)

Define and propagate timeouts up into the API layer (don’t just crash)

Return consistent error codes so the test harness can identify:

:fail – request failed

:unknown – timed out / indeterminate

Jepsen requires this mapping to accurately track histories.

4) Instrumentation & Logging for Histories

To verify linearizability, Jepsen needs a detailed history:

Log operation start / operation finish with timestamps

Include:

Node that received the request

Operation type (PUT/GET/CAS)

Result (value/unknown/failure)

Format logs in recordable structured events consumable by Knossos

You will need:

Hooks in the API dispatcher

Hooks in internal Paxos apply/commit callbacks

This can be implemented via a central history module that writes structured logs (JSON or CSV).

5) Networking Abstractions Must Be Testable

Jepsen introduces nemeses that partition networks or crash nodes. The repository’s current TCP network code must be adapted to support:

Configurable hooks for simulated network failures

A control channel that allows test harness to:

Partition links

Drop or delay messages

Kill and restart nodes

Easiest approach:

Wrap the communication layer in an interface that Jepsen/Maelstrom can control

Provide built-in support for “link control” (accept/delay/drop per peer)

6) Node Lifecycle & Restart Semantics

Jepsen will crash nodes and restart them.

The server process must support graceful restart (persist storage)

On restart:

Reload Paxos log

Re-establish cluster membership

Still respond to API

Without persistent storage changes (bonus requirement), restarts will lose state.

7) Define a Test Harness Interface

You must choose either:

Jepsen (Clojure)
– requires a Jepsen client that drives your HTTP/TCP API
– generates operations for linearizability testing
– implements Nemesis (partition, crash)

Maelstrom (Rust)
– simpler Rust-native harness for concurrent operation generation and fault injection

Either way, the modified OmniPaxos server needs to implement a client interface accessible to the harness, consistent with the design you choose.

Summary of Required Modifications
Category	Changes Required
API	JSON API for PUT/GET/CAS
Reads	Enforce linearizable reads
Timeouts	Explicit timeout semantics
Logging	Structured history logging
Network	Controlled network abstraction
Restarts	Persistent storage support
Test Harness	Bindings for Jepsen or Maelstrom