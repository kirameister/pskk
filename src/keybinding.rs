//! IMF-agnostic keybinding parsing, matching and canonicalisation.
//!
//! PSKK stores keybindings in the config as strings such as `Ctrl+Shift+L`,
//! `Ctrl+;` or `Ctrl+ArrowUp`. The canonical form deliberately avoids any input
//! method framework's key names, so a binding means the same thing under IBus,
//! Fcitx 5 or the JSON protocol:
//!
//! * modifiers are `Ctrl`, `Alt`, `Shift`, `Super`, always in that order.
//!   Aliases (`Control`, `Meta`, `Cmd`, ...) are accepted when parsing and are
//!   rewritten to the canonical spelling;
//! * a key that produces a character is identified by that character (`l`,
//!   `;`, `:`), never by a framework keyval name. The space key is written
//!   `Space`;
//! * keys without a character use a fixed canonical name (`Enter`, `Escape`,
//!   `Tab`, `Backspace`, `Delete`, `ArrowUp` ... `ArrowDown`, `F1` ...).
//!
//! Older configs that leaked IBus/X11 keyval names (`semicolon`, `Return`,
//! `Up`, `BackSpace`) or MDN `KeyboardEvent.code` names (`KeyL`, `Digit1`) are
//! still accepted on *input*, so existing configs keep working; they are
//! rewritten to the canonical form the next time the settings are saved.

/// Modifier state of a key event or binding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_: bool,
}

/// A key that does not produce a printable character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Function(u8),
}

impl NamedKey {
    fn push_str(&self, out: &mut String) {
        match self {
            NamedKey::Enter => out.push_str("Enter"),
            NamedKey::Escape => out.push_str("Escape"),
            NamedKey::Tab => out.push_str("Tab"),
            NamedKey::Backspace => out.push_str("Backspace"),
            NamedKey::Delete => out.push_str("Delete"),
            NamedKey::ArrowUp => out.push_str("ArrowUp"),
            NamedKey::ArrowDown => out.push_str("ArrowDown"),
            NamedKey::ArrowLeft => out.push_str("ArrowLeft"),
            NamedKey::ArrowRight => out.push_str("ArrowRight"),
            NamedKey::Function(n) => {
                out.push('F');
                out.push_str(&n.to_string());
            }
        }
    }
}

/// The key half of a binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyToken {
    /// A key that produces a printable character. Compared case-insensitively:
    /// the shift state belongs to the modifier set, not to the letter's case.
    Char(char),
    Named(NamedKey),
}

impl KeyToken {
    fn matches(&self, other: &KeyToken) -> bool {
        match (self, other) {
            (KeyToken::Char(a), KeyToken::Char(b)) => a.eq_ignore_ascii_case(b),
            (KeyToken::Named(a), KeyToken::Named(b)) => a == b,
            _ => false,
        }
    }
}

/// A parsed keybinding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    pub modifiers: Modifiers,
    pub key: KeyToken,
}

impl KeyBinding {
    /// Parse a binding string such as `Ctrl+Shift+L`, `Control+semicolon` or
    /// `Meta+ArrowUp`. Returns `None` when the string is not a keybinding.
    pub fn parse(spec: &str) -> Option<Self> {
        let spec = spec.trim();
        if spec.is_empty() {
            return None;
        }

        // Split the key off the end. A trailing "++" means the key *is* '+'
        // (e.g. "Ctrl++"), which a plain split would lose.
        let (modifier_part, key_part) = if spec == "+" {
            ("", "+")
        } else if let Some(rest) = spec.strip_suffix("++") {
            (rest, "+")
        } else {
            match spec.rsplit_once('+') {
                Some((modifiers, key)) if !key.is_empty() => (modifiers, key),
                Some(_) => return None,
                None => ("", spec),
            }
        };

        let mut modifiers = Modifiers::default();
        if !modifier_part.is_empty() {
            for part in modifier_part.split('+') {
                match modifier_from_name(part)? {
                    Modifier::Ctrl => modifiers.ctrl = true,
                    Modifier::Alt => modifiers.alt = true,
                    Modifier::Shift => modifiers.shift = true,
                    Modifier::Super => modifiers.super_ = true,
                }
            }
        }

        Some(Self {
            modifiers,
            key: key_token(key_part)?,
        })
    }

