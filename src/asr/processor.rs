use crate::asr::client::{CheckProcessingStatusResponse, SpeechModel};
use crate::garbage::collector::{ClearAsr, GarbageCollector};
use crate::webrtc::get_audio_path;
use crate::{UserId, VkApi};
use actix::prelude::*;
use log::error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
use vkclient::upload::{Form, VkUploader};

pub struct AsrProcessor {
    id: Uuid,
    user_id: UserId,
    result: Option<Arc<std::io::Result<String>>>,
    senders: Vec<futures::channel::oneshot::Sender<Arc<std::io::Result<String>>>>,
    garbage_collector: Arc<Addr<GarbageCollector>>,
}

impl AsrProcessor {
    pub fn new(
        id: Uuid,
        user_id: UserId,
        client: Arc<VkApi>,
        uploader: Arc<VkUploader>,
        dir: PathBuf,
        garbage_collector: Arc<Addr<GarbageCollector>>,
        speech_model: SpeechModel,
    ) -> Addr<Self> {
        Self::create(|ctx| {
            let processor = Self {
                id,
                user_id,
                result: None,
                senders: vec![],
                garbage_collector,
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

                        let mut form = Form::default();
                        form.add_file("file", audio_path)?;

                        let uploader_info =
                            uploader.upload(upload_url, form).await.map_err(|e| {
                                std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                            })?;

                        let process_response = client
                            .process_speech(uploader_info, speech_model)
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

        self.garbage_collector
            .do_send(ClearAsr(self.user_id, self.id))
    }
}

#[derive(Message)]
#[rtype(result = "ProcessResponse")]
pub struct WaitForResponse;

pub struct ProcessResponse(pub futures::channel::oneshot::Receiver<Arc<std::io::Result<String>>>);

#[derive(Message)]
#[rtype(result = "()")]
struct AcceptResult(std::io::Result<String>);
