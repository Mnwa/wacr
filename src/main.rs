mod api;
mod asr;
mod webrtc;

use crate::api::asr::api_text_to_speech;
use crate::api::jwt::{generate_vk_jwt_method, jwt_token_guard, JwtConfig, UserId};
use crate::api::session::{api_create_session, api_get_audio, SessionConfig};
use crate::asr::client::VkApi;
use crate::asr::processor::AsrProcessor;
use crate::asr::AsrProcessorStorage;
use crate::webrtc::{create_api, SessionStorage};
use actix_files::Files;
use actix_web::web::scope;
use actix_web::{web, App, HttpServer};
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub type UserSessionStorage = DashMap<UserId, Arc<SessionStorage>>;
pub type UserAsrProcessorStorage = DashMap<UserId, Arc<AsrProcessorStorage>>;

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    /*
    TODO: Привязать сессию к uid
    TODO: Сделать процессинг asr в одном экземпляре на один uuid
    TODO: Ускорить инфу о дисконнекте
     */

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
        expiration: 3600,
    });

    HttpServer::new(move || {
        App::new()
            .app_data(vk_client.clone())
            .app_data(web_rtc_api.clone())
            .app_data(user_session_storage.clone())
            .app_data(user_asr_processor_storage.clone())
            .app_data(config.clone())
            .app_data(jwt_config.clone())
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
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
