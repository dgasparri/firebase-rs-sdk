use std::collections::HashMap;
use std::sync::Arc;

/// Payload displayed to the user when a notification is shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationPayload {
    pub title: Option<String>,
    pub body: Option<String>,
    pub image: Option<String>,
    pub icon: Option<String>,
}

/// Additional FCM options for a payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FcmOptions {
    pub link: Option<String>,
    pub analytics_label: Option<String>,
}

/// Message data delivered by Firebase Cloud Messaging.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessagePayload {
    pub notification: Option<NotificationPayload>,
    pub data: Option<HashMap<String, String>>,
    pub fcm_options: Option<FcmOptions>,
    pub from: Option<String>,
    pub collapse_key: Option<String>,
    pub message_id: Option<String>,
}

pub type MessageHandler = Arc<dyn Fn(MessagePayload) + Send + Sync + 'static>;

pub type Unsubscribe = Box<dyn FnOnce() + Send + 'static>;
