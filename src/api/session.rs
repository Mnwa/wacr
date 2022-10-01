use crate::webrtc::{create_session, OfferRequest, OfferResponse, SessionStorage};
use actix_web::http::StatusCode;
use actix_web::{post, web, HttpResponse, Responder};
use log::error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
use webrtc::api::API;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

#[post("/create")]
pub async fn api_create_session(
    api: web::Data<API>,
    session_storage: web::Data<SessionStorage>,
    config: web::Data<SessionConfig>,
    offer_request: web::Json<CreateSessionRequest>,
) -> impl Responder {
    let (session_id, session) = match create_session(
        api.as_ref(),
        config.dir.clone(),
        session_storage.into_inner(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            error!(target: "api_session", "error on creating session {}", e);
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(
                SessionErrorResponse {
                    error: e.to_string(),
                },
            );
        }
    };

    let CreateSessionRequest { offer } = offer_request.into_inner();

    let OfferResponse(receiver) = match session.send(OfferRequest(offer)).await {
        Ok(r) => r,
        Err(e) => {
            error!(target: "api_session", "error on sending offer {}", e);
            return HttpResponse::build(StatusCode::SERVICE_UNAVAILABLE).json(
                SessionErrorResponse {
                    error: e.to_string(),
                },
            );
        }
    };

    let offer = match receiver
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    {
        Ok(Ok(r)) => r,
        Err(e) | Ok(Err(e)) => {
            error!(target: "api_session", "error on accepting offer {}", e);
            return HttpResponse::build(StatusCode::NOT_ACCEPTABLE).json(SessionErrorResponse {
                error: e.to_string(),
            });
        }
    };

    HttpResponse::build(StatusCode::OK).json(SessionCreatedResponse { session_id, offer })
}

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    offer: RTCSessionDescription,
}

#[derive(Serialize)]
pub struct SessionCreatedResponse {
    session_id: Uuid,
    offer: RTCSessionDescription,
}

#[derive(Serialize)]
pub struct SessionErrorResponse<E> {
    error: E,
}

pub struct SessionConfig {
    pub dir: PathBuf,
}
