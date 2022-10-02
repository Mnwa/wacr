use crate::asr::processor::{ProcessRequest, ProcessResponse};
use crate::webrtc::CloseSession;
use crate::{AsrProcessor, UserId, UserSessionStorage};
use actix::Addr;
use actix_web::http::StatusCode;
use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use log::error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[post("/asr")]
pub async fn api_text_to_speech(
    req: HttpRequest,
    user_session_storage: web::Data<UserSessionStorage>,
    asr_processor: web::Data<Addr<AsrProcessor>>,
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

    let session_storage = user_session_storage.entry(user_id).or_default();

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

    let ProcessResponse(rx) = match asr_processor
        .send(ProcessRequest {
            id: session.session_id,
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            session_storage.remove(&session.session_id);
            error!(target: "api_asr", "error on sending asr request {}", e);
            return HttpResponse::build(StatusCode::SERVICE_UNAVAILABLE).json(ProcessAsrError {
                error: e.to_string(),
            });
        }
    };

    let text = match rx
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    {
        Ok(Ok(t)) => t,
        Err(e) | Ok(Err(e)) => {
            session_storage.remove(&session.session_id);
            error!(target: "api_asr", "error on processing asr {}", e);
            return HttpResponse::build(StatusCode::SERVICE_UNAVAILABLE).json(ProcessAsrError {
                error: e.to_string(),
            });
        }
    };

    session_storage.remove(&session.session_id);

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
