use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU16, Ordering};
use strum::FromRepr;

/// Matter Cluster ID for Camera AV Stream Management
pub const CLUSTER_ID: u32 = 0x0551;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Feature flags for the Camera AV Stream Management cluster
#[derive(Debug, Clone, Copy, Default)]
pub struct Features {
    /// Audio support (ADO)
    pub audio: bool,
    /// Video support (VDO)
    pub video: bool,
    /// Snapshot support (SNP)
    pub snapshot: bool,
    /// Privacy support (PRIV)
    pub privacy: bool,
    /// Speaker support (SPKR)
    pub speaker: bool,
    /// Image control support (ICTL)
    pub image_control: bool,
    /// Watermark support (WMARK)
    pub watermark: bool,
    /// On-screen display support (OSD)
    pub osd: bool,
    /// Local storage support (STOR)
    pub local_storage: bool,
    /// HDR support (HDR)
    pub hdr: bool,
    /// Night vision support (NV)
    pub night_vision: bool,
}

/// Audio codec enumeration (Matter spec)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, FromRepr)]
#[repr(u8)]
pub enum AudioCodec {
    Opus = 0x00,
    AacLc = 0x01,
}

/// Image codec enumeration (Matter spec)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ImageCodec {
    Jpeg = 0x00,
}

/// Video codec enumeration (Matter spec)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, FromRepr)]
#[repr(u8)]
pub enum VideoCodec {
    H264 = 0x00,
    Hevc = 0x01,
    Vvc = 0x02,
    Av1 = 0x03,
}

/// Stream usage type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, FromRepr)]
#[repr(u8)]
pub enum StreamUsage {
    Internal = 0x00,
    Recording = 0x01,
    Analysis = 0x02,
    LiveView = 0x03,
}

/// Tri-state auto enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TriStateAuto {
    Off = 0x00,
    On = 0x01,
    Auto = 0x02,
}

/// Two-way talk support type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TwoWayTalkSupportType {
    NotSupported = 0x00,
    HalfDuplex = 0x01,
    FullDuplex = 0x02,
}

/// Video resolution structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoResolution {
    pub width: u16,
    pub height: u16,
}

impl VideoResolution {
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}

/// Video sensor parameters structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSensorParams {
    pub sensor_width: u16,
    pub sensor_height: u16,
    pub max_fps: u16,
    pub max_hdr_fps: Option<u16>,
}

/// Audio capabilities structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCapabilities {
    pub max_number_of_channels: u8,
    pub supported_codecs: Vec<AudioCodec>,
    pub supported_sample_rates: Vec<u32>,
    pub supported_bit_depths: Vec<u8>,
}

/// Video stream structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoStream {
    pub video_stream_id: u16,
    pub stream_usage: StreamUsage,
    pub video_codec: VideoCodec,
    pub min_frame_rate: u16,
    pub max_frame_rate: u16,
    pub min_resolution: VideoResolution,
    pub max_resolution: VideoResolution,
    pub min_bit_rate: u32,
    pub max_bit_rate: u32,
    pub key_frame_interval: u16,
    pub watermark_enabled: bool,
    pub osd_enabled: bool,
    pub reference_count: u8,
}

/// Audio stream structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStream {
    pub audio_stream_id: u16,
    pub stream_usage: StreamUsage,
    pub audio_codec: AudioCodec,
    pub channel_count: u8,
    pub sample_rate: u32,
    pub bit_rate: u32,
    pub bit_depth: u8,
    pub reference_count: u8,
}

/// Snapshot capabilities structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotCapabilities {
    pub resolution: VideoResolution,
    pub max_frame_rate: u16,
    pub image_codec: ImageCodec,
    pub requires_encoded_pixels: bool,
    pub requires_hardware_encoder: bool,
}

/// Snapshot stream structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotStream {
    pub snapshot_stream_id: u16,
    pub image_codec: ImageCodec,
    pub frame_rate: u16,
    pub min_resolution: VideoResolution,
    pub max_resolution: VideoResolution,
    pub quality: u8,
    pub reference_count: u8,
    pub encoded_pixels: bool,
    pub hardware_encoder: bool,
    pub watermark_enabled: bool,
    pub osd_enabled: bool,
}

