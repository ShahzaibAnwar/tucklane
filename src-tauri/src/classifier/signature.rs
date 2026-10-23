//! Opt-in file-signature ("magic bytes") detection. Reads at most 512 bytes
//! from the start of a file. Nothing is executed or parsed beyond the header.

use std::io::Read;
use std::path::Path;

use super::kinds::FileKind;

pub fn sniff_path(path: &Path) -> Option<(FileKind, &'static str)> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 512];
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return None,
        }
    }
    sniff(&buf[..n])
}

pub fn sniff(b: &[u8]) -> Option<(FileKind, &'static str)> {
    use FileKind::*;
    let starts = |sig: &[u8]| b.len() >= sig.len() && &b[..sig.len()] == sig;
    if starts(b"%PDF-") {
        return Some((Pdf, "PDF"));
    }
    if starts(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some((Image, "PNG image"));
    }
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Some((Image, "JPEG image"));
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some((Image, "GIF image"));
    }
    if b.len() >= 12 && starts(b"RIFF") {
        match &b[8..12] {
            b"WEBP" => return Some((Image, "WebP image")),
            b"AVI " => return Some((Video, "AVI video")),
            b"WAVE" => return Some((Audio, "WAV audio")),
            _ => {}
        }
    }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        return match &b[8..12] {
            b"heic" | b"heix" | b"mif1" | b"msf1" | b"avif" => Some((Image, "HEIF/AVIF image")),
            b"M4A " | b"M4B " => Some((Audio, "MPEG-4 audio")),
            _ => Some((Video, "MPEG-4/QuickTime video")),
        };
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some((Video, "Matroska/WebM video"));
    }
    if b.len() > 376 && b[0] == 0x47 && b[188] == 0x47 && b[376] == 0x47 {
        return Some((Video, "MPEG transport stream"));
    }
    if starts(&[0x00, 0x00, 0x01, 0xBA]) {
        return Some((Video, "MPEG program stream"));
    }
    if starts(b"ID3")
        || (b.len() >= 2
            && b[0] == 0xFF
            && (b[1] & 0xE0) == 0xE0
            && b[1] != 0xFF
            && b[1] & 0x06 != 0)
    {
        return Some((Audio, "MP3 audio"));
    }
    if starts(b"OggS") {
        return Some((Audio, "Ogg audio"));
    }
    if starts(b"fLaC") {
        return Some((Audio, "FLAC audio"));
    }
    if starts(b"PK\x03\x04") {
        return Some((Archive, "ZIP-based archive"));
    }
    if starts(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some((Archive, "7-Zip archive"));
    }
    if starts(b"Rar!\x1A\x07") {
        return Some((Archive, "RAR archive"));
    }
    if starts(&[0x1F, 0x8B]) {
        return Some((Archive, "gzip archive"));
    }
    if starts(b"BZh") {
        return Some((Archive, "bzip2 archive"));
    }
    if starts(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        return Some((Archive, "xz archive"));
    }
    if starts(b"MZ") {
        return Some((Installer, "Windows executable"));
    }
    if starts(b"{\\rtf") {
        return Some((Word, "Rich Text document"));
    }
    if starts(b"d8:announce") || starts(b"d13:announce-list") {
        return Some((Torrent, "torrent metadata"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_signatures() {
        assert_eq!(sniff(b"%PDF-1.7\n...").unwrap().0, FileKind::Pdf);
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0])
                .unwrap()
                .0,
            FileKind::Image
        );
        assert_eq!(sniff(b"\0\0\0\x18ftypmp42\0\0").unwrap().0, FileKind::Video);
        assert_eq!(sniff(b"\0\0\0\x18ftypheic\0\0").unwrap().0, FileKind::Image);
        assert_eq!(sniff(b"PK\x03\x04rest").unwrap().0, FileKind::Archive);
        assert_eq!(sniff(b"MZ\x90\0").unwrap().0, FileKind::Installer);
        assert!(sniff(b"hello world").is_none());
        let mut ts = vec![0u8; 400];
        ts[0] = 0x47;
        ts[188] = 0x47;
        ts[376] = 0x47;
        assert_eq!(sniff(&ts).unwrap().0, FileKind::Video);
    }
}
