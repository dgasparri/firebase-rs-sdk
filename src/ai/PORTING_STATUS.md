## Porting status
- ai 30% `[###       ]`

*(Status updated October 2025 after porting backend selection helpers, `getAI` wiring, the shared error surface, and the GenerativeModel skeleton.)*


## Implemented
- Component registration for the `ai` Firebase namespace with multi-instance support.
- `Backend` configuration enum plus `GoogleAiBackend`/`VertexAiBackend`, matching `packages/ai/src/backend.ts`.
- Instance identifier helpers (`encode_instance_identifier`, `decode_instance_identifier`) to keep parity with JS caching semantics.
- Public async `get_ai` API that mirrors the JavaScript `getAI()` surface, including backend-aware caching and runtime option updates suitable for native and wasm callers.
- `AiService::generate_text` now issues real `generateContent` REST requests (using `reqwest`) and automatically attaches Auth/App Check tokens when available.
- Basic unit coverage for backend differentiation, caching behaviour, and prompt validation.
- Rich error surface (`AiError`, `AiErrorCode`, `CustomErrorData`) aligned with `packages/ai/src/errors.ts`, plus helper tests.
- HTTP request factory (`RequestOptions`, `PreparedRequest`) mirroring `constructRequest` in `packages/ai/src/requests/request.ts`, so clients can build REST calls without leaving Rust.
- `GenerativeModel` skeleton that normalises model names across backends and prepares `generateContent` requests on top of the request factory.

## Still to do
- Implement real REST/streaming backends to call Google AI and Vertex AI endpoints.
- Use the shared error taxonomy with real request/response handling once the REST pipeline lands.
- Complete the `GenerativeModel` API (streaming, chat, token counting) and port `ImagenModel` and `LiveGenerativeModel` classes along with their builders and helpers.
- Add schema builders, mapper utilities, and browser-specific adapters (`chromeAdapter`, WebSocket handler).
- Translate the comprehensive TypeScript test suites (backend, helpers, mappers, service) to Rust.

## Next steps - Detailed completion plan
1. **Streaming groundwork** – Add placeholder traits and data structures for streaming responses (mirroring `websocket.ts`) so later work can focus on transport implementations without refactoring the public API.
2. **Error mapping integration** – Use `CustomErrorData` to map HTTP/provider failures once real network calls are wired up.
3. **Credential attachment** – Surface hooks for App Check and Auth token providers so prepared requests can include the relevant headers.
4. **Generative responses** – Introduce response types and helpers so `generate_content` callers receive structured data rather than raw JSON.
5. **Test migration** – Start translating `packages/ai/src/api.test.ts` and `helpers.test.ts` into Rust unit tests to lock in the new behaviours.