/// Camera AV Stream Management cluster attributes
#[derive(Debug, Clone)]
pub struct CameraAvStreamMgmtAttributes {
    pub max_concurrent_encoders: u8,
    pub max_encoded_pixel_rate: u32,
    pub video_sensor_params: VideoSensorParams,
    pub min_viewport_resolution: VideoResolution,
    pub microphone_capabilities: Option<AudioCapabilities>,
    pub speaker_capabilities: Option<AudioCapabilities>,
    pub two_way_talk_support: TwoWayTalkSupportType,
    pub max_content_buffer_size: u32,
    pub max_network_bandwidth: u32,
    pub current_frame_rate: u16,
    pub hdr_mode_enabled: bool,
    pub speaker_muted: bool,
    pub speaker_volume_level: u8,
    pub microphone_muted: bool,
    pub microphone_volume_level: u8,
    pub microphone_agc_enabled: bool,
    pub night_vision: TriStateAuto,
    pub image_rotation: u16,
    pub image_flip_horizontal: bool,
    pub image_flip_vertical: bool,
    pub local_video_recording_enabled: bool,
    pub local_snapshot_recording_enabled: bool,
    pub soft_recording_privacy_mode_enabled: bool,
    pub soft_livestream_privacy_mode_enabled: bool,
    pub hard_privacy_mode_on: bool,
}

impl Default for CameraAvStreamMgmtAttributes {
    fn default() -> Self {
        Self {
            max_concurrent_encoders: 4,
            max_encoded_pixel_rate: 1920 * 1080 * 30,
            video_sensor_params: VideoSensorParams {
                sensor_width: 1920,
                sensor_height: 1080,
                max_fps: 30,
                max_hdr_fps: None,
            },
            min_viewport_resolution: VideoResolution::new(320, 240),
            microphone_capabilities: Some(AudioCapabilities {
                max_number_of_channels: 1,
                supported_codecs: vec![AudioCodec::Opus],
                supported_sample_rates: vec![16000, 48000],
                supported_bit_depths: vec![16],
            }),
            speaker_capabilities: None,
            two_way_talk_support: TwoWayTalkSupportType::NotSupported,
            max_content_buffer_size: 10 * 1024 * 1024,
            max_network_bandwidth: 10_000_000,
            current_frame_rate: 30,
            hdr_mode_enabled: false,
            speaker_muted: true,
            speaker_volume_level: 0,
            microphone_muted: false,
            microphone_volume_level: 100,
            microphone_agc_enabled: true,
            night_vision: TriStateAuto::Auto,
            image_rotation: 0,
            image_flip_horizontal: false,
            image_flip_vertical: false,
            local_video_recording_enabled: false,
            local_snapshot_recording_enabled: false,
            soft_recording_privacy_mode_enabled: false,
            soft_livestream_privacy_mode_enabled: false,
            hard_privacy_mode_on: false,
        }
    }
}

/// Camera AV Stream Management cluster handler
pub struct CameraAvStreamMgmtCluster {
    pub features: Features,
    pub attributes: CameraAvStreamMgmtAttributes,
    allocated_video_streams: Vec<VideoStream>,
    allocated_audio_streams: Vec<AudioStream>,
    allocated_snapshot_streams: Vec<SnapshotStream>,
    next_video_stream_id: AtomicU16,
    next_audio_stream_id: AtomicU16,
    next_snapshot_stream_id: AtomicU16,
}

impl CameraAvStreamMgmtCluster {
    pub fn new(features: Features) -> Self {
        Self {
            features,
            attributes: CameraAvStreamMgmtAttributes::default(),
            allocated_video_streams: Vec::new(),
            allocated_audio_streams: Vec::new(),
            allocated_snapshot_streams: Vec::new(),
            next_video_stream_id: AtomicU16::new(1),
            next_audio_stream_id: AtomicU16::new(1),
            next_snapshot_stream_id: AtomicU16::new(1),
        }
    }

