use crate::asr::processor::{ProcessResponse, WaitForResponse};
use crate::webrtc::CloseSession;
use crate::{
    AsrProcessor, SessionConfig, UserAsrProcessorStorage, UserId, UserSessionStorage, VkApi,
};
use actix_web::http::StatusCode;
use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use log::error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[post("/asr")]
pub async fn api_text_to_speech(
    req: HttpRequest,
    user_session_storage: web::Data<UserSessionStorage>,
    user_asr_processor_storage: web::Data<UserAsrProcessorStorage>,
    vk_client: web::Data<VkApi>,
    config: web::Data<SessionConfig>,
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
                return HttpResponse::build(StatusCode::BAD_REQUEST).json(ProcessAsrError {
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
                vk_client.into_inner(),
                config.dir.clone(),
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
}

#[derive(Serialize)]
pub struct ProcessAsrResponse {
    text: String,
}

#[derive(Serialize)]
pub struct ProcessAsrError<E> {
    error: E,
}
