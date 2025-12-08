//! Camera AV Stream Management cluster handler for rs-matter integration.
//!
//! This module implements the Matter Camera AV Stream Management cluster (0x0551)
//! by bridging the existing CameraAvStreamMgmtCluster business logic to rs-matter's Handler trait.

use std::sync::Arc;

use parking_lot::RwLock;
use strum::FromRepr;

use rs_matter::dm::{
    Access, Attribute, Cluster, Command, Dataver, Handler, InvokeContext, InvokeReply,
    NonBlockingHandler, Quality, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{TLVTag, TLVWrite};
use rs_matter::{attribute_enum, attributes, command_enum, commands, with};

use super::camera_av_stream_mgmt_cluster::{
    AudioCodec, AudioStream, CameraAvStreamMgmtCluster, StreamUsage, VideoCodec, VideoResolution,
    VideoStream,
};

/// Camera AV Stream Management Cluster ID (Matter spec)
pub const CLUSTER_ID: u32 = 0x0551;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Feature bits for this cluster
pub mod features {
    pub const AUDIO: u32 = 0x0001;
    pub const VIDEO: u32 = 0x0002;
    pub const SNAPSHOT: u32 = 0x0004;
    pub const PRIVACY: u32 = 0x0008;
    pub const SPEAKER: u32 = 0x0010;
    pub const IMAGE_CONTROL: u32 = 0x0020;
    pub const WATERMARK: u32 = 0x0040;
    pub const OSD: u32 = 0x0080;
    pub const LOCAL_STORAGE: u32 = 0x0100;
    pub const HDR: u32 = 0x0200;
    pub const NIGHT_VISION: u32 = 0x0400;
}

/// Attribute IDs for the Camera AV Stream Management cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum CameraAttribute {
    MaxConcurrentVideoEncoders = 0x0000,
    MaxEncodedPixelRate = 0x0001,
    VideoSensorParams = 0x0002,
    NightVisionCapable = 0x0003,
    MinViewport = 0x0004,
    RateDistortionTradeOffPoints = 0x0005,
    MaxContentBufferSize = 0x0006,
    MicrophoneCapabilities = 0x0007,
    SpeakerCapabilities = 0x0008,
    TwoWayTalkSupport = 0x0009,
    SupportedSnapshotParams = 0x000A,
    MaxNetworkBandwidth = 0x000B,
    CurrentFrameRate = 0x000C,
    HDRModeEnabled = 0x000D,
    CurrentVideoCodecs = 0x000E,
    CurrentSnapshotConfig = 0x000F,
    FabricsUsingCamera = 0x0010,
    AllocatedVideoStreams = 0x0011,
    AllocatedAudioStreams = 0x0012,
    AllocatedSnapshotStreams = 0x0013,
    RankedVideoStreamPrioritiesList = 0x0014,
    SoftRecordingPrivacyModeEnabled = 0x0015,
    SoftLivestreamPrivacyModeEnabled = 0x0016,
    HardPrivacyModeOn = 0x0017,
    NightVision = 0x0018,
    NightVisionIllum = 0x0019,
    AWBEnabled = 0x001A,
    AutoShutterSpeedEnabled = 0x001B,
    AutoISOEnabled = 0x001C,
    Viewport = 0x001D,
    SpeakerMuted = 0x001E,
    SpeakerVolumeLevel = 0x001F,
    SpeakerMaxLevel = 0x0020,
    SpeakerMinLevel = 0x0021,
    MicrophoneMuted = 0x0022,
    MicrophoneVolumeLevel = 0x0023,
    MicrophoneMaxLevel = 0x0024,
    MicrophoneMinLevel = 0x0025,
    MicrophoneAGCEnabled = 0x0026,
    ImageRotation = 0x0027,
    ImageFlipHorizontal = 0x0028,
    ImageFlipVertical = 0x0029,
    LocalVideoRecordingEnabled = 0x002A,
    LocalSnapshotRecordingEnabled = 0x002B,
    StatusLightEnabled = 0x002C,
    StatusLightBrightness = 0x002D,
    DepthSensorStatus = 0x002E,
}

