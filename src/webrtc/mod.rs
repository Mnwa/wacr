use actix::Addr;
use dashmap::DashMap;
use log::info;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use webrtc::api::{APIBuilder, API};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::io::ogg_writer::OggWriter;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};

mod session;
pub use session::{CloseSession, OfferRequest, OfferResponse, Session};

pub type SessionStorage = DashMap<Uuid, Addr<Session>>;

pub fn create_api() -> webrtc::error::Result<API> {
    let mut m = MediaEngine::default();

    m.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RTPCodecType::Audio,
    )?;

    let mut registry = Registry::new();

    registry = register_default_interceptors(registry, &mut m)?;

    Ok(APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build())
}

pub async fn create_session(
    api: &API,
    dir: PathBuf,
    session_storage: Arc<SessionStorage>,
) -> std::io::Result<(Uuid, Addr<Session>)> {
    let uuid = Uuid::new_v4();

    let dir = get_audio_path(uuid, dir);

    let writer = actix_web::web::block(move || {
        let file = File::create(dir)?;

        OggWriter::new(file, 48000, 2)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    })
    .await
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))??;

    let peer = api
        .new_peer_connection(create_config())
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let session = Session::new(writer, Arc::new(peer));

    session_storage.insert(uuid, session.clone());

    info!(target: "webrtc", "created session: {}", uuid);

    Ok((uuid, session))
}

fn create_config() -> RTCConfiguration {
    RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    }
}

pub fn get_audio_path(uuid: Uuid, mut dir: PathBuf) -> PathBuf {
    dir.push(format!("{}.ogg", uuid));
    dir
}
