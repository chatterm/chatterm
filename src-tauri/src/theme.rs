#[cfg(target_os = "macos")]
use plist::Value;
use serde::Serialize;
#[cfg(target_os = "macos")]
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct ThemeColors {
    pub name: String,
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    #[serde(rename = "selectionBackground")]
    pub selection_background: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    #[serde(rename = "brightBlack")]
    pub bright_black: String,
    #[serde(rename = "brightRed")]
    pub bright_red: String,
    #[serde(rename = "brightGreen")]
    pub bright_green: String,
    #[serde(rename = "brightYellow")]
    pub bright_yellow: String,
    #[serde(rename = "brightBlue")]
    pub bright_blue: String,
    #[serde(rename = "brightMagenta")]
    pub bright_magenta: String,
    #[serde(rename = "brightCyan")]
    pub bright_cyan: String,
    #[serde(rename = "brightWhite")]
    pub bright_white: String,
}

/// Parse a .terminal plist file and extract theme colors
#[cfg(target_os = "macos")]
pub fn parse_terminal_file(path: &str) -> Result<ThemeColors, String> {
    let val = Value::from_file(path).map_err(|e| format!("Failed to read plist: {e}"))?;
    let dict = val
        .as_dictionary()
        .ok_or("Invalid .terminal file: not a dictionary")?;

    let name = dict
        .get("name")
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Imported")
        })
        .to_string();

    let get = |key: &str, fallback: &str| -> String {
        dict.get(key)
            .and_then(|v| v.as_data())
            .and_then(parse_nscolor)
            .unwrap_or_else(|| fallback.to_string())
    };

    Ok(ThemeColors {
        name,
        background: get("BackgroundColor", "#000000"),
        foreground: get("TextColor", "#cccccc"),
        cursor: get("CursorColor", "#ffffff"),
        selection_background: get("SelectionColor", "#264f78"),
        black: get("ANSIBlackColor", "#000000"),
        red: get("ANSIRedColor", "#ff0000"),
        green: get("ANSIGreenColor", "#00ff00"),
        yellow: get("ANSIYellowColor", "#ffff00"),
        blue: get("ANSIBlueColor", "#0000ff"),
        magenta: get("ANSIMagentaColor", "#ff00ff"),
        cyan: get("ANSICyanColor", "#00ffff"),
        white: get("ANSIWhiteColor", "#ffffff"),
        bright_black: get("ANSIBrightBlackColor", "#808080"),
        bright_red: get("ANSIBrightRedColor", "#ff0000"),
        bright_green: get("ANSIBrightGreenColor", "#00ff00"),
        bright_yellow: get("ANSIBrightYellowColor", "#ffff00"),
        bright_blue: get("ANSIBrightBlueColor", "#0000ff"),
        bright_magenta: get("ANSIBrightMagentaColor", "#ff00ff"),
        bright_cyan: get("ANSIBrightCyanColor", "#00ffff"),
        bright_white: get("ANSIBrightWhiteColor", "#ffffff"),
    })
}

#[cfg(not(target_os = "macos"))]
pub fn parse_terminal_file(_path: &str) -> Result<ThemeColors, String> {
    Err("Terminal theme import is only available on macOS".to_string())
}