attribute_enum!(CameraAttribute);

/// Command IDs for the Camera AV Stream Management cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum CameraCommand {
    AudioStreamAllocate = 0x00,
    AudioStreamDeallocate = 0x01,
    VideoStreamAllocate = 0x02,
    VideoStreamDeallocate = 0x03,
    SnapshotStreamAllocate = 0x04,
    SnapshotStreamDeallocate = 0x05,
    SetStreamPriorities = 0x06,
    CaptureSnapshot = 0x07,
    SetViewport = 0x08,
    SetImageRotation = 0x09,
}

command_enum!(CameraCommand);

/// Response command IDs
pub mod response_commands {
    pub const AUDIO_STREAM_ALLOCATE_RESPONSE: u32 = 0x00;
    pub const VIDEO_STREAM_ALLOCATE_RESPONSE: u32 = 0x02;
    pub const SNAPSHOT_STREAM_ALLOCATE_RESPONSE: u32 = 0x04;
    pub const CAPTURE_SNAPSHOT_RESPONSE: u32 = 0x07;
}

/// Get the feature map from the cluster's features
fn compute_feature_map(cluster: &CameraAvStreamMgmtCluster) -> u32 {
    let f = &cluster.features;
    let mut map = 0u32;
    if f.audio {
        map |= features::AUDIO;
    }
    if f.video {
        map |= features::VIDEO;
    }
    if f.snapshot {
        map |= features::SNAPSHOT;
    }
    if f.privacy {
        map |= features::PRIVACY;
    }
    if f.speaker {
        map |= features::SPEAKER;
    }
    if f.image_control {
        map |= features::IMAGE_CONTROL;
    }
    if f.watermark {
        map |= features::WATERMARK;
    }
    if f.osd {
        map |= features::OSD;
    }
    if f.local_storage {
        map |= features::LOCAL_STORAGE;
    }
    if f.hdr {
        map |= features::HDR;
    }
    if f.night_vision {
        map |= features::NIGHT_VISION;
    }
    map
}

