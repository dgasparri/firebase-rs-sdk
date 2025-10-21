# Firebase Performance Monitoring Port (Rust)

This directory holds the starting point for the Rust port of Firebase Performance Monitoring. The current
implementation provides a minimal stub so that other modules can resolve the performance component and record
synthetic traces.

## Porting status

- performance 3% `[#         ]`

==As of October 21th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/performance and the ported files in Rust in ./src/performance, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module

Thinking
  I'm estimating that the Rust stub covers about 3-5% of the full JavaScript features because it only handles manual
  trace storing. The JS version includes extensive instrumentation, API services, and tracing capabilities that the Rust
  code doesn't. I'll explain this and suggest logical next steps based on these differences.

Port Estimate

  - About 5 % of the JS Performance SDK has a Rust equivalent today. The Rust crate stops at component registration
  plus an in-memory trace stub (src/performance/api.rs:40) and simple error/constants (src/performance/error.rs:4, src/
  performance/constants.rs:1).
  - The JS package delivers full functionality—PerformanceController setup with IndexedDB validation and transport
  bootstrapping (packages/performance/src/controllers/perf.ts:1), API service wiring, installations integration,
  remote-config driven settings, automatic page-load and network instrumentation (packages/performance/src/resources/
  trace.ts:1, packages/performance/src/resources/network_request.ts), session handling, logging, and upload via
  transport_service. None of these surfaces or backends are present in Rust.
  - Advanced trace abilities (attributes, increment metrics, web vitals, OOB routes, user timing hooks) plus sampling,
  error reporting, and console logging provided through services/* and utils/* modules are absent in the Rust stub,
  leaving only manual timing with TraceHandle::stop.
  - Test coverage mirrors this gap: the JS suite spans controllers, resources, services, and utilities, while Rust has
  just one unit test around trace recording (src/performance/api.rs:169).

1. Next actions: implement a real PerformanceController that wires installations tokens and transport uploads; port
  Trace/NetworkRequest semantics (attributes, metrics, auto instrumentation); add settings/remote-config handling and
  the transport service to move beyond local stubs.

## Current Functionality

- **Component wiring** – `register_performance_component` registers the `performance` component, enabling
  `get_performance` to retrieve a `Performance` instance through the shared component container.
- **Trace handling** – `Performance::new_trace` returns a `TraceHandle` that records duration and custom metrics in
  memory when `stop` is called.
- **Recorded traces** – `Performance::recorded_trace` allows inspection of the last stored trace by name.
- **Errors/constants** – Basic error codes (`performance/invalid-argument`, `performance/internal`) and component name
  constant.
- **Tests** – Unit test covering trace creation, metric recording, and stored trace retrieval.

This stub does not interact with the real Performance Monitoring backend, nor does it capture network traces or auto
instrumentation.

## Work Remaining (vs `packages/performance`)

1. **Backend transport**
   - Implement data collection and upload (custom traces, network requests) conforming to the Performance backend API.
2. **Automatic instrumentation**
   - Port automatic page load, network, and resource timing instrumentation from the JS SDK.
3. **Trace lifecycle**
   - Support attribute recording, measure start/stop APIs, increment metrics, and session handling.
4. **Settings & sampling**
   - Integrate remote config, sampling rates, and data collection enablement toggles.
5. **Environment guards**
   - Mirror browser-specific checks (e.g., `isSupported`) and React Native/node behaviour.
6. **Logging/diagnostics**
   - Port logger utilities to surface diagnostic info and error conditions.
7. **Testing parity**
   - Translate unit/integration tests (traces, network collection, settings) and verify with the emulator/real backend.

Addressing these items will bring the Rust Performance module to parity with the JavaScript SDK and enable production use.
