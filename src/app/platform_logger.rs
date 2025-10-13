use crate::app::types::{PlatformLoggerService, VersionService};
use crate::component::types::ComponentType;
use crate::component::ComponentContainer;

pub struct PlatformLoggerServiceImpl {
    container: ComponentContainer,
}

impl PlatformLoggerServiceImpl {
    /// Creates the platform logger service using the component container from an app.
    pub fn new(container: ComponentContainer) -> Self {
        Self { container }
    }
}

impl PlatformLoggerService for PlatformLoggerServiceImpl {
    fn platform_info_string(&self) -> String {
        let providers = self.container.get_providers();
        let mut entries = Vec::new();
        for provider in providers {
            if provider.component_type() == Some(ComponentType::Version) {
                if let Some(service) = provider.get_immediate::<VersionService>() {
                    entries.push(format!("{}/{}", service.library, service.version));
                }
            }
        }
        entries.join(" ")
    }
}
