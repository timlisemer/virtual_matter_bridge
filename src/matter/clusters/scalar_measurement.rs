macro_rules! define_scalar_measurement_handler {
    (
        $handler:ident,
        $sensor:ident,
        $attr:ident,
        $cluster:ident,
        $min:expr,
        $max:expr,
        $write_raw:ident
    ) => {
        define_versioned_read_only_cluster_handler!(
            $handler,
            $sensor,
            $attr,
            $cluster,
            |sensor, tw, tag, attr| {
                match attr {
                    $attr::MeasuredValue => {
                        tw.$write_raw(tag, sensor.raw_value())?;
                    }
                    $attr::MinMeasuredValue => {
                        tw.$write_raw(tag, $min)?;
                    }
                    $attr::MaxMeasuredValue => {
                        tw.$write_raw(tag, $max)?;
                    }
                    $attr::Tolerance => {
                        tw.u16(tag, 0)?;
                    }
                }
                Ok::<(), rs_matter::error::Error>(())
            }
        );
    };
}

pub(crate) use define_scalar_measurement_handler;
