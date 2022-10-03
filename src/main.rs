mod api;
mod asr;
mod garbage;
mod webrtc;

use crate::api::asr::api_text_to_speech;
use crate::api::jwt::{generate_vk_jwt_method, jwt_token_guard, JwtConfig, UserId};
use crate::api::session::{api_create_session, api_get_audio, SessionConfig};
use crate::asr::client::VkApi;
use crate::asr::processor::AsrProcessor;
use crate::asr::AsrProcessorStorage;
use crate::garbage::collector::GarbageCollector;
use crate::webrtc::{create_api, SessionStorage};
use actix_files::Files;
use actix_web::web::scope;
use actix_web::{web, App, HttpServer};
use dashmap::DashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub type UserSessionStorage = DashMap<UserId, Arc<SessionStorage>>;
pub type UserAsrProcessorStorage = DashMap<UserId, Arc<AsrProcessorStorage>>;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let addr = std::env::var("LISTEN_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse::<SocketAddr>()
        .expect("socket address is invalid");

    let jwt_expiration = std::env::var("JWT_EXPIRATION")
        .unwrap_or_else(|_| "3600".to_string())
        .parse()
        .expect("jwt expiration is invalid");

    let garbage_collector_ttl = std::env::var("GARBAGE_COLLECTOR_TTL")
        .unwrap_or_else(|_| "3600".to_string())
        .parse()
        .expect("jwt expiration is invalid");

    let service_token = std::env::var("VK_API_SERVICE_TOKEN").expect("missed env SERVICE_TOKEN");
    let service_key = std::env::var("VK_API_SERVICE_KEY").expect("missed env SERVICE_KEY");
    let audio_path =
        PathBuf::from(std::env::var("AUDIO_PATH").unwrap_or_else(|_| "/tmp".to_string()));

    let vk_client = web::Data::new(VkApi::new(service_token.clone()));
    let web_rtc_api = web::Data::new(create_api().expect("fail to create api instance"));
    let user_session_storage = web::Data::new(UserSessionStorage::new());
    let user_asr_processor_storage = web::Data::new(UserAsrProcessorStorage::new());

    let config = web::Data::new(SessionConfig { dir: audio_path });

    let jwt_config = web::Data::new(JwtConfig {
        service_key,
        expiration: jwt_expiration,
    });

    let garbage_collector = web::Data::new(GarbageCollector::new(
        user_session_storage.clone().into_inner(),
        user_asr_processor_storage.clone().into_inner(),
        config.dir.clone(),
        garbage_collector_ttl,
    ));

    HttpServer::new(move || {
        App::new()
            .app_data(vk_client.clone())
            .app_data(web_rtc_api.clone())
            .app_data(user_session_storage.clone())
            .app_data(user_asr_processor_storage.clone())
            .app_data(config.clone())
            .app_data(jwt_config.clone())
            .app_data(garbage_collector.clone())
            .service(
                scope("/session")
                    .guard(jwt_token_guard(jwt_config.service_key.clone()))
                    .service(api_create_session)
                    .service(api_get_audio)
                    .service(api_text_to_speech),
            )
            .service(Files::new("/static", "./public").show_files_listing())
            .service(generate_vk_jwt_method)
    })
    .bind(addr)?
    .run()
    .await
}
