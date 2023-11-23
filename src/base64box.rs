use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error;
use base64::{Engine as _, alphabet, engine::{GeneralPurpose, general_purpose}};
use lazy_static::lazy_static;

lazy_static! {
    static ref ENGINE: GeneralPurpose = GeneralPurpose::new(
        &alphabet::STANDARD,
        general_purpose::NO_PAD
    );
}
#[derive(Debug)]
pub struct Base64Box(pub Vec<u8>);
impl Serialize for Base64Box {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&ENGINE.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Base64Box {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Vis;
        impl serde::de::Visitor<'_> for Vis {
            type Value = Base64Box;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a base64 string")
            }

            fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
                ENGINE.decode(v).map(Base64Box).map_err(Error::custom)
            }
        }
        deserializer.deserialize_str(Vis)
    }
}