use crate::webrtc::get_audio_path;
use crate::{UserAsrProcessorStorage, UserId, UserSessionStorage};
use actix::prelude::*;
use log::{error, info};
use std::fs::remove_file;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub struct GarbageCollector {
    user_session_storage: Arc<UserSessionStorage>,
    user_asr_processor_storage: Arc<UserAsrProcessorStorage>,
    dir: PathBuf,
    objects_ttl: u64,
}

impl GarbageCollector {
    pub fn new(
        user_session_storage: Arc<UserSessionStorage>,
        user_asr_processor_storage: Arc<UserAsrProcessorStorage>,
        dir: PathBuf,
        objects_ttl: u64,
    ) -> Addr<Self> {
        Self::create(|_| Self {
            user_session_storage,
            user_asr_processor_storage,
            dir,
            objects_ttl,
        })
    }
}

impl Actor for GarbageCollector {
    type Context = Context<Self>;
}

impl Handler<ClearSession> for GarbageCollector {
    type Result = ();

    fn handle(
        &mut self,
        ClearSession(user_id, session_id): ClearSession,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        ctx.run_later(Duration::from_secs(self.objects_ttl), move |s, _ctx| {
            if let Some(session_storage) = s.user_session_storage.get(&user_id) {
                info!(target: "garbage_collector", "clearing session from storage {} -> {}", user_id.0, session_id);
                session_storage.remove(&session_id);
                if let Err(e) = remove_file(get_audio_path(session_id,s.dir.clone() )) {
                    error!(target: "garbage_collector", "fail to clear audio file {} from filesystem: {}", session_id, e)
                }
            }
        });
    }
}

impl Handler<ClearAsr> for GarbageCollector {
    type Result = ();

    fn handle(
        &mut self,
        ClearAsr(user_id, session_id): ClearAsr,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        ctx.run_later(Duration::from_secs(self.objects_ttl), move |s, _ctx| {
            if let Some(asr_processor_storage) = s.user_asr_processor_storage.get(&user_id) {
                info!(target: "garbage_collector", "clearing asr from storage {} -> {}", user_id.0, session_id);
                asr_processor_storage.remove(&session_id);
            }
        });
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ClearSession(pub UserId, pub Uuid);

#[derive(Message)]
#[rtype(result = "()")]
pub struct ClearAsr(pub UserId, pub Uuid);
