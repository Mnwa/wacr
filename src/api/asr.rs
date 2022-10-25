use crate::asr::client::SpeechModel;
use crate::asr::processor::{ProcessResponse, WaitForResponse};
use crate::garbage::collector::GarbageCollector;
use crate::webrtc::CloseSession;
use crate::{
    AsrProcessor, SessionConfig, UserAsrProcessorStorage, UserId, UserSessionStorage, VkApi,
};
use actix::Addr;
use actix_web::http::StatusCode;
use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use log::error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vkclient::upload::VkUploader;

#[allow(clippy::too_many_arguments)]
#[post("/asr")]
pub async fn api_text_to_speech(
    req: HttpRequest,
    user_session_storage: web::Data<UserSessionStorage>,
    user_asr_processor_storage: web::Data<UserAsrProcessorStorage>,
    vk_client: web::Data<VkApi>,
    vk_uploader: web::Data<VkUploader>,
    config: web::Data<SessionConfig>,
    garbage_collector: web::Data<Addr<GarbageCollector>>,
    session: web::Json<ProcessAsrRequest>,
) -> impl Responder {
    let user_id = match req.extensions().get::<UserId>() {
        None => {
            return HttpResponse::build(StatusCode::UNAUTHORIZED).json(ProcessAsrError {
                error: "authorization is failed",
            });
        }
        Some(&uid) => uid,
    };

    let asr_processor_storage = user_asr_processor_storage
        .entry(user_id)
        .or_default()
        .clone();

    let session_storage = user_session_storage.entry(user_id).or_default().clone();

    if !asr_processor_storage.contains_key(&session.session_id) {
        match session_storage.get(&session.session_id) {
            Some(s) if s.connected() => {
                let _ = s.send(CloseSession).await;
            }
            None => {
                return HttpResponse::build(StatusCode::NOT_FOUND).json(ProcessAsrError {
                    error: "webrtc session wasn't created",
                });
            }
            _ => {}
        }
    }

    let asr_processor = asr_processor_storage
        .entry(session.session_id)
        .or_insert_with(|| {
            AsrProcessor::new(
                session.session_id,
                user_id,
                vk_client.into_inner(),
                vk_uploader.into_inner(),
                config.dir.clone(),
                garbage_collector.into_inner(),
                session.speech,
            )
        })
        .downgrade();

    let ProcessResponse(rx) = match asr_processor.send(WaitForResponse).await {
        Ok(r) => r,
        Err(e) => {
            error!(target: "api_asr", "error on sending asr request {}", e);
            return HttpResponse::build(StatusCode::SERVICE_UNAVAILABLE).json(ProcessAsrError {
                error: e.to_string(),
            });
        }
    };

    let sr = match rx.await {
        Ok(r) => r,
        Err(e) => {
            error!(target: "api_asr", "error on preparing asr {}", e);
            return HttpResponse::build(StatusCode::SERVICE_UNAVAILABLE).json(ProcessAsrError {
                error: e.to_string(),
            });
        }
    };

    let text = match sr.as_ref() {
        Ok(t) => t.clone(),
        Err(e) => {
            error!(target: "api_asr", "error on processing asr {}", e);
            return HttpResponse::build(StatusCode::SERVICE_UNAVAILABLE).json(ProcessAsrError {
                error: e.to_string(),
            });
        }
    };

    HttpResponse::Ok().json(ProcessAsrResponse { text })
}

#[derive(Deserialize)]
pub struct ProcessAsrRequest {
    session_id: Uuid,
    speech: SpeechModel,
}

#[derive(Serialize)]
pub struct ProcessAsrResponse {
    text: String,
}

#[derive(Serialize)]
pub struct ProcessAsrError<E> {
    error: E,
}
