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

## Porting status

- ai 5% `[#         ]`

==As of October 21th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/ai and the ported files in Rust in ./src/ai, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module

Thinking
I’m putting together a clear summary highlighting which AI features were ported to Rust—with file line references like src/ai/api.rs:12 for AiService and src/ai/error.rs:1 for error codes—alongside noting the missing big pieces from the TypeScript side. I'll include a rough coverage estimate around 5% and mention the stub status from the README, plus details like watchers and TOT. This will give the user a concise, bullet-pointed snapshot of the current state with relevant paths and balanced reasoning.

AI Module Coverage

  - Rust currently registers an ai component and exposes a stub AiService::generate_text that just echoes prompt
  length (src/ai/api.rs:12). Supporting pieces are limited to the component constant and a two-code error enum (src/
  ai/constants.rs:1, src/ai/error.rs:1). The module README explicitly labels the port as a minimal placeholder with
  synthetic responses only (src/ai/README.md:1).
  - The JS SDK delivers full functionality: rich API surface (packages/ai/src/api.ts:1), real backends for Vertex/Google
  AI (packages/ai/src/backend.ts:1), streaming/websocket handling (packages/ai/src/websocket.ts:1), generative and
  Imagen model classes (packages/ai/src/models), chat/live session helpers (packages/ai/src/methods), schema builders
  (packages/ai/src/requests), and extensive error taxonomy (packages/ai/src/types & packages/ai/src/errors.ts:1). None
  of these have Rust counterparts.
  - Given the stubbed implementation versus the expansive JS feature set, the Rust port covers only the bare
  scaffolding. Estimated completion stands at roughly 5 % of the Firebase AI SDK surface.

Natural next step: prioritize porting the real backend/client pipeline (model classes, request builders, streaming) so the Rust API can produce genuine inference results.


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
