//! BridgedDeviceBasicInformation Cluster (0x0039) handler.
//!
//! Provides endpoint names via the NodeLabel attribute for Matter bridges.
//! Controllers like Home Assistant read NodeLabel to display bridged device names.

use rs_matter::dm::{Cluster, Dataver, ReadContext};
use rs_matter::error::Error;
use rs_matter::tlv::{TLVBuilderParent, Utf8StrBuilder};
use rs_matter::with;

// Import BridgedDeviceBasicInformation cluster from Matter spec
// This generates the `bridged_device_basic_information` module with cluster handlers
rs_matter::import!(BridgedDeviceBasicInformation);

pub use bridged_device_basic_information::ClusterHandler as BridgedClusterHandler;
pub use bridged_device_basic_information::HandlerAdaptor;

/// Handler for BridgedDeviceBasicInformation cluster.
///
/// Provides endpoint names via the NodeLabel attribute. Controllers like Home Assistant
/// read this to name bridged device endpoints.
#[derive(Clone, Debug)]
pub struct BridgedHandler {
    dataver: Dataver,
    /// The display name for this endpoint
    name: &'static str,
}

impl BridgedHandler {
    /// Create a new handler with the given endpoint name
    pub const fn new(dataver: Dataver, name: &'static str) -> Self {
        Self { dataver, name }
    }

    /// Adapt this handler for use in the data model chain
    pub const fn adapt(self) -> HandlerAdaptor<Self> {
        bridged_device_basic_information::HandlerAdaptor(self)
    }
}

impl BridgedClusterHandler for BridgedHandler {
    /// Cluster with required attributes + node_label for naming
    const CLUSTER: Cluster<'static> = bridged_device_basic_information::FULL_CLUSTER
        .with_features(0)
        .with_attrs(with!(required; bridged_device_basic_information::AttributeId::NodeLabel))
        .with_cmds(with!());

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    fn dataver_changed(&self) {
        self.dataver.changed();
    }

    /// Mandatory attribute - report device as reachable
    fn reachable(&self, _ctx: impl ReadContext) -> Result<bool, Error> {
        Ok(true)
    }

    /// Return the endpoint name for display in controllers
    fn node_label<P: TLVBuilderParent>(
        &self,
        _ctx: impl ReadContext,
        out: Utf8StrBuilder<P>,
    ) -> Result<P, Error> {
        out.set(self.name)
    }
}
