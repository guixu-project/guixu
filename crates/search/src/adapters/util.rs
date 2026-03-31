use data_core::types::DataType;

/// and content keywords.
pub(crate) fn infer_data_type_from_title(title: &str) -> DataType {
    let t = title.to_lowercase();
    // Tabular data extensions
    for ext in [".csv", ".tsv", ".parquet", ".arrow", ".xlsx", ".xls"] {
        if t.contains(ext) {
            return DataType::Tabular;
        }
    }
    // Video extensions + encoding hints
    for kw in [
        ".mp4", ".avi", ".mkv", ".mov", ".webm", ".ts", "x264", "x265", "hevc", "h264", "h265",
        "avc", "1080p", "720p", "2160p", "4k", "bluray", "bdrip", "webrip", "web-dl", "hdtv",
        "dvdrip", "remux",
    ] {
        if t.contains(kw) {
            return DataType::Video;
        }
    }
    // Image
    for kw in [
        ".png",
        ".jpg",
        ".jpeg",
        ".webp",
        ".tiff",
        ".bmp",
        ".raw",
        "imagenet",
        "coco dataset",
        "photos",
    ] {
        if t.contains(kw) {
            return DataType::Image;
        }
    }
    // Audio
    for kw in [
        ".mp3",
        ".wav",
        ".flac",
        ".ogg",
        ".aac",
        ".m4a",
        "audiobook",
        "podcast",
        "lossless",
    ] {
        if t.contains(kw) {
            return DataType::Audio;
        }
    }
    // Text
    for kw in [
        ".txt", ".md", ".jsonl", ".json", ".pdf", ".epub", ".doc", ".docx", ".ndjson",
    ] {
        if t.contains(kw) {
            return DataType::Text;
        }
    }
    // Tabular keyword hints
    if t.contains("dataset") || t.contains("database") {
        return DataType::Tabular;
    }
    // Season/episode patterns → video
    if t.contains(" s0")
        || t.contains(" s1")
        || t.contains(" s2")
        || t.contains("season")
        || t.contains("episode")
    {
        return DataType::Video;
    }
    DataType::Tabular
}

pub(super) fn parse_data_type(value: &str) -> Option<DataType> {
    match value.trim().to_lowercase().as_str() {
        "tabular" => Some(DataType::Tabular),
        "image" => Some(DataType::Image),
        "video" => Some(DataType::Video),
        "audio" => Some(DataType::Audio),
        "text" => Some(DataType::Text),
        _ => None,
    }
}

pub(super) fn contains_any_adapter(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}