/// Parse NSKeyedArchiver color data to #rrggbb hex
#[cfg(target_os = "macos")]
fn parse_nscolor(data: &[u8]) -> Option<String> {
    let val = plist::Value::from_reader(std::io::Cursor::new(data)).ok()?;
    let dict = val.as_dictionary()?;
    let objects = dict.get("$objects")?.as_array()?;

    for item in objects {
        if let Some(d) = item.as_dictionary() {
            // Grayscale: NSWhite
            if let Some(w) = d.get("NSWhite").and_then(|v| v.as_data()) {
                let s = String::from_utf8_lossy(w)
                    .trim_end_matches('\0')
                    .to_string();
                if let Ok(v) = s.trim().parse::<f64>() {
                    let c = (v * 255.0).round() as u8;
                    return Some(format!("#{:02x}{:02x}{:02x}", c, c, c));
                }
            }
            // RGB: NSRGB
            if let Some(rgb) = d.get("NSRGB").and_then(|v| v.as_data()) {
                return parse_rgb_components(rgb);
            }
            // Extended: NSComponents
            if let Some(comp) = d.get("NSComponents").and_then(|v| v.as_data()) {
                return parse_rgb_components(comp);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn parse_rgb_components(data: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(data)
        .trim_end_matches('\0')
        .to_string();
    let parts: Vec<f64> = s
        .split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    if parts.len() >= 3 {
        let r = (parts[0] * 255.0).round() as u8;
        let g = (parts[1] * 255.0).round() as u8;
        let b = (parts[2] * 255.0).round() as u8;
        Some(format!("#{:02x}{:02x}{:02x}", r, g, b))
    } else {
        None
    }
}

/// List .terminal theme files from macOS Terminal's preferences
#[cfg(target_os = "macos")]
pub fn list_system_themes() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let plist_path = format!("{}/Library/Preferences/com.apple.Terminal.plist", home);

    let val = match Value::from_file(&plist_path) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let dict = match val.as_dictionary() {
        Some(d) => d,
        None => return vec![],
    };
    let profiles = match dict.get("Window Settings").and_then(|v| v.as_dictionary()) {
        Some(d) => d,
        None => return vec![],
    };

    profiles.keys().cloned().collect()
}

#[cfg(not(target_os = "macos"))]
pub fn list_system_themes() -> Vec<String> {
    Vec::new()
}

/// Export a theme from macOS Terminal preferences by name
#[cfg(target_os = "macos")]
pub fn export_system_theme(name: &str) -> Result<ThemeColors, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let plist_path = format!("{}/Library/Preferences/com.apple.Terminal.plist", home);

    let val =
        Value::from_file(&plist_path).map_err(|e| format!("Cannot read Terminal prefs: {e}"))?;
    let profiles = val
        .as_dictionary()
        .and_then(|d| d.get("Window Settings"))
        .and_then(|v| v.as_dictionary())
        .ok_or("Cannot find Window Settings")?;

    let profile = profiles
        .get(name)
        .and_then(|v| v.as_dictionary())
        .ok_or_else(|| format!("Theme '{}' not found", name))?;

    let get = |key: &str, fallback: &str| -> String {
        profile
            .get(key)
            .and_then(|v| v.as_data())
            .and_then(parse_nscolor)
            .unwrap_or_else(|| fallback.to_string())
    };

    Ok(ThemeColors {
        name: name.to_string(),
        background: get("BackgroundColor", "#000000"),
        foreground: get("TextColor", "#cccccc"),
        cursor: get("CursorColor", "#ffffff"),
        selection_background: get("SelectionColor", "#264f78"),
        black: get("ANSIBlackColor", "#000000"),
        red: get("ANSIRedColor", "#ff0000"),
        green: get("ANSIGreenColor", "#00ff00"),
        yellow: get("ANSIYellowColor", "#ffff00"),
        blue: get("ANSIBlueColor", "#0000ff"),
        magenta: get("ANSIMagentaColor", "#ff00ff"),
        cyan: get("ANSICyanColor", "#00ffff"),
        white: get("ANSIWhiteColor", "#ffffff"),
        bright_black: get("ANSIBrightBlackColor", "#808080"),
        bright_red: get("ANSIBrightRedColor", "#ff0000"),
        bright_green: get("ANSIBrightGreenColor", "#00ff00"),
        bright_yellow: get("ANSIBrightYellowColor", "#ffff00"),
        bright_blue: get("ANSIBrightBlueColor", "#0000ff"),
        bright_magenta: get("ANSIBrightMagentaColor", "#ff00ff"),
        bright_cyan: get("ANSIBrightCyanColor", "#00ffff"),
        bright_white: get("ANSIBrightWhiteColor", "#ffffff"),
    })
}

#[cfg(not(target_os = "macos"))]
pub fn export_system_theme(_name: &str) -> Result<ThemeColors, String> {
    Err("System Terminal theme import is only available on macOS".to_string())
}
