//! PC commands said or typed into the overlay ("open Spotify", "pause the music", "volume up"). They
//! run on this PC at once instead of going to Gemini, which can't touch the PC. Pure parsing and app
//! matching; `win::apps` lists the installed apps and runs the command.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PcCommand {
    /// Open an installed app by (spoken) name.
    Open(String),
    PlayPause,
    NextTrack,
    PreviousTrack,
    VolumeUp,
    VolumeDown,
    Mute,
    Screenshot,
}

/// An installed app as Windows' Start menu lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    pub name: String,
    /// What `shell:AppsFolder\<id>` opens.
    pub id: String,
}

const LEAD: [&str; 14] = ["hey gemini", "ok gemini", "okay gemini", "gemini", "please", "can you", "could you",
    "would you", "will you", "i want to", "i'd like to", "go ahead and", "just", "and"];
const TRAIL: [&str; 6] = ["please", "for me", "now", "thanks", "thank you", "right now"];
const QUESTION: [&str; 12] = ["how", "what", "where", "why", "when", "who", "which", "is", "does", "do", "can i", "should"];

/// Lower case, no end punctuation, no polite lead-in or tail ("can you … for me").
fn clean(text: &str) -> String {
    let mut t: String = text.to_lowercase().trim().trim_end_matches(['.', '!', '?', ',']).split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let before = t.clone();
        for w in LEAD {
            if let Some(r) = t.strip_prefix(w).filter(|r| r.is_empty() || r.starts_with([' ', ','])) {
                t = r.trim_start_matches([' ', ',']).to_string();
            }
        }
        for w in TRAIL {
            if let Some(r) = t.strip_suffix(w).filter(|r| r.is_empty() || r.ends_with([' ', ','])) {
                t = r.trim_end_matches([' ', ',']).to_string();
            }
        }
        if t == before {
            return t;
        }
    }
}

/// Without "the", "this", "my", "a", "some".
fn bare(t: &str) -> String {
    t.split(' ').filter(|w| !matches!(*w, "the" | "this" | "my" | "a" | "some" | "it")).collect::<Vec<_>>().join(" ")
}

/// A PC command, or `None` to ask Gemini. Only short, command-shaped sentences count, so questions
/// like "how do I open the gate" still go to Gemini.
pub fn parse(text: &str) -> Option<PcCommand> {
    use PcCommand::*;
    let t = clean(text);
    if QUESTION.iter().any(|q| t == *q || t.starts_with(&format!("{q} "))) || t.split(' ').count() > 7 {
        return None;
    }
    let (verb, rest) = t.split_once(' ').unwrap_or((t.as_str(), ""));
    if matches!(verb, "open" | "launch" | "start" | "run" | "load") && !rest.is_empty() {
        let mut target = rest.trim_start_matches("up ").trim_start_matches("the ").to_string();
        for tail in [" app", " application", " program"] {
            if let Some(x) = target.strip_suffix(tail) {
                target = x.to_string();
            }
        }
        return (!target.is_empty() && target.split(' ').count() <= 4).then_some(Open(target));
    }
    let b = bare(&t);
    let (v, obj) = b.split_once(' ').unwrap_or((b.as_str(), ""));
    let media = |o: &str| matches!(o, "" | "music" | "song" | "track" | "video" | "spotify" | "playback" | "audio");
    match (v, obj) {
        ("pause" | "play" | "resume" | "unpause", o) if media(o) => Some(PlayPause),
        ("stop", o) if media(o) && !o.is_empty() => Some(PlayPause),
        ("next" | "skip", "" | "song" | "track" | "one") => Some(NextTrack),
        ("previous" | "last", "song" | "track" | "one") | ("previous", "") => Some(PreviousTrack),
        _ => match b.as_str() {
            "volume up" | "turn volume up" | "turn up volume" | "louder" | "turn up" | "raise volume" | "increase volume" => Some(VolumeUp),
            "volume down" | "turn volume down" | "turn down volume" | "quieter" | "turn down" | "lower volume" | "decrease volume" => Some(VolumeDown),
            "mute" | "unmute" | "mute sound" | "unmute sound" | "mute volume" => Some(Mute),
            "screenshot" | "take screenshot" | "capture screen" | "screenshot screen" => Some(Screenshot),
            _ => None,
        },
    }
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Edit distance, for one typo in a spoken or typed app name.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = cur;
        }
    }
    row[b.len()]
}

