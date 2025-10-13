# Firebase AI Port (Rust)

This directory houses the very early Rust port of the experimental Firebase AI client module. The TypeScript version
provides helpers to call hosted AI models (Vertex AI / Google AI Studio). The Rust port currently exposes only a minimal
stub to preserve component structure.

## Current Functionality

- **Component wiring** – `register_ai_component` registers an `ai` component with the shared component system so apps can
  resolve an `AiService` via `get_ai_service`.
- **AI service stub** – `AiService::generate_text` validates prompts and returns a synthetic response based on the prompt
  length, optionally honouring a per-request model name.
- **Caching** – Simple in-process cache keyed by app name so repeated lookups return the same stub instance.
- **Errors/constants** – Basic error types (`ai/invalid-argument`, `ai/internal`) and component name constant.
- **Tests** – Unit tests covering successful generation and input validation.

This is sufficient for other modules to depend on the component interface, but it does not call any real AI model.

## Work Remaining (vs `packages/ai`)

1. **Backend integration**
   - Implement the full REST/websocket transport to Vertex AI / Google AI Studio, including authentication, streaming,
     and request serialization.
2. **Model configuration**
   - Mirror model registry/config loading, default model resolution, and helpers for text, chat, and media models.
3. **Mappers & helpers**
   - Port mapper functions (`googleai-mappers.ts`, `helpers.ts`) for converting between Firebase AI types and provider
     payloads.
4. **Streaming & callbacks**
   - Add streaming response handling, observer callbacks, and abort support for long-running generations.
5. **Error handling & logging**
   - Map provider errors to rich `AiError` codes and port logging utilities.
6. **Browser-specific implementations**
   - Implement browser factory logic (`factory-browser.ts`), WebSocket handling, and environment guards.
7. **Testing parity**
   - Translate the JS tests (API, backend, mappers, service) and add integration tests against emulated/real endpoints.

Completing these steps will evolve the Rust AI module from a stub into a functional client suitable for production AI
workloads.
