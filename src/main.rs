use anyhow::{Context, Result};
use chrono::{Datelike, Duration, Offset, TimeZone, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::env;
use std::path::Path;
use tracing::{debug, info, warn};

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
struct DiscoverResponse {
    #[serde(rename = "DeviceAuth")]
    device_auth: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
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

#[derive(Debug, Deserialize)]
struct ChannelEpgSegment {
    #[serde(rename = "GuideNumber")]
    guide_number: String,
    #[serde(rename = "Guide")]
    guide: Vec<Programme>,
    #[serde(rename = "ImageURL", default)]
    image_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct Programme {
    #[serde(rename = "StartTime")]
    start_time: i64,
    #[serde(rename = "EndTime")]
    end_time: i64,
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "EpisodeTitle", default)]
    episode_title: Option<String>,
    #[serde(rename = "Synopsis", default)]
    synopsis: Option<String>,
    #[serde(rename = "EpisodeNumber", default)]
    episode_number: Option<String>,
    #[serde(rename = "Filter", default)]
    filter: Option<Vec<String>>,
    #[serde(rename = "ImageURL", default)]
    image_url: Option<String>,
    #[serde(rename = "OriginalAirdate", default)]
    original_airdate: Option<i64>,
    #[serde(rename = "First", default)]
    first: Option<bool>,
    #[serde(skip)]
    guide_number: Option<String>,
}

#[derive(Debug)]
struct EpgData {
    channels: Vec<Channel>,
    programmes: Vec<Programme>,
}

async fn discover_device_auth(host: &str) -> Result<String> {
    info!("Fetching HDHomeRun Web API Device Auth");
    let url = format!("http://{}/discover.json", host);
    let response = reqwest::get(&url)
        .await
        .context("Failed to discover device")?;
    
    let discover: DiscoverResponse = response
        .json()
        .await
        .context("Failed to parse discover response")?;
    
    info!("Discovered device auth: {}", discover.device_auth);
    Ok(discover.device_auth)
}

async fn fetch_channels(host: &str) -> Result<Vec<Channel>> {
    info!("Fetching HDHomeRun Web API Lineup");
    let url = format!("http://{}/lineup.json", host);
    let response = reqwest::get(&url)
        .await
        .context("Failed to fetch channels")?;
    
    let channels: Vec<Channel> = response
        .json()
        .await
        .context("Failed to parse channels")?;
    
    info!("Fetched {} channels", channels.len());
    Ok(channels)
}

async fn fetch_epg_data(
    device_auth: &str,
    channels: &[Channel],
    days: i64,
    hours: i64,
) -> Result<EpgData> {
    use std::collections::HashSet;
    
    let mut epg_data = EpgData {
        channels: Vec::new(),
        programmes: Vec::new(),
    };
    
    // Track seen programmes for O(1) duplicate detection
    let mut seen_programmes: HashSet<(i64, String, String)> = HashSet::new();
    
    let mut next_start_date = Utc::now();
    let end_time = next_start_date + Duration::days(days);
    
    // NOTE: HDHomeRun API uses a self-signed certificate, so we need to accept invalid certs.
    // This matches the behavior of the original Python script which used an unverified SSL context.
    // The connection is still encrypted, but certificate validation is skipped.
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    
    while next_start_date < end_time {
        let url_start_date = next_start_date.timestamp();
        let url = format!(
            "https://api.hdhomerun.com/api/guide.php?DeviceAuth={}&Start={}",
            device_auth, url_start_date
        );
        
        debug!(
            "Fetching EPG for all channels starting {} from {}",
            next_start_date.format("%Y-%m-%d %H:%M:%S"),
            url
        );
        
        let response = client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch EPG data")?;
        
        let epg_segment: Vec<ChannelEpgSegment> = response
            .json()
            .await
            .context("Failed to parse EPG segment")?;
        
        info!(
            "Processing from {}",
            next_start_date.format("%Y-%m-%d %H:%M:%S")
        );
        
        for channel_epg_segment in epg_segment {
            for mut programme in channel_epg_segment.guide {
                // Check if the epg program channel is within our tuned channel list
                let channel = channels
                    .iter()
                    .find(|ch| ch.guide_number == channel_epg_segment.guide_number);
                
                if channel.is_none() {
                    debug!(
                        "Skipping program for untuned channel {}",
                        channel_epg_segment.guide_number
                    );
                    continue;
                }
                
                // Check if the epg program has already been retrieved due to overlapping requests
                let programme_key = (
                    programme.start_time,
                    programme.title.clone(),
                    channel_epg_segment.guide_number.clone(),
                );
                
                if seen_programmes.contains(&programme_key) {
                    debug!(
                        "Skipping duplicate program {} starting at {}",
                        programme.title, programme.start_time
                    );
                    continue;
                }
                
                seen_programmes.insert(programme_key);
                
                // Add channel to epg_data if not already present
                if !epg_data
                    .channels
                    .iter()
                    .any(|ch| ch.guide_number == channel_epg_segment.guide_number)
                {
                    let mut channel = channel.unwrap().clone();
                    if channel.image_url.is_none() {
                        channel.image_url = channel_epg_segment.image_url.clone();
                    }
                    epg_data.channels.push(channel);
                }
                
                programme.guide_number = Some(channel_epg_segment.guide_number.clone());
                debug!(
                    "Appending: {} from {} to {}",
                    programme.title, programme.start_time, programme.end_time
                );
                epg_data.programmes.push(programme);
            }
        }
        
        next_start_date = next_start_date + Duration::hours(hours);
    }
    
    Ok(epg_data)
}

fn format_datetime(timestamp: i64, tz: &chrono_tz::Tz) -> Option<String> {
    let dt = Utc.timestamp_opt(timestamp, 0).single()?;
    let local_dt = dt.with_timezone(tz);
    let offset_seconds = local_dt.offset().fix().local_minus_utc();
    let offset_hours = offset_seconds / 3600;
    let offset_minutes = (offset_seconds % 3600).abs() / 60;
    Some(format!(
        "{:04}{:02}{:02}{:02}{:02}{:02} {:+03}{:02}",
        local_dt.year(),
        local_dt.month(),
        local_dt.day(),
        local_dt.hour(),
        local_dt.minute(),
        local_dt.second(),
        offset_hours,
        offset_minutes
    ))
}

fn generate_xmltv(epg_data: EpgData, output_path: &Path, timezone: &chrono_tz::Tz) -> Result<()> {
    use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
    use quick_xml::Writer;
    use std::collections::HashMap;
    use std::io::Cursor;
    
    info!("HDHomeRun XMLTV Transformation Started");
    
    // Organize programmes by channel for O(1) lookup
    let mut programmes_by_channel: HashMap<String, Vec<&Programme>> = HashMap::new();
    for programme in &epg_data.programmes {
        if let Some(ref guide_number) = programme.guide_number {
            programmes_by_channel
                .entry(guide_number.clone())
                .or_insert_with(Vec::new)
                .push(programme);
        }
    }
    
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b'\t', 1);
    
    // Write XML declaration
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
    
    // Write root element
    let mut tv = BytesStart::new("tv");
    tv.push_attribute(("source-info-name", "HDHomeRun"));
    tv.push_attribute(("generator-info-name", "hdhr-xmltv"));
    writer.write_event(Event::Start(tv))?;
    
    // Write channels
    for channel in &epg_data.channels {
        let mut channel_elem = BytesStart::new("channel");
        channel_elem.push_attribute(("id", channel.guide_number.as_str()));
        writer.write_event(Event::Start(channel_elem.clone()))?;
        
        // Display name
        writer.write_event(Event::Start(BytesStart::new("display-name")))?;
        writer.write_event(Event::Text(BytesText::new(&channel.guide_name)))?;
        writer.write_event(Event::End(BytesEnd::new("display-name")))?;
        
        // Icon
        if let Some(ref image_url) = channel.image_url {
            let mut icon = BytesStart::new("icon");
            icon.push_attribute(("src", image_url.as_str()));
            writer.write_event(Event::Empty(icon))?;
        }
        
        writer.write_event(Event::End(BytesEnd::new("channel")))?;
    }
    
    // Write programmes
    for channel in &epg_data.channels {
        if let Some(programmes) = programmes_by_channel.get(&channel.guide_number) {
            for programme in programmes {
                let start_str = format_datetime(programme.start_time, timezone);
                let end_str = format_datetime(programme.end_time, timezone);
                
                // Skip programmes with invalid timestamps
                let (start_str, end_str) = match (start_str, end_str) {
                    (Some(s), Some(e)) => (s, e),
                    _ => {
                        warn!(
                            "Skipping programme {} with invalid timestamp",
                            programme.title
                        );
                        continue;
                    }
                };
            
            let mut prog_elem = BytesStart::new("programme");
            prog_elem.push_attribute(("start", start_str.as_str()));
            prog_elem.push_attribute(("stop", end_str.as_str()));
            prog_elem.push_attribute(("channel", channel.guide_number.as_str()));
            writer.write_event(Event::Start(prog_elem))?;
            
            // Title
            let mut title_elem = BytesStart::new("title");
            title_elem.push_attribute(("lang", "en"));
            writer.write_event(Event::Start(title_elem))?;
            writer.write_event(Event::Text(BytesText::new(&programme.title)))?;
            writer.write_event(Event::End(BytesEnd::new("title")))?;
            
            // Sub-title
            if let Some(ref episode_title) = programme.episode_title {
                let mut subtitle_elem = BytesStart::new("sub-title");
                subtitle_elem.push_attribute(("lang", "en"));
                writer.write_event(Event::Start(subtitle_elem))?;
                writer.write_event(Event::Text(BytesText::new(episode_title)))?;
                writer.write_event(Event::End(BytesEnd::new("sub-title")))?;
            }
            
            // Description
            if let Some(ref synopsis) = programme.synopsis {
                let mut desc_elem = BytesStart::new("desc");
                desc_elem.push_attribute(("lang", "en"));
                writer.write_event(Event::Start(desc_elem))?;
                writer.write_event(Event::Text(BytesText::new(synopsis)))?;
                writer.write_event(Event::End(BytesEnd::new("desc")))?;
            }
            
            // Categories
            if let Some(ref filters) = programme.filter {
                for filter in filters {
                    let mut cat_elem = BytesStart::new("category");
                    cat_elem.push_attribute(("lang", "en"));
                    writer.write_event(Event::Start(cat_elem))?;
                    writer.write_event(Event::Text(BytesText::new(filter)))?;
                    writer.write_event(Event::End(BytesEnd::new("category")))?;
                }
            }
            
            // Icon
            if let Some(ref image_url) = programme.image_url {
                let mut icon = BytesStart::new("icon");
                icon.push_attribute(("src", image_url.as_str()));
                writer.write_event(Event::Empty(icon))?;
            }
            
            // Episode number
            if let Some(ref episode_number) = programme.episode_number {
                // Onscreen format
                let mut ep_elem = BytesStart::new("episode-num");
                ep_elem.push_attribute(("system", "onscreen"));
                writer.write_event(Event::Start(ep_elem))?;
                writer.write_event(Event::Text(BytesText::new(episode_number)))?;
                writer.write_event(Event::End(BytesEnd::new("episode-num")))?;
                
                // XMLTV format
                if episode_number.contains('S') && episode_number.contains('E') {
                    if let (Some(s_pos), Some(e_pos)) = (
                        episode_number.find('S'),
                        episode_number.find('E'),
                    ) {
                        let series_str = &episode_number[s_pos + 1..e_pos];
                        let episode_str = &episode_number[e_pos + 1..];
                        
                        if let (Ok(series), Ok(episode)) = (
                            series_str.parse::<i32>(),
                            episode_str.parse::<i32>(),
                        ) {
                            let xmltv_format = format!("{}.{}.0/0", series - 1, episode - 1);
                            let mut ep_elem = BytesStart::new("episode-num");
                            ep_elem.push_attribute(("system", "xmltv_ns"));
                            writer.write_event(Event::Start(ep_elem))?;
                            writer.write_event(Event::Text(BytesText::new(&xmltv_format)))?;
                            writer.write_event(Event::End(BytesEnd::new("episode-num")))?;
                        }
                    }
                }
            }
            
            // Previously shown / New
            if let Some(original_airdate) = programme.original_airdate {
                if let (Some(air_date_dt), Some(start_time_dt)) = (
                    Utc.timestamp_opt(original_airdate, 0).single(),
                    Utc.timestamp_opt(programme.start_time, 0).single(),
                ) {
                    let air_date = air_date_dt.with_timezone(timezone);
                    let start_time = start_time_dt.with_timezone(timezone);
                    
                    if let Some(start_date_naive) = start_time.date_naive().and_hms_opt(0, 0, 0) {
                        if let Some(start_date) = start_date_naive.and_local_timezone(*timezone).single() {
                            if air_date != start_date {
                                let air_date_str = format!(
                                    "{:04}{:02}{:02}{:02}{:02}{:02}",
                                    air_date.year(),
                                    air_date.month(),
                                    air_date.day(),
                                    air_date.hour(),
                                    air_date.minute(),
                                    air_date.second()
                                );
                                let mut prev_elem = BytesStart::new("previously-shown");
                                prev_elem.push_attribute(("start", air_date_str.as_str()));
                                writer.write_event(Event::Empty(prev_elem))?;
                            } else if programme.first != Some(true) {
                                writer.write_event(Event::Empty(BytesStart::new("previously-shown")))?;
                            }
                        }
                    }
                }
            }
            
            if programme.first == Some(true) {
                writer.write_event(Event::Empty(BytesStart::new("new")))?;
            }
            
            writer.write_event(Event::End(BytesEnd::new("programme")))?;
            }
        }
    }
    
    // Close root element
    writer.write_event(Event::End(BytesEnd::new("tv")))?;
    
    let result = writer.into_inner().into_inner();
    std::fs::write(output_path, result).context("Failed to write XMLTV file")?;
    
    info!("HDHomeRun XMLTV Transformation Completed");
    info!("Writing XMLTV to file {} Completed", output_path.display());
    
    Ok(())
}

