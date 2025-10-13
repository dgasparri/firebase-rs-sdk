# Firebase Performance Monitoring Port (Rust)

This directory holds the starting point for the Rust port of Firebase Performance Monitoring. The current
implementation provides a minimal stub so that other modules can resolve the performance component and record
synthetic traces.

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
