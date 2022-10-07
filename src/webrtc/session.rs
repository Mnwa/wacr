use crate::garbage::collector::{ClearSession, GarbageCollector};
use crate::UserId;
use actix::prelude::*;
use log::{debug, error, trace, warn};
use std::fs::File;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::media::io::ogg_writer::OggWriter;
use webrtc::media::io::Writer;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp::packet::Packet;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use webrtc::track::track_remote::TrackRemote;

const CHECKS_INTERVAL: Duration = Duration::from_secs(10);

pub struct Session {
    id: Uuid,
    user_id: UserId,
    garbage_collector: Arc<Addr<GarbageCollector>>,
    writer: OggWriter<File>,
    peer_connection: Arc<RTCPeerConnection>,
    startup: Instant,
    update_time: Instant,
    total_timeout: Duration,
    timeout: Duration,
}

impl Session {
    pub fn new(
        id: Uuid,
        user_id: UserId,
        garbage_collector: Arc<Addr<GarbageCollector>>,
        writer: OggWriter<File>,
        peer_connection: Arc<RTCPeerConnection>,
        total_timeout: Duration,
        timeout: Duration,
    ) -> Addr<Self> {
        Self::create(|ctx| {
            let addr = ctx.address();

            let session = Session {
                id,
                user_id,
                garbage_collector,
                writer,
                peer_connection: peer_connection.clone(),
                startup: Instant::now(),
                update_time: Instant::now(),
                total_timeout,
                timeout,
            };

            ctx.spawn(
                async move {
                    if let Err(e) = peer_connection
                        .add_transceiver_from_kind(RTPCodecType::Audio, &[])
                        .await
                    {
                        warn!(target: "session", "add transceiver error: {}", e);
                        addr.do_send(CloseSession);
                        return;
                    }

                    peer_connection
                        .on_track({
                            let addr = addr.clone();
                            Box::new(
                                move |track: Option<Arc<TrackRemote>>,
                                      _receiver: Option<Arc<RTCRtpReceiver>>| {
                                    let addr = addr.clone();
                                    match track {
                                        Some(track) => Box::pin(async move {
                                            if let Err(e) = addr.send(AcceptRemote(track)).await {
                                                warn!(target: "session", "fail to send remote: {}", e)
                                            }
                                        }),
                                        None => Box::pin(async {}),
                                    }
                                },
                            )
                        })
                        .await;

                    peer_connection
                        .on_ice_connection_state_change(Box::new(move |connection_state: RTCIceConnectionState| {
                            debug!(target: "session", "connection state has changed {}", connection_state);

                            let addr = addr.clone();

                            Box::pin(async move {
                                if matches!(connection_state, RTCIceConnectionState::Failed | RTCIceConnectionState::Disconnected) {
                                    if let Err(e) = addr.send(CloseSession).await {
                                        warn!(target: "session", "fail to close session: {}", e)
                                    }
                                }
                            })
                        }))
                        .await;
                }
                    .into_actor(&session),
            );

            session
        })
    }
}

impl Actor for Session {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.add_stream(futures::stream::unfold(
            actix_web::rt::time::interval(CHECKS_INTERVAL),
            |mut interval| async move {
                interval.tick().await;
                Some((TimeoutChecks, interval))
            },
        ));
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        debug!(target: "session", "session closing");
        if let Err(e) = self.writer.close() {
            error!(target: "session", "close ogg writer error: {}", e);
        }

        let pc = self.peer_connection.clone();
        ctx.spawn(
            async move {
                if let Err(e) = pc.close().await {
                    warn!(target: "session", "close peer connection error: {}", e);
                }
            }
            .into_actor(self),
        );

        self.garbage_collector
            .do_send(ClearSession(self.user_id, self.id));

        Running::Stop
    }
}

impl Handler<AcceptRemote> for Session {
    type Result = ();

    fn handle(
        &mut self,
        AcceptRemote(track): AcceptRemote,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        debug!(target: "session", "handle track: {:#?}", track);
        ctx.add_stream(futures::stream::unfold(track, move |track| async {
            match track.read_rtp().await {
                Ok((p, _)) => Some((RtpPacket(p), track)),
                Err(e) => {
                    warn!(target: "session", "read rtp error: {}", e);
                    None
                }
            }
        }));
    }
}

impl StreamHandler<RtpPacket> for Session {
    fn handle(&mut self, RtpPacket(packet): RtpPacket, _ctx: &mut Self::Context) {
        if packet.payload.is_empty() {
            return;
        }

        trace!(target: "session", "process packet {:#?}", packet);
        if let Err(e) = self.writer.write_rtp(&packet) {
            warn!(target: "session", "write rtp error: {}", e);
        }
        self.update_time = Instant::now();
    }
}

impl StreamHandler<TimeoutChecks> for Session {
    fn handle(&mut self, _: TimeoutChecks, ctx: &mut Self::Context) {
        let startup_time_left = self.startup.elapsed();
        let last_update_time_left = self.update_time.elapsed();
        debug!(
            target: "session",
            "checking timeout(startup: {}ms, last update: {}ms)",
            startup_time_left.as_millis(),
            last_update_time_left.as_millis()
        );
        if last_update_time_left > self.timeout || startup_time_left > self.total_timeout {
            ctx.notify(CloseSession)
        }
    }
}

impl Handler<CloseSession> for Session {
    type Result = ();

    fn handle(&mut self, _msg: CloseSession, ctx: &mut Self::Context) -> Self::Result {
        debug!(
            target: "session",
            "stopped from close message, total time: {}ms",
            self.startup.elapsed().as_millis()
        );
        ctx.stop()
    }
}

impl Handler<OfferRequest> for Session {
    type Result = MessageResult<OfferRequest>;

    fn handle(
        &mut self,
        OfferRequest(offer): OfferRequest,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let (tx, rx) = futures::channel::oneshot::channel();
        let peer_connection = self.peer_connection.clone();
        ctx.spawn(
            async move {
                let response: webrtc::error::Result<Option<RTCSessionDescription>> = async {
                    peer_connection.set_remote_description(offer).await?;

                    let answer = peer_connection.create_answer(None).await?;

                    let mut gather_complete = peer_connection.gathering_complete_promise().await;

                    peer_connection.set_local_description(answer).await?;

                    let _ = gather_complete.recv().await;

                    Ok(peer_connection.local_description().await)
                }
                .await;

                let _ = tx.send(
                    response
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                        .and_then(|ld| {
                            ld.ok_or_else(|| {
                                std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "generate local_description failed",
                                )
                            })
                        }),
                );
            }
            .into_actor(self),
        );

        MessageResult(OfferResponse(rx))
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct AcceptRemote(Arc<TrackRemote>);

#[derive(Message)]
#[rtype(result = "()")]
struct RtpPacket(Packet);

#[derive(Message)]
#[rtype(result = "()")]
pub struct CloseSession;

#[derive(Message)]
#[rtype(result = "()")]
struct TimeoutChecks;

#[derive(Message)]
#[rtype(result = "OfferResponse")]
pub struct OfferRequest(pub RTCSessionDescription);

pub struct OfferResponse(
    pub futures::channel::oneshot::Receiver<std::io::Result<RTCSessionDescription>>,
);