/// Build cluster definition - we use a static definition with all attributes
/// The actual availability is controlled by feature flags at runtime
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: features::VIDEO | features::AUDIO, // Default features
    attributes: attributes!(
        // Core attributes (always present)
        Attribute::new(
            CameraAttribute::MaxConcurrentVideoEncoders as _,
            Access::RV,
            Quality::F
        ),
        Attribute::new(
            CameraAttribute::MaxEncodedPixelRate as _,
            Access::RV,
            Quality::F
        ),
        Attribute::new(
            CameraAttribute::VideoSensorParams as _,
            Access::RV,
            Quality::F
        ),
        Attribute::new(
            CameraAttribute::NightVisionCapable as _,
            Access::RV,
            Quality::F
        ),
        Attribute::new(CameraAttribute::MinViewport as _, Access::RV, Quality::F),
        Attribute::new(
            CameraAttribute::MaxContentBufferSize as _,
            Access::RV,
            Quality::F
        ),
        Attribute::new(
            CameraAttribute::MaxNetworkBandwidth as _,
            Access::RV,
            Quality::F
        ),
        // Dynamic attributes
        Attribute::new(
            CameraAttribute::CurrentFrameRate as _,
            Access::RV,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::AllocatedVideoStreams as _,
            Access::RV,
            Quality::A
        ),
        Attribute::new(
            CameraAttribute::AllocatedAudioStreams as _,
            Access::RV,
            Quality::A
        ),
        Attribute::new(
            CameraAttribute::AllocatedSnapshotStreams as _,
            Access::RV,
            Quality::A
        ),
        // Privacy attributes
        Attribute::new(
            CameraAttribute::SoftRecordingPrivacyModeEnabled as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::SoftLivestreamPrivacyModeEnabled as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::HardPrivacyModeOn as _,
            Access::RV,
            Quality::NONE
        ),
        // Audio attributes
        Attribute::new(
            CameraAttribute::MicrophoneCapabilities as _,
            Access::RV,
            Quality::F
        ),
        Attribute::new(
            CameraAttribute::SpeakerCapabilities as _,
            Access::RV,
            Quality::F
        ),
        Attribute::new(
            CameraAttribute::TwoWayTalkSupport as _,
            Access::RV,
            Quality::F
        ),
        Attribute::new(
            CameraAttribute::SpeakerMuted as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::SpeakerVolumeLevel as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::MicrophoneMuted as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::MicrophoneVolumeLevel as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::MicrophoneAGCEnabled as _,
            Access::RWVM,
            Quality::NONE
        ),
        // Image control attributes
        Attribute::new(
            CameraAttribute::NightVision as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::HDRModeEnabled as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::ImageRotation as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::ImageFlipHorizontal as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::ImageFlipVertical as _,
            Access::RWVM,
            Quality::NONE
        ),
        // Local storage attributes
        Attribute::new(
            CameraAttribute::LocalVideoRecordingEnabled as _,
            Access::RWVM,
            Quality::NONE
        ),
        Attribute::new(
            CameraAttribute::LocalSnapshotRecordingEnabled as _,
            Access::RWVM,
            Quality::NONE
        ),
    ),
    commands: commands!(
        // Audio stream commands
        Command::new(
            CameraCommand::AudioStreamAllocate as _,
            Some(response_commands::AUDIO_STREAM_ALLOCATE_RESPONSE),
            Access::WO
        ),
        Command::new(CameraCommand::AudioStreamDeallocate as _, None, Access::WO),
        // Video stream commands
        Command::new(
            CameraCommand::VideoStreamAllocate as _,
            Some(response_commands::VIDEO_STREAM_ALLOCATE_RESPONSE),
            Access::WO
        ),
        Command::new(CameraCommand::VideoStreamDeallocate as _, None, Access::WO),
        // Snapshot commands
        Command::new(
            CameraCommand::SnapshotStreamAllocate as _,
            Some(response_commands::SNAPSHOT_STREAM_ALLOCATE_RESPONSE),
            Access::WO
        ),
        Command::new(
            CameraCommand::SnapshotStreamDeallocate as _,
            None,
            Access::WO
        ),
        // Other commands
        Command::new(CameraCommand::SetStreamPriorities as _, None, Access::WO),
        Command::new(
            CameraCommand::CaptureSnapshot as _,
            Some(response_commands::CAPTURE_SNAPSHOT_RESPONSE),
            Access::WO
        ),
        Command::new(CameraCommand::SetViewport as _, None, Access::WO),
        Command::new(CameraCommand::SetImageRotation as _, None, Access::WO),
    ),
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Handler that bridges the existing CameraAvStreamMgmtCluster to rs-matter
pub struct CameraAvStreamMgmtHandler {
    dataver: Dataver,
    cluster: Arc<RwLock<CameraAvStreamMgmtCluster>>,
}

impl CameraAvStreamMgmtHandler {
    /// The cluster definition for this handler
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler
    pub fn new(dataver: Dataver, cluster: Arc<RwLock<CameraAvStreamMgmtCluster>>) -> Self {
        Self { dataver, cluster }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();

        // Get the dataver-aware writer
        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(()); // No update needed (dataver match)
        };

        // Handle global attributes via the cluster definition
        if attr.is_system() {
            // For feature_map, we need to compute it dynamically
            if attr.attr_id == 0xFFFC {
                // FeatureMap
                let cluster = self.cluster.read();
                let feature_map = compute_feature_map(&cluster);
                return writer.set(feature_map);
            }
            return CLUSTER.read(attr, writer);
        }

        // Get cluster state
        let cluster = self.cluster.read();
        let attrs = &cluster.attributes;