/// The installed app that best fits what was said, if any fits at all.
pub fn best_app<'a>(query: &str, apps: &'a [App]) -> Option<&'a App> {
    let q = match norm(query).as_str() {
        "files" | "file manager" | "explorer" | "my files" | "windows explorer" => "file explorer".to_string(),
        "browser" | "internet" => "microsoft edge".to_string(),
        "store" => "microsoft store".to_string(),
        other => other.to_string(),
    };
    if q.is_empty() {
        return None;
    }
    let score = |name: &str| -> u32 {
        let n = norm(name);
        let words: Vec<&str> = n.split(' ').collect();
        if n == q {
            100
        } else if n.starts_with(&format!("{q} ")) {
            90
        } else if words.contains(&q.as_str()) {
            80
        } else if q.len() >= 3 && words.iter().any(|w| w.starts_with(&q)) {
            70
        } else if q.len() >= 3 && n.contains(&q) {
            60
        } else if q.len() >= 4 && (distance(&q, &n) <= 1 || words.iter().any(|w| distance(&q, w) <= 1)) {
            50
        } else if q.len() >= 7 && distance(&q, &n) <= 2 {
            45
        } else {
            0
        }
    };
    apps.iter()
        .map(|a| (score(&a.name), a))
        .filter(|(s, _)| *s > 0)
        .max_by(|(sa, a), (sb, b)| sa.cmp(sb).then(b.name.len().cmp(&a.name.len())))
        .map(|(_, a)| a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PcCommand::*;

    #[test]
    fn open_an_app_in_many_phrasings() {
        for t in ["open spotify", "Open Spotify.", "launch spotify", "start spotify please", "can you open spotify for me",
            "hey gemini open the spotify app", "please run Spotify"] {
            assert_eq!(parse(t), Some(Open("spotify".into())), "{t}");
        }
        assert_eq!(parse("open file explorer"), Some(Open("file explorer".into())));
    }

    #[test]
    fn media_volume_and_screenshot() {
        assert_eq!(parse("pause the music"), Some(PlayPause));
        assert_eq!(parse("play music"), Some(PlayPause));
        assert_eq!(parse("resume"), Some(PlayPause));
        assert_eq!(parse("next song"), Some(NextTrack));
        assert_eq!(parse("skip this track"), Some(NextTrack));
        assert_eq!(parse("previous song"), Some(PreviousTrack));
        assert_eq!(parse("volume up"), Some(VolumeUp));
        assert_eq!(parse("turn the volume down"), Some(VolumeDown));
        assert_eq!(parse("louder"), Some(VolumeUp));
        assert_eq!(parse("mute"), Some(Mute));
        assert_eq!(parse("take a screenshot"), Some(Screenshot));
    }

    #[test]
    fn questions_still_go_to_gemini() {
        for t in ["how do i open the gate in elden ring", "what is a hadouken", "where do i start the dlc quest",
            "open", "how do I pause the game", "start"] {
            assert_eq!(parse(t), None, "{t}");
        }
    }

    fn apps() -> Vec<App> {
        ["Spotify", "File Explorer", "Google Chrome", "Steam", "Settings", "Spotify Wrapped Helper", "Microsoft Edge", "Discord", "Xbox"]
            .iter()
            .map(|n| App { name: n.to_string(), id: format!("id.{n}") })
            .collect()
    }

    #[test]
    fn app_names_match_loosely() {
        let a = apps();
        let pick = |q: &str| best_app(q, &a).map(|x| x.name.as_str());
        assert_eq!(pick("spotify"), Some("Spotify"), "exact beats a longer name");
        assert_eq!(pick("chrome"), Some("Google Chrome"), "a word of the name");
        assert_eq!(pick("explorer"), Some("File Explorer"));
        assert_eq!(pick("files"), Some("File Explorer"), "common alias");
        assert_eq!(pick("edge"), Some("Microsoft Edge"));
        assert_eq!(pick("discrod"), Some("Discord"), "one typo");
        assert_eq!(pick("photoshop"), None);
    }
}
