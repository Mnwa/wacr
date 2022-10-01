use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;
use vkclient::{VkApi as VkApiInner, VkApiError};

#[derive(Clone)]
pub struct VkApi {
    inner: VkApiInner,
}

impl VkApi {
    pub fn new(service_token: String) -> Self {
        Self {
            inner: VkApiInner::from_access_token(service_token),
        }
    }

    pub async fn check_status(
        &self,
        task_id: Uuid,
    ) -> Result<CheckProcessingStatusResponse, VkApiError> {
        self.inner
            .send_request("asr.checkStatus", CheckProcessingStatusRequest { task_id })
            .await
    }

    pub async fn process_speech(
        &self,
        audio: String,
        model: SpeechModel,
    ) -> Result<ProcessAudioResponse, VkApiError> {
        self.inner
            .send_request("asr.process", ProcessAudioRequest { audio, model })
            .await
    }

    pub async fn get_upload_url(&self) -> Result<UploadUrlResponse, VkApiError> {
        self.inner.send_request("asr.getUploadUrl", ()).await
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct CheckProcessingStatusRequest {
    task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "status")]
pub enum CheckProcessingStatusResponse {
    Processing { id: Uuid },
    Finished { id: Uuid, text: String },
    InternalError { id: Uuid },
    TranscodingError { id: Uuid },
    RecognitionError { id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessAudioRequest {
    audio: String,
    model: SpeechModel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechModel {
    Neutral,
    Spontaneous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessAudioResponse {
    pub task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadUrlResponse {
    pub upload_url: Url,
}