        match attr.attr_id.try_into()? {
            CameraAttribute::MaxConcurrentVideoEncoders => {
                writer.set(attrs.max_concurrent_encoders)
            }
            CameraAttribute::MaxEncodedPixelRate => writer.set(attrs.max_encoded_pixel_rate),
            CameraAttribute::VideoSensorParams => {
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u16(&TLVTag::Context(0), attrs.video_sensor_params.sensor_width)?;
                    tw.u16(&TLVTag::Context(1), attrs.video_sensor_params.sensor_height)?;
                    tw.u16(&TLVTag::Context(2), attrs.video_sensor_params.max_fps)?;
                    if let Some(hdr_fps) = attrs.video_sensor_params.max_hdr_fps {
                        tw.u16(&TLVTag::Context(3), hdr_fps)?;
                    }
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::NightVisionCapable => writer.set(cluster.features.night_vision),
            CameraAttribute::MinViewport => {
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u16(&TLVTag::Context(0), attrs.min_viewport_resolution.width)?;
                    tw.u16(&TLVTag::Context(1), attrs.min_viewport_resolution.height)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::RateDistortionTradeOffPoints => {
                // Return empty array - not implemented
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_array(tag)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::MaxContentBufferSize => writer.set(attrs.max_content_buffer_size),
            CameraAttribute::MicrophoneCapabilities => {
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    if let Some(ref cap) = attrs.microphone_capabilities {
                        tw.start_struct(tag)?;
                        tw.u8(&TLVTag::Context(0), cap.max_number_of_channels)?;
                        // Supported codecs array
                        tw.start_array(&TLVTag::Context(1))?;
                        for codec in &cap.supported_codecs {
                            tw.u8(&TLVTag::Anonymous, *codec as u8)?;
                        }
                        tw.end_container()?;
                        // Supported sample rates
                        tw.start_array(&TLVTag::Context(2))?;
                        for rate in &cap.supported_sample_rates {
                            tw.u32(&TLVTag::Anonymous, *rate)?;
                        }
                        tw.end_container()?;
                        // Supported bit depths
                        tw.start_array(&TLVTag::Context(3))?;
                        for depth in &cap.supported_bit_depths {
                            tw.u8(&TLVTag::Anonymous, *depth)?;
                        }
                        tw.end_container()?;
                        tw.end_container()?;
                    } else {
                        tw.null(tag)?;
                    }
                }
                writer.complete()
            }
            CameraAttribute::SpeakerCapabilities => {
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    if let Some(ref cap) = attrs.speaker_capabilities {
                        tw.start_struct(tag)?;
                        tw.u8(&TLVTag::Context(0), cap.max_number_of_channels)?;
                        tw.start_array(&TLVTag::Context(1))?;
                        for codec in &cap.supported_codecs {
                            tw.u8(&TLVTag::Anonymous, *codec as u8)?;
                        }
                        tw.end_container()?;
                        tw.start_array(&TLVTag::Context(2))?;
                        for rate in &cap.supported_sample_rates {
                            tw.u32(&TLVTag::Anonymous, *rate)?;
                        }
                        tw.end_container()?;
                        tw.start_array(&TLVTag::Context(3))?;
                        for depth in &cap.supported_bit_depths {
                            tw.u8(&TLVTag::Anonymous, *depth)?;
                        }
                        tw.end_container()?;
                        tw.end_container()?;
                    } else {
                        tw.null(tag)?;
                    }
                }
                writer.complete()
            }
            CameraAttribute::TwoWayTalkSupport => writer.set(attrs.two_way_talk_support as u8),
            CameraAttribute::SupportedSnapshotParams => {
                // Return empty array - not fully implemented
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_array(tag)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::MaxNetworkBandwidth => writer.set(attrs.max_network_bandwidth),
            CameraAttribute::CurrentFrameRate => writer.set(attrs.current_frame_rate),
            CameraAttribute::HDRModeEnabled => writer.set(attrs.hdr_mode_enabled),
            CameraAttribute::CurrentVideoCodecs => {
                // Return empty array for now
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_array(tag)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::CurrentSnapshotConfig => {
                // Return null - not configured
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.null(tag)?;
                }
                writer.complete()
            }
            CameraAttribute::FabricsUsingCamera => {
                // Return empty array
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_array(tag)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::AllocatedVideoStreams => {
                let streams = cluster.get_allocated_video_streams();
                let list_index = attr.list_index.clone().map(|li| li.into_option());
                let tag = writer.tag();

                {
                    let mut tw = writer.writer();

                    if list_index.is_none() {
                        tw.start_array(tag)?;
                    }

                    if let Some(Some(index)) = list_index.as_ref() {
                        let stream = streams
                            .get(*index as usize)
                            .ok_or(ErrorCode::ConstraintError)?;
                        Self::write_video_stream(&mut tw, stream)?;
                    } else {
                        for stream in streams {
                            Self::write_video_stream(&mut tw, stream)?;
                        }
                    }

                    if list_index.is_none() {
                        tw.end_container()?;
                    }
                }
                writer.complete()
            }
            CameraAttribute::AllocatedAudioStreams => {
                let streams = cluster.get_allocated_audio_streams();
                let list_index = attr.list_index.clone().map(|li| li.into_option());
                let tag = writer.tag();

                {
                    let mut tw = writer.writer();

                    if list_index.is_none() {
                        tw.start_array(tag)?;
                    }

                    if let Some(Some(index)) = list_index.as_ref() {
                        let stream = streams
                            .get(*index as usize)
                            .ok_or(ErrorCode::ConstraintError)?;
                        Self::write_audio_stream(&mut tw, stream)?;
                    } else {
                        for stream in streams {
                            Self::write_audio_stream(&mut tw, stream)?;
                        }
                    }

                    if list_index.is_none() {
                        tw.end_container()?;
                    }
                }
                writer.complete()
            }
            CameraAttribute::AllocatedSnapshotStreams => {
                // Return empty array - snapshots not implemented
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_array(tag)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::RankedVideoStreamPrioritiesList => {
                // Return empty array
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_array(tag)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::SoftRecordingPrivacyModeEnabled => {
                writer.set(attrs.soft_recording_privacy_mode_enabled)
            }
            CameraAttribute::SoftLivestreamPrivacyModeEnabled => {
                writer.set(attrs.soft_livestream_privacy_mode_enabled)
            }
            CameraAttribute::HardPrivacyModeOn => writer.set(attrs.hard_privacy_mode_on),
            CameraAttribute::NightVision => writer.set(attrs.night_vision as u8),
            CameraAttribute::NightVisionIllum => {
                // Not implemented - return 0
                writer.set(0u8)
            }
            CameraAttribute::AWBEnabled => {
                // Not implemented - return true
                writer.set(true)
            }
            CameraAttribute::AutoShutterSpeedEnabled => {
                // Not implemented - return true
                writer.set(true)
            }
            CameraAttribute::AutoISOEnabled => {
                // Not implemented - return true
                writer.set(true)
            }
            CameraAttribute::Viewport => {
                // Return min viewport as current viewport
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u16(&TLVTag::Context(0), 0)?; // x1
                    tw.u16(&TLVTag::Context(1), 0)?; // y1
                    tw.u16(&TLVTag::Context(2), attrs.video_sensor_params.sensor_width)?; // x2
                    tw.u16(&TLVTag::Context(3), attrs.video_sensor_params.sensor_height)?; // y2
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraAttribute::SpeakerMuted => writer.set(attrs.speaker_muted),
            CameraAttribute::SpeakerVolumeLevel => writer.set(attrs.speaker_volume_level),
            CameraAttribute::SpeakerMaxLevel => writer.set(255u8),
            CameraAttribute::SpeakerMinLevel => writer.set(0u8),
            CameraAttribute::MicrophoneMuted => writer.set(attrs.microphone_muted),
            CameraAttribute::MicrophoneVolumeLevel => writer.set(attrs.microphone_volume_level),
            CameraAttribute::MicrophoneMaxLevel => writer.set(255u8),
            CameraAttribute::MicrophoneMinLevel => writer.set(0u8),
            CameraAttribute::MicrophoneAGCEnabled => writer.set(attrs.microphone_agc_enabled),
            CameraAttribute::ImageRotation => writer.set(attrs.image_rotation),
            CameraAttribute::ImageFlipHorizontal => writer.set(attrs.image_flip_horizontal),
            CameraAttribute::ImageFlipVertical => writer.set(attrs.image_flip_vertical),
            CameraAttribute::LocalVideoRecordingEnabled => {
                writer.set(attrs.local_video_recording_enabled)
            }
            CameraAttribute::LocalSnapshotRecordingEnabled => {
                writer.set(attrs.local_snapshot_recording_enabled)
            }
            CameraAttribute::StatusLightEnabled => {
                // Not implemented
                writer.set(false)
            }
            CameraAttribute::StatusLightBrightness => {
                // Not implemented
                writer.set(0u8)
            }
            CameraAttribute::DepthSensorStatus => {
                // Not implemented - return 0 (disabled)
                writer.set(0u8)
            }
        }
    }

    fn write_video_stream(tw: &mut impl TLVWrite, stream: &VideoStream) -> Result<(), Error> {
        tw.start_struct(&TLVTag::Anonymous)?;
        tw.u16(&TLVTag::Context(0), stream.video_stream_id)?;
        tw.u8(&TLVTag::Context(1), stream.stream_usage as u8)?;
        tw.u8(&TLVTag::Context(2), stream.video_codec as u8)?;
        tw.u16(&TLVTag::Context(3), stream.min_frame_rate)?;
        tw.u16(&TLVTag::Context(4), stream.max_frame_rate)?;
        // Min resolution
        tw.start_struct(&TLVTag::Context(5))?;
        tw.u16(&TLVTag::Context(0), stream.min_resolution.width)?;
        tw.u16(&TLVTag::Context(1), stream.min_resolution.height)?;
        tw.end_container()?;
        // Max resolution
        tw.start_struct(&TLVTag::Context(6))?;
        tw.u16(&TLVTag::Context(0), stream.max_resolution.width)?;
        tw.u16(&TLVTag::Context(1), stream.max_resolution.height)?;
        tw.end_container()?;
        tw.u32(&TLVTag::Context(7), stream.min_bit_rate)?;
        tw.u32(&TLVTag::Context(8), stream.max_bit_rate)?;
        tw.u16(&TLVTag::Context(9), stream.key_frame_interval)?;
        tw.u8(&TLVTag::Context(10), stream.reference_count)?;
        tw.end_container()?;
        Ok(())
    }

    fn write_audio_stream(tw: &mut impl TLVWrite, stream: &AudioStream) -> Result<(), Error> {
        tw.start_struct(&TLVTag::Anonymous)?;
        tw.u16(&TLVTag::Context(0), stream.audio_stream_id)?;
        tw.u8(&TLVTag::Context(1), stream.stream_usage as u8)?;
        tw.u8(&TLVTag::Context(2), stream.audio_codec as u8)?;
        tw.u8(&TLVTag::Context(3), stream.channel_count)?;
        tw.u32(&TLVTag::Context(4), stream.sample_rate)?;
        tw.u32(&TLVTag::Context(5), stream.bit_rate)?;
        tw.u8(&TLVTag::Context(6), stream.bit_depth)?;
        tw.u8(&TLVTag::Context(7), stream.reference_count)?;
        tw.end_container()?;
        Ok(())
    }

    fn write_impl(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let attr = ctx.attr();
        let data = ctx.data();

        // Verify dataver
        attr.check_dataver(self.dataver.get())?;

        let mut cluster = self.cluster.write();
        let attrs = &mut cluster.attributes;

        match attr.attr_id.try_into()? {
            // Read-only attributes
            CameraAttribute::MaxConcurrentVideoEncoders
            | CameraAttribute::MaxEncodedPixelRate
            | CameraAttribute::VideoSensorParams
            | CameraAttribute::NightVisionCapable
            | CameraAttribute::MinViewport
            | CameraAttribute::RateDistortionTradeOffPoints
            | CameraAttribute::MaxContentBufferSize
            | CameraAttribute::MicrophoneCapabilities
            | CameraAttribute::SpeakerCapabilities
            | CameraAttribute::TwoWayTalkSupport
            | CameraAttribute::SupportedSnapshotParams
            | CameraAttribute::MaxNetworkBandwidth
            | CameraAttribute::CurrentFrameRate
            | CameraAttribute::CurrentVideoCodecs
            | CameraAttribute::CurrentSnapshotConfig
            | CameraAttribute::FabricsUsingCamera
            | CameraAttribute::AllocatedVideoStreams
            | CameraAttribute::AllocatedAudioStreams
            | CameraAttribute::AllocatedSnapshotStreams
            | CameraAttribute::RankedVideoStreamPrioritiesList
            | CameraAttribute::HardPrivacyModeOn
            | CameraAttribute::SpeakerMaxLevel
            | CameraAttribute::SpeakerMinLevel
            | CameraAttribute::MicrophoneMaxLevel
            | CameraAttribute::MicrophoneMinLevel
            | CameraAttribute::DepthSensorStatus => Err(ErrorCode::UnsupportedAccess.into()),

            // Writable attributes
            CameraAttribute::HDRModeEnabled => {
                attrs.hdr_mode_enabled = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::SoftRecordingPrivacyModeEnabled => {
                attrs.soft_recording_privacy_mode_enabled = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::SoftLivestreamPrivacyModeEnabled => {
                attrs.soft_livestream_privacy_mode_enabled = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::NightVision => {
                let val = data.u8()?;
                attrs.night_vision = match val {
                    0 => super::camera_av_stream_mgmt_cluster::TriStateAuto::Off,
                    1 => super::camera_av_stream_mgmt_cluster::TriStateAuto::On,
                    _ => super::camera_av_stream_mgmt_cluster::TriStateAuto::Auto,
                };
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::NightVisionIllum
            | CameraAttribute::AWBEnabled
            | CameraAttribute::AutoShutterSpeedEnabled
            | CameraAttribute::AutoISOEnabled
            | CameraAttribute::Viewport => {
                // Not fully implemented - accept but ignore
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::SpeakerMuted => {
                attrs.speaker_muted = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::SpeakerVolumeLevel => {
                attrs.speaker_volume_level = data.u8()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::MicrophoneMuted => {
                attrs.microphone_muted = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::MicrophoneVolumeLevel => {
                attrs.microphone_volume_level = data.u8()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::MicrophoneAGCEnabled => {
                attrs.microphone_agc_enabled = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::ImageRotation => {
                attrs.image_rotation = data.u16()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::ImageFlipHorizontal => {
                attrs.image_flip_horizontal = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::ImageFlipVertical => {
                attrs.image_flip_vertical = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::LocalVideoRecordingEnabled => {
                attrs.local_video_recording_enabled = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::LocalSnapshotRecordingEnabled => {
                attrs.local_snapshot_recording_enabled = data.bool()?;
                self.dataver.changed();
                Ok(())
            }
            CameraAttribute::StatusLightEnabled | CameraAttribute::StatusLightBrightness => {
                // Not implemented - accept but ignore
                self.dataver.changed();
                Ok(())
            }
        }
    }

    fn invoke_impl(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        let cmd = ctx.cmd();
        let data = ctx.data();

        match cmd.cmd_id.try_into()? {
            CameraCommand::VideoStreamAllocate => {
                // Parse request fields from TLV struct
                let mut seq = data.structure()?;
                let stream_usage =
                    StreamUsage::from_repr(seq.scan_ctx(0)?.u8()?).unwrap_or(StreamUsage::LiveView);
                let video_codec =
                    VideoCodec::from_repr(seq.scan_ctx(1)?.u8()?).unwrap_or(VideoCodec::H264);
                let min_frame_rate = seq.scan_ctx(2)?.u16()?;
                let max_frame_rate = seq.scan_ctx(3)?.u16()?;
                let min_res_elem = seq.scan_ctx(4)?;
                let mut min_res_seq = min_res_elem.structure()?;
                let min_resolution = VideoResolution::new(
                    min_res_seq.scan_ctx(0)?.u16()?,
                    min_res_seq.scan_ctx(1)?.u16()?,
                );
                let max_res_elem = seq.scan_ctx(5)?;
                let mut max_res_seq = max_res_elem.structure()?;
                let max_resolution = VideoResolution::new(
                    max_res_seq.scan_ctx(0)?.u16()?,
                    max_res_seq.scan_ctx(1)?.u16()?,
                );
                let min_bit_rate = seq.scan_ctx(6)?.u32()?;
                let max_bit_rate = seq.scan_ctx(7)?.u32()?;

                let mut cluster = self.cluster.write();
                let stream_id = cluster
                    .video_stream_allocate(
                        stream_usage,
                        video_codec,
                        min_frame_rate,
                        max_frame_rate,
                        min_resolution,
                        max_resolution,
                        min_bit_rate,
                        max_bit_rate,
                    )
                    .map_err(|e| {
                        log::warn!("VideoStreamAllocate failed: {}", e);
                        Error::new(ErrorCode::ResourceExhausted)
                    })?;

                self.dataver.changed();

                // Send response
                let mut writer =
                    reply.with_command(response_commands::VIDEO_STREAM_ALLOCATE_RESPONSE)?;
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u16(&TLVTag::Context(0), stream_id)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraCommand::VideoStreamDeallocate => {
                let mut seq = data.structure()?;
                let video_stream_id = seq.scan_ctx(0)?.u16()?;

                let mut cluster = self.cluster.write();
                cluster
                    .video_stream_deallocate(video_stream_id)
                    .map_err(|e| {
                        log::warn!("VideoStreamDeallocate failed: {}", e);
                        Error::new(ErrorCode::NotFound)
                    })?;

                self.dataver.changed();
                Ok(())
            }
            CameraCommand::AudioStreamAllocate => {
                let mut seq = data.structure()?;
                let stream_usage =
                    StreamUsage::from_repr(seq.scan_ctx(0)?.u8()?).unwrap_or(StreamUsage::LiveView);
                let audio_codec =
                    AudioCodec::from_repr(seq.scan_ctx(1)?.u8()?).unwrap_or(AudioCodec::Opus);
                let channel_count = seq.scan_ctx(2)?.u8()?;
                let sample_rate = seq.scan_ctx(3)?.u32()?;
                let bit_rate = seq.scan_ctx(4)?.u32()?;
                let bit_depth = seq.scan_ctx(5)?.u8()?;

                let mut cluster = self.cluster.write();
                let stream_id = cluster
                    .audio_stream_allocate(
                        stream_usage,
                        audio_codec,
                        channel_count,
                        sample_rate,
                        bit_rate,
                        bit_depth,
                    )
                    .map_err(|e| {
                        log::warn!("AudioStreamAllocate failed: {}", e);
                        Error::new(ErrorCode::ResourceExhausted)
                    })?;

                self.dataver.changed();

                // Send response
                let mut writer =
                    reply.with_command(response_commands::AUDIO_STREAM_ALLOCATE_RESPONSE)?;
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u16(&TLVTag::Context(0), stream_id)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            CameraCommand::AudioStreamDeallocate => {
                let mut seq = data.structure()?;
                let audio_stream_id = seq.scan_ctx(0)?.u16()?;

                let mut cluster = self.cluster.write();
                cluster
                    .audio_stream_deallocate(audio_stream_id)
                    .map_err(|e| {
                        log::warn!("AudioStreamDeallocate failed: {}", e);
                        Error::new(ErrorCode::NotFound)
                    })?;

                self.dataver.changed();
                Ok(())
            }
            CameraCommand::SnapshotStreamAllocate => {
                // Not implemented - return error
                Err(Error::new(ErrorCode::InvalidAction))
            }
            CameraCommand::SnapshotStreamDeallocate => {
                // Not implemented - return error
                Err(Error::new(ErrorCode::InvalidAction))
            }
            CameraCommand::SetStreamPriorities => {
                // Not implemented - accept but no-op
                Ok(())
            }
            CameraCommand::CaptureSnapshot => {
                // Not implemented - return error
                Err(Error::new(ErrorCode::InvalidAction))
            }
            CameraCommand::SetViewport => {
                // Not implemented - accept but no-op
                Ok(())
            }
            CameraCommand::SetImageRotation => {
                let mut seq = data.structure()?;
                let rotation = seq.scan_ctx(0)?.u16()?;
                let mut cluster = self.cluster.write();
                cluster.attributes.image_rotation = rotation;
                self.dataver.changed();
                Ok(())
            }
        }
    }
}

impl Handler for CameraAvStreamMgmtHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }

    fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        self.invoke_impl(ctx, reply)
    }
}

impl NonBlockingHandler for CameraAvStreamMgmtHandler {}
