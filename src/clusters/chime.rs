use serde::{Deserialize, Serialize};

/// Matter Cluster ID for Chime
pub const CLUSTER_ID: u32 = 0x0556;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Chime sound structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChimeSound {
    pub chime_id: u8,
    pub name: String,
}

/// Chime cluster attributes
#[derive(Debug, Clone)]
pub struct ChimeAttributes {
    pub installed_chime_sounds: Vec<ChimeSound>,
    pub selected_chime: u8,
    pub enabled: bool,
}

impl Default for ChimeAttributes {
    fn default() -> Self {
        Self {
            installed_chime_sounds: vec![
                ChimeSound {
                    chime_id: 0,
                    name: "Default".to_string(),
                },
                ChimeSound {
                    chime_id: 1,
                    name: "Classic".to_string(),
                },
                ChimeSound {
                    chime_id: 2,
                    name: "Modern".to_string(),
                },
            ],
            selected_chime: 0,
            enabled: true,
        }
    }
}

/// Callback type for chime playback
pub type ChimePlaybackCallback = Box<dyn Fn(u8) + Send + Sync>;

/// Chime cluster handler
pub struct ChimeCluster {
    pub attributes: ChimeAttributes,
    playback_callback: Option<ChimePlaybackCallback>,
}

impl ChimeCluster {
    pub fn new() -> Self {
        Self {
            attributes: ChimeAttributes::default(),
            playback_callback: None,
        }
    }

    pub fn with_sounds(sounds: Vec<ChimeSound>) -> Self {
        Self {
            attributes: ChimeAttributes {
                installed_chime_sounds: sounds,
                selected_chime: 0,
                enabled: true,
            },
            playback_callback: None,
        }
    }

    /// Set the callback for when a chime should play
    pub fn set_playback_callback(&mut self, callback: ChimePlaybackCallback) {
        self.playback_callback = Some(callback);
    }

    /// Get installed chime sounds
    pub fn get_installed_chime_sounds(&self) -> &[ChimeSound] {
        &self.attributes.installed_chime_sounds
    }

    /// Get the currently selected chime
    pub fn get_selected_chime(&self) -> u8 {
        self.attributes.selected_chime
    }

    /// Set the selected chime
    pub fn set_selected_chime(&mut self, chime_id: u8) -> Result<(), &'static str> {
        if self
            .attributes
            .installed_chime_sounds
            .iter()
            .any(|s| s.chime_id == chime_id)
        {
            self.attributes.selected_chime = chime_id;
            Ok(())
        } else {
            Err("Chime ID not found in installed sounds")
        }
    }

    /// Check if chime is enabled
    pub fn is_enabled(&self) -> bool {
        self.attributes.enabled
    }

    /// Set chime enabled state
    pub fn set_enabled(&mut self, enabled: bool) {
        self.attributes.enabled = enabled;
    }

    /// Handle PlayChimeSound command
    /// Plays the currently selected chime sound
    pub fn play_chime_sound(&self) -> Result<(), &'static str> {
        if !self.attributes.enabled {
            return Err("Chime is disabled");
        }

        log::info!(
            "Playing chime sound: {} (ID: {})",
            self.get_selected_chime_name(),
            self.attributes.selected_chime
        );

        if let Some(callback) = &self.playback_callback {
            callback(self.attributes.selected_chime);
        }

        Ok(())
    }

    /// Get the name of the currently selected chime
    fn get_selected_chime_name(&self) -> &str {
        self.attributes
            .installed_chime_sounds
            .iter()
            .find(|s| s.chime_id == self.attributes.selected_chime)
            .map(|s| s.name.as_str())
            .unwrap_or("Unknown")
    }
}

impl Default for ChimeCluster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chime_creation() {
        let cluster = ChimeCluster::new();
        assert_eq!(cluster.get_installed_chime_sounds().len(), 3);
        assert_eq!(cluster.get_selected_chime(), 0);
        assert!(cluster.is_enabled());
    }

    #[test]
    fn test_set_selected_chime() {
        let mut cluster = ChimeCluster::new();
        assert!(cluster.set_selected_chime(1).is_ok());
        assert_eq!(cluster.get_selected_chime(), 1);
    }

    #[test]
    fn test_invalid_chime_selection() {
        let mut cluster = ChimeCluster::new();
        assert!(cluster.set_selected_chime(99).is_err());
    }

    #[test]
    fn test_play_disabled_chime() {
        let mut cluster = ChimeCluster::new();
        cluster.set_enabled(false);
        assert!(cluster.play_chime_sound().is_err());
    }

    #[test]
    fn test_play_enabled_chime() {
        let cluster = ChimeCluster::new();
        assert!(cluster.play_chime_sound().is_ok());
    }
}