    /// Build the binding for an incoming key event.
    ///
    /// `key_char` is preferred over `key_name` because it is the
    /// framework-independent description of the key: IBus reports the ';' key
    /// as `semicolon`, Fcitx 5 may report something else, but both deliver it
    /// as the character `;`. `key_name` is only consulted for keys that have no
    /// character, and is normalised through the same alias table.
    pub fn from_event(
        key_char: Option<char>,
        key_name: &str,
        modifiers: Modifiers,
    ) -> Option<Self> {
        let key = match key_char {
            Some(c) if !c.is_control() => KeyToken::Char(c),
            _ => key_token(key_name)?,
        };
        Some(Self { modifiers, key })
    }

    /// Does this configured binding refer to the same key event?
    pub fn matches(&self, event: &KeyBinding) -> bool {
        self.modifiers == event.modifiers && self.key.matches(&event.key)
    }

    /// Canonical string form, e.g. `Ctrl+Shift+L`, `Ctrl+;`, `Ctrl+Space`.
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        if self.modifiers.ctrl {
            out.push_str("Ctrl+");
        }
        if self.modifiers.alt {
            out.push_str("Alt+");
        }
        if self.modifiers.shift {
            out.push_str("Shift+");
        }
        if self.modifiers.super_ {
            out.push_str("Super+");
        }
        match self.key {
            KeyToken::Char(' ') => out.push_str("Space"),
            KeyToken::Char(c) => out.push(c),
            KeyToken::Named(named) => named.push_str(&mut out),
        }
        out
    }
}

/// Canonicalise a binding string. Used when saving settings so that the stored
/// config does not accumulate ad-hoc spellings, and so that two spellings of
/// the same key are detected as a conflict. Returns `None` if the string is not
/// a keybinding, in which case callers should keep it unchanged.
pub fn normalize(spec: &str) -> Option<String> {
    KeyBinding::parse(spec).map(|binding| binding.canonical())
}

enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

fn modifier_from_name(name: &str) -> Option<Modifier> {
    match name.trim().to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "ctl" => Some(Modifier::Ctrl),
        "alt" | "option" | "opt" => Some(Modifier::Alt),
        "shift" => Some(Modifier::Shift),
        "super" | "meta" | "cmd" | "command" | "win" | "windows" => Some(Modifier::Super),
        _ => None,
    }
}

fn key_token(part: &str) -> Option<KeyToken> {
    let name = part.trim();
    let lower = name.to_ascii_lowercase();

    // The space key is a character key: it is written `Space` and matched
    // against the character the framework delivers.
    if lower == "space" {
        return Some(KeyToken::Char(' '));
    }
    if let Some(named) = named_key(&lower) {
        return Some(KeyToken::Named(named));
    }
    if let Some(function) = function_key(&lower) {
        return Some(KeyToken::Named(NamedKey::Function(function)));
    }
    if let Some(c) = mdn_code_char(&lower) {
        return Some(KeyToken::Char(c));
    }
    if let Some(c) = punctuation_char(&lower) {
        return Some(KeyToken::Char(c));
    }

    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(KeyToken::Char(c)),
        _ => None,
    }
}

fn named_key(lower: &str) -> Option<NamedKey> {
    Some(match lower {
        "enter" | "return" | "kp_enter" => NamedKey::Enter,
        "escape" | "esc" => NamedKey::Escape,
        "tab" | "iso_left_tab" => NamedKey::Tab,
        "backspace" | "back_space" | "bs" => NamedKey::Backspace,
        "delete" | "del" => NamedKey::Delete,
        "up" | "arrowup" => NamedKey::ArrowUp,
        "down" | "arrowdown" => NamedKey::ArrowDown,
        "left" | "arrowleft" => NamedKey::ArrowLeft,
        "right" | "arrowright" => NamedKey::ArrowRight,
        _ => return None,
    })
}

fn function_key(lower: &str) -> Option<u8> {
    let digits = lower.strip_prefix('f')?;
    let number: u8 = digits.parse().ok()?;
    (1..=24).contains(&number).then_some(number)
}

/// MDN `KeyboardEvent.code` spellings (`KeyL`, `Digit1`), which the default
/// config points users at.
fn mdn_code_char(lower: &str) -> Option<char> {
    let rest = lower
        .strip_prefix("key")
        .or_else(|| lower.strip_prefix("digit"))?;
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c),
        _ => None,
    }
}

