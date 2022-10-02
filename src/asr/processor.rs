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
    result: Option<Arc<std::io::Result<String>>>,
    senders: Vec<futures::channel::oneshot::Sender<Arc<std::io::Result<String>>>>,
}

impl AsrProcessor {
    pub fn new(id: Uuid, client: Arc<VkApi>, dir: PathBuf) -> Addr<Self> {
        Self::create(|ctx| {
            let processor = Self {
                result: None,
                senders: vec![],
            };
            let addr = ctx.address();

            ctx.spawn(
                async move {
                    let audio_path = get_audio_path(id, dir);
                    let response: std::io::Result<String> = async {
                        let upload_url = client
                            .get_upload_url()
                            .await
                            .map_err(|e| {
                                std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                            })?
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

                    if let Err(e) = addr.send(AcceptResult(response)).await {
                        error!("fail to accept result after processing {}", e)
                    }
                }
                .into_actor(&processor),
            );

            processor
        })
    }
}

impl Actor for AsrProcessor {
    type Context = Context<Self>;
}

impl Handler<WaitForResponse> for AsrProcessor {
    type Result = MessageResult<WaitForResponse>;

    fn handle(&mut self, _: WaitForResponse, _ctx: &mut Self::Context) -> Self::Result {
        let (tx, rx) = futures::channel::oneshot::channel();

        if let Some(r) = self.result.clone() {
            let _ = tx.send(r);
        } else {
            self.senders.push(tx);
        }

        MessageResult(ProcessResponse(rx))
    }
}

impl Handler<AcceptResult> for AsrProcessor {
    type Result = ();

    fn handle(
        &mut self,
        AcceptResult(result): AcceptResult,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let r = Arc::new(result);
        self.result = Some(r.clone());
        let senders = std::mem::take(&mut self.senders);

        for sender in senders {
            let _ = sender.send(r.clone());
        }
    }
}

#[derive(Message)]
#[rtype(result = "ProcessResponse")]
pub struct WaitForResponse;

pub struct ProcessResponse(pub futures::channel::oneshot::Receiver<Arc<std::io::Result<String>>>);

#[derive(Message)]
#[rtype(result = "()")]
struct AcceptResult(std::io::Result<String>);
