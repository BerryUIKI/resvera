# ADR-0002: Local Persistence Strategy & Crash Consistency

## Status
Accepted

## Context
Resvera manages persistent state across application restarts and unexpected shutdowns:
1. **Persistent Job Queue**: Jobs in states (`queued`, `preparing`, `running`, `finalizing`, `succeeded`, `failed`, `cancelled`, `interrupted`) must maintain transactional integrity.
2. **Crash Consistency**: If the application is terminated abruptly (kill signal, OS restart, power loss), interrupted jobs must reliably transition to `interrupted` upon reboot, and queued jobs must not be lost or corrupted.
3. **Component & Model Registries**: Installed model versions, active version pointers, downloaded runtime components, and catalog manifests must be updated atomically to prevent broken partial states.
4. **Settings & History**: User configuration and job history must be queryable and resilient against corruption.

A single file rewriting strategy (e.g., rewriting a monolithic `state.json`) suffers from race conditions, torn writes, and lack of atomic multi-entity transactions.

## Decision
1. **Adopt SQLite in WAL (Write-Ahead Logging) Mode via Rust (`rusqlite` / `sqlx`).**
   - SQLite provides embedded, zero-dependency, crash-safe ACID transactions.
   - WAL mode (`PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;`) ensures high throughput, non-blocking reads while writing, and crash recovery.
2. **Schema & State Transitions:**
   - Explicit tables: `jobs`, `job_events`, `model_packages`, `runtime_components`, `settings`, and `transactions`.
   - On startup, the persistence layer executes a recovery sweep:
     ```sql
     UPDATE jobs
     SET state = 'interrupted', updated_at = CURRENT_TIMESTAMP
     WHERE state IN ('preparing', 'running', 'finalizing');
     ```
3. **Batch & Atomic Operations:**
   - Enqueueing batch jobs is executed in an explicit SQL transaction: either all batch items are committed as `queued` or none are.
   - Model installation transactions are finalized with an atomic DB commit matching the filesystem state.

## Consequences
### Positive
- Strict ACID compliance preventing queue and metadata corruption during power outages or crashes.
- Fast indexed querying for history pagination, status lookups, and state transitions.
- Standardized migration mechanism for future schema versions.

### Negative / Trade-offs
- Slight overhead of SQLite C-dependency / crate compile time compared to pure in-memory structures.
