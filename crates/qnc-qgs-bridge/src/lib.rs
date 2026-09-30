#![forbid(unsafe_code)]

//! In-process bridge a QNC controller can call.
//!
//! The bridge maps `qnc-qgs-contract` values onto the QGS control surface and
//! projects public snapshots back. It does not decode, present, own QNC UI, or
//! keep a second player state machine.

use qgs_media_runtime::{
    QgsBroadcastPlayerAssembly, QgsBroadcastPlayerStatus, QgsInputPlanQueueRequirements,
    QgsQncCommandKind, QgsQncCommandPayload, QgsQncCommandReplyEnvelope,
    QgsQncCommandRequestEnvelope, QgsQncControlSurface, QgsQncGeneration, QgsQncPreparedInputLike,
    QgsQncRuntimeSnapshot,
};
use qnc_qgs_contract::PreparedInput;

mod actions;
mod host;
mod passive;
pub use host::QncHostPreparedRecord;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorAction {
    TogglePlayPause,
    Step(i64),
    Cue(u64),
}
pub use passive::{
    MonitorView, PassiveAudioLane, PassiveProjection, PlayerClientView, StatusProjection,
    TimelineProjection, WaveProjection,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeSnapshot {
    pub session_id: String,
    pub generation: u64,
    pub status: &'static str,
    pub source_loaded: bool,
    pub public_source_uri: Option<String>,
    pub source_mode: Option<&'static str>,
    pub picture: Option<&'static str>,
    pub authoritative_audio: &'static str,
    pub proxy_aac_authoritative: bool,
    pub active_range: Option<(u64, u64)>,
    pub current_frame: Option<u64>,
    pub current_audio_sample_range: Option<(u64, u64)>,
    pub prepared_frame_count: usize,
    pub video_payload_ready: bool,
    pub audio_payload_ready: bool,
    pub transport_ready: bool,
    pub private_path_exposed: bool,
    pub realtime_verified: bool,
    pub visual_verified: bool,
    pub audio_device_verified: bool,
    pub av_sync_verified: bool,
    pub real_display: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeEvent {
    pub kind: &'static str,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeReply {
    pub accepted: bool,
    pub rejection_reason: Option<&'static str>,
    pub generation: u64,
    pub snapshot: BridgeSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeError {
    Rejected(&'static str),
    PrivatePathExposed,
    MissingProxy,
    Closed,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(reason) => f.write_str(reason),
            Self::PrivatePathExposed => f.write_str("private path exposure rejected"),
            Self::MissingProxy => f.write_str("proxy playback requested but no proxy is available"),
            Self::Closed => f.write_str("bridge session is closed"),
        }
    }
}

impl std::error::Error for BridgeError {}

pub struct Bridge {
    surface: Option<QgsQncControlSurface>,
    session_id: String,
    generation: u64,
    next_command: u64,
    events: Vec<BridgeEvent>,
    source_facts: passive::SourceFacts,
    last_snapshot: Option<BridgeSnapshot>,
    last_rejection: Option<&'static str>,
}

impl Bridge {
    pub fn open_session(
        session_id: impl Into<String>,
        input: PreparedInput,
    ) -> Result<Self, BridgeError> {
        let session_id = session_id.into();
        if exposes_private_path(&session_id) || input.exposes_private_path() {
            return Err(BridgeError::PrivatePathExposed);
        }
        let source_facts = passive::SourceFacts::from_contract(&input);
        let qgs_input = QgsQncPreparedInputLike::from_contract(input)
            .map_err(|error| BridgeError::Rejected(error.label()))?;
        let mapping = qgs_input
            .map_to_qgs_descriptor()
            .map_err(|error| BridgeError::Rejected(error.label()))?;
        if mapping.private_path_exposed {
            return Err(BridgeError::PrivatePathExposed);
        }
        let assembly = QgsBroadcastPlayerAssembly::from_prepared_descriptor(
            &mapping.descriptor,
            QgsInputPlanQueueRequirements::standard(),
        )
        .map_err(|_| BridgeError::Rejected("mapped QGS descriptor did not validate"))?;
        let surface = QgsQncControlSurface::from_assembly(&assembly, session_id.clone())
            .map_err(|_| BridgeError::Rejected("mapped QGS descriptor did not validate"))?;
        let mut bridge = Self {
            surface: Some(surface),
            session_id,
            generation: 0,
            next_command: 1,
            events: Vec::new(),
            source_facts,
            last_snapshot: None,
            last_rejection: None,
        };
        let reply = bridge.command(
            QgsQncCommandKind::LoadPreparedInput,
            QgsQncCommandPayload::PreparedInput { input: qgs_input },
        )?;
        if !reply.accepted {
            return Err(BridgeError::Rejected(
                reply.rejection_reason.unwrap_or("load rejected"),
            ));
        }
        Ok(bridge)
    }

    /// Entry a QNC player controller calls after it has already built
    /// `qnc_player_input::PreparedInput`. The record is the portable extract.
    /// QGS does not import QNC crates or the QNC snapshot blob.
    pub fn open_from_host(
        session_id: impl Into<String>,
        record: QncHostPreparedRecord,
    ) -> Result<Self, BridgeError> {
        let input = record.into_prepared_input()?;
        Self::open_session(session_id, input)
    }

    pub fn prepare(&mut self) -> Result<BridgeReply, BridgeError> {
        self.command(QgsQncCommandKind::Prepare, QgsQncCommandPayload::Empty)
    }

    pub fn cue(&mut self, frame: u64) -> Result<BridgeReply, BridgeError> {
        self.command(
            QgsQncCommandKind::Cue,
            QgsQncCommandPayload::Frame { frame },
        )
    }

    pub fn preroll(&mut self, target_frame: Option<u64>) -> Result<BridgeReply, BridgeError> {
        self.command(
            QgsQncCommandKind::Preroll,
            QgsQncCommandPayload::Preroll { target_frame },
        )
    }

    pub fn play(&mut self) -> Result<BridgeReply, BridgeError> {
        self.command(
            QgsQncCommandKind::Play,
            QgsQncCommandPayload::Play { frame_count: None },
        )
    }

    pub fn play_for(&mut self, frames: u64) -> Result<BridgeReply, BridgeError> {
        self.command(
            QgsQncCommandKind::Play,
            QgsQncCommandPayload::Play {
                frame_count: Some(frames),
            },
        )
    }

    pub fn pause(&mut self) -> Result<BridgeReply, BridgeError> {
        self.command(QgsQncCommandKind::Pause, QgsQncCommandPayload::Empty)
    }

    pub fn seek(&mut self, frame: u64) -> Result<BridgeReply, BridgeError> {
        self.command(
            QgsQncCommandKind::Seek,
            QgsQncCommandPayload::Frame { frame },
        )
    }

    pub fn stop(&mut self) -> Result<BridgeReply, BridgeError> {
        self.command(QgsQncCommandKind::Stop, QgsQncCommandPayload::Empty)
    }

    pub fn unload(&mut self) -> Result<BridgeReply, BridgeError> {
        self.command(QgsQncCommandKind::Unload, QgsQncCommandPayload::Empty)
    }

    pub fn snapshot(&mut self) -> Result<BridgeReply, BridgeError> {
        self.command(QgsQncCommandKind::Snapshot, QgsQncCommandPayload::Empty)
    }

    pub fn close_session(&mut self) -> Result<BridgeReply, BridgeError> {
        let reply = self.unload()?;
        self.surface = None;
        Ok(reply)
    }

    pub fn drain_events(&mut self) -> Vec<BridgeEvent> {
        std::mem::take(&mut self.events)
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Monitor, timeline, wave, and status facts for QNC passive views.
    /// Picture stays empty until the snapshot reports visual verification.
    /// QNC player-client operator action. A missing carrier frame is rejected
    /// here, before the runtime command.
    pub fn apply_action(&mut self, action: OperatorAction) -> Result<BridgeReply, BridgeError> {
        if self.surface.is_none() {
            return Err(BridgeError::Closed);
        }
        let Some(snapshot) = self.last_snapshot.clone() else {
            return self.local_reject("Player has no confirmed position.");
        };
        let confirmed = snapshot.source_loaded
            && snapshot.current_frame.is_some()
            && snapshot
                .active_range
                .is_some_and(|(start, end)| end > start)
            && self.source_facts.frame_rate_numerator > 0
            && self.source_facts.frame_rate_denominator > 0;
        if !confirmed {
            return self.local_reject("Player has no confirmed position.");
        }
        let frame = snapshot.current_frame.unwrap_or(0);
        let (start, end) = snapshot.active_range.unwrap_or((0, 1));
        match action {
            OperatorAction::TogglePlayPause if snapshot.status == "Playing" => self.pause(),
            OperatorAction::TogglePlayPause => self.play(),
            OperatorAction::Step(delta) => {
                self.seek(actions::stepped_frame(frame, delta, start, end))
            }
            OperatorAction::Cue(target) => self.cue(target),
        }
    }

    /// `qnc-player-client::View` predicates. Picture visibility stays off.
    pub fn player_view(&self) -> PlayerClientView {
        let Some(snapshot) = &self.last_snapshot else {
            return PlayerClientView {
                preparing: true,
                video_visible: false,
                error: self.last_rejection,
                playing: false,
                ready: false,
                confirmed_position: false,
                can_start_playback: false,
                frame_interval: None,
            };
        };
        PlayerClientView::from_snapshot(snapshot, &self.source_facts, self.last_rejection)
    }

    pub fn project_passive(&self) -> PassiveProjection {
        let Some(snapshot) = &self.last_snapshot else {
            return PassiveProjection::empty();
        };
        passive::project(
            snapshot,
            &self.source_facts,
            self.last_rejection,
            &self.events,
        )
    }

    fn local_reject(&mut self, reason: &'static str) -> Result<BridgeReply, BridgeError> {
        let snapshot = self
            .last_snapshot
            .clone()
            .ok_or(BridgeError::Rejected(reason))?;
        self.last_rejection = Some(reason);
        Ok(BridgeReply {
            accepted: false,
            rejection_reason: Some(reason),
            generation: self.generation,
            snapshot,
        })
    }

    fn command(
        &mut self,
        kind: QgsQncCommandKind,
        payload: QgsQncCommandPayload,
    ) -> Result<BridgeReply, BridgeError> {
        let command_id = self.next_command;
        self.next_command = self.next_command.saturating_add(1);
        let request = QgsQncCommandRequestEnvelope::new(format!("bridge-{command_id}"), kind)
            .with_expected_generation(QgsQncGeneration(self.generation))
            .with_payload(payload);
        let surface = self.surface.as_mut().ok_or(BridgeError::Closed)?;
        let reply = surface.handle_command(request);
        if reply.exposes_private_path() {
            return Err(BridgeError::PrivatePathExposed);
        }
        if reply.accepted {
            self.generation = reply.generation_after.0;
        }
        let generation = reply.generation_after.0;
        self.events
            .extend(reply.events.iter().map(|event| BridgeEvent {
                kind: event.event_kind.label(),
                generation,
            }));
        let snapshot = project_snapshot(&self.session_id, &reply);
        self.last_snapshot = Some(snapshot.clone());
        self.last_rejection = if reply.accepted {
            None
        } else {
            reply.rejection_reason
        };
        Ok(BridgeReply {
            accepted: reply.accepted,
            rejection_reason: reply.rejection_reason,
            generation: self.generation,
            snapshot,
        })
    }
}

fn project_snapshot(session_id: &str, reply: &QgsQncCommandReplyEnvelope) -> BridgeSnapshot {
    project_runtime_snapshot(session_id, reply.generation_after.0, &reply.snapshot)
}

fn project_runtime_snapshot(
    session_id: &str,
    generation: u64,
    snapshot: &QgsQncRuntimeSnapshot,
) -> BridgeSnapshot {
    BridgeSnapshot {
        session_id: session_id.to_string(),
        generation,
        status: status_label(snapshot.status),
        source_loaded: snapshot.source_loaded,
        public_source_uri: snapshot.public_source_uri.clone(),
        source_mode: snapshot.source_mode.map(|mode| match mode {
            qgs_media_runtime::QgsInputPlanSourceMode::ProxyPreview => "ProxyPreview",
            qgs_media_runtime::QgsInputPlanSourceMode::OriginalMedia => "OriginalMedia",
        }),
        picture: snapshot
            .picture_representation
            .map(|picture| match picture {
                qgs_media_runtime::QgsPlaybackRepresentation::Original => "Original",
                qgs_media_runtime::QgsPlaybackRepresentation::Proxy => "Proxy",
            }),
        authoritative_audio: snapshot.authoritative_audio_source,
        proxy_aac_authoritative: snapshot.proxy_aac_authoritative,
        active_range: snapshot.active_range,
        current_frame: snapshot.current_frame,
        current_audio_sample_range: snapshot.current_audio_sample_range,
        prepared_frame_count: snapshot
            .prepared_window
            .as_ref()
            .map(|window| window.prepared_frame_count)
            .unwrap_or(0),
        video_payload_ready: snapshot.video_payload_ready,
        audio_payload_ready: snapshot.audio_payload_ready,
        transport_ready: snapshot.transport_ready,
        private_path_exposed: snapshot.private_path_exposed,
        realtime_verified: snapshot.device_status.realtime_verified,
        visual_verified: snapshot.device_status.visual_verified,
        audio_device_verified: snapshot.device_status.audio_device_verified,
        av_sync_verified: snapshot.device_status.av_sync_verified,
        real_display: snapshot.device_status.real_display_status,
    }
}

fn status_label(status: QgsBroadcastPlayerStatus) -> &'static str {
    match status {
        QgsBroadcastPlayerStatus::Empty => "Empty",
        QgsBroadcastPlayerStatus::Loaded => "Loaded",
        QgsBroadcastPlayerStatus::Preparing => "Preparing",
        QgsBroadcastPlayerStatus::Ready => "Ready",
        QgsBroadcastPlayerStatus::Playing => "Playing",
        QgsBroadcastPlayerStatus::Paused => "Paused",
        QgsBroadcastPlayerStatus::Stopped => "Stopped",
        QgsBroadcastPlayerStatus::Completed => "Completed",
        QgsBroadcastPlayerStatus::Failed => "Failed",
    }
}

fn exposes_private_path(value: &str) -> bool {
    value.starts_with('/') || value.starts_with("file:") || value.contains("file:")
}

#[cfg(test)]
fn accept(reply: BridgeReply) -> BridgeReply {
    assert!(reply.accepted, "{:?}", reply.rejection_reason);
    reply
}

#[cfg(test)]
fn assert_advanced(before: u64, reply: &BridgeReply) -> u64 {
    assert!(reply.generation > before, "{}", reply.generation);
    assert_eq!(reply.generation, reply.snapshot.generation);
    reply.generation
}

#[cfg(test)]
fn assert_public_original(snapshot: &BridgeSnapshot) {
    assert_eq!(
        snapshot.public_source_uri.as_deref(),
        Some("qnc://fixture/media/original/block-x-original-media")
    );
    assert_eq!(snapshot.source_mode, Some("OriginalMedia"));
    assert_eq!(snapshot.picture, Some("Original"));
    assert_eq!(snapshot.authoritative_audio, "original MXF");
    assert!(!snapshot.proxy_aac_authoritative);
    assert!(!snapshot.private_path_exposed);
    assert!(!snapshot.realtime_verified);
    assert!(!snapshot.visual_verified);
    assert!(!snapshot.audio_device_verified);
    assert!(!snapshot.av_sync_verified);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_projects_contract_fixture_without_private_paths() {
        let mut bridge =
            Bridge::open_session("qnc-bridge-session", PreparedInput::frozen_original_media())
                .expect("open");
        let snapshot = bridge.snapshot().expect("snapshot");
        assert!(snapshot.accepted);
        assert_eq!(snapshot.generation, bridge.generation());
        assert_eq!(
            snapshot.snapshot.public_source_uri.as_deref(),
            Some("qnc://fixture/media/original/block-x-original-media")
        );
        assert_eq!(snapshot.snapshot.source_mode, Some("OriginalMedia"));
        assert_eq!(snapshot.snapshot.picture, Some("Original"));
        assert!(!snapshot.snapshot.proxy_aac_authoritative);
        assert!(!snapshot.snapshot.private_path_exposed);
        assert!(!snapshot.snapshot.realtime_verified);
        assert!(!snapshot.snapshot.visual_verified);
        assert!(!snapshot.snapshot.audio_device_verified);
        assert!(!snapshot.snapshot.av_sync_verified);
        let generation = bridge.generation();
        let again = bridge.snapshot().expect("snapshot");
        assert_eq!(again.generation, generation);
        assert!(bridge
            .drain_events()
            .iter()
            .all(|event| event.kind != "FramePresented"));
    }

    #[test]
    fn play_before_prepare_is_rejected_and_prepare_uses_fixture_range() {
        let mut bridge =
            Bridge::open_session("qnc-bridge-session", PreparedInput::frozen_original_media())
                .expect("open");
        let play = bridge.play().expect("play reply");
        assert!(!play.accepted);
        let generation = bridge.generation();
        let prepare = bridge.prepare().expect("prepare");
        assert!(prepare.accepted, "{:?}", prepare.rejection_reason);
        assert_eq!(prepare.snapshot.active_range, Some((10, 90)));
        assert!(bridge.generation() > generation);
    }

    #[test]
    fn path_shaped_binding_never_opens_a_session() {
        let mut input = PreparedInput::frozen_proxy_preview();
        input.private_binding.original_binding_ref = "/tmp/original.mxf".to_string();
        match Bridge::open_session("qnc-bridge-session", input) {
            Err(error) => assert_eq!(error, BridgeError::PrivatePathExposed),
            Ok(_) => panic!("path-shaped binding opened a session"),
        }
    }

    #[test]
    fn qnc_host_record_opens_original_media_and_keeps_play_from_preparing() {
        let mut bridge =
            Bridge::open_from_host("qnc-controller", host::original_media_host_record())
                .expect("host open");
        let play = bridge.play().expect("play");
        assert!(!play.accepted);
        let prepare = bridge.prepare().expect("prepare");
        assert!(prepare.accepted, "{:?}", prepare.rejection_reason);
        assert_eq!(prepare.snapshot.active_range, Some((10, 90)));
        assert_eq!(prepare.snapshot.source_mode, Some("OriginalMedia"));
        assert_eq!(prepare.snapshot.authoritative_audio, "original MXF");
        let generation = bridge.generation();
        let snapshot = bridge.snapshot().expect("snapshot");
        assert_eq!(snapshot.generation, generation);
        bridge.close_session().expect("close");
        assert!(matches!(bridge.snapshot(), Err(BridgeError::Closed)));
    }

    #[test]
    fn qnc_host_proxy_playback_without_proxy_does_not_open() {
        let mut record = host::original_media_host_record();
        record.playback_input = qnc_qgs_contract::PlaybackInput::Proxy;
        record.proxy_available = false;
        record.proxy_media_uri = None;
        record.proxy_binding_ref = None;
        record.proxy_frame_rate_numerator = None;
        record.proxy_frame_rate_denominator = None;
        record.proxy_duration_frames = None;
        record.proxy_duration = None;
        match Bridge::open_from_host("qnc-controller", record) {
            Err(BridgeError::MissingProxy) => {}
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("proxy playback opened without a proxy"),
        }
    }

    #[test]
    fn command_script_matches_public_snapshot_contract() {
        let mut bridge =
            Bridge::open_from_host("qnc-controller", host::original_media_host_record())
                .expect("host open");
        let loaded = bridge.snapshot().expect("loaded snapshot");
        assert!(loaded.accepted);
        assert_eq!(loaded.snapshot.status, "Loaded");
        assert!(loaded.snapshot.source_loaded);
        assert_public_original(&loaded.snapshot);
        let mut generation = bridge.generation();

        let early_play = bridge.play().expect("play reply");
        assert!(!early_play.accepted);
        assert_eq!(bridge.generation(), generation);
        assert_eq!(early_play.snapshot.status, "Loaded");

        let prepare = accept(bridge.prepare().expect("prepare"));
        generation = assert_advanced(generation, &prepare);
        assert_eq!(prepare.snapshot.status, "Loaded");
        assert_eq!(prepare.snapshot.active_range, Some((10, 90)));
        assert_public_original(&prepare.snapshot);

        let outside = bridge.cue(90).expect("cue reply");
        assert!(!outside.accepted);
        assert_eq!(bridge.generation(), generation);
        assert_eq!(outside.snapshot.current_frame, None);

        let cue = accept(bridge.cue(10).expect("cue"));
        generation = assert_advanced(generation, &cue);
        assert_eq!(cue.snapshot.status, "Ready");
        assert_eq!(cue.snapshot.current_frame, Some(10));
        assert_public_original(&cue.snapshot);

        let preroll = accept(bridge.preroll(Some(10)).expect("preroll"));
        generation = assert_advanced(generation, &preroll);
        assert_eq!(preroll.snapshot.current_frame, Some(10));

        let observer = bridge.snapshot().expect("observer");
        assert!(observer.accepted);
        assert_eq!(observer.generation, generation);
        assert_eq!(bridge.generation(), generation);

        let play = accept(bridge.play().expect("play"));
        generation = assert_advanced(generation, &play);
        assert_eq!(play.snapshot.status, "Playing");
        assert_eq!(play.snapshot.current_frame, Some(10));

        let advanced = accept(bridge.play_for(2).expect("play_for"));
        generation = assert_advanced(generation, &advanced);
        assert_eq!(advanced.snapshot.status, "Playing");
        assert_eq!(advanced.snapshot.current_frame, Some(12));
        assert_public_original(&advanced.snapshot);

        let pause = accept(bridge.pause().expect("pause"));
        generation = assert_advanced(generation, &pause);
        assert_eq!(pause.snapshot.status, "Paused");
        assert_eq!(pause.snapshot.current_frame, Some(12));

        let outside_seek = bridge.seek(90).expect("seek reply");
        assert!(!outside_seek.accepted);
        assert_eq!(bridge.generation(), generation);
        assert_eq!(outside_seek.snapshot.current_frame, Some(12));

        let seek = accept(bridge.seek(20).expect("seek"));
        generation = assert_advanced(generation, &seek);
        assert_eq!(seek.snapshot.current_frame, Some(20));
        assert_public_original(&seek.snapshot);

        let play_after_seek = bridge.play().expect("play after seek");
        assert!(!play_after_seek.accepted);
        assert_eq!(bridge.generation(), generation);

        let rearmed = accept(bridge.preroll(Some(20)).expect("preroll after seek"));
        generation = assert_advanced(generation, &rearmed);
        let resumed = accept(bridge.play().expect("play after preroll"));
        generation = assert_advanced(generation, &resumed);
        assert_eq!(resumed.snapshot.status, "Playing");

        let stop = accept(bridge.stop().expect("stop"));
        generation = assert_advanced(generation, &stop);
        assert_eq!(stop.snapshot.status, "Stopped");
        assert!(stop.snapshot.source_loaded);
        assert_eq!(stop.snapshot.active_range, Some((10, 90)));
        assert_public_original(&stop.snapshot);

        let unload = accept(bridge.unload().expect("unload"));
        assert!(unload.generation > generation);
        assert_eq!(unload.snapshot.status, "Empty");
        assert!(!unload.snapshot.source_loaded);
        assert!(unload.snapshot.public_source_uri.is_none());
        assert!(!unload.snapshot.private_path_exposed);
        assert!(!unload.snapshot.realtime_verified);
        assert!(!unload.snapshot.visual_verified);
        assert!(!unload.snapshot.audio_device_verified);
        assert!(!unload.snapshot.av_sync_verified);

        let kinds: Vec<_> = bridge
            .drain_events()
            .into_iter()
            .map(|event| event.kind)
            .collect();
        for kind in [
            "SourceLoaded",
            "PreparedInputAccepted",
            "Cued",
            "PrerollReady",
            "Started",
            "Ticked",
            "Paused",
            "Seeked",
            "Stopped",
            "Unloaded",
            "CommandRejected",
        ] {
            assert!(kinds.contains(&kind), "missing {kind} in {kinds:?}");
        }
        assert!(!kinds.contains(&"FramePresented"));
    }

    #[test]
    fn passive_projection_follows_the_carrier_and_keeps_picture_empty() {
        let mut bridge =
            Bridge::open_from_host("qnc-controller", host::original_media_host_record())
                .expect("host open");
        let loaded = bridge.project_passive();
        assert!(matches!(
            loaded.monitor,
            MonitorView::Status {
                transport: "Loaded"
            }
        ));
        assert!(loaded.timeline.playhead_frame.is_none());
        assert!(!loaded.timeline.cue_enabled);
        assert!(!loaded.status.play_enabled);
        assert_eq!(loaded.wave.lanes.len(), 4);
        assert_eq!(loaded.wave.lanes[0].label, "A1");
        assert_eq!(loaded.wave.lanes[3].source_track_index, 4);
        assert_eq!(loaded.wave.source_kind, "original");
        assert!(!loaded.wave.peaks_included);
        assert_eq!(loaded.status.picture_label, "original MXF");
        assert_eq!(loaded.status.audio_label, "original MXF");
        assert!(!loaded.status.presented);

        let early = bridge.play().expect("play");
        assert!(!early.accepted);
        let rejected = bridge.project_passive();
        assert_eq!(rejected.status.last_rejection, early.rejection_reason);
        assert!(rejected.timeline.playhead_frame.is_none());

        accept(bridge.prepare().expect("prepare"));
        let prepared = bridge.project_passive();
        assert_eq!(prepared.timeline.range_start_frame, 10);
        assert_eq!(prepared.timeline.duration_frames, 80);
        assert!(prepared.timeline.playhead_frame.is_none());
        assert!(!prepared.timeline.cue_enabled);
        assert!(!prepared.status.play_enabled);

        accept(bridge.cue(10).expect("cue"));
        let cued = bridge.project_passive();
        assert_eq!(cued.timeline.playhead_frame, Some(10));
        assert!(cued.timeline.cue_enabled);
        assert!(cued.status.play_enabled);
        assert_eq!(cued.wave.sample_cursor, Some((19_200, 21_120)));
        assert!(matches!(cued.monitor, MonitorView::Status { .. }));

        accept(bridge.preroll(Some(10)).expect("preroll"));
        let prerolled = bridge.project_passive();
        assert_eq!(prerolled.timeline.playhead_frame, Some(10));
        assert!(prerolled.status.prepared_frame_count > 0);
        assert!(prerolled.status.video_payload_ready);
        assert!(prerolled.status.audio_payload_ready);
        assert!(!prerolled.status.presented);

        accept(bridge.play().expect("play"));
        accept(bridge.play_for(2).expect("ticks"));
        let playing = bridge.project_passive();
        assert_eq!(playing.timeline.playhead_frame, Some(12));
        assert!(playing.status.pause_enabled);
        assert!(!playing.status.play_enabled);
        assert_eq!(playing.wave.sample_cursor, Some((23_040, 24_960)));

        accept(bridge.stop().expect("stop"));
        accept(bridge.unload().expect("unload"));
        let cleared = bridge.project_passive();
        assert!(matches!(cleared.monitor, MonitorView::Empty));
        assert!(cleared.timeline.playhead_frame.is_none());
        assert!(cleared.wave.lanes.is_empty());
        assert!(cleared.status.public_source_uri.is_none());
        assert_eq!(cleared.status.picture_label, "none");
    }

    #[test]
    fn operator_actions_wait_for_a_confirmed_playhead_and_keep_picture_empty() {
        let mut bridge =
            Bridge::open_from_host("qnc-controller", host::original_media_host_record())
                .expect("host open");
        let generation = bridge.generation();
        let blocked = bridge
            .apply_action(OperatorAction::TogglePlayPause)
            .expect("toggle reply");
        assert!(!blocked.accepted);
        assert_eq!(
            blocked.rejection_reason,
            Some("Player has no confirmed position.")
        );
        assert_eq!(bridge.generation(), generation);
        assert!(bridge
            .apply_action(OperatorAction::Step(1))
            .expect("step")
            .rejection_reason
            .is_some());
        assert!(bridge
            .apply_action(OperatorAction::Cue(10))
            .expect("cue action")
            .rejection_reason
            .is_some());
        assert_eq!(bridge.generation(), generation);

        accept(bridge.prepare().expect("prepare"));
        accept(bridge.cue(10).expect("backend cue"));
        let armed = bridge
            .apply_action(OperatorAction::TogglePlayPause)
            .expect("play");
        assert!(armed.accepted, "{:?}", armed.rejection_reason);
        assert_eq!(armed.snapshot.status, "Playing");
        let paused = bridge
            .apply_action(OperatorAction::TogglePlayPause)
            .expect("pause");
        assert!(paused.accepted, "{:?}", paused.rejection_reason);
        assert_eq!(paused.snapshot.status, "Paused");
        assert_eq!(paused.snapshot.current_frame, Some(10));

        let stepped = bridge.apply_action(OperatorAction::Step(1)).expect("step");
        assert!(stepped.accepted, "{:?}", stepped.rejection_reason);
        assert_eq!(stepped.snapshot.current_frame, Some(11));
        let clamped_forward = bridge
            .apply_action(OperatorAction::Step(1_000))
            .expect("step forward");
        assert_eq!(clamped_forward.snapshot.current_frame, Some(89));
        let clamped_back = bridge
            .apply_action(OperatorAction::Step(-1_000))
            .expect("step back");
        assert_eq!(clamped_back.snapshot.current_frame, Some(10));

        let outside = bridge.apply_action(OperatorAction::Cue(90)).expect("cue");
        assert!(!outside.accepted);
        assert_eq!(outside.snapshot.current_frame, Some(10));
        let cued = bridge
            .apply_action(OperatorAction::Cue(20))
            .expect("cue 20");
        assert!(cued.accepted, "{:?}", cued.rejection_reason);
        assert_eq!(cued.snapshot.current_frame, Some(20));

        let view = bridge.project_passive();
        assert!(!view.status.presented);
        assert!(matches!(view.monitor, MonitorView::Status { .. }));
        assert_eq!(view.timeline.playhead_frame, Some(20));
        assert!(!view.status.visual_verified);
    }

    #[test]
    fn player_view_matches_qnc_client_predicates_without_video() {
        let mut bridge =
            Bridge::open_from_host("qnc-controller", host::original_media_host_record())
                .expect("host open");
        let loaded = bridge.player_view();
        assert!(!loaded.preparing);
        assert!(!loaded.video_visible);
        assert!(!loaded.has_confirmed_position());
        assert!(!loaded.ready());
        assert!(!loaded.can_start_playback());
        assert!(loaded.source_frame_interval().is_none());

        accept(bridge.prepare().expect("prepare"));
        assert!(!bridge.player_view().has_confirmed_position());

        accept(bridge.cue(10).expect("cue"));
        let cued = bridge.player_view();
        assert!(cued.has_confirmed_position());
        assert!(cued.ready());
        assert!(cued.can_start_playback());
        assert!(!cued.playing());
        assert!(!cued.video_visible);
        assert_eq!(
            cued.source_frame_interval(),
            Some(std::time::Duration::from_millis(40))
        );

        accept(
            bridge
                .apply_action(OperatorAction::TogglePlayPause)
                .expect("play"),
        );
        let playing = bridge.player_view();
        assert!(playing.playing());
        assert!(!playing.can_start_playback());
        assert!(!playing.video_visible);

        accept(
            bridge
                .apply_action(OperatorAction::TogglePlayPause)
                .expect("pause"),
        );
        accept(bridge.apply_action(OperatorAction::Step(1)).expect("step"));
        let stepped = bridge.player_view();
        assert!(stepped.has_confirmed_position());
        assert!(!stepped.ready());
        assert!(!stepped.can_start_playback());
        assert_eq!(
            stepped.source_frame_interval(),
            Some(std::time::Duration::from_millis(40))
        );

        accept(bridge.unload().expect("unload"));
        let cleared = bridge.player_view();
        assert!(!cleared.has_confirmed_position());
        assert!(cleared.source_frame_interval().is_none());
        assert!(!cleared.video_visible);
    }

    #[test]
    fn qnc_host_path_shaped_binding_does_not_open() {
        let mut record = host::original_media_host_record();
        record.original_binding_ref = "/srv/media/original.mxf".to_string();
        match Bridge::open_from_host("qnc-controller", record) {
            Err(BridgeError::PrivatePathExposed) => {}
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("path-shaped host binding opened a session"),
        }
    }
}
