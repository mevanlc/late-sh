use crate::api_types::Track;
use anyhow::{Context, Result};

#[derive(serde::Deserialize)]
struct Source {
    title: Option<String>,
}

#[derive(serde::Deserialize)]
struct IceStats {
    source: Option<Source>,
}

#[derive(serde::Deserialize)]
struct StatusRoot {
    icestats: IceStats,
}

pub fn fetch_track(url: &str) -> Result<Track> {
    let status_url = url.to_string() + "/status-json.xsl";
    let body = reqwest::blocking::get(status_url)
        .context("fetching icecast status")?
        .text()
        .context("reading icecast status body")?;

    parse_track(&body)
}

fn parse_track(body: &str) -> Result<Track> {
    let parsed: StatusRoot = serde_json::from_str(body).context("parsing icecast status json")?;

    let full_title = parsed
        .icestats
        .source
        .and_then(|s| s.title)
        .unwrap_or_else(|| "Unknown - Unknown Track".to_string());

    // Format: "Artist - Title | Duration"

    // 1. Extract Duration if present
    let (metadata, duration_seconds) = if let Some((rest, dur_str)) = full_title.rsplit_once(" | ")
    {
        let dur = dur_str.parse::<u64>().ok();
        (rest, dur)
    } else {
        (full_title.as_str(), None)
    };

    // 2. Extract Artist and Title
    // We split once by " - ". If not found, assume entire string is Title and Artist is Unknown.
    let (artist, title) = if let Some((a, t)) = metadata.split_once(" - ") {
        (Some(a.trim().to_string()), t.trim().to_string())
    } else {
        (None, metadata.trim().to_string())
    };

    Ok(Track {
        title,
        artist,
        duration_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_track_full_format() {
        let json = r#"{
            "icestats": {
                "source": { "title": "My Artist - My Song | 180" }
            }
        }"#;

        let track = parse_track(json).unwrap();
        assert_eq!(track.artist.as_deref(), Some("My Artist"));
        assert_eq!(track.title, "My Song");
        assert_eq!(track.duration_seconds, Some(180));
    }

    #[test]
    fn parse_track_no_duration() {
        let json = r#"{
            "icestats": {
                "source": { "title": "My Artist - My Song" }
            }
        }"#;

        let track = parse_track(json).unwrap();
        assert_eq!(track.artist.as_deref(), Some("My Artist"));
        assert_eq!(track.title, "My Song");
        assert_eq!(track.duration_seconds, None);
    }

    #[test]
    fn parse_track_only_title() {
        let json = r#"{
            "icestats": {
                "source": { "title": "Just A Title" }
            }
        }"#;

        let track = parse_track(json).unwrap();
        assert_eq!(track.title, "Just A Title");
        assert!(track.artist.is_none());
    }

    #[test]
    fn parse_track_fallback() {
        let json = r#"{
            "icestats": {
                "source": {}
            }
        }"#;

        let track = parse_track(json).unwrap();
        assert_eq!(track.title, "Unknown Track");
        assert_eq!(track.artist.as_deref(), Some("Unknown"));
    }

    #[test]
    fn parse_track_no_source() {
        let json = r#"{
            "icestats": {
                "admin": "admin@localhost",
                "dummy": null
            }
        }"#;

        let track = parse_track(json).unwrap();
        assert_eq!(track.title, "Unknown Track");
        assert_eq!(track.artist.as_deref(), Some("Unknown"));
    }

    #[test]
    fn parse_track_invalid_json() {
        assert!(parse_track("not json").is_err());
    }

    #[test]
    fn parse_track_multiple_dashes() {
        let json = r#"{
            "icestats": {
                "source": { "title": "A - B - C | 60" }
            }
        }"#;

        let track = parse_track(json).unwrap();
        // split_once on " - " gives artist="A", title="B - C"
        assert_eq!(track.artist.as_deref(), Some("A"));
        assert_eq!(track.title, "B - C");
        assert_eq!(track.duration_seconds, Some(60));
    }

    #[test]
    fn parse_track_non_numeric_duration() {
        let json = r#"{
            "icestats": {
                "source": { "title": "Artist - Title | abc" }
            }
        }"#;

        let track = parse_track(json).unwrap();
        assert_eq!(track.artist.as_deref(), Some("Artist"));
        assert_eq!(track.title, "Title");
        assert!(track.duration_seconds.is_none());
    }
}