/// X11/IBus keyval names for punctuation, accepted for backwards compatibility
/// and rewritten to the character they produce.
fn punctuation_char(lower: &str) -> Option<char> {
    Some(match lower {
        "semicolon" => ';',
        "colon" => ':',
        "period" => '.',
        "comma" => ',',
        "slash" => '/',
        "backslash" => '\\',
        "minus" => '-',
        "equal" => '=',
        "plus" => '+',
        "bracketleft" => '[',
        "bracketright" => ']',
        "apostrophe" | "quote" => '\'',
        "grave" => '`',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(spec: &str) -> KeyBinding {
        KeyBinding::parse(spec).unwrap_or_else(|| panic!("failed to parse {spec:?}"))
    }

    fn event(key_char: Option<char>, key_name: &str, modifiers: Modifiers) -> KeyBinding {
        KeyBinding::from_event(key_char, key_name, modifiers)
            .unwrap_or_else(|| panic!("failed to build event for {key_name:?}"))
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Default::default()
        }
    }

    #[test]
    fn canonicalises_modifier_aliases_and_order() {
        assert_eq!(normalize("Control+l").unwrap(), "Ctrl+l");
        assert_eq!(normalize("Shift+Control+L").unwrap(), "Ctrl+Shift+L");
        assert_eq!(
            normalize("ctrl+alt+shift+super+x").unwrap(),
            "Ctrl+Alt+Shift+Super+x"
        );
        assert_eq!(normalize("Meta+K").unwrap(), "Super+K");
        assert_eq!(normalize("Cmd+K").unwrap(), "Super+K");
    }

    #[test]
    fn canonicalises_legacy_and_mdn_key_names() {
        assert_eq!(normalize("Control+semicolon").unwrap(), "Ctrl+;");
        assert_eq!(normalize("Ctrl+Return").unwrap(), "Ctrl+Enter");
        assert_eq!(normalize("Ctrl+BackSpace").unwrap(), "Ctrl+Backspace");
        assert_eq!(normalize("Control+Shift+Up").unwrap(), "Ctrl+Shift+ArrowUp");
        assert_eq!(normalize("Ctrl+space").unwrap(), "Ctrl+Space");
        assert_eq!(normalize("Control+KeyL").unwrap(), "Ctrl+l");
        assert_eq!(normalize("Control+Digit1").unwrap(), "Ctrl+1");
        assert_eq!(normalize("Ctrl+F6").unwrap(), "Ctrl+F6");
        assert_eq!(normalize("Ctrl++").unwrap(), "Ctrl++");
    }

    #[test]
    fn ui_captured_binding_matches_the_engine_event() {
        // The reported bug: the settings UI writes "Control+l" from
        // KeyboardEvent, the engine sees IBus' key name "l" with ctrl held.
        assert!(parse("Control+l").matches(&event(Some('l'), "l", ctrl())));

        // Punctuation: the UI writes ";", IBus reports the "semicolon" keyval.
        assert!(parse("Control+;").matches(&event(Some(';'), "semicolon", ctrl())));

        // Named key: the UI writes "ArrowUp", IBus reports "Up".
        assert!(parse("Control+ArrowUp").matches(&event(None, "Up", ctrl())));

        // Function key.
        assert!(parse("Control+F6").matches(&event(None, "F6", ctrl())));

        // Legacy config spelling still matches the same event.
        assert!(parse("Ctrl+semicolon").matches(&event(Some(';'), "semicolon", ctrl())));
    }

    #[test]
    fn shift_belongs_to_the_modifiers_not_the_letter_case() {
        let shifted = Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        assert!(parse("Ctrl+Shift+L").matches(&event(Some('L'), "L", shifted)));
        assert!(parse("Control+Shift+l").matches(&event(Some('L'), "L", shifted)));
        // Ctrl+L (no shift) is a different binding.
        assert!(!parse("Ctrl+L").matches(&event(Some('L'), "L", shifted)));
    }

    #[test]
    fn modifier_order_and_extra_modifiers_matter() {
        let ctrl_alt = Modifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        };
        assert!(parse("Alt+Ctrl+K").matches(&event(Some('k'), "k", ctrl_alt)));
        assert!(!parse("Ctrl+K").matches(&event(Some('k'), "k", ctrl_alt)));
    }

    #[test]
    fn rejects_what_is_not_a_binding() {
        assert!(KeyBinding::parse("").is_none());
        assert!(KeyBinding::parse("   ").is_none());
        assert!(KeyBinding::parse("Hyper+K").is_none());
        assert!(KeyBinding::parse("Ctrl+Frobnicate").is_none());
        assert!(KeyBinding::parse("Ctrl+").is_none());
    }
}
