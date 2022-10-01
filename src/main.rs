mod api;
mod asr;
mod webrtc;

use crate::api::asr::api_text_to_speech;
use crate::api::jwt::{generate_vk_jwt_method, jwt_token_guard, JwtConfig};
use crate::api::session::{api_create_session, SessionConfig};
use crate::asr::client::VkApi;
use crate::asr::processor::AsrProcessor;
use crate::webrtc::{create_api, SessionStorage};
use actix_files::Files;
use actix_web::web::scope;
use actix_web::{web, App, HttpServer};
use std::path::PathBuf;

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
    let session_storage = web::Data::new(SessionStorage::new());
    let asr_processor = web::Data::new(AsrProcessor::new(
        vk_client.clone().into_inner(),
        audio_path.clone(),
    ));

    let config = web::Data::new(SessionConfig { dir: audio_path });

    let jwt_config = web::Data::new(JwtConfig {
        service_key,
        expiration: 3600,
    });

    HttpServer::new(move || {
        App::new()
            .app_data(vk_client.clone())
            .app_data(web_rtc_api.clone())
            .app_data(session_storage.clone())
            .app_data(config.clone())
            .app_data(jwt_config.clone())
            .app_data(asr_processor.clone())
            .service(
                scope("/session")
                    .guard(jwt_token_guard(jwt_config.service_key.clone()))
                    .service(api_create_session)
                    .service(api_text_to_speech),
            )
            .service(Files::new("/static", "./public").show_files_listing())
            .service(generate_vk_jwt_method)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