    /// Allocate a new video stream
    #[allow(clippy::too_many_arguments)]
    pub fn video_stream_allocate(
        &mut self,
        stream_usage: StreamUsage,
        video_codec: VideoCodec,
        min_frame_rate: u16,
        max_frame_rate: u16,
        min_resolution: VideoResolution,
        max_resolution: VideoResolution,
        min_bit_rate: u32,
        max_bit_rate: u32,
    ) -> Result<u16, &'static str> {
        if self.allocated_video_streams.len() >= self.attributes.max_concurrent_encoders as usize {
            return Err("Maximum concurrent encoders reached");
        }

        let stream_id = self.next_video_stream_id.fetch_add(1, Ordering::SeqCst);

        let stream = VideoStream {
            video_stream_id: stream_id,
            stream_usage,
            video_codec,
            min_frame_rate,
            max_frame_rate,
            min_resolution,
            max_resolution,
            min_bit_rate,
            max_bit_rate,
            key_frame_interval: 30,
            watermark_enabled: false,
            osd_enabled: false,
            reference_count: 1,
        };

        self.allocated_video_streams.push(stream);
        Ok(stream_id)
    }

    /// Deallocate a video stream
    pub fn video_stream_deallocate(&mut self, video_stream_id: u16) -> Result<(), &'static str> {
        if let Some(pos) = self
            .allocated_video_streams
            .iter()
            .position(|s| s.video_stream_id == video_stream_id)
        {
            self.allocated_video_streams.remove(pos);
            Ok(())
        } else {
            Err("Video stream not found")
        }
    }

    /// Allocate a new audio stream
    pub fn audio_stream_allocate(
        &mut self,
        stream_usage: StreamUsage,
        audio_codec: AudioCodec,
        channel_count: u8,
        sample_rate: u32,
        bit_rate: u32,
        bit_depth: u8,
    ) -> Result<u16, &'static str> {
        let stream_id = self.next_audio_stream_id.fetch_add(1, Ordering::SeqCst);

        let stream = AudioStream {
            audio_stream_id: stream_id,
            stream_usage,
            audio_codec,
            channel_count,
            sample_rate,
            bit_rate,
            bit_depth,
            reference_count: 1,
        };

        self.allocated_audio_streams.push(stream);
        Ok(stream_id)
    }

    /// Deallocate an audio stream
    pub fn audio_stream_deallocate(&mut self, audio_stream_id: u16) -> Result<(), &'static str> {
        if let Some(pos) = self
            .allocated_audio_streams
            .iter()
            .position(|s| s.audio_stream_id == audio_stream_id)
        {
            self.allocated_audio_streams.remove(pos);
            Ok(())
        } else {
            Err("Audio stream not found")
        }
    }

    /// Get allocated video streams
    pub fn get_allocated_video_streams(&self) -> &[VideoStream] {
        &self.allocated_video_streams
    }

    /// Get allocated audio streams
    pub fn get_allocated_audio_streams(&self) -> &[AudioStream] {
        &self.allocated_audio_streams
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_stream_allocation() {
        let features = Features {
            video: true,
            audio: true,
            ..Default::default()
        };
        let mut cluster = CameraAvStreamMgmtCluster::new(features);

        let stream_id = cluster
            .video_stream_allocate(
                StreamUsage::LiveView,
                VideoCodec::H264,
                15,
                30,
                VideoResolution::new(640, 480),
                VideoResolution::new(1920, 1080),
                500_000,
                4_000_000,
            )
            .unwrap();

        assert_eq!(stream_id, 1);
        assert_eq!(cluster.get_allocated_video_streams().len(), 1);
    }

    #[test]
    fn test_video_stream_deallocation() {
        let features = Features {
            video: true,
            ..Default::default()
        };
        let mut cluster = CameraAvStreamMgmtCluster::new(features);

        let stream_id = cluster
            .video_stream_allocate(
                StreamUsage::LiveView,
                VideoCodec::H264,
                15,
                30,
                VideoResolution::new(640, 480),
                VideoResolution::new(1920, 1080),
                500_000,
                4_000_000,
            )
            .unwrap();

        cluster.video_stream_deallocate(stream_id).unwrap();
        assert_eq!(cluster.get_allocated_video_streams().len(), 0);
    }
}
