use serde::{Deserialize, Deserializer};

// Custom deserializer for integer-to-boolean conversion
fn deserialize_int_as_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i != 0)
            } else if let Some(f) = n.as_f64() {
                Ok(f != 0.0)
            } else {
                Ok(false)
            }
        }
        serde_json::Value::Bool(b) => Ok(b),
        serde_json::Value::Null => Ok(false),
        _ => Err(Error::custom("expected integer, boolean, or null for HD field")),
    }
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(rename = "GuideNumber")]
    guide_number: String,
    #[serde(rename = "GuideName")]
    guide_name: String,
    #[serde(rename = "URL", default)]
    url: Option<String>,
    #[serde(rename = "ImageURL", default)]
    image_url: Option<String>,
    #[serde(rename = "HD", default, deserialize_with = "deserialize_int_as_bool")]
    hd: bool,
    #[serde(rename = "VideoCodec", default)]
    video_codec: Option<String>,
    #[serde(rename = "AudioCodec", default)]
    audio_codec: Option<String>,
    #[serde(rename = "SignalStrength", default)]
    signal_strength: Option<i32>,
    #[serde(rename = "SignalQuality", default)]
    signal_quality: Option<i32>,
}

#[test]
fn test_hd_as_integer_1() {
    let json = r#"{"GuideNumber":"2.1","GuideName":"WCBD-HD","VideoCodec":"MPEG2","AudioCodec":"AC3","HD":1,"SignalStrength":100,"SignalQuality":100,"URL":"http://hdhr-1047c695.local:5004/auto/v2.1"}"#;
    let channel: Channel = serde_json::from_str(json).expect("Failed to parse channel with HD=1");
    assert_eq!(channel.hd, true);
    assert_eq!(channel.guide_number, "2.1");
    assert_eq!(channel.guide_name, "WCBD-HD");
    assert_eq!(channel.video_codec, Some("MPEG2".to_string()));
    assert_eq!(channel.audio_codec, Some("AC3".to_string()));
    assert_eq!(channel.signal_strength, Some(100));
    assert_eq!(channel.signal_quality, Some(100));
}

#[test]
fn test_hd_as_integer_0() {
    let json = r#"{"GuideNumber":"5.1","GuideName":"TEST-SD","HD":0,"URL":"http://test.local:5004/auto/v5.1"}"#;
    let channel: Channel = serde_json::from_str(json).expect("Failed to parse channel with HD=0");
    assert_eq!(channel.hd, false);
}

#[test]
fn test_missing_hd_field() {
    let json = r#"{"GuideNumber":"7.1","GuideName":"TEST-NO-HD","URL":"http://test.local:5004/auto/v7.1"}"#;
    let channel: Channel = serde_json::from_str(json).expect("Failed to parse channel without HD field");
    assert_eq!(channel.hd, false);
}

#[test]
fn test_hd_as_boolean_true() {
    let json = r#"{"GuideNumber":"9.1","GuideName":"TEST-BOOL-HD","HD":true,"URL":"http://test.local:5004/auto/v9.1"}"#;
    let channel: Channel = serde_json::from_str(json).expect("Failed to parse channel with HD=true");
    assert_eq!(channel.hd, true);
}

#[test]
fn test_hd_as_boolean_false() {
    let json = r#"{"GuideNumber":"11.1","GuideName":"TEST-BOOL-SD","HD":false,"URL":"http://test.local:5004/auto/v11.1"}"#;
    let channel: Channel = serde_json::from_str(json).expect("Failed to parse channel with HD=false");
    assert_eq!(channel.hd, false);
}
