//! File kinds recognized by extension, and the folders each preset uses.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Pdf,
    Word,
    Text,
    Ebook,
    Spreadsheet,
    Presentation,
    Report,
    Image,
    Video,
    Audio,
    Subtitle,
    Installer,
    Package,
    Archive,
    Code,
    Data,
    Script,
    Notebook,
    Torrent,
    Font,
    DiskImage,
}

impl FileKind {
    pub fn label(self) -> &'static str {
        match self {
            FileKind::Pdf => "PDF document",
            FileKind::Word => "Word document",
            FileKind::Text => "Text document",
            FileKind::Ebook => "E-book",
            FileKind::Spreadsheet => "Spreadsheet",
            FileKind::Presentation => "Presentation",
            FileKind::Report => "Report file",
            FileKind::Image => "Image",
            FileKind::Video => "Video",
            FileKind::Audio => "Audio",
            FileKind::Subtitle => "Subtitles",
            FileKind::Installer => "Installer",
            FileKind::Package => "Software package",
            FileKind::Archive => "Archive",
            FileKind::Code => "Source code",
            FileKind::Data => "Data file",
            FileKind::Script => "Script",
            FileKind::Notebook => "Notebook",
            FileKind::Torrent => "Torrent metadata",
            FileKind::Font => "Font",
            FileKind::DiskImage => "Disk image",
        }
    }

    pub fn from_extension(ext: &str) -> Option<FileKind> {
        use FileKind::*;
        let k = match ext {
            "pdf" => Pdf,
            "doc" | "docx" | "odt" | "rtf" | "dotx" | "pages" => Word,
            "txt" | "md" | "markdown" | "log" => Text,
            "epub" | "mobi" | "azw3" | "djvu" => Ebook,
            "xls" | "xlsx" | "xlsm" | "xlsb" | "csv" | "tsv" | "ods" | "numbers" => Spreadsheet,
            "ppt" | "pptx" | "pps" | "ppsx" | "odp" | "key" => Presentation,
            "rpt" | "rdl" | "rdlc" | "mrt" | "jrxml" | "pbix" => Report,
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "heif" | "svg" | "tif"
            | "tiff" | "avif" | "ico" | "raw" | "cr2" | "nef" | "arw" | "dng" | "psd" => Image,
            "mp4" | "mkv" | "avi" | "mov" | "webm" | "wmv" | "m4v" | "mpg" | "mpeg" | "flv"
            | "3gp" | "m2ts" | "mts" => Video,
            "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus" | "wma" | "aiff" | "mid"
            | "midi" => Audio,
            "srt" | "ass" | "ssa" | "vtt" | "sub" | "sbv" => Subtitle,
            "exe" | "msi" => Installer,
            "msix" | "msixbundle" | "appx" | "appxbundle" | "deb" | "rpm" | "apk" | "dmg"
            | "pkg" | "appimage" | "flatpak" | "snap" | "vsix" | "nupkg" => Package,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" | "tar.gz"
            | "tar.bz2" | "tar.xz" | "tar.zst" | "cab" | "lz" | "lzma" => Archive,
            "iso" | "img" | "vhd" | "vhdx" => DiskImage,
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "py" | "rs" | "go" | "java" | "kt"
            | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "rb" | "php" | "swift" | "html" | "htm"
            | "css" | "scss" | "vue" | "svelte" | "lua" | "dart" | "r" | "scala" | "sql" => Code,
            "json" | "xml" | "yaml" | "yml" | "toml" | "ini" | "parquet" | "sqlite" | "db"
            | "ndjson" | "geojson" => Data,
            "sh" | "ps1" | "psm1" | "bat" | "cmd" | "vbs" | "reg" => Script,
            "ipynb" => Notebook,
            "torrent" => Torrent,
            "ttf" | "otf" | "woff" | "woff2" => Font,
            _ => return None,
        };
        Some(k)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Simple,
    #[default]
    Balanced,
    Advanced,
}

/// Destination folder (relative to the root, `/`-separated) for a kind.
pub fn preset_folder(preset: Preset, kind: FileKind) -> &'static str {
    use FileKind::*;
    match preset {
        Preset::Simple => match kind {
            Pdf | Word | Text | Ebook | Spreadsheet | Presentation | Report | Code | Data
            | Script | Notebook => "Documents",
            Image => "Images",
            Video | Subtitle => "Videos",
            Audio => "Audio",
            Installer | Package | DiskImage => "Software",
            Archive => "Archives",
            Torrent | Font => "Other",
        },
        Preset::Balanced => match kind {
            Pdf => "Documents/PDF",
            Word => "Documents/Word",
            Text | Ebook => "Documents",
            Spreadsheet => "Documents/Excel",
            Presentation => "Documents/Presentations",
            Report => "Documents/Reports",
            Image => "Media/Images",
            Video => "Media/Videos",
            Audio => "Media/Audio",
            Subtitle => "Media/Subtitles",
            Installer => "Software/Installers",
            Package | DiskImage => "Software/Packages",
            Archive => "Archives",
            Code | Script | Notebook => "Developer/Code",
            Data => "Developer/Data",
            Torrent => "Other/Torrents",
            Font => "Other",
        },
        Preset::Advanced => match kind {
            Pdf => "Documents/PDF",
            Word => "Documents/Word",
            Text => "Documents/Text",
            Ebook => "Documents/E-books",
            Spreadsheet => "Documents/Excel",
            Presentation => "Documents/Presentations",
            Report => "Documents/Reports",
            Image => "Media/Images",
            Video => "Media/Videos",
            Audio => "Media/Audio",
            Subtitle => "Media/Subtitles",
            Installer => "Software/Installers",
            Package => "Software/Packages",
            DiskImage => "Software/Disk Images",
            Archive => "Archives",
            Code => "Developer/Code",
            Script => "Developer/Scripts",
            Notebook => "Developer/Notebooks",
            Data => "Developer/Data",
            Torrent => "Other/Torrents",
            Font => "Other/Fonts",
        },
    }
}

/// Every folder a preset may create, for display in onboarding and rules.
pub fn preset_folders(preset: Preset) -> Vec<&'static str> {
    use FileKind::*;
    let mut v: Vec<&'static str> = [
        Pdf,
        Word,
        Text,
        Ebook,
        Spreadsheet,
        Presentation,
        Report,
        Image,
        Video,
        Audio,
        Subtitle,
        Installer,
        Package,
        Archive,
        Code,
        Data,
        Script,
        Notebook,
        Torrent,
        Font,
        DiskImage,
    ]
    .iter()
    .map(|k| preset_folder(preset, *k))
    .collect();
    v.push("Other");
    if preset != Preset::Simple {
        v.extend([
            "Documents/Invoices",
            "Documents/Resumes",
            "Documents/Receipts",
        ]);
    }
    if preset == Preset::Advanced {
        v.push("Media/Images/Screenshots");
    }
    v.sort();
    v.dedup();
    v
}
