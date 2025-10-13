use httpmock::MockServer;

/// Start a fresh `httpmock::MockServer` instance for use in unit or integration tests.
pub fn start_mock_server() -> MockServer {
    MockServer::start()
}
