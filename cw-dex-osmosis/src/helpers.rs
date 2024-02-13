use std::time::Duration;

pub(crate) trait ToProtobufDuration {
    fn to_protobuf_duration(&self) -> osmosis_std::shim::Duration;
}

impl ToProtobufDuration for Duration {
    fn to_protobuf_duration(&self) -> osmosis_std::shim::Duration {
        osmosis_std::shim::Duration {
            seconds: self.as_secs() as i64,
            nanos: self.subsec_nanos() as i32,
        }
    }
}
