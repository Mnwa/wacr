use crate::asr::client::{CheckProcessingStatusResponse, SpeechModel};
use crate::asr::upload::Uploader;
use crate::webrtc::get_audio_path;
use crate::VkApi;
use actix::prelude::*;
use log::error;
use std::fs::remove_file;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub struct AsrProcessor {
    client: Arc<VkApi>,
    dir: PathBuf,
}

impl AsrProcessor {
    pub fn new(client: Arc<VkApi>, dir: PathBuf) -> Addr<Self> {
        Self::create(|_| Self { client, dir })
    }
}

impl Actor for AsrProcessor {
    type Context = Context<Self>;
}

impl Handler<ProcessRequest> for AsrProcessor {
    type Result = MessageResult<ProcessRequest>;

    fn handle(
        &mut self,
        ProcessRequest { id }: ProcessRequest,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let (tx, rx) = futures::channel::oneshot::channel();
        let client = self.client.clone();
        let dir = self.dir.clone();

        ctx.spawn(
            async move {
                let audio_path = get_audio_path(id, dir);
                let response: std::io::Result<String> = async {
                    let upload_url = client
                        .get_upload_url()
                        .await
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
                        .upload_url;

                    let uploader_info = Uploader {
                        upload_url,
                        audio_path: audio_path.clone(),
                    }
                    .await?;

                    let process_response = client
                        .process_speech(uploader_info, SpeechModel::Spontaneous)
                        .await
                        .map_err(|e| {
                            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                        })?;

                    loop {
                        let status = client
                            .check_status(process_response.task_id)
                            .await
                            .map_err(|e| {
                                std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                            })?;

                        match status {
                            CheckProcessingStatusResponse::Processing { .. } => {
                                actix_web::rt::time::sleep(Duration::from_secs(1)).await
                            }
                            CheckProcessingStatusResponse::Finished { text, .. } => {
                                return Ok(text)
                            }
                            CheckProcessingStatusResponse::InternalError { .. } => {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "internal error of the VK speech recognition service",
                                ))
                            }
                            CheckProcessingStatusResponse::TranscodingError { .. } => {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "error transcoding audio recording to internal format",
                                ))
                            }
                            CheckProcessingStatusResponse::RecognitionError { .. } => {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "speech recognition error, difficulty in recognition",
                                ))
                            }
                        }
                    }
                }
                .await;

                if let Err(e) = remove_file(audio_path) {
                    error!("fail to drop audio file: {}", e)
                }

                let _ = tx.send(response);
            }
            .into_actor(self),
        );

        MessageResult(ProcessResponse(rx))
    }
}

#[derive(Message)]
#[rtype(result = "ProcessResponse")]
pub struct ProcessRequest {
    pub id: Uuid,
}

pub struct ProcessResponse(pub futures::channel::oneshot::Receiver<std::io::Result<String>>);
