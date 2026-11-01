use crate::{Cue, Document, Error, Result};
use filmcraft_time::{TICKS_PER_SECOND, Tick};

fn timestamp(value: &str) -> Option<Tick> {
    let (minutes, seconds) = value.split_once(':')?;
    if minutes.is_empty() || !minutes.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let minutes: i128 = minutes.parse().ok()?;
    let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    if whole.is_empty() || whole.len() > 2 || !whole.bytes().all(|b| b.is_ascii_digit()) || fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let seconds: i128 = whole.parse().ok()?;
    if seconds >= 60 {
        return None;
    }
    let scale = 10_i128.pow(fraction.len() as u32);
    let fraction: i128 = if fraction.is_empty() { 0 } else { fraction.parse().ok()? };
    let ticks = (minutes.checked_mul(60)?.checked_add(seconds)?)
        .checked_mul(TICKS_PER_SECOND as i128)?
        .checked_add(fraction.checked_mul(TICKS_PER_SECOND as i128)?.checked_div(scale)?)?;
    i64::try_from(ticks).ok().map(Tick)
}

pub fn parse(input: &str) -> Result<Document> {
    let mut cues = Vec::new();
    let mut offset_ms = 0_i64;
    for line in input.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("[offset:").and_then(|v| v.strip_suffix(']')) {
            offset_ms = value.trim().parse::<i64>().map_err(|_| Error::NotFormat("LRC"))?;
            continue;
        }
        let mut rest = line;
        let mut times = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some((mark, tail)) = inner.split_once(']') else { break };
            let Some(time) = timestamp(mark) else { break };
            times.push(time);
            rest = tail;
        }
        if times.is_empty() || rest.trim().is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for time in times {
            let shifted = i128::from(time.0) + i128::from(offset_ms) * (TICKS_PER_SECOND as i128 / 1000);
            if let Ok(start) = i64::try_from(shifted) {
                cues.push(Cue { start: Tick(start.max(0)), text: text.clone(), ..Cue::default() });
            }
        }
    }
    if cues.is_empty() {
        return Err(Error::NotFormat("LRC"));
    }
    cues.sort_by_key(|c| c.start);
    let mut merged: Vec<Cue> = Vec::with_capacity(cues.len());
    for cue in cues {
        if let Some(last) = merged.last_mut()
            && last.start == cue.start
        {
            if !last.text.lines().any(|line| line == cue.text) {
                last.text.push('\n');
                last.text.push_str(&cue.text);
            }
            continue;
        }
        merged.push(cue);
    }
    for idx in 0..merged.len() {
        let next = merged.get(idx + 1).map(|c| c.start);
        merged[idx].end = next.filter(|end| *end > merged[idx].start).unwrap_or_else(|| Tick(merged[idx].start.0.saturating_add(3 * TICKS_PER_SECOND)));
    }
    let mut doc = Document { cues: merged, ..Document::default() };
    doc.normalize();
    Ok(doc)
}

pub fn write(doc: &Document) -> String {
    let mut output = String::new();
    for cue in &doc.cues {
        let milliseconds = (i128::from(cue.start.0.max(0)) * 1000 / i128::from(TICKS_PER_SECOND)) as u64;
        let minutes = milliseconds / 60_000;
        let seconds = milliseconds / 1000 % 60;
        let fraction = milliseconds % 1000;
        for line in cue.text.lines().filter(|line| !line.is_empty()) {
            output.push_str(&format!("[{minutes:02}:{seconds:02}.{fraction:03}]{line}\n"));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timed_lyrics_and_offset() {
        let doc = parse("[ar:Artist]\n[offset:250]\n[00:01.00][00:03.00]Hello\n[00:05.50]World").unwrap();
        assert_eq!(doc.cues.len(), 3);
        assert_eq!(doc.cues[0].start, Tick(TICKS_PER_SECOND * 5 / 4));
        assert_eq!(doc.cues[1].start, Tick(TICKS_PER_SECOND * 13 / 4));
        assert_eq!(doc.cues[2].text, "World");
        assert!(doc.cues[0].end <= doc.cues[1].start);
    }

    #[test]
    fn rejects_metadata_only_and_malformed_timestamps() {
        assert!(parse("[ar:Artist]").is_err());
        assert!(parse("[00:80.00]Bad").is_err());
    }

    #[test]
    fn duplicate_times_preserve_all_lyrics() {
        let doc = parse("[00:01.00][00:03.00]First\n[00:03.00]Second").unwrap();
        assert_eq!(doc.cues.len(), 2);
        assert_eq!(doc.cues[1].text, "First\nSecond");
        let copy = parse(&write(&doc)).unwrap();
        assert_eq!(copy.cues[1].text, doc.cues[1].text);
    }

    #[test]
    fn fractional_precision_and_negative_offset() {
        let doc = parse("[offset:-500]\n[00:01.125]First\n[00:02.5]Second").unwrap();
        assert_eq!(doc.cues[0].start, Tick(TICKS_PER_SECOND * 5 / 8));
        assert_eq!(doc.cues[1].start, Tick(TICKS_PER_SECOND * 2));
    }

    #[test]
    fn millisecond_precision_survives_lyric_export() {
        let original = parse("[00:01.125]Hold\n[00:02.999]On").unwrap();
        let exported = write(&original);
        assert!(exported.contains("[00:01.125]Hold"));
        assert!(exported.contains("[00:02.999]On"));
        let imported = parse(&exported).unwrap();
        assert_eq!(imported.cues[0].start, original.cues[0].start);
        assert_eq!(imported.cues[1].start, original.cues[1].start);
    }

    #[test]
    fn round_trip_lyrics() {
        let doc = parse("[00:01.25]Stay\n[00:03.75]Here").unwrap();
        let again = parse(&write(&doc)).unwrap();
        assert_eq!(again.cues[0].start, doc.cues[0].start);
        assert_eq!(again.cues[1].text, "Here");
    }
}
