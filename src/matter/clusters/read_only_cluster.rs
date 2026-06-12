macro_rules! define_versioned_read_only_cluster_handler {
    (
        $handler:ident,
        $sensor:ty,
        $attr:ty,
        $cluster:ident,
        |$sensor_ref:ident, $tw:ident, $tag:ident, $read_attr:ident| $read:block
    ) => {
        pub struct $handler {
            state: crate::matter::clusters::versioned_state::VersionedClusterState<$sensor>,
        }

        impl $handler {
            pub const CLUSTER: rs_matter::dm::Cluster<'static> = $cluster;

            pub fn new(dataver: rs_matter::dm::Dataver, sensor: std::sync::Arc<$sensor>) -> Self {
                Self {
                    state: crate::matter::clusters::versioned_state::VersionedClusterState::new(
                        dataver, sensor,
                    ),
                }
            }

            fn read_impl(
                &self,
                ctx: impl rs_matter::dm::ReadContext,
                reply: impl rs_matter::dm::ReadReply,
            ) -> Result<(), rs_matter::error::Error> {
                use rs_matter::dm::Reply;

                self.state.sync_dataver();

                let attr = ctx.attr();
                let Some(mut writer) = reply.with_dataver(self.state.dataver().get())? else {
                    return Ok(());
                };

                if attr.is_system() {
                    return $cluster.read(attr, writer);
                }

                let $tag = writer.tag();
                {
                    let mut $tw = writer.writer();
                    let $read_attr: $attr = attr.attr_id.try_into()?;
                    let $sensor_ref = self.state.sensor();
                    $read?;
                }

                writer.complete()
            }

            fn write_impl(
                &self,
                _ctx: impl rs_matter::dm::WriteContext,
            ) -> Result<(), rs_matter::error::Error> {
                Err(rs_matter::error::ErrorCode::UnsupportedAccess.into())
            }
        }

        impl rs_matter::dm::Handler for $handler {
            fn read(
                &self,
                ctx: impl rs_matter::dm::ReadContext,
                reply: impl rs_matter::dm::ReadReply,
            ) -> Result<(), rs_matter::error::Error> {
                self.read_impl(ctx, reply)
            }

            fn write(
                &self,
                ctx: impl rs_matter::dm::WriteContext,
            ) -> Result<(), rs_matter::error::Error> {
                self.write_impl(ctx)
            }

            fn bump_dataver(&self, _ctx: impl rs_matter::dm::MatchContext) {
                self.state.bump_dataver();
            }
        }

        impl rs_matter::dm::NonBlockingHandler for $handler {}
    };
}

pub(crate) use define_versioned_read_only_cluster_handler;