async fn run_once(
    host: &str,
    days: i64,
    hours: i64,
    output_path: &Path,
    timezone: &chrono_tz::Tz,
) -> Result<()> {
    info!("Starting HDHomeRun EPG extraction");
    
    let device_auth = discover_device_auth(host).await?;
    let channels = fetch_channels(host).await?;
    
    if channels.is_empty() {
        anyhow::bail!("No channels retrieved");
    }
    
    info!("HDHomeRun EPG Extraction Started");
    let epg_data = fetch_epg_data(&device_auth, &channels, days, hours).await?;
    info!("HDHomeRun EPG Extraction Completed");
    
    generate_xmltv(epg_data, output_path, timezone)?;
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();
    
    // Read configuration from environment variables
    let host = env::var("HDHR_HOST")
        .unwrap_or_else(|_| "hdhomerun.local".to_string());
    let days = env::var("DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);
    let hours = env::var("HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let output_file = env::var("OUTPUT_FILE").unwrap_or_else(|_| "epg.xml".to_string());
    let output_dir = env::var("OUTPUT_DIR").unwrap_or_else(|_| "/output".to_string());
    let interval = env::var("INTERVAL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let timezone_str = env::var("TZ")
        .unwrap_or_else(|_| "UTC".to_string());
    
    let timezone: chrono_tz::Tz = timezone_str
        .parse()
        .unwrap_or_else(|_| {
            warn!("Invalid timezone '{}', using UTC", timezone_str);
            chrono_tz::UTC
        });
    
    info!("Configuration:");
    info!("  HDHR_HOST: {}", host);
    info!("  DAYS: {}", days);
    info!("  HOURS: {}", hours);
    info!("  OUTPUT_DIR: {}", output_dir);
    info!("  OUTPUT_FILE: {}", output_file);
    info!("  INTERVAL: {} seconds", interval);
    info!("  TIMEZONE: {}", timezone);
    
    // Create output directory if it doesn't exist
    std::fs::create_dir_all(&output_dir).context("Failed to create output directory")?;
    
    let output_path = Path::new(&output_dir).join(&output_file);
    
    // Run once initially
    run_once(&host, days, hours, &output_path, &timezone).await?;
    
    // If interval is set, run periodically
    if interval > 0 {
        info!("Running in periodic mode with interval of {} seconds", interval);
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
            info!("Starting periodic EPG update");
            if let Err(e) = run_once(&host, days, hours, &output_path, &timezone).await {
                warn!("Error during periodic update: {}", e);
            }
        }
    }
    
    Ok(())
}
