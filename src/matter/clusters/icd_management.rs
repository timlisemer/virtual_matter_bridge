//! Minimal ICD Management cluster compatibility handler for the root endpoint.
//!
//! This is not real Intermittently Connected Device support. It only answers
//! commissioning-time controller reads for an always-on bridge.

use rs_matter::dm::clusters::decl::icd_management as icd;
use rs_matter::dm::{Cluster, Dataver, InvokeContext, ReadContext};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{TLVBuilderParent, Utf8StrBuilder};
use rs_matter::with;

/// Handler for a minimal read-only ICD Management cluster.
pub struct IcdManagementHandler {
    dataver: Dataver,
}

impl IcdManagementHandler {
    /// Cluster definition for use in the data model.
    pub const CLUSTER: Cluster<'static> = icd::FULL_CLUSTER
        .with_features(icd::Feature::USER_ACTIVE_MODE_TRIGGER.bits())
        .with_attrs(with!(required;
            icd::AttributeId::UserActiveModeTriggerHint
                | icd::AttributeId::UserActiveModeTriggerInstruction
        ))
        .with_cmds(with!())
        .with_events(with!());

    /// Create a new handler.
    pub const fn new(dataver: Dataver) -> Self {
        Self { dataver }
    }
}

impl icd::ClusterHandler for IcdManagementHandler {
    const CLUSTER: Cluster<'static> = Self::CLUSTER;

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    fn dataver_changed(&self) {
        self.dataver.changed();
    }

    fn idle_mode_duration(&self, _ctx: impl ReadContext) -> Result<u32, Error> {
        Ok(0)
    }

    fn active_mode_duration(&self, _ctx: impl ReadContext) -> Result<u32, Error> {
        Ok(0)
    }

    fn active_mode_threshold(&self, _ctx: impl ReadContext) -> Result<u16, Error> {
        Ok(0)
    }

    fn user_active_mode_trigger_hint(
        &self,
        _ctx: impl ReadContext,
    ) -> Result<icd::UserActiveModeTriggerBitmap, Error> {
        Ok(icd::UserActiveModeTriggerBitmap::empty())
    }

    fn user_active_mode_trigger_instruction<P: TLVBuilderParent>(
        &self,
        _ctx: impl ReadContext,
        builder: Utf8StrBuilder<P>,
    ) -> Result<P, Error> {
        builder.set("")
    }

    fn handle_register_client<P: TLVBuilderParent>(
        &self,
        _ctx: impl InvokeContext,
        _request: icd::RegisterClientRequest<'_>,
        _response: icd::RegisterClientResponseBuilder<P>,
    ) -> Result<P, Error> {
        Err(ErrorCode::CommandNotFound.into())
    }

    fn handle_unregister_client(
        &self,
        _ctx: impl InvokeContext,
        _request: icd::UnregisterClientRequest<'_>,
    ) -> Result<(), Error> {
        Err(ErrorCode::CommandNotFound.into())
    }

    fn handle_stay_active_request<P: TLVBuilderParent>(
        &self,
        _ctx: impl InvokeContext,
        _request: icd::StayActiveRequestRequest<'_>,
        _response: icd::StayActiveResponseBuilder<P>,
    ) -> Result<P, Error> {
        Err(ErrorCode::CommandNotFound.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs_matter::dm::clusters::decl::icd_management::ClusterHandler;

    fn attr(id: icd::AttributeId) -> &'static rs_matter::dm::Attribute {
        IcdManagementHandler::CLUSTER
            .attributes
            .iter()
            .find(|attr| attr.id == id as u32)
            .expect("ICD Management attribute exists in generated metadata")
    }

    #[test]
    fn exposes_minimal_read_only_icd_management_metadata() {
        let cluster = IcdManagementHandler::CLUSTER;

        assert_eq!(cluster.id, 0x0046);
        assert_eq!(cluster.revision, 3);
        assert_eq!(
            cluster.feature_map,
            icd::Feature::USER_ACTIVE_MODE_TRIGGER.bits()
        );
        assert!((cluster.with_attrs)(
            attr(icd::AttributeId::UserActiveModeTriggerHint),
            cluster.revision,
            cluster.feature_map,
        ));
        assert!((cluster.with_attrs)(
            attr(icd::AttributeId::UserActiveModeTriggerInstruction),
            cluster.revision,
            cluster.feature_map,
        ));
        assert!(!(cluster.with_attrs)(
            attr(icd::AttributeId::RegisteredClients),
            cluster.revision,
            cluster.feature_map,
        ));
    }

    #[test]
    fn dataver_changes_when_handler_reports_mutation() {
        let handler = IcdManagementHandler::new(Dataver::new(7));

        assert_eq!(handler.dataver(), 7);
        handler.dataver_changed();
        assert_eq!(handler.dataver(), 8);
    }
}
