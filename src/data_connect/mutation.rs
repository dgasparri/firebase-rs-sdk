use std::sync::Arc;
use std::time::SystemTime;

use crate::data_connect::error::DataConnectResult;
use crate::data_connect::reference::{DataSource, MutationRef, MutationResult};
use crate::data_connect::transport::DataConnectTransport;

#[derive(Clone)]
pub struct MutationManager {
    transport: Arc<dyn DataConnectTransport>,
}

impl MutationManager {
    pub fn new(transport: Arc<dyn DataConnectTransport>) -> Self {
        Self { transport }
    }

    pub async fn execute_mutation(&self, mutation_ref: MutationRef) -> DataConnectResult<MutationResult> {
        let data = self
            .transport
            .invoke_mutation(mutation_ref.operation_name(), mutation_ref.variables())
            .await?;
        Ok(MutationResult {
            data,
            source: DataSource::Server,
            fetch_time: SystemTime::now(),
            mutation_ref,
        })
    }
}
